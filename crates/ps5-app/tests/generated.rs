// SPDX-License-Identifier: GPL-3.0-or-later
//! Scripted host observations; no physical clock, capture, sleep or radio call.
use bt_audio::BlockEncoder;
use bt_core::{
    Error, SbcAllocation, SbcBlocks, SbcChannelMode, SbcParameters, SbcSampleRate, SbcSubbands,
    media::SampleClock,
};
use ps5_app::generated::*;
fn parameters(mode: SbcChannelMode, bitpool: u8) -> SbcParameters {
    SbcParameters::new(
        SbcSampleRate::Hz48000,
        mode,
        SbcBlocks::Sixteen,
        SbcSubbands::Eight,
        SbcAllocation::Loudness,
        bitpool,
    )
    .unwrap()
}
fn config(blocks: u32) -> GeneratedConfig {
    GeneratedConfig {
        parameters: parameters(SbcChannelMode::JointStereo, 32),
        epoch_us: 1000,
        duration_us: DEFAULT_DURATION_US,
        max_blocks: blocks,
        max_operations: DEFAULT_OPERATIONS,
        max_lateness_us: DEFAULT_LATENESS_US,
    }
}
struct Fake {
    parameters: SbcParameters,
    calls: u32,
    error: Option<Error>,
    invalid: bool,
}
impl Fake {
    fn new() -> Self {
        Self {
            parameters: config(1).parameters,
            calls: 0,
            error: None,
            invalid: false,
        }
    }
}
impl BlockEncoder for Fake {
    fn parameters(&self) -> SbcParameters {
        self.parameters
    }
    fn encode_stereo(&mut self, pcm: &[[i16; 2]], out: &mut [u8]) -> Result<usize, Error> {
        assert_eq!(pcm.len(), 128);
        self.calls += 1;
        if let Some(error) = self.error {
            return Err(error);
        }
        out.fill(0);
        out[0] = if self.invalid { 0 } else { 0x9c };
        Ok(self.parameters.frame_length_bytes())
    }
}
#[test]
fn observed_sample_deadlines_gate_generation_and_codec_calls() {
    let cfg = config(2);
    let mut storage = GeneratedStorage::new();
    let mut work = GeneratedWorkload::new(cfg, &mut storage).unwrap();
    let mut encoder = Fake::new();
    let clock = SampleClock::new(48_000, cfg.epoch_us).unwrap();
    assert!(work.generate(cfg.epoch_us).unwrap().progressed);
    assert!(work.encode(cfg.epoch_us, &mut encoder).unwrap().progressed);
    assert!(work.discard(cfg.epoch_us).unwrap().progressed);
    let due = clock.deadline_us(128).unwrap();
    assert_eq!(work.generate(due - 1).unwrap().wake_at_us, Some(due));
    assert!(!work.encode(due - 1, &mut encoder).unwrap().progressed);
    assert_eq!(encoder.calls, 1);
    assert!(work.generate(due).unwrap().progressed);
    assert!(work.encode(due, &mut encoder).unwrap().progressed);
    assert!(work.discard(due).unwrap().finished);
    let snapshot = work.snapshot();
    assert_eq!(snapshot.phase, Phase::Complete);
    assert_eq!(snapshot.generated_blocks, 2);
    assert_eq!(snapshot.discarded_blocks, 2);
    assert_eq!(snapshot.discarded_bytes, 154);
    assert_eq!(snapshot.scheduler.statistics.concealed_frames, 0);
    assert_eq!(snapshot.summary_matches, None); // Custom block count is not a pinned bitstream test.
    assert!(work.generate(u64::MAX).unwrap().finished);
    assert!(work.encode(u64::MAX, &mut encoder).unwrap().finished);
    assert_eq!(encoder.calls, 2);
}
#[test]
fn queue_backpressure_and_retained_frame_never_reencode_or_lose_sample_blocks() {
    let cfg = config(3);
    let mut storage = GeneratedStorage::new();
    let mut work = GeneratedWorkload::new(cfg, &mut storage).unwrap();
    let mut encoder = Fake::new();
    let clock = SampleClock::new(48_000, cfg.epoch_us).unwrap();
    let first = cfg.epoch_us;
    work.generate(first).unwrap();
    work.encode(first, &mut encoder).unwrap();
    let second = clock.deadline_us(128).unwrap();
    work.generate(second).unwrap(); // one queued block while the first encoded offer remains retained
    let third = clock.deadline_us(256).unwrap();
    assert!(!work.generate(third).unwrap().progressed);
    assert!(!work.generate(third).unwrap().progressed);
    assert_eq!(work.snapshot().staged_frames, 128);
    assert_eq!(work.snapshot().generation_backpressure, 2);
    assert!(!work.encode(third, &mut encoder).unwrap().progressed);
    assert_eq!(encoder.calls, 1);
    work.discard(third).unwrap();
    work.encode(third, &mut encoder).unwrap();
    work.generate(third).unwrap(); // retained staged input now fits
    work.discard(third).unwrap();
    work.encode(third, &mut encoder).unwrap();
    assert!(work.discard(third).unwrap().finished);
    assert_eq!(encoder.calls, 3);
    assert_eq!(work.snapshot().discarded_blocks, 3);
    assert_eq!(
        work.snapshot().scheduler.statistics.skipped_sample_frames,
        0
    );
}
#[test]
fn clock_deadline_operation_and_expiry_faults_stop_without_hashing_late_offers() {
    for fault in 0..4 {
        let mut cfg = config(1);
        if fault == 2 {
            cfg.max_operations = 3;
        }
        let mut storage = GeneratedStorage::new();
        let mut work = GeneratedWorkload::new(cfg, &mut storage).unwrap();
        let mut encoder = Fake::new();
        let error = match fault {
            0 => work.generate(cfg.epoch_us - 1).unwrap_err(),
            1 => work.generate(cfg.epoch_us + cfg.duration_us).unwrap_err(),
            2 => {
                work.encode(cfg.epoch_us, &mut encoder).unwrap();
                work.discard(cfg.epoch_us).unwrap();
                work.generate(cfg.epoch_us).unwrap();
                work.encode(cfg.epoch_us, &mut encoder).unwrap_err()
            }
            _ => {
                work.generate(cfg.epoch_us).unwrap();
                work.encode(cfg.epoch_us, &mut encoder).unwrap();
                work.discard(cfg.epoch_us + cfg.max_lateness_us)
                    .unwrap_err()
            }
        };
        let expected = [
            GeneratedFailure::ClockRegression,
            GeneratedFailure::Deadline,
            GeneratedFailure::OperationBudget,
            GeneratedFailure::ExpiredOffer,
        ][fault];
        assert_eq!(error, GeneratedError::Failed(expected));
        assert_eq!(work.snapshot().discarded_blocks, 0);
        assert_eq!(work.snapshot().digest, 0x811c9dc5);
        assert_eq!(work.snapshot().error, Some(expected));
        work.stop();
        assert_eq!(work.snapshot().phase, Phase::Failed);
        assert_eq!(work.generate(u64::MAX), Err(error));
    }
}
#[test]
fn scheduler_catchup_is_observable_failure_instead_of_fabricated_continuity() {
    let cfg = config(3);
    let mut storage = GeneratedStorage::new();
    let mut work = GeneratedWorkload::new(cfg, &mut storage).unwrap();
    let mut encoder = Fake::new();
    work.generate(cfg.epoch_us).unwrap();
    assert_eq!(
        work.encode(cfg.epoch_us + cfg.max_lateness_us, &mut encoder),
        Err(GeneratedError::Failed(GeneratedFailure::Continuity))
    );
    assert_eq!(encoder.calls, 0);
    assert!(work.snapshot().scheduler.statistics.skipped_sample_frames > 0);
    assert_eq!(work.snapshot().scheduler.statistics.expired_pcm_frames, 128);
}
#[test]
fn encoder_errors_configuration_change_and_invalid_frame_remain_terminal() {
    for fault in 0..3 {
        let cfg = config(1);
        let mut storage = GeneratedStorage::new();
        let mut work = GeneratedWorkload::new(cfg, &mut storage).unwrap();
        let mut encoder = Fake::new();
        match fault {
            0 => encoder.error = Some(Error::Backend(-99)),
            1 => encoder.parameters = parameters(SbcChannelMode::Stereo, 32),
            _ => encoder.invalid = true,
        }
        work.generate(cfg.epoch_us).unwrap();
        let error = work.encode(cfg.epoch_us, &mut encoder).unwrap_err();
        let expected = match fault {
            0 => bt_audio::Failure::Encoder(Error::Backend(-99)),
            1 => bt_audio::Failure::EncoderConfigurationChanged,
            _ => bt_audio::Failure::InvalidEncodedFrame,
        };
        assert_eq!(
            error,
            GeneratedError::Failed(GeneratedFailure::Audio(bt_audio::Error::Failed(expected)))
        );
        let calls = encoder.calls;
        assert_eq!(work.encode(cfg.epoch_us, &mut encoder), Err(error));
        assert_eq!(encoder.calls, calls);
    }
}
#[test]
fn stop_is_clock_independent_and_discards_only_diagnostic_owned_input_and_offer() {
    let cfg = config(3);
    let mut storage = GeneratedStorage::new();
    let mut work = GeneratedWorkload::new(cfg, &mut storage).unwrap();
    let mut encoder = Fake::new();
    let clock = SampleClock::new(48_000, cfg.epoch_us).unwrap();
    work.generate(cfg.epoch_us).unwrap();
    work.encode(cfg.epoch_us, &mut encoder).unwrap();
    work.generate(clock.deadline_us(128).unwrap()).unwrap();
    work.generate(clock.deadline_us(256).unwrap()).unwrap();
    work.stop();
    work.stop();
    let snapshot = work.snapshot();
    assert_eq!(snapshot.phase, Phase::Stopped);
    assert_eq!(snapshot.reason, Some(StopReason::Requested));
    assert_eq!(snapshot.stopped_staged_frames, 128);
    assert_eq!(snapshot.scheduler.statistics.stopped_pcm_frames, 128);
    assert_eq!(snapshot.scheduler.statistics.stopped_encoded_blocks, 1);
    assert_eq!(
        work.encode(u64::MAX, &mut encoder),
        Err(GeneratedError::Stopped)
    );
    assert_eq!(encoder.calls, 1);
    drop(work);
    // Workload never owns/finishes this encoder; its caller may still invoke it.
    encoder.encode_stereo(&[[0; 2]; 128], &mut [0; 77]).unwrap();
    assert_eq!(encoder.calls, 2);
}
#[test]
fn invalid_configuration_is_rejected_before_any_work_or_codec_call() {
    for fault in 0..8 {
        let mut cfg = config(1);
        match fault {
            0 => cfg.max_blocks = 0,
            1 => cfg.max_blocks = MAX_BLOCKS + 1,
            2 => cfg.duration_us = 1,
            3 => cfg.duration_us = MAX_DURATION_US + 1,
            4 => cfg.max_operations = 2,
            5 => cfg.max_lateness_us = DEFAULT_LATENESS_US + 1,
            6 => cfg.epoch_us = u64::MAX,
            _ => cfg.parameters = parameters(SbcChannelMode::JointStereo, 53),
        }
        assert!(matches!(
            GeneratedWorkload::new(cfg, &mut GeneratedStorage::new()),
            Err(GeneratedError::InvalidConfig)
        ));
    }
}

