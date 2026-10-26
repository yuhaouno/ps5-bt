// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded continuous normalized PCM -> complete SBC block scheduling.
//! No capture ABI, allocation, transport, HCI credits or platform clock calls.
#![no_std]
#![forbid(unsafe_code)]

use bt_core::media::{MediaError, PcmQueue, RenderOutcome, SampleClock, TimedStereoFrame};
use bt_core::{Error as CodecError, SbcChannelMode, SbcParameters};

#[cfg(feature = "host-libsbc")]
mod host;
#[cfg(feature = "host-libsbc")]
pub use host::HostSbcEncoder;

/// One persistent, single-owner codec. Errors are terminal to this scheduler:
/// the encoder may have advanced even when it cannot provide a complete block.
/// Implementations must consume exactly one complete block without allocation,
/// retain no input/output pointers, and report the actual written byte count.
pub trait BlockEncoder {
    fn parameters(&self) -> SbcParameters;
    fn encode_stereo(&mut self, pcm: &[[i16; 2]], output: &mut [u8]) -> Result<usize, CodecError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    pub parameters: SbcParameters,
    /// Delivery time for stream sample zero, independent of the native capture ABI.
    pub epoch_us: u64,
    /// A block expires exclusively at its nominal due time plus this bound.
    pub max_lateness_us: u64,
    pub max_concealed_frames: usize,
    pub max_rebuffer_us: u64,
    /// Optional absolute exclusive end for a bounded candidate run.
    pub deadline_us: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    ClockRegressed,
    Overflow,
    Deadline,
    RebufferDeadline,
    EncoderConfigurationChanged,
    Encoder(CodecError),
    InvalidEncodedFrame,
    Consumer(CodecError),
    Pcm(MediaError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidConfig,
    BufferTooSmall,
    Pcm(MediaError),
    StaleTicket,
    Stopped,
    Failed(Failure),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Running,
    Rebuffering,
    Stopped,
    Failed,
}

/// Instance-scoped; never exchange tickets between schedulers or stream epochs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameTicket(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EncodedBlock {
    pub ticket: FrameTicket,
    /// Relative to this stream's sample-zero origin, not a wall-clock timestamp.
    pub first_sample: u64,
    pub pcm_frames: usize,
    pub bytes: usize,
    pub due_us: u64,
    pub expires_us: u64,
    pub source_frames: usize,
    pub concealed_frames: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Ready(EncodedBlock),
    WaitUntil(u64),
    Rebuffer { first_sample: u64, deadline_us: u64 },
    Stopped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backpressure {
    Busy,
    NoCredits,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Statistics {
    pub input_frames: u64,
    pub rendered_source_frames: u64,
    pub concealed_frames: u64,
    pub encoded_blocks: u64,
    pub accepted_blocks: u64,
    pub busy_rejections: u64,
    pub no_credit_rejections: u64,
    /// Distinct evaluated sample windows with concealment or a rebuffer result.
    pub underrun_windows: u64,
    pub rebuffer_episodes: u64,
    /// Polling results, not a count of audible interruptions.
    pub rebuffer_returns: u64,
    pub expired_pcm_frames: u64,
    pub skipped_sample_frames: u64,
    pub expired_encoded_blocks: u64,
    pub expired_encoded_frames: u64,
    pub stopped_pcm_frames: u64,
    pub stopped_encoded_blocks: u64,
    /// Accepted by the consumer after expiry; ownership is never taken back.
    pub late_acceptances: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub phase: Phase,
    pub failure: Option<Failure>,
    pub next_sample: u64,
    pub queued_pcm_frames: usize,
    pub pending: Option<EncodedBlock>,
    pub rebuffer_deadline_us: Option<u64>,
    pub statistics: Statistics,
}

/// One retained complete SBC block before downstream whole-block acceptance.
/// This type never owns a fragmented PDU. Once confirmed accepted, the downstream
/// source session owns all partial-PDU preservation and cancellation obligations.
pub struct AudioScheduler<'storage> {
    config: Config,
    clock: SampleClock,
    queue: PcmQueue<'storage>,
    render: &'storage mut [[i16; 2]],
    encoded: &'storage mut [u8],
    next_sample: u64,
    next_ticket: u64,
    pending: Option<EncodedBlock>,
    last_now: Option<u64>,
    last_underrun: Option<u64>,
    rebuffer_deadline: Option<u64>,
    phase: Phase,
    failure: Option<Failure>,
    statistics: Statistics,
}

impl<'storage> AudioScheduler<'storage> {
    pub fn new(
        config: Config,
        pcm_storage: &'storage mut [TimedStereoFrame],
        render_storage: &'storage mut [[i16; 2]],
        encoded_storage: &'storage mut [u8],
    ) -> Result<Self, Error> {
        if !matches!(
            config.parameters.channel_mode(),
            SbcChannelMode::Stereo | SbcChannelMode::JointStereo
        ) || config.max_lateness_us == 0
            || config.max_rebuffer_us == 0
            || config.deadline_us.is_some_and(|end| end <= config.epoch_us)
            || config
                .epoch_us
                .checked_add(config.max_lateness_us)
                .is_none()
        {
            return Err(Error::InvalidConfig);
        }
        let frames = config.parameters.pcm_frames();
        if pcm_storage.len() < frames
            || render_storage.len() < frames
            || encoded_storage.len() < config.parameters.frame_length_bytes()
        {
            return Err(Error::BufferTooSmall);
        }
        let queue = PcmQueue::new(pcm_storage, config.max_concealed_frames).map_err(Error::Pcm)?;
        let clock = SampleClock::new(config.parameters.sample_rate().hz(), config.epoch_us)
            .map_err(Error::Pcm)?;
        Ok(Self {
            config,
            clock,
            queue,
            render: &mut render_storage[..frames],
            encoded: encoded_storage,
            next_sample: 0,
            next_ticket: 0,
            pending: None,
            last_now: None,
            last_underrun: None,
            rebuffer_deadline: None,
            phase: Phase::Running,
            failure: None,
            statistics: Statistics::default(),
        })
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            phase: self.phase,
            failure: self.failure,
            next_sample: self.next_sample,
            queued_pcm_frames: self.queue.queued_frames(),
            pending: self.pending,
            rebuffer_deadline_us: self.rebuffer_deadline,
            statistics: self.statistics,
        }
    }

