// SPDX-License-Identifier: GPL-3.0-or-later
use bt_audio::{
    Action, AudioScheduler, Backpressure, BlockEncoder, Config, EncodedBlock, Error, Failure, Phase,
};
use bt_core::media::{MediaError, SampleClock, TimedStereoFrame};
use bt_core::{
    Error as CodecError, SbcAllocation, SbcBlocks, SbcChannelMode, SbcParameters, SbcSampleRate,
    SbcSubbands,
};

fn parameters() -> SbcParameters {
    SbcParameters::new(
        SbcSampleRate::Hz48000,
        SbcChannelMode::JointStereo,
        SbcBlocks::Sixteen,
        SbcSubbands::Eight,
        SbcAllocation::Loudness,
        32,
    )
    .unwrap()
}
fn config() -> Config {
    Config {
        parameters: parameters(),
        epoch_us: 1_000,
        max_lateness_us: 1_000,
        max_concealed_frames: 0,
        max_rebuffer_us: 20_000,
        deadline_us: None,
    }
}
struct Encoder {
    calls: usize,
    parameters: SbcParameters,
    error: Option<CodecError>,
    invalid_length: bool,
    last_source: [i16; 2],
}
impl Default for Encoder {
    fn default() -> Self {
        Self {
            calls: 0,
            parameters: parameters(),
            error: None,
            invalid_length: false,
            last_source: [0; 2],
        }
    }
}
impl BlockEncoder for Encoder {
    fn parameters(&self) -> SbcParameters {
        self.parameters
    }
    fn encode_stereo(&mut self, pcm: &[[i16; 2]], out: &mut [u8]) -> Result<usize, CodecError> {
        self.calls += 1;
        assert_eq!(pcm.len(), 128);
        assert_eq!(out.len(), 77);
        self.last_source = pcm[0];
        if let Some(error) = self.error {
            return Err(error);
        }
        out.fill(self.calls as u8);
        out[0] = 0x9c;
        Ok(if self.invalid_length { 76 } else { 77 })
    }
}
fn ready(action: Action) -> EncodedBlock {
    match action {
        Action::Ready(block) => block,
        other => panic!("expected encoded block, got {other:?}"),
    }
}

#[test]
fn continuous_sample_clock_paces_complete_blocks_without_accumulated_rounding() {
    let mut pcm = [TimedStereoFrame::default(); 256];
    let mut render = [[0; 2]; 128];
    let mut encoded = [0; 119];
    let mut audio = AudioScheduler::new(config(), &mut pcm, &mut render, &mut encoded).unwrap();
    let mut encoder = Encoder::default();
    let clock = SampleClock::new(48_000, 1_000).unwrap();
    assert_eq!(audio.pull(999, &mut encoder), Ok(Action::WaitUntil(1_000)));
    for index in 0..1_000_u64 {
        let position = index * 128;
        let due = clock.deadline_us(position).unwrap();
        audio
            .push_pcm(position, &[[index as i16, -(index as i16)]; 128])
            .unwrap();
        let block = ready(audio.pull(due, &mut encoder).unwrap());
        assert_eq!(
            (block.first_sample, block.pcm_frames, block.bytes),
            (position, 128, 77)
        );
        assert_eq!(
            (block.due_us, block.source_frames, block.concealed_frames),
            (due, 128, 0)
        );
        assert_eq!(encoder.last_source, [index as i16, -(index as i16)]);
        audio.confirm_accepted(due, block.ticket).unwrap();
        let next_due = clock.deadline_us(position + 128).unwrap();
        assert_eq!(
            audio.pull(next_due - 1, &mut encoder),
            Ok(Action::WaitUntil(next_due))
        );
    }
    let stats = audio.snapshot().statistics;
    assert_eq!(
        (stats.input_frames, stats.rendered_source_frames),
        (128_000, 128_000)
    );
    assert_eq!(
        (
            stats.encoded_blocks,
            stats.accepted_blocks,
            stats.underrun_windows
        ),
        (1_000, 1_000, 0)
    );
    assert_eq!(
        stats.skipped_sample_frames + stats.expired_pcm_frames + stats.concealed_frames,
        0
    );
    assert_eq!(encoder.calls, 1_000);
}

