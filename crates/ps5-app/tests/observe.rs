// SPDX-License-Identifier: GPL-3.0-or-later
use bt_capture::{
    Backend, Budget, CaptureError, CaptureOwner, Cleanup, Diagnostic, NativeTime, ReadKind,
    ReadOutcome,
};
use ps5_app::observe::*;
use std::{cell::RefCell, collections::VecDeque, rc::Rc};
#[derive(Default)]
struct Calls {
    open: u32,
    start: u32,
    read: u32,
    stop: u32,
    close: u32,
}
struct Script {
    outcomes: VecDeque<Result<ReadOutcome, Diagnostic>>,
    calls: Rc<RefCell<Calls>>,
    stop_error: bool,
}
fn diagnostic() -> Diagnostic {
    Diagnostic {
        abi_version: bt_capture::ABI_VERSION,
        stage: 6,
        status: -5,
        raw_status: -99,
        raw_lookup: -0x100000005,
        ..Diagnostic::default()
    }
}
impl Backend for Script {
    fn open(&mut self) -> Result<(), Diagnostic> {
        self.calls.borrow_mut().open += 1;
        Ok(())
    }
    fn start(&mut self) -> Result<(), Diagnostic> {
        self.calls.borrow_mut().start += 1;
        Ok(())
    }
    fn read_once(&mut self, bytes: &mut [u8]) -> Result<ReadOutcome, Diagnostic> {
        self.calls.borrow_mut().read += 1;
        bytes.fill(0xd4);
        self.outcomes
            .pop_front()
            .unwrap_or(Ok(ReadOutcome::empty()))
    }
    fn stop(&mut self) -> Result<(), Diagnostic> {
        self.calls.borrow_mut().stop += 1;
        if self.stop_error {
            Err(Diagnostic {
                stage: 7,
                raw_stop: -88,
                ..diagnostic()
            })
        } else {
            Ok(())
        }
    }
    fn close(&mut self) -> Result<(), Diagnostic> {
        self.calls.borrow_mut().close += 1;
        Ok(())
    }
}
fn make_owner(
    outcomes: Vec<Result<ReadOutcome, Diagnostic>>,
    reads: u32,
    deadline: u64,
    stop_error: bool,
) -> (CaptureOwner<Script>, Rc<RefCell<Calls>>) {
    let calls = Rc::new(RefCell::new(Calls::default()));
    let backend = Script {
        outcomes: outcomes.into(),
        calls: calls.clone(),
        stop_error,
    };
    let mut owner = CaptureOwner::open(
        backend,
        Budget {
            max_reads: reads,
            deadline_us: deadline,
        },
        0,
    )
    .unwrap();
    owner.start().unwrap();
    (owner, calls)
}
fn read(kind: ReadKind, raw: i32, bytes: usize, discontinuity: bool) -> ReadOutcome {
    ReadOutcome {
        kind,
        raw_status: raw,
        bytes,
        discontinuity,
        metadata: [0xa5; 16],
        native_time: None,
    }
}
fn config(reads: u32) -> ObserveConfig {
    ObserveConfig {
        max_reads: reads,
        first_settle_us: 0,
        ..ObserveConfig::BOUNDED
    }
}
#[test]
fn pacing_stitches_no_samples_and_counts_exact_raw_outcomes() {
    let mut data = read(ReadKind::Data, 3, 3, true);
    data.native_time = Some(NativeTime {
        value: u64::MAX,
        domain: 99,
    });
    let outcomes = vec![
        Ok(data),
        Ok(read(ReadKind::Empty, 0x81950002u32 as i32, 0, false)),
        Ok(read(ReadKind::Zero, 0, 0, false)),
        Ok(read(ReadKind::Overrun, 0x81950004u32 as i32, 0, true)),
        Ok(read(ReadKind::Ended, 0x81950008u32 as i32, 0, false)),
    ];
    let (mut owner, calls) = make_owner(outcomes, 20, 2_000_000, false);
    let mut work = ObserveWorkload::new(config(20), 0).unwrap();
    let mut raw = [0; 64];
    let first = work.step(&mut owner, &mut raw, 0, false).unwrap();
    assert_eq!(first.wake_at_us, Some(100_000));
    assert!(first.progressed);
    assert!(!first.finished);
    assert_eq!(raw, [0; 64]);
    let snapshot = work.snapshot();
    assert_eq!(
        snapshot.last_read.unwrap().native_time,
        Some(NativeTime {
            value: u64::MAX,
            domain: 99
        })
    );
    assert_eq!(snapshot.last_read.unwrap().host_observed_us, 0);
    raw.fill(7);
    assert!(
        !work
            .step(&mut owner, &mut raw, 99_999, false)
            .unwrap()
            .progressed
    );
    assert_eq!(raw, [0; 64]);
    assert_eq!(calls.borrow().read, 1);
    for now in [100_000, 200_000, 300_000, 400_000] {
        work.step(&mut owner, &mut raw, now, false).unwrap();
        assert_eq!(raw, [0; 64]);
    }
    let snapshot = work.snapshot();
    assert_eq!(snapshot.stop_reason, Some(ObserveStop::Ended));
    assert_eq!(
        snapshot.statistics,
        ObserveStatistics {
            owner_calls: 5,
            native_reads: 5,
            data: 1,
            empty: 1,
            zero: 1,
            overruns: 1,
            ended: 1,
            discontinuities: 2,
            unknown_gap_events: 2,
            total_bytes: 3,
            max_read_bytes: 3,
            ..ObserveStatistics::default()
        }
    );
    assert_eq!(snapshot.owner_reads_initial, Some(0));
    assert_eq!(snapshot.owner_reads_final, Some(5));
    assert_eq!(
        snapshot.owner_cleanup,
        Cleanup {
            stop: Some(Ok(())),
            close: Some(Ok(()))
        }
    );
    assert!(
        work.step(&mut owner, &mut raw, 500_000, false)
            .unwrap()
            .finished
    );
    owner.close();
    drop(owner);
    assert_eq!(calls.borrow().stop, 1);
    assert_eq!(calls.borrow().close, 1);
}
#[test]
fn explicit_settle_defers_first_read_and_stop_preempts_validation() {
    let (mut owner, calls) = make_owner(vec![], 20, 3_000_000, false);
    let mut work = ObserveWorkload::new(ObserveConfig::BOUNDED, 0).unwrap();
    let mut raw = [9; 64];
    assert_eq!(
        work.step(&mut owner, &mut raw, 0, false)
            .unwrap()
            .wake_at_us,
        Some(100_000)
    );
    assert_eq!(calls.borrow().read, 0);
    assert_eq!(raw, [0; 64]);
    assert!(
        !work
            .step(&mut owner, &mut raw, 99_999, false)
            .unwrap()
            .progressed
    );
    assert_eq!(calls.borrow().read, 0);
    assert!(
        work.step(&mut owner, &mut raw, 100_000, false)
            .unwrap()
            .progressed
    );
    assert_eq!(calls.borrow().read, 1);
    assert!(work.step(&mut owner, &mut [], 1, true).unwrap().finished);
    assert_eq!(work.snapshot().stop_reason, Some(ObserveStop::Cancelled));
    assert_eq!(calls.borrow().read, 1);
    assert_eq!(calls.borrow().stop, 0);
    assert_eq!(calls.borrow().close, 0);
    owner.close();
    assert_eq!(calls.borrow().stop, 1);
    assert_eq!(calls.borrow().close, 1);
}
#[test]
fn exact_read_and_time_budgets_do_not_close_or_reset_the_capture_owner() {
    let (mut owner, calls) = make_owner(vec![], 20, 3_000_000, false);
    let mut work = ObserveWorkload::new(config(20), 0).unwrap();
    for i in 0..20 {
        let result = work
            .step(&mut owner, &mut [0; 32], i * 100_000, false)
            .unwrap();
        assert_eq!(result.finished, i == 19);
    }
    assert_eq!(work.snapshot().stop_reason, Some(ObserveStop::ReadBudget));
    assert_eq!(calls.borrow().read, 20);
    assert_eq!(calls.borrow().stop, 0);
    work.step(&mut owner, &mut [0; 32], 2_000_000, false)
        .unwrap();
    assert_eq!(calls.borrow().read, 20);
    owner.close();
    assert_eq!(calls.borrow().stop, 1);
    let (mut owner, calls) = make_owner(vec![], 20, 3_000_000, false);
    let mut work = ObserveWorkload::new(ObserveConfig::BOUNDED, 0).unwrap();
    for i in 1..20 {
        work.step(&mut owner, &mut [0; 32], i * 100_000, false)
            .unwrap();
    }
    assert!(
        work.step(&mut owner, &mut [0; 32], 2_000_000, false)
            .unwrap()
            .finished
    );
    assert_eq!(work.snapshot().stop_reason, Some(ObserveStop::Deadline));
    assert_eq!(calls.borrow().read, 19);
    assert_eq!(calls.borrow().stop, 0);
}
#[test]
fn late_turns_make_one_read_without_catch_up_or_sample_gap_estimates() {
    let (mut owner, calls) = make_owner(vec![], 20, 3_000_000, false);
    let mut work = ObserveWorkload::new(config(20), 0).unwrap();
    let progress = work
        .step(&mut owner, &mut [0; 32], 1_000_000, false)
        .unwrap();
    assert_eq!(calls.borrow().read, 1);
    assert_eq!(progress.wake_at_us, Some(1_100_000));
    work.step(&mut owner, &mut [0; 32], 1_099_999, false)
        .unwrap();
    assert_eq!(calls.borrow().read, 1);
    assert_eq!(work.snapshot().statistics.unknown_gap_events, 0);
}
#[test]
fn prior_owner_reads_and_stricter_budget_are_preserved() {
    let (mut owner, calls) = make_owner(vec![], 2, 3_000_000, false);
    owner.read_once(1, &mut [0; 32]).unwrap();
    let mut work = ObserveWorkload::new(config(20), 1).unwrap();
    work.step(&mut owner, &mut [0; 32], 1, false).unwrap();
    assert_eq!(work.snapshot().owner_reads_initial, Some(1));
    let error = work
        .step(&mut owner, &mut [0; 32], 100_001, false)
        .unwrap_err();
    assert!(matches!(error,ObserveError::Owner(f) if f.error==CaptureError::ReadBudget));
    assert_eq!(calls.borrow().read, 2);
    assert_eq!(work.snapshot().statistics.owner_calls, 2);
    assert_eq!(work.snapshot().statistics.native_reads, 1);
    assert_eq!(work.snapshot().owner_reads_final, Some(2));
    assert_eq!(
        work.step(&mut owner, &mut [0; 32], 200_001, false),
        Err(error)
    );
    assert_eq!(calls.borrow().read, 2);
    assert_eq!(calls.borrow().stop, 1);
}
#[test]
fn native_and_backend_errors_preserve_raw_failures_and_auto_cleanup() {
    for backend_error in [false, true] {
        let outcome = if backend_error {
            Err(diagnostic())
        } else {
            Ok(read(ReadKind::Error, -123, 0, false))
        };
        let (mut owner, calls) = make_owner(vec![outcome], 20, 3_000_000, true);
        let mut work = ObserveWorkload::new(config(20), 0).unwrap();
        let mut raw = [9; 32];
        let error = work.step(&mut owner, &mut raw, 0, false).unwrap_err();
        assert_eq!(raw, [0; 32]);
        assert!(work.snapshot().owner_cleanup.uncertain());
        if backend_error {
            assert!(
                matches!(error,ObserveError::Owner(f) if matches!(f.error,CaptureError::Backend(d) if d.raw_status==-99&&d.raw_lookup==-0x100000005))
            );
            assert_eq!(work.snapshot().statistics.owner_failures, 1);
        } else {
            assert!(
                matches!(error,ObserveError::NativeRead{raw_status:-123,cleanup} if cleanup.uncertain())
            );
            assert_eq!(work.snapshot().statistics.native_errors, 1);
            assert_eq!(work.snapshot().last_read.unwrap().raw_status, -123);
        }
        owner.close();
        drop(owner);
        assert_eq!(calls.borrow().read, 1);
        assert_eq!(calls.borrow().stop, 1);
        assert_eq!(calls.borrow().close, 1);
    }
}
#[test]
fn terminal_cleanup_uncertainty_remains_failure_after_ended() {
    let (mut owner, calls) = make_owner(
        vec![Ok(read(ReadKind::Ended, 0x81950008u32 as i32, 0, false))],
        20,
        3_000_000,
        true,
    );
    let mut work = ObserveWorkload::new(config(20), 0).unwrap();
    assert!(
        matches!(work.step(&mut owner,&mut[0;32],0,false),Err(ObserveError::Cleanup(cleanup)) if cleanup.uncertain())
    );
    assert_eq!(work.snapshot().statistics.ended, 1);
    assert_eq!(work.snapshot().stop_reason, Some(ObserveStop::Failed));
    drop(owner);
    assert_eq!(calls.borrow().close, 1);
}
#[test]
fn config_clock_buffer_and_owner_interleaving_fail_before_an_extra_native_read() {
    for config in [
        ObserveConfig {
            max_reads: 0,
            ..config(20)
        },
        ObserveConfig {
            max_reads: MAX_OBSERVE_READS + 1,
            ..config(20)
        },
        ObserveConfig {
            duration_us: 2_000_001,
            ..config(20)
        },
        ObserveConfig {
            first_settle_us: 99,
            ..config(20)
        },
        ObserveConfig {
            interval_us: 1,
            ..config(20)
        },
    ] {
        assert!(matches!(
            ObserveWorkload::new(config, 0),
            Err(ObserveError::InvalidConfig)
        ));
    }
    assert!(matches!(
        ObserveWorkload::new(config(20), u64::MAX),
        Err(ObserveError::ClockOverflow)
    ));
    let (mut owner, calls) = make_owner(vec![], 20, 3_000_000, false);
    let mut work = ObserveWorkload::new(config(20), 10).unwrap();
    assert_eq!(
        work.step(&mut owner, &mut [9; 32], 9, false),
        Err(ObserveError::ClockRegressed)
    );
    assert_eq!(calls.borrow().read, 0);
    let mut work = ObserveWorkload::new(config(20), 10).unwrap();
    assert_eq!(
        work.step(&mut owner, &mut [], 10, false),
        Err(ObserveError::InvalidBuffer)
    );
    assert_eq!(calls.borrow().read, 0);
    let mut work = ObserveWorkload::new(config(20), 10).unwrap();
    work.step(&mut owner, &mut [0; 32], 10, false).unwrap();
    owner.read_once(11, &mut [0; 32]).unwrap();
    assert_eq!(
        work.step(&mut owner, &mut [9; 32], 100_010, false),
        Err(ObserveError::OwnerProgressChanged)
    );
    assert_eq!(calls.borrow().read, 2);
}
#[cfg(feature = "host-candidate")]
#[test]
fn actual_existing_c_capture_fixture_produces_only_redacted_raw_observations() {
    use bt_capture::{Avcap2Backend, CaptureStorage, ResolverOps};
    use core::ffi::{c_char, c_void};
    unsafe extern "C" {
        fn ps5_capture_test_reset();
        fn ps5_capture_test_read(raw: i32);
        fn ps5_capture_test_calls(phase: u32) -> u32;
        fn ps5_capture_test_current(name: *const c_char, out: *mut *mut c_void) -> i64;
        fn ps5_capture_test_module(module: i32, name: *const c_char, out: *mut *mut c_void) -> i64;
    }
    // SAFETY: this test exclusively owns the process-global synthetic reviewed C
    // exports. Their signatures/layouts match the existing actual C adapter and
    // all calls stay on this thread; there is no native PS5 or OS capture access.
    unsafe {
        ps5_capture_test_reset();
        let mut storage = CaptureStorage::<9216>::new();
        let backend = Avcap2Backend::new(
            &mut storage,
            ResolverOps {
                abi_version: bt_capture::ABI_VERSION,
                struct_bytes: core::mem::size_of::<ResolverOps>() as u32,
                lookup_current: ps5_capture_test_current,
                lookup_module: ps5_capture_test_module,
            },
        )
        .unwrap();
        let mut owner = CaptureOwner::open(
            backend,
            Budget {
                max_reads: 3,
                deadline_us: 2_000_000,
            },
            0,
        )
        .unwrap();
        owner.start().unwrap();
        let mut work = ObserveWorkload::new(config(3), 0).unwrap();
        let mut raw = [9; 64];
        ps5_capture_test_read(7);
        work.step(&mut owner, &mut raw, 0, false).unwrap();
        assert_eq!(raw, [0; 64]);
        assert_eq!(work.snapshot().statistics.total_bytes, 7);
        assert_eq!(work.snapshot().last_read.unwrap().native_time, None);
        ps5_capture_test_read(0x81950004u32 as i32);
        work.step(&mut owner, &mut raw, 100_000, false).unwrap();
        assert_eq!(work.snapshot().statistics.unknown_gap_events, 1);
        ps5_capture_test_read(0x81950002u32 as i32);
        assert!(
            work.step(&mut owner, &mut raw, 200_000, false)
                .unwrap()
                .finished
        );
        assert_eq!(work.snapshot().stop_reason, Some(ObserveStop::ReadBudget));
        assert_eq!(ps5_capture_test_calls(4), 3);
        assert_eq!(ps5_capture_test_calls(5), 0);
        owner.close();
        drop(owner);
        for phase in [5, 6, 7] {
            assert_eq!(ps5_capture_test_calls(phase), 1);
        }
        // Reuse this single fixture-owning test for the faster profile, avoiding
        // parallel access to the C fixture's process-global synthetic exports.
        ps5_capture_test_reset();
        let backend = Avcap2Backend::new(
            &mut storage,
            ResolverOps {
                abi_version: bt_capture::ABI_VERSION,
                struct_bytes: core::mem::size_of::<ResolverOps>() as u32,
                lookup_current: ps5_capture_test_current,
                lookup_module: ps5_capture_test_module,
            },
        )
        .unwrap();
        let mut owner = CaptureOwner::open(
            backend,
            Budget {
                max_reads: 256,
                deadline_us: 3_000_000,
            },
            0,
        )
        .unwrap();
        owner.start().unwrap();
        let mut work = ObserveWorkload::new(ObserveConfig::CONTINUOUS, 0).unwrap();
        ps5_capture_test_read(7);
        for index in 0..190 {
            work.step(&mut owner, &mut raw, 100_000 + index * 10_000, false)
                .unwrap();
            assert_eq!(raw, [0; 64]);
        }
        assert_eq!(work.snapshot().statistics.native_reads, 190);
        assert_eq!(work.snapshot().statistics.data, 190);
        assert_eq!(work.snapshot().statistics.total_bytes, 190 * 7);
        assert!(work.snapshot().last_read.unwrap().native_time.is_none());
        assert!(
            work.step(&mut owner, &mut raw, 2_000_000, false)
                .unwrap()
                .finished
        );
        assert_eq!(work.snapshot().stop_reason, Some(ObserveStop::Deadline));
        assert_eq!(ps5_capture_test_calls(4), 190);
        assert_eq!(ps5_capture_test_calls(5), 0);
        owner.close();
        drop(owner);
        for phase in [5, 6, 7] {
            assert_eq!(ps5_capture_test_calls(phase), 1);
        }
    }
}

