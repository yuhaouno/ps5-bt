// SPDX-License-Identifier: GPL-3.0-or-later
//! Concrete capture -> normalized PCM -> persistent codec -> application seam.
use crate::{AudioStep, Continuity, OutputPump, PumpError, Quantum};
use bt_app::{Application, AudioProgress, Phase};
use bt_audio::{Action, AudioScheduler, BlockEncoder, FinishEncoder};
use bt_capture::{
    Backend, BridgeError, BridgeEvent, CaptureBridge, CaptureError, CaptureOwner, Cleanup,
    MAX_READ_BYTES, OwnerPhase,
};
use bt_core::{BluetoothTransport, media::MediaError};
use bt_device::BondStore;
use bt_hci::flow::AclCreditOwner;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PipelineError {
    InvalidConfiguration,
    Stopped,
    UnknownGap,
    Ended,
    NativeError(i32),
    Capture(CaptureError),
    Bridge(BridgeError),
    Audio(bt_audio::Error),
    Output(PumpError),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PipelineFailure {
    pub error: PipelineError,
    pub cleanup: PipelineCleanup,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PipelineCleanup {
    pub capture: Cleanup,
    pub encoder: Option<Result<(), bt_core::Error>>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PipelineSnapshot {
    pub format: bt_capture::Format,
    pub stopped: bool,
    pub capture: OwnerPhase,
    pub capture_reads: u32,
    pub bridge: bt_capture::BridgeStatistics,
    pub audio: bt_audio::Snapshot,
    pub encode_failures: u64,
    pub cleanup: PipelineCleanup,
}
pub struct SendBuffers<'a> {
    pub rtp: &'a mut [u8],
    pub basic: &'a mut [u8],
    pub acl: &'a mut [u8],
}

/// Owns one capture and one encoder for a single negotiated stream epoch.
/// Fixed buffers are borrowed once. Reconstruction after stop requires a new
/// scheduler/bridge stream origin; this never silently restarts capture or
/// resets an existing fault episode. Native/declared format evidence remains
/// visible through the bridge, and sample rates must match without resampling.
pub struct AudioPipeline<'a, B: Backend, E: FinishEncoder> {
    capture: CaptureOwner<B>,
    bridge: CaptureBridge<'a>,
    audio: AudioScheduler<'a>,
    encoder: EncoderSlot<E>,
    encoder_cleanup: Option<Result<(), bt_core::Error>>,
    raw: &'a mut [u8],
    stopped: bool,
    encode_failures: u64,
}
struct EncoderSlot<E: FinishEncoder> {
    encoder: Option<E>,
    parameters: bt_core::SbcParameters,
}
impl<E: FinishEncoder> BlockEncoder for EncoderSlot<E> {
    fn parameters(&self) -> bt_core::SbcParameters {
        self.parameters
    }
    fn encode_stereo(
        &mut self,
        pcm: &[[i16; 2]],
        output: &mut [u8],
    ) -> Result<usize, bt_core::Error> {
        self.encoder
            .as_mut()
            .ok_or(bt_core::Error::Disconnected)?
            .encode_stereo(pcm, output)
    }
}
impl<'a, B: Backend, E: FinishEncoder> AudioPipeline<'a, B, E> {
    /// Takes an opened capture, validates all stream contracts, then starts once.
    /// Even a validation/start failure closes the capture exactly once and
    /// returns its cleanup result. Construct this only after SBC negotiation.
    // Fixed diagnostic/cleanup records deliberately avoid heap allocation.
    #[allow(clippy::result_large_err)]
    pub fn new(
        mut capture: CaptureOwner<B>,
        bridge: CaptureBridge<'a>,
        audio: AudioScheduler<'a>,
        encoder: E,
        raw: &'a mut [u8],
    ) -> Result<Self, PipelineFailure> {
        let parameters = encoder.parameters();
        if capture.phase() != OwnerPhase::Open
            || audio.parameters() != parameters
            || bridge.format().description().sample_rate_hz != parameters.sample_rate().hz()
            || bridge.block_frames() != parameters.pcm_frames()
            || raw.is_empty()
            || raw.len() > MAX_READ_BYTES
        {
            raw.fill(0);
            return Err(PipelineFailure {
                error: PipelineError::InvalidConfiguration,
                cleanup: PipelineCleanup {
                    capture: capture.close(),
                    encoder: Some(encoder.finish()),
                },
            });
        }
        if let Err(failure) = capture.start() {
            raw.fill(0);
            return Err(PipelineFailure {
                error: PipelineError::Capture(failure.error),
                cleanup: PipelineCleanup {
                    capture: failure.cleanup,
                    encoder: Some(encoder.finish()),
                },
            });
        }
        Ok(Self {
            capture,
            bridge,
            audio,
            encoder: EncoderSlot {
                encoder: Some(encoder),
                parameters,
            },
            encoder_cleanup: None,
            raw,
            stopped: false,
            encode_failures: 0,
        })
    }
    pub fn parameters(&self) -> bt_core::SbcParameters {
        self.audio.parameters()
    }
    pub fn snapshot(&self) -> PipelineSnapshot {
        PipelineSnapshot {
            format: self.bridge.format().description(),
            stopped: self.stopped,
            capture: self.capture.phase(),
            capture_reads: self.capture.reads(),
            bridge: self.bridge.statistics(),
            audio: self.audio.snapshot(),
            encode_failures: self.encode_failures,
            cleanup: PipelineCleanup {
                capture: self.capture.cleanup(),
                encoder: self.encoder_cleanup,
            },
        }
    }
    fn active(&self) -> Result<(), PipelineError> {
        if self.stopped {
            Err(PipelineError::Stopped)
        } else {
            Ok(())
        }
    }
    fn queued(&self) -> Continuity {
        Continuity {
            queue_frames: Some(self.audio.snapshot().queued_pcm_frames),
            ..Continuity::default()
        }
    }
    /// At most one NOWAIT read/normalization and one complete PCM queue offer.
    /// A Busy queue retains the exact bridge block and forbids another read.
    /// Empty never means PCM success. Native overrun has no known sample count;
    /// it requires a fresh stream epoch instead of a fabricated timestamp gap.
    pub fn capture_step(&mut self, now_us: u64) -> Result<Quantum, PipelineError> {
        self.active()?;
        if self.bridge.needs_epoch() {
            return Err(PipelineError::UnknownGap);
        }
        if let Some(block) = self.bridge.prepare_block() {
            return match self.audio.push_pcm(block.first_sample, block.samples) {
                Ok(()) => {
                    self.bridge.confirm_block().map_err(PipelineError::Bridge)?;
                    Ok(Quantum {
                        progressed: true,
                        continuity: self.queued(),
                        ..Quantum::default()
                    })
                }
                Err(bt_audio::Error::Pcm(MediaError::Busy)) => Ok(Quantum {
                    continuity: self.queued(),
                    ..Quantum::default()
                }),
                Err(error) => Err(PipelineError::Audio(error)),
            };
        }
        if matches!(self.capture.phase(), OwnerPhase::Ended | OwnerPhase::Closed) {
            return Ok(Quantum {
                continuity: self.queued(),
                ..Quantum::default()
            });
        }
        let outcome = self
            .capture
            .read_once(now_us, self.raw)
            .map_err(|failure| PipelineError::Capture(failure.error))?;
        let result = self.bridge.ingest(outcome, self.raw, now_us, None);
        self.raw.fill(0);
        let event = result.map_err(PipelineError::Bridge)?;
        let mut continuity = self.queued();
        match event {
            BridgeEvent::Data { .. } => {}
            BridgeEvent::Empty => continuity.capture_empty = 1,
            BridgeEvent::Overrun { known_lost_frames } => {
                continuity.capture_overruns = 1;
                continuity.capture_gap_frames = known_lost_frames.unwrap_or(0);
                continuity.last_reason = Some(-210);
            }
            BridgeEvent::NativeError(raw) => {
                self.stop();
                return Err(PipelineError::NativeError(raw));
            }
            BridgeEvent::Ended => {
                self.stop();
                return Err(PipelineError::Ended);
            }
        }
        Ok(Quantum {
            progressed: matches!(event, BridgeEvent::Data { .. }),
            continuity,
            ..Quantum::default()
        })
    }
    /// At most one real encode call; a retained unaccepted block is never
    /// regenerated. This lane does not touch transport or controller credits.
    pub fn encode_step(&mut self, now_us: u64) -> Result<Quantum, PipelineError> {
        self.active()?;
        if self.bridge.needs_epoch() {
            self.stop();
            return Err(PipelineError::UnknownGap);
        }
        let before = self.audio.snapshot();
        let action = match self.audio.pull(now_us, &mut self.encoder) {
            Ok(action) => action,
            Err(error) => {
                if matches!(
                    error,
                    bt_audio::Error::Failed(
                        bt_audio::Failure::Encoder(_)
                            | bt_audio::Failure::InvalidEncodedFrame
                            | bt_audio::Failure::EncoderConfigurationChanged
                    )
                ) {
                    self.encode_failures = self.encode_failures.saturating_add(1);
                }
                return Err(PipelineError::Audio(error));
            }
        };
        let after = self.audio.snapshot();
        let mut continuity = self.queued();
        continuity.rebuffer_episodes = after
            .statistics
            .rebuffer_episodes
            .saturating_sub(before.statistics.rebuffer_episodes);
        Ok(Quantum {
            progressed: after.statistics.encoded_blocks > before.statistics.encoded_blocks,
            wake_at_us: match action {
                Action::WaitUntil(due) => Some(due),
                Action::Rebuffer { deadline_us, .. } => Some(deadline_us),
                _ => None,
            },
            continuity,
        })
    }
    /// Submit/queue at most one existing block or media fragment. No encoding
    /// occurs here: only an unexpired scheduler offer or an already queued PDU
    /// can enter the application pump at this exact same host clock instant.
    #[allow(clippy::too_many_arguments)]
    pub fn send_step<S: BondStore, T: BluetoothTransport + ?Sized>(
        &mut self,
        app: &mut Application<'_, S>,
        output: &mut OutputPump<'_>,
        credits: &mut AclCreditOwner,
        transport: &mut T,
        now_us: u64,
        scratch: SendBuffers<'_>,
    ) -> Result<Quantum, PipelineError> {
        let queued = app.status().source.is_some_and(|s| s.media_queued);
        if self.bridge.needs_epoch() {
            self.stop();
        }
        // Stop may still drain an accepted healthy prefix. It may not encode or
        // queue a new block, even if a scheduler offer was already prepared.
        if self.stopped && !queued {
            return Ok(Quantum::default());
        }
        let offered = self
            .audio
            .snapshot()
            .pending
            .is_some_and(|b| now_us < b.expires_us);
        if !queued && (!offered || app.status().phase != Phase::Streaming) {
            return Ok(Quantum::default());
        }
        let result = output
            .poll_audio(
                app,
                now_us,
                &mut self.audio,
                &mut self.encoder,
                credits,
                transport,
                scratch.rtp,
                scratch.basic,
                scratch.acl,
            )
            .map_err(PipelineError::Output)?;
        let mut continuity = self.queued();
        let progressed = match result {
            AudioStep::ControlPending => {
                continuity.send_waits = 1;
                false
            }
            AudioStep::Progress(AudioProgress::Sending(
                bt_media_session::TransmitOutcome::Busy
                | bt_media_session::TransmitOutcome::NoCredits,
            )) => {
                continuity.send_waits = 1;
                false
            }
            AudioStep::Progress(
                AudioProgress::Queued(_)
                | AudioProgress::Sending(bt_media_session::TransmitOutcome::Accepted(_)),
            ) => true,
            _ => false,
        };
        Ok(Quantum {
            progressed,
            continuity,
            ..Quantum::default()
        })
    }
    /// Prevents all later reads/encoding immediately, independently of clock
    /// validity. Source/OutputPump remain responsible for healthy partial drain.
    pub fn stop(&mut self) -> PipelineCleanup {
        self.stopped = true;
        self.audio.stop();
        self.raw.fill(0);
        let capture = self.capture.close();
        if let Some(encoder) = self.encoder.encoder.take() {
            self.encoder_cleanup = Some(encoder.finish());
        }
        PipelineCleanup {
            capture,
            encoder: self.encoder_cleanup,
        }
    }
}
impl<B: Backend, E: FinishEncoder> Drop for AudioPipeline<'_, B, E> {
    fn drop(&mut self) {
        self.stop();
    }
}