#[test]
fn full_profile_requires_the_pinned_real_bitstream_summary() {
    let cfg = config(DEFAULT_BLOCKS);
    let mut storage = GeneratedStorage::new();
    let mut work = GeneratedWorkload::new(cfg, &mut storage).unwrap();
    let mut encoder = Fake::new();
    let clock = SampleClock::new(48_000, cfg.epoch_us).unwrap();
    for block in 0..DEFAULT_BLOCKS {
        let now = clock.deadline_us(u64::from(block) * 128).unwrap();
        work.generate(now).unwrap();
        work.encode(now, &mut encoder).unwrap();
        let result = work.discard(now);
        if block == DEFAULT_BLOCKS - 1 {
            assert_eq!(
                result,
                Err(GeneratedError::Failed(GeneratedFailure::SummaryMismatch))
            );
        } else {
            result.unwrap();
        }
    }
    assert_eq!(work.snapshot().phase, Phase::Failed);
    assert_eq!(work.snapshot().summary_matches, Some(false));
}

#[cfg(feature = "host-libsbc")]
#[test]
fn persistent_real_codec_matches_independent_full_profile_for_joint_and_stereo() {
    use bt_codec::{Encoder, EncoderStorage};
    use bt_core::SbcBitpoolRange;
    for mode in [SbcChannelMode::JointStereo, SbcChannelMode::Stereo] {
        let mut cfg = config(DEFAULT_BLOCKS);
        cfg.parameters = parameters(mode, 32);
        let mut storage = GeneratedStorage::new();
        let mut work = GeneratedWorkload::new(cfg, &mut storage).unwrap();
        let mut codec_storage = EncoderStorage::<8192>::new();
        let codec = Encoder::new(
            &mut codec_storage,
            cfg.parameters,
            SbcBitpoolRange::new(2, 53).unwrap(),
        )
        .unwrap();
        let mut interleaved = [0; 256];
        let mut encoder = bt_audio::SbcEncoder::new(codec, &mut interleaved).unwrap();
        let mut reference_storage = EncoderStorage::<8192>::new();
        let mut reference = Encoder::new(
            &mut reference_storage,
            cfg.parameters,
            SbcBitpoolRange::new(2, 53).unwrap(),
        )
        .unwrap();
        let clock = SampleClock::new(48_000, cfg.epoch_us).unwrap();
        let mut digest = 0x811c9dc5_u32;
        for block in 0..DEFAULT_BLOCKS {
            let now = clock.deadline_us(u64::from(block) * 128).unwrap();
            assert!(work.generate(now).unwrap().progressed);
            assert!(work.encode(now, &mut encoder).unwrap().progressed);
            let mut pcm = [0_i16; 256];
            for (offset, frame) in pcm.chunks_exact_mut(2).enumerate() {
                let v = (((u64::from(block) * 128 + offset as u64) % 101) as i16 - 50) * 200;
                frame.copy_from_slice(&[v, -v]);
            }
            let mut output = [0; 119];
            let frame = reference.encode_frame(&pcm, &mut output).unwrap();
            for byte in &output[..frame.bytes] {
                digest = (digest ^ u32::from(*byte)).wrapping_mul(0x01000193);
            }
            let progress = work.discard(now).unwrap();
            assert_eq!(progress.finished, block == DEFAULT_BLOCKS - 1);
        }
        let snapshot = work.snapshot();
        assert_eq!(snapshot.phase, Phase::Complete);
        assert_eq!(snapshot.digest, digest);
        assert_eq!(
            snapshot.discarded_bytes,
            DEFAULT_BLOCKS * cfg.parameters.frame_length_bytes() as u32
        );
        assert_eq!(snapshot.generated_blocks, 750);
        assert_eq!(snapshot.discarded_blocks, 750);
        assert_eq!(snapshot.operations, 2250);
        assert_eq!(snapshot.scheduler.statistics.rendered_source_frames, 96_000);
        assert_eq!(snapshot.scheduler.statistics.concealed_frames, 0);
        assert_eq!(snapshot.summary_matches, Some(true));
        assert!(snapshot.elapsed_us < cfg.duration_us);
        assert!(!snapshot.completion_observed);
        let observed_done = cfg.epoch_us + snapshot.elapsed_us + 1;
        work.confirm_completion(observed_done).unwrap();
        let confirmed = work.snapshot();
        assert!(confirmed.completion_observed);
        assert_eq!(confirmed.operations, 2251);
        assert_eq!(confirmed.elapsed_us, snapshot.elapsed_us + 1);
        assert_eq!(confirmed.digest, snapshot.digest);
        assert_eq!(confirmed.scheduler, snapshot.scheduler);
        work.stop();
        assert_eq!(work.snapshot().phase, Phase::Complete);
        encoder.finish().unwrap();
        reference.finish().unwrap();
    }
}