#[test]
fn valid_deadline_near_u64_max_clamps_next_wake_without_overflow() {
    let (mut owner, calls) = make_owner(vec![], 20, u64::MAX, false);
    let mut work = ObserveWorkload::new(ObserveConfig::BOUNDED, u64::MAX - 2_000_000).unwrap();
    let progress = work
        .step(&mut owner, &mut [0; 32], u64::MAX - 1, false)
        .unwrap();
    assert_eq!(progress.wake_at_us, Some(u64::MAX));
    assert_eq!(calls.borrow().read, 1);
    let progress = work
        .step(&mut owner, &mut [0; 32], u64::MAX, false)
        .unwrap();
    assert!(progress.finished);
    assert_eq!(work.snapshot().stop_reason, Some(ObserveStop::Deadline));
    assert_eq!(calls.borrow().read, 1);
}

#[test]
fn continuous_profile_drains_every_10ms_with_256_budget_and_190_window_ceiling() {
    assert_eq!(ObserveConfig::BOUNDED.max_reads, 20);
    assert_eq!(ObserveConfig::BOUNDED.interval_us, 100_000);
    assert_eq!(ObserveConfig::CONTINUOUS.max_reads, 256);
    assert_eq!(ObserveConfig::CONTINUOUS.interval_us, 10_000);
    let (mut owner, calls) = make_owner(vec![], 256, 3_000_000, false);
    let mut work = ObserveWorkload::new(ObserveConfig::CONTINUOUS, 0).unwrap();
    let mut raw = [9; 32];
    assert_eq!(
        work.step(&mut owner, &mut raw, 99_999, false)
            .unwrap()
            .wake_at_us,
        Some(100_000)
    );
    assert_eq!(calls.borrow().read, 0);
    assert_eq!(raw, [0; 32]);
    for index in 0..190 {
        let now = 100_000 + index * 10_000;
        let progress = work.step(&mut owner, &mut raw, now, false).unwrap();
        assert!(progress.progressed);
        assert!(!progress.finished);
        assert_eq!(progress.wake_at_us, Some(now + 10_000));
        assert_eq!(raw, [0; 32]);
    }
    assert_eq!(calls.borrow().read, 190);
    assert_eq!(work.snapshot().statistics.native_reads, 190);
    assert_eq!(work.snapshot().owner_reads_final, Some(190));
    assert!(
        work.step(&mut owner, &mut raw, 2_000_000, false)
            .unwrap()
            .finished
    );
    assert_eq!(work.snapshot().stop_reason, Some(ObserveStop::Deadline));
    assert_eq!(calls.borrow().read, 190);
    assert_eq!(calls.borrow().stop, 0);
    owner.close();
    drop(owner);
    assert_eq!(calls.borrow().stop, 1);
    assert_eq!(calls.borrow().close, 1);
}