#[test]
fn busy_and_no_credit_retain_identical_encoded_bytes_until_wholly_unsent_expiry() {
    let mut pcm = [TimedStereoFrame::default(); 256];
    let mut render = [[0; 2]; 128];
    let mut encoded = [0; 77];
    let mut audio = AudioScheduler::new(config(), &mut pcm, &mut render, &mut encoded).unwrap();
    audio.push_pcm(0, &[[7, -7]; 256]).unwrap();
    let mut encoder = Encoder::default();
    let first = ready(audio.pull(1_000, &mut encoder).unwrap());
    let bytes = audio.encoded_frame(first.ticket).unwrap().to_vec();
    audio
        .reject(1_001, first.ticket, Backpressure::Busy)
        .unwrap();
    audio
        .reject(1_002, first.ticket, Backpressure::NoCredits)
        .unwrap();
    assert_eq!(audio.pull(1_999, &mut encoder), Ok(Action::Ready(first)));
    assert_eq!(audio.encoded_frame(first.ticket).unwrap(), bytes);
    assert_eq!(encoder.calls, 1);
    assert_eq!(
        audio.pull(2_000, &mut encoder),
        Ok(Action::WaitUntil(3_667))
    );
    assert_eq!(audio.encoded_frame(first.ticket), Err(Error::StaleTicket));
    let next = ready(audio.pull(3_667, &mut encoder).unwrap());
    assert_eq!(next.first_sample, 128);
    assert_ne!(first.ticket, next.ticket);
    assert_eq!(
        audio.confirm_accepted(3_667, first.ticket),
        Err(Error::StaleTicket)
    );
    audio.confirm_accepted(3_667, next.ticket).unwrap();
    let stats = audio.snapshot().statistics;
    assert_eq!((stats.busy_rejections, stats.no_credit_rejections), (1, 1));
    assert_eq!(
        (stats.expired_encoded_blocks, stats.expired_encoded_frames),
        (1, 128)
    );
    assert_eq!((stats.accepted_blocks, stats.encoded_blocks), (1, 2));
}

#[test]
fn short_gap_conceals_exactly_then_rebuffer_is_atomic_and_actual_input_recovers() {
    let mut pcm = [TimedStereoFrame::default(); 256];
    let mut render = [[99; 2]; 128];
    let mut encoded = [0; 77];
    let mut audio = AudioScheduler::new(
        Config {
            max_concealed_frames: 128,
            ..config()
        },
        &mut pcm,
        &mut render,
        &mut encoded,
    )
    .unwrap();
    let mut encoder = Encoder::default();
    let concealed = ready(audio.pull(1_000, &mut encoder).unwrap());
    assert_eq!(
        (concealed.source_frames, concealed.concealed_frames),
        (0, 128)
    );
    assert_eq!(encoder.last_source, [0; 2]);
    audio.confirm_accepted(1_000, concealed.ticket).unwrap();
    for _ in 0..3 {
        assert_eq!(
            audio.pull(3_667, &mut encoder),
            Ok(Action::Rebuffer {
                first_sample: 128,
                deadline_us: 23_667
            })
        );
    }
    assert_eq!(encoder.calls, 1);
    assert_eq!(audio.snapshot().next_sample, 128);
    audio.push_pcm(128, &[[33, -33]; 128]).unwrap();
    let recovered = ready(audio.pull(3_667, &mut encoder).unwrap());
    assert_eq!(
        (
            recovered.first_sample,
            recovered.source_frames,
            recovered.concealed_frames
        ),
        (128, 128, 0)
    );
    assert_eq!(audio.snapshot().phase, Phase::Running);
    assert_eq!(audio.snapshot().rebuffer_deadline_us, None);
    let stats = audio.snapshot().statistics;
    assert_eq!((stats.concealed_frames, stats.underrun_windows), (128, 2));
    assert_eq!((stats.rebuffer_episodes, stats.rebuffer_returns), (1, 3));
}