fn complete_one<'a>(
    cfg: GeneratedConfig,
    storage: &'a mut GeneratedStorage,
    encoder: &mut Fake,
) -> GeneratedWorkload<'a> {
    let mut work = GeneratedWorkload::new(cfg, storage).unwrap();
    work.generate(cfg.epoch_us).unwrap();
    work.encode(cfg.epoch_us, encoder).unwrap();
    assert!(work.discard(cfg.epoch_us).unwrap().finished);
    assert_eq!(work.snapshot().phase, Phase::Complete);
    assert!(!work.snapshot().completion_observed);
    work
}

#[test]
fn completion_requires_a_separate_post_discard_observation_without_reencoding() {
    let mut cfg = config(1);
    cfg.max_operations = 4;
    let mut storage = GeneratedStorage::new();
    let mut encoder = Fake::new();
    let mut work = complete_one(cfg, &mut storage, &mut encoder);
    let before = work.snapshot();
    assert_eq!(before.operations, 3);
    assert_eq!(before.elapsed_us, 0);
    assert_eq!(encoder.calls, 1);
    work.confirm_completion(cfg.epoch_us + 7).unwrap();
    let confirmed = work.snapshot();
    assert!(confirmed.completion_observed);
    assert_eq!(confirmed.operations, 4);
    assert_eq!(confirmed.elapsed_us, 7);
    assert_eq!(confirmed.digest, before.digest);
    assert_eq!(confirmed.prefix, before.prefix);
    assert_eq!(confirmed.scheduler, before.scheduler);
    // Already confirmed completion is an idempotent observation record. No
    // second hash, codec call or budget use occurs when an outer owner repeats it.
    work.confirm_completion(u64::MAX).unwrap();
    work.stop();
    work.stop();
    assert_eq!(work.snapshot(), confirmed);
    assert_eq!(encoder.calls, 1);
    drop(work);
    encoder.encode_stereo(&[[0; 2]; 128], &mut [0; 77]).unwrap();
    assert_eq!(encoder.calls, 2); // Workload never consumes the external encoder.
}