#[test]
fn continuous_late_poll_and_stop_do_not_burst_or_reset_budget() {
    let (mut owner, calls) = make_owner(vec![], 256, 3_000_000, false);
    let mut work = ObserveWorkload::new(ObserveConfig::CONTINUOUS, 0).unwrap();
    let progress = work
        .step(&mut owner, &mut [0; 32], 1_000_000, false)
        .unwrap();
    assert_eq!(progress.wake_at_us, Some(1_010_000));
    assert_eq!(calls.borrow().read, 1);
    for _ in 0..32 {
        assert!(
            !work
                .step(&mut owner, &mut [0; 32], 1_009_999, false)
                .unwrap()
                .progressed
        );
    }
    assert_eq!(calls.borrow().read, 1);
    work.step(&mut owner, &mut [0; 32], 1_010_000, false)
        .unwrap();
    assert_eq!(calls.borrow().read, 2);
    assert!(
        work.step(&mut owner, &mut [], 1_010_000, true)
            .unwrap()
            .finished
    );
    assert_eq!(work.snapshot().stop_reason, Some(ObserveStop::Cancelled));
    assert_eq!(work.snapshot().statistics.native_reads, 2);
    assert_eq!(owner.reads(), 2);
    assert_eq!(calls.borrow().stop, 0);
    owner.close();
    assert_eq!(calls.borrow().stop, 1);
}