#[test]
fn rebuffer_budget_never_resets_on_polling_or_skipping_to_a_new_window() {
    let mut pcm = [TimedStereoFrame::default(); 128];
    let mut render = [[0; 2]; 128];
    let mut encoded = [0; 77];
    let mut audio = AudioScheduler::new(
        Config {
            max_rebuffer_us: 10_000,
            ..config()
        },
        &mut pcm,
        &mut render,
        &mut encoded,
    )
    .unwrap();
    let mut encoder = Encoder::default();
    assert_eq!(
        audio.pull(1_000, &mut encoder),
        Ok(Action::Rebuffer {
            first_sample: 0,
            deadline_us: 11_000
        })
    );
    for now in [2_000, 4_000, 7_000, 10_000] {
        audio.pull(now, &mut encoder).unwrap();
        assert_eq!(audio.snapshot().rebuffer_deadline_us, Some(11_000));
    }
    assert_eq!(
        audio.pull(11_000, &mut encoder),
        Err(Error::Failed(Failure::RebufferDeadline))
    );
    assert_eq!(encoder.calls, 0);
    assert_eq!(audio.snapshot().statistics.rebuffer_episodes, 1);
    audio.stop();
    assert_eq!(audio.pull(0, &mut encoder), Ok(Action::Stopped));
    assert_eq!(audio.snapshot().failure, Some(Failure::RebufferDeadline));
}

#[test]
fn delayed_owner_skips_expired_unencoded_samples_with_exact_discard_counts() {
    let mut pcm = [TimedStereoFrame::default(); 1_024];
    let mut render = [[0; 2]; 128];
    let mut encoded = [0; 77];
    let mut audio = AudioScheduler::new(config(), &mut pcm, &mut render, &mut encoded).unwrap();
    let mut encoder = Encoder::default();
    audio.push_pcm(0, &[[1, -1]; 512]).unwrap();
    assert_eq!(
        audio.pull(10_000, &mut encoder),
        Ok(Action::WaitUntil(11_667))
    );
    assert_eq!(audio.snapshot().statistics.expired_pcm_frames, 512);
    assert_eq!(audio.snapshot().statistics.skipped_sample_frames, 512);
    assert_eq!(audio.snapshot().next_sample, 512);
    assert_eq!(encoder.calls, 0);
    audio.push_pcm(512, &[[2, -2]; 128]).unwrap();
    let next = ready(audio.pull(11_667, &mut encoder).unwrap());
    assert_eq!(next.first_sample, 512);
    assert_eq!(encoder.last_source, [2, -2]);
}

#[test]
fn accepted_block_is_not_reoffered_even_when_acceptance_reports_bad_or_expired_time() {
    for now in [999, 2_000] {
        let mut pcm = [TimedStereoFrame::default(); 128];
        let mut render = [[0; 2]; 128];
        let mut encoded = [0; 77];
        let mut audio = AudioScheduler::new(
            Config {
                deadline_us: Some(2_000),
                ..config()
            },
            &mut pcm,
            &mut render,
            &mut encoded,
        )
        .unwrap();
        let mut encoder = Encoder::default();
        audio.push_pcm(0, &[[1, -1]; 128]).unwrap();
        let frame = ready(audio.pull(1_000, &mut encoder).unwrap());
        let failure = if now == 999 {
            Failure::ClockRegressed
        } else {
            Failure::Deadline
        };
        assert_eq!(
            audio.confirm_accepted(now, frame.ticket),
            Err(Error::Failed(failure))
        );
        assert_eq!(audio.snapshot().pending, None);
        assert_eq!(audio.snapshot().statistics.accepted_blocks, 1);
        audio.stop();
        assert_eq!(audio.snapshot().statistics.stopped_encoded_blocks, 0);
    }
}

#[test]
fn late_known_acceptance_transfers_ownership_and_duplicate_confirmation_cannot_count_twice() {
    let mut pcm = [TimedStereoFrame::default(); 256];
    let mut render = [[0; 2]; 128];
    let mut encoded = [0; 77];
    let mut audio = AudioScheduler::new(config(), &mut pcm, &mut render, &mut encoded).unwrap();
    let mut encoder = Encoder::default();
    audio.push_pcm(0, &[[1, -1]; 256]).unwrap();
    let first = ready(audio.pull(1_000, &mut encoder).unwrap());
    audio.confirm_accepted(2_000, first.ticket).unwrap();
    assert_eq!(
        audio.confirm_accepted(2_000, first.ticket),
        Err(Error::StaleTicket)
    );
    assert_eq!(audio.snapshot().statistics.late_acceptances, 1);
    assert_eq!(audio.snapshot().statistics.expired_encoded_blocks, 0);
    assert_eq!(audio.snapshot().statistics.accepted_blocks, 1);
}