    pub const fn parameters(&self) -> SbcParameters {
        self.config.parameters
    }

    /// Normalized, known-format stereo i16 with monotonic stream sample positions.
    /// Capacity/stale rejection never overwrites queued input or advances counters.
    pub fn push_pcm(&mut self, first_sample: u64, samples: &[[i16; 2]]) -> Result<(), Error> {
        self.active()?;
        let input = self.checked_add(self.statistics.input_frames, samples.len() as u64)?;
        self.queue.push(first_sample, samples).map_err(Error::Pcm)?;
        self.statistics.input_frames = input;
        Ok(())
    }

    fn active(&self) -> Result<(), Error> {
        if let Some(failure) = self.failure {
            Err(Error::Failed(failure))
        } else if self.phase == Phase::Stopped {
            Err(Error::Stopped)
        } else {
            Ok(())
        }
    }

    fn fail<T>(&mut self, failure: Failure) -> Result<T, Error> {
        if self.failure.is_none() {
            self.failure = Some(failure);
        }
        self.phase = Phase::Failed;
        Err(Error::Failed(self.failure.unwrap_or(failure)))
    }

    fn tick(&mut self, now_us: u64) -> Result<(), Error> {
        self.active()?;
        if self.last_now.is_some_and(|previous| now_us < previous) {
            return self.fail(Failure::ClockRegressed);
        }
        self.last_now = Some(now_us);
        if self.config.deadline_us.is_some_and(|end| now_us >= end) {
            return self.fail(Failure::Deadline);
        }
        if self.rebuffer_deadline.is_some_and(|end| now_us >= end) {
            return self.fail(Failure::RebufferDeadline);
        }
        Ok(())
    }

    fn checked_add(&mut self, value: u64, extra: u64) -> Result<u64, Error> {
        value.checked_add(extra).ok_or_else(|| {
            self.failure = Some(Failure::Overflow);
            self.phase = Phase::Failed;
            Error::Failed(Failure::Overflow)
        })
    }

    fn count(
        &mut self,
        statistic: fn(&mut Statistics) -> &mut u64,
        count: u64,
    ) -> Result<(), Error> {
        let value = *statistic(&mut self.statistics);
        let total = self.checked_add(value, count)?;
        *statistic(&mut self.statistics) = total;
        Ok(())
    }

    fn media_error(&mut self, error: MediaError) -> Error {
        let failure = if error == MediaError::Overflow {
            Failure::Overflow
        } else {
            Failure::Pcm(error)
        };
        self.failure = Some(failure);
        self.phase = Phase::Failed;
        Error::Failed(failure)
    }