#[test]
fn continuous_custom_tighter_read_limit_still_stops_once_and_invalid_cadences_fail() {
    let (mut owner, calls) = make_owner(vec![], 256, 3_000_000, false);
    let mut work = ObserveWorkload::new(
        ObserveConfig {
            max_reads: 3,
            ..ObserveConfig::CONTINUOUS
        },
        0,
    )
    .unwrap();
    for now in [100_000, 110_000, 120_000] {
        work.step(&mut owner, &mut [0; 32], now, false).unwrap();
    }
    assert_eq!(work.snapshot().stop_reason, Some(ObserveStop::ReadBudget));
    assert_eq!(calls.borrow().read, 3);
    assert!(
        work.step(&mut owner, &mut [0; 32], 130_000, false)
            .unwrap()
            .finished
    );
    assert_eq!(calls.borrow().read, 3);
    for interval_us in [0, 1, 9999, 10001, 99999, 100001] {
        assert!(matches!(
            ObserveWorkload::new(
                ObserveConfig {
                    interval_us,
                    ..ObserveConfig::CONTINUOUS
                },
                0
            ),
            Err(ObserveError::InvalidConfig)
        ));
    }
    assert!(matches!(
        ObserveWorkload::new(
            ObserveConfig {
                max_reads: 257,
                ..ObserveConfig::CONTINUOUS
            },
            0
        ),
        Err(ObserveError::InvalidConfig)
    ));
}