#[test]
fn encoder_error_or_bad_frame_fails_closed_and_does_not_encode_again() {
    for bad_frame in [false, true] {
        let mut pcm = [TimedStereoFrame::default(); 128];
        let mut render = [[0; 2]; 128];
        let mut encoded = [0; 77];
        let mut audio = AudioScheduler::new(config(), &mut pcm, &mut render, &mut encoded).unwrap();
        audio.push_pcm(0, &[[1, -1]; 128]).unwrap();
        let mut encoder = Encoder {
            error: if bad_frame {
                None
            } else {
                Some(CodecError::Backend(-123))
            },
            invalid_length: bad_frame,
            ..Encoder::default()
        };
        let failure = if bad_frame {
            Failure::InvalidEncodedFrame
        } else {
            Failure::Encoder(CodecError::Backend(-123))
        };
        assert_eq!(audio.pull(1_000, &mut encoder), Err(Error::Failed(failure)));
        assert_eq!(audio.pull(1_001, &mut encoder), Err(Error::Failed(failure)));
        assert_eq!(encoder.calls, 1);
        assert_eq!(audio.snapshot().statistics.rendered_source_frames, 128);
        assert_eq!(audio.snapshot().statistics.encoded_blocks, 0);
        audio.stop();
        assert_eq!(audio.snapshot().failure, Some(failure));
    }
}

#[test]
fn input_capacity_and_late_input_rejections_preserve_counters_and_data() {
    let mut pcm = [TimedStereoFrame::default(); 128];
    let mut render = [[0; 2]; 128];
    let mut encoded = [0; 77];
    let mut audio = AudioScheduler::new(config(), &mut pcm, &mut render, &mut encoded).unwrap();
    audio.push_pcm(0, &[[1, -1]; 128]).unwrap();
    let before = audio.snapshot();
    assert_eq!(
        audio.push_pcm(128, &[[2, -2]]),
        Err(Error::Pcm(MediaError::Busy))
    );
    assert_eq!(audio.snapshot(), before);
    let mut encoder = Encoder::default();
    let block = ready(audio.pull(1_000, &mut encoder).unwrap());
    assert_eq!(
        audio.push_pcm(0, &[[9, -9]]),
        Err(Error::Pcm(MediaError::Stale))
    );
    assert_eq!(audio.snapshot().statistics.input_frames, 128);
    audio.confirm_accepted(1_000, block.ticket).unwrap();
    audio.push_pcm(128, &[[2, -2]; 128]).unwrap();
}

#[test]
fn stop_is_clock_independent_drops_only_owned_buffers_and_is_idempotent() {
    let mut pcm = [TimedStereoFrame::default(); 256];
    let mut render = [[0; 2]; 128];
    let mut encoded = [0; 77];
    let mut audio = AudioScheduler::new(config(), &mut pcm, &mut render, &mut encoded).unwrap();
    audio.push_pcm(0, &[[1, -1]; 256]).unwrap();
    let mut encoder = Encoder::default();
    let block = ready(audio.pull(1_000, &mut encoder).unwrap());
    audio.stop();
    audio.stop();
    let stats = audio.snapshot().statistics;
    assert_eq!(
        (stats.stopped_pcm_frames, stats.stopped_encoded_blocks),
        (128, 1)
    );
    assert_eq!(audio.encoded_frame(block.ticket), Err(Error::Stopped));
    assert_eq!(audio.pull(0, &mut encoder), Ok(Action::Stopped));
    assert_eq!(audio.push_pcm(256, &[[1, -1]]), Err(Error::Stopped));
}

#[test]
fn consumer_uncertainty_is_terminal_and_cannot_be_relabelled_busy() {
    let mut pcm = [TimedStereoFrame::default(); 128];
    let mut render = [[0; 2]; 128];
    let mut encoded = [0; 77];
    let mut audio = AudioScheduler::new(config(), &mut pcm, &mut render, &mut encoded).unwrap();
    audio.push_pcm(0, &[[1, -1]; 128]).unwrap();
    let mut encoder = Encoder::default();
    let block = ready(audio.pull(1_000, &mut encoder).unwrap());
    assert_eq!(
        audio.consumer_failed(CodecError::IntegrityLost),
        Err(Error::Failed(Failure::Consumer(CodecError::IntegrityLost)))
    );
    assert_eq!(
        audio.reject(1_000, block.ticket, Backpressure::Busy),
        Err(Error::Failed(Failure::Consumer(CodecError::IntegrityLost)))
    );
    assert_eq!(audio.snapshot().statistics.busy_rejections, 0);
    assert_eq!(audio.snapshot().pending, Some(block));
}