    fn window_time(&mut self, sample: u64) -> Result<(u64, u64), Error> {
        let due = self
            .clock
            .deadline_us(sample)
            .map_err(|e| self.media_error(e))?;
        let expires = self.checked_add(due, self.config.max_lateness_us)?;
        Ok((due, expires))
    }

    fn expire_pending(&mut self, now_us: u64) -> Result<(), Error> {
        if let Some(pending) = self.pending
            && now_us >= pending.expires_us
        {
            // Only scheduler-owned, wholly unaccepted encoded bytes can expire.
            self.count(|s| &mut s.expired_encoded_blocks, 1)?;
            self.count(|s| &mut s.expired_encoded_frames, pending.pcm_frames as u64)?;
            self.pending = None;
        }
        Ok(())
    }

    fn catch_up(&mut self, now_us: u64) -> Result<(), Error> {
        let (_, expires) = self.window_time(self.next_sample)?;
        if now_us < expires {
            return Ok(());
        }
        let threshold = now_us - self.config.max_lateness_us;
        let past = self
            .clock
            .frame_at(threshold)
            .map_err(|e| self.media_error(e))?;
        let quantum = self.config.parameters.pcm_frames() as u64;
        let after = self.checked_add(past, 1)?;
        let aligned = match after.div_ceil(quantum).checked_mul(quantum) {
            Some(aligned) => aligned,
            None => return self.fail(Failure::Overflow),
        };
        if aligned > self.next_sample {
            let skipped = aligned - self.next_sample;
            let dropped = self.queue.discard_before(aligned);
            self.count(|s| &mut s.expired_pcm_frames, dropped as u64)?;
            self.count(|s| &mut s.skipped_sample_frames, skipped)?;
            self.next_sample = aligned;
        }
        Ok(())
    }

    fn wake_at(&self, due: u64) -> u64 {
        self.config
            .deadline_us
            .into_iter()
            .chain(self.rebuffer_deadline)
            .fold(due, |value, end| value.min(end))
    }

    /// At most one encode call. The offered bytes are retained verbatim under
    /// whole-block Busy/NoCredits and are never regenerated for polling.
    /// Complete the synchronous consumer call and confirm/reject before another
    /// pull: a consumer with an unknown acceptance outcome is a terminal failure.
    pub fn pull<E: BlockEncoder + ?Sized>(
        &mut self,
        now_us: u64,
        encoder: &mut E,
    ) -> Result<Action, Error> {
        if self.phase == Phase::Stopped {
            return Ok(Action::Stopped);
        }
        self.tick(now_us)?;
        self.expire_pending(now_us)?;
        if let Some(pending) = self.pending {
            return Ok(Action::Ready(pending));
        }
        if encoder.parameters() != self.config.parameters {
            return self.fail(Failure::EncoderConfigurationChanged);
        }
        self.catch_up(now_us)?;
        let first_sample = self.next_sample;
        let (due_us, expires_us) = self.window_time(first_sample)?;
        if now_us < due_us {
            return Ok(Action::WaitUntil(self.wake_at(due_us)));
        }
        let next_sample = self.checked_add(first_sample, self.render.len() as u64)?;
        let ticket = self.checked_add(self.next_ticket, 1)?;
        let outcome = self
            .queue
            .render(first_sample, self.render)
            .map_err(|e| self.media_error(e))?;
        let report = match outcome {
            RenderOutcome::Rebuffer { .. } => {
                if self.last_underrun != Some(first_sample) {
                    self.count(|s| &mut s.underrun_windows, 1)?;
                    self.last_underrun = Some(first_sample);
                }
                if self.rebuffer_deadline.is_none() {
                    self.rebuffer_deadline =
                        Some(self.checked_add(now_us, self.config.max_rebuffer_us)?);
                    self.count(|s| &mut s.rebuffer_episodes, 1)?;
                }
                self.phase = Phase::Rebuffering;
                self.count(|s| &mut s.rebuffer_returns, 1)?;
                return Ok(Action::Rebuffer {
                    first_sample,
                    deadline_us: self.rebuffer_deadline.unwrap_or(now_us),
                });
            }
            RenderOutcome::Rendered(report) => report,
        };
        self.phase = Phase::Running;
        self.rebuffer_deadline = None;
        self.count(
            |s| &mut s.rendered_source_frames,
            report.source_frames as u64,
        )?;
        self.count(|s| &mut s.concealed_frames, report.silence_frames as u64)?;
        self.count(|s| &mut s.expired_pcm_frames, report.expired_frames as u64)?;
        // Catch-up already counts the jump from next_sample to the new window;
        // report.skipped_timeline_frames includes that same jump, so do not count it twice.
        if report.silence_frames > 0 && self.last_underrun != Some(first_sample) {
            self.count(|s| &mut s.underrun_windows, 1)?;
            self.last_underrun = Some(first_sample);
        }
        let bytes = match encoder.encode_stereo(
            self.render,
            &mut self.encoded[..self.config.parameters.frame_length_bytes()],
        ) {
            Ok(bytes) => bytes,
            Err(error) => return self.fail(Failure::Encoder(error)),
        };
        if bytes != self.config.parameters.frame_length_bytes()
            || self.encoded.first() != Some(&0x9c)
        {
            return self.fail(Failure::InvalidEncodedFrame);
        }
        self.next_sample = next_sample;
        self.next_ticket = ticket;
        self.count(|s| &mut s.encoded_blocks, 1)?;
        let block = EncodedBlock {
            ticket: FrameTicket(ticket),
            first_sample,
            pcm_frames: self.render.len(),
            bytes,
            due_us,
            expires_us,
            source_frames: report.source_frames,
            concealed_frames: report.silence_frames,
        };
        self.pending = Some(block);
        Ok(Action::Ready(block))
    }