#[test]
fn post_discard_deadline_regression_and_operation_exhaustion_are_sticky() {
    for fault in 0..4 {
        let mut cfg = config(1);
        if fault == 2 {
            cfg.max_operations = 3;
        }
        let mut storage = GeneratedStorage::new();
        let mut encoder = Fake::new();
        let mut work = complete_one(cfg, &mut storage, &mut encoder);
        let before = work.snapshot();
        let now = match fault {
            0 => cfg.epoch_us + cfg.duration_us, // exact exclusive deadline
            1 => cfg.epoch_us - 1,
            2 => cfg.epoch_us + 1,
            _ => u64::MAX,
        };
        let expected = match fault {
            0 | 3 => GeneratedFailure::Deadline,
            1 => GeneratedFailure::ClockRegression,
            _ => GeneratedFailure::OperationBudget,
        };
        let error = GeneratedError::Failed(expected);
        assert_eq!(work.confirm_completion(now), Err(error));
        assert_eq!(work.confirm_completion(cfg.epoch_us), Err(error));
        let after = work.snapshot();
        assert_eq!(after.phase, Phase::Failed);
        assert!(!after.completion_observed);
        assert_eq!(after.reason, Some(StopReason::Failure));
        assert_eq!(after.operations, 3);
        assert_eq!(after.digest, before.digest);
        assert_eq!(after.discarded_blocks, 1);
        assert_eq!(after.scheduler, before.scheduler);
        assert_eq!(encoder.calls, 1);
        work.stop();
        assert_eq!(work.snapshot(), after);
    }
}