#[test]
fn changed_encoder_configuration_and_clock_arithmetic_are_explicit_failures() {
    let mut pcm = [TimedStereoFrame::default(); 128];
    let mut render = [[0; 2]; 128];
    let mut encoded = [0; 77];
    let mut audio = AudioScheduler::new(config(), &mut pcm, &mut render, &mut encoded).unwrap();
    let mut encoder = Encoder {
        parameters: SbcParameters::new(
            SbcSampleRate::Hz48000,
            SbcChannelMode::Stereo,
            SbcBlocks::Sixteen,
            SbcSubbands::Eight,
            SbcAllocation::Loudness,
            32,
        )
        .unwrap(),
        ..Encoder::default()
    };
    assert_eq!(
        audio.pull(1_000, &mut encoder),
        Err(Error::Failed(Failure::EncoderConfigurationChanged))
    );
    assert_eq!(encoder.calls, 0);

    let mut pcm = [TimedStereoFrame::default(); 128];
    let mut render = [[0; 2]; 128];
    let mut encoded = [0; 77];
    let mut audio = AudioScheduler::new(
        Config {
            epoch_us: u64::MAX - 1_000,
            ..config()
        },
        &mut pcm,
        &mut render,
        &mut encoded,
    )
    .unwrap();
    let mut encoder = Encoder::default();
    audio.push_pcm(0, &[[1, -1]; 128]).unwrap();
    let first = ready(audio.pull(u64::MAX - 1_000, &mut encoder).unwrap());
    audio
        .confirm_accepted(u64::MAX - 1_000, first.ticket)
        .unwrap();
    assert_eq!(
        audio.pull(u64::MAX - 999, &mut encoder),
        Err(Error::Failed(Failure::Overflow))
    );
}

#[test]
fn waiting_wakes_at_run_deadline_before_the_next_audio_sample_window() {
    let mut pcm = [TimedStereoFrame::default(); 256];
    let mut render = [[0; 2]; 128];
    let mut encoded = [0; 77];
    let mut audio = AudioScheduler::new(
        Config {
            deadline_us: Some(2_000),
            ..config()
        },
        &mut pcm,
        &mut render,
        &mut encoded,
    )
    .unwrap();
    audio.push_pcm(0, &[[1, -1]; 256]).unwrap();
    let mut encoder = Encoder::default();
    let first = ready(audio.pull(1_000, &mut encoder).unwrap());
    audio.confirm_accepted(1_000, first.ticket).unwrap();
    assert_eq!(
        audio.pull(1_999, &mut encoder),
        Ok(Action::WaitUntil(2_000))
    );
    assert_eq!(
        audio.pull(2_000, &mut encoder),
        Err(Error::Failed(Failure::Deadline))
    );
    assert_eq!(encoder.calls, 1);
    audio.stop();
    assert_eq!(audio.snapshot().statistics.stopped_pcm_frames, 128);
}

#[test]
fn constructor_rejects_unusable_policy_and_storage_before_runtime_operations() {
    for bad in [
        Config {
            max_lateness_us: 0,
            ..config()
        },
        Config {
            max_rebuffer_us: 0,
            ..config()
        },
        Config {
            epoch_us: u64::MAX,
            ..config()
        },
        Config {
            deadline_us: Some(1_000),
            ..config()
        },
    ] {
        let mut pcm = [TimedStereoFrame::default(); 128];
        let mut render = [[0; 2]; 128];
        let mut encoded = [0; 77];
        assert!(matches!(
            AudioScheduler::new(bad, &mut pcm, &mut render, &mut encoded),
            Err(Error::InvalidConfig)
        ));
    }
    for (pcm_len, render_len, encoded_len) in [(127, 128, 77), (128, 127, 77), (128, 128, 76)] {
        let mut pcm = [TimedStereoFrame::default(); 128];
        let mut render = [[0; 2]; 128];
        let mut encoded = [0; 77];
        assert!(matches!(
            AudioScheduler::new(
                config(),
                &mut pcm[..pcm_len],
                &mut render[..render_len],
                &mut encoded[..encoded_len]
            ),
            Err(Error::BufferTooSmall)
        ));
    }
}
