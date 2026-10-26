// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(feature = "host-libsbc")]
use bt_audio::diagnostic::{
    EXPECTED_DIGEST, EXPECTED_PREFIX, MAX_BLOCKS, MAX_ELAPSED_US, Outcome, run,
};
use bt_core::Error;

#[test]
fn persistent_generated_candidate_is_bounded_and_deterministic() {
    let report = run(|| Ok(1_000), MAX_BLOCKS, MAX_ELAPSED_US);
    assert_eq!(report.outcome, Outcome::Complete);
    assert_eq!(
        (report.blocks, report.pcm_frames, report.encoded_bytes),
        (750, 96_000, 57_750)
    );
    assert!(report.cleanup_attempted);
    assert_eq!(report.cleanup_error, None);
    assert_eq!(report.elapsed_us, 0); // No physical-time claim from a fixed clock.
    let second = run(|| Ok(1_000), MAX_BLOCKS, MAX_ELAPSED_US);
    assert_eq!(report, second);
    assert_eq!(report.prefix, EXPECTED_PREFIX);
    assert_eq!(report.digest, EXPECTED_DIGEST);
}

#[test]
fn budgets_are_validated_before_any_clock_or_codec_call() {
    for (blocks, time) in [(0, 1), (751, 1), (1, 0), (1, MAX_ELAPSED_US + 1)] {
        let report = run(
            || panic!("invalid config must not call clock"),
            blocks,
            time,
        );
        assert_eq!(report.outcome, Outcome::InvalidBudget);
        assert!(!report.cleanup_attempted);
    }
    let report = run(|| Ok(u64::MAX), 1, 1);
    assert_eq!(report.outcome, Outcome::Overflow);
    assert!(!report.cleanup_attempted);
}

#[test]
fn deadline_and_clock_failures_finish_the_live_codec_once() {
    let mut times = [Ok(100), Ok(100), Ok(105)].into_iter();
    let report = run(|| times.next().expect("bounded clock calls"), 2, 5);
    assert_eq!(report.outcome, Outcome::Deadline);
    assert_eq!((report.blocks, report.elapsed_us), (1, 5));
    assert!(report.cleanup_attempted);
    assert_eq!(report.cleanup_error, None);

    let mut times = [Ok(100), Ok(99)].into_iter();
    let report = run(|| times.next().unwrap(), 2, 5);
    assert_eq!(report.outcome, Outcome::ClockRegressed);
    assert_eq!(report.blocks, 0);
    assert!(report.cleanup_attempted);

    let mut times = [Ok(100), Err(Error::Backend(-1234))].into_iter();
    let report = run(|| times.next().unwrap(), 2, 5);
    assert_eq!(report.outcome, Outcome::Clock);
    assert_eq!(report.backend_error, Some(Error::Backend(-1234)));
    assert!(report.cleanup_attempted);

    let report = run(|| Err(Error::Io), 2, 5);
    assert_eq!(report.outcome, Outcome::Clock);
    assert!(!report.cleanup_attempted);
}