#[test]
fn completion_confirmation_rejects_unfinished_or_stopped_work_without_new_work() {
    let cfg = config(1);
    let mut storage = GeneratedStorage::new();
    let mut work = GeneratedWorkload::new(cfg, &mut storage).unwrap();
    assert_eq!(
        work.confirm_completion(cfg.epoch_us),
        Err(GeneratedError::NotComplete)
    );
    assert_eq!(work.snapshot().phase, Phase::Running);
    assert_eq!(work.snapshot().operations, 0);
    assert!(!work.snapshot().completion_observed);
    work.stop();
    assert_eq!(
        work.confirm_completion(u64::MAX),
        Err(GeneratedError::Stopped)
    );
    assert!(!work.snapshot().completion_observed);
    assert_eq!(work.snapshot().operations, 0);
}

#[test]
fn completion_observation_near_u64_max_uses_checked_admission_and_ordering() {
    let mut cfg = config(1);
    cfg.epoch_us = u64::MAX - cfg.duration_us;
    let mut storage = GeneratedStorage::new();
    let mut encoder = Fake::new();
    let mut work = complete_one(cfg, &mut storage, &mut encoder);
    work.confirm_completion(u64::MAX - 1).unwrap();
    assert!(work.snapshot().completion_observed);
    assert_eq!(work.snapshot().elapsed_us, cfg.duration_us - 1);
    assert_eq!(encoder.calls, 1);
    let mut overflowing = config(1);
    overflowing.epoch_us = u64::MAX - overflowing.duration_us + 1;
    assert!(matches!(
        GeneratedWorkload::new(overflowing, &mut GeneratedStorage::new()),
        Err(GeneratedError::InvalidConfig)
    ));
}