    /// Borrow only the exact pending complete block. Call immediately after a
    /// fresh pull; this method cannot independently establish clock freshness.
    pub fn encoded_frame(&self, ticket: FrameTicket) -> Result<&[u8], Error> {
        self.active()?;
        let block = self
            .pending
            .filter(|block| block.ticket == ticket)
            .ok_or(Error::StaleTicket)?;
        Ok(&self.encoded[..block.bytes])
    }

    /// Whole-block ownership transfer, not transmission or controller completion.
    /// A known acceptance is recorded before checking a possibly late/bad clock:
    /// an expired accepted block must never be offered or dropped a second time.
    pub fn confirm_accepted(&mut self, now_us: u64, ticket: FrameTicket) -> Result<(), Error> {
        self.active()?;
        let block = self
            .pending
            .filter(|block| block.ticket == ticket)
            .ok_or(Error::StaleTicket)?;
        self.pending = None;
        self.count(|s| &mut s.accepted_blocks, 1)?;
        if now_us >= block.expires_us {
            self.count(|s| &mut s.late_acceptances, 1)?;
        }
        self.tick(now_us)
    }

    /// Busy/NoCredits must mean no part of this complete block was accepted.
    /// The reason is counted; retained bytes expire only while still ours.
    pub fn reject(
        &mut self,
        now_us: u64,
        ticket: FrameTicket,
        reason: Backpressure,
    ) -> Result<(), Error> {
        self.active()?;
        if !self.pending.is_some_and(|block| block.ticket == ticket) {
            return Err(Error::StaleTicket);
        }
        self.tick(now_us)?;
        self.count(
            match reason {
                Backpressure::Busy => |s| &mut s.busy_rejections,
                Backpressure::NoCredits => |s| &mut s.no_credit_rejections,
            },
            1,
        )?;
        self.expire_pending(now_us)
    }

    /// Unknown acceptance or a terminal downstream failure is never Busy.
    /// This stops new offers; it does not cancel downstream PDUs or restore credit.
    pub fn consumer_failed(&mut self, error: CodecError) -> Result<(), Error> {
        self.fail(Failure::Consumer(error))
    }

    /// Clock-independent and idempotent. Drops only still-owned PCM/encoded bytes;
    /// the caller separately drains/stops SourceSession and finishes the encoder.
    /// A prior failure remains observable in Snapshot after stop.
    pub fn stop(&mut self) {
        if self.phase == Phase::Stopped {
            return;
        }
        let discarded = self.queue.discard_before(u64::MAX);
        // Bounded storage cannot exceed u64 on supported workspace targets. An
        // exhausted diagnostic counter is made explicit rather than wrapping.
        if let Some(total) = self
            .statistics
            .stopped_pcm_frames
            .checked_add(discarded as u64)
        {
            self.statistics.stopped_pcm_frames = total;
        } else {
            self.failure = Some(Failure::Overflow);
        }
        if self.pending.take().is_some() {
            if let Some(total) = self.statistics.stopped_encoded_blocks.checked_add(1) {
                self.statistics.stopped_encoded_blocks = total;
            } else {
                self.failure = Some(Failure::Overflow);
            }
        }
        self.rebuffer_deadline = None;
        self.phase = Phase::Stopped;
    }
}
