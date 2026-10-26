// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(feature = "host-clock-wait-tests")]
use core::ffi::{c_char, c_void};
use ps5_platform::clock_wait::*;
unsafe extern "C" {
    fn ps5_scheduler_test_reset();
    fn ps5_scheduler_test_lookup_failure(which: u32, raw: i64, null_symbol: u32);
    fn ps5_scheduler_test_clock(raw: i32, seconds: i64, nanoseconds: i64);
    fn ps5_scheduler_test_wait_status(raw: i32);
    fn ps5_scheduler_test_count(which: u32) -> u32;
    fn ps5_scheduler_test_requested() -> u32;
    fn ps5_scheduler_test_lookup(name: *const c_char, out: *mut *mut c_void) -> i64;
}
#[test]
fn real_native_clock_wait_boundary_is_fallible_cached_and_cancellable() {
    assert_eq!(core::mem::size_of::<ClockDiagnostic>(), 48);
    assert_eq!(deadline_after(10, 20), Ok(30));
    assert_eq!(
        deadline_after(u64::MAX, 1),
        Err(ClockWaitError::DeadlineOverflow)
    );
    // SAFETY: one test exclusively owns the synthetic C export resolver and
    // adapter fixture. Callbacks have the documented signatures and no OS I/O.
    unsafe {
        for which in 1..=2 {
            ps5_scheduler_test_reset();
            ps5_scheduler_test_lookup_failure(which, -0x100000123, 0);
            let mut storage = ClockWaitStorage::<128>::new();
            let result = ClockWait::new(&mut storage, ps5_scheduler_test_lookup);
            let Err(ClockWaitError::Native(report)) = result else {
                panic!("missing resolver failure");
            };
            assert_eq!(report.raw_lookup, -0x100000123);
            assert_eq!(ps5_scheduler_test_count(2), 0);
            assert_eq!(ps5_scheduler_test_count(3), 0);
        }
        ps5_scheduler_test_reset();
        let mut storage = ClockWaitStorage::<128>::new();
        let mut clock = ClockWait::new(&mut storage, ps5_scheduler_test_lookup).unwrap();
        assert_eq!(clock.wait_slice(10000, true), Ok(WaitOutcome::Cancelled));
        assert_eq!(ps5_scheduler_test_count(2), 0);
        assert_eq!(ps5_scheduler_test_count(3), 0);
        ps5_scheduler_test_clock(0, 0, 10000);
        assert_eq!(clock.now_us(), Ok(10));
        assert_eq!(
            clock.wait_slice(10, false),
            Ok(WaitOutcome::DeadlineReached { observed_us: 10 })
        );
        assert_eq!(ps5_scheduler_test_count(3), 0);
        assert_eq!(
            clock.wait_slice(15, false),
            Ok(WaitOutcome::WaitReturned {
                observed_before_us: 10,
                requested_us: 5
            })
        );
        assert_eq!(ps5_scheduler_test_requested(), 5);
        assert_eq!(
            clock.wait_slice(u64::MAX, false),
            Ok(WaitOutcome::WaitReturned {
                observed_before_us: 10,
                requested_us: 1000
            })
        );
        assert_eq!(ps5_scheduler_test_requested(), 1000);
        assert_eq!(ps5_scheduler_test_count(0), 1);
        assert_eq!(ps5_scheduler_test_count(1), 1);
        ps5_scheduler_test_wait_status(-88);
        assert!(matches!(
            clock.wait_slice(10000, false),
            Err(ClockWaitError::Native(ClockDiagnostic {
                raw_status: -88,
                ..
            }))
        ));
        let calls = ps5_scheduler_test_count(2);
        let waits = ps5_scheduler_test_count(3);
        assert!(clock.now_us().is_err());
        assert!(clock.wait_slice(10000, false).is_err());
        assert_eq!(ps5_scheduler_test_count(2), calls);
        assert_eq!(ps5_scheduler_test_count(3), waits);
        clock.close().unwrap();
        clock.close().unwrap();
        assert_eq!(clock.now_us(), Err(ClockWaitError::Closed));
        drop(clock);
        for (raw, sec, ns, expected) in [
            (-77, 0, 0, 0),
            (0, -1, 0, 1),
            (0, 0, 1000000000, 1),
            (0, i64::MAX, 0, 2),
        ] {
            ps5_scheduler_test_reset();
            let mut storage = ClockWaitStorage::<128>::new();
            let mut clock = ClockWait::new(&mut storage, ps5_scheduler_test_lookup).unwrap();
            ps5_scheduler_test_clock(raw, sec, ns);
            let error = clock.now_us().unwrap_err();
            assert!(matches!(
                (expected, error),
                (0, ClockWaitError::Native(_))
                    | (1, ClockWaitError::InvalidTime(_))
                    | (2, ClockWaitError::TimeOverflow(_))
            ));
            clock.close().unwrap();
        }
        ps5_scheduler_test_reset();
        let mut storage = ClockWaitStorage::<128>::new();
        let mut clock = ClockWait::new(&mut storage, ps5_scheduler_test_lookup).unwrap();
        ps5_scheduler_test_clock(0, 1, 100);
        assert_eq!(clock.now_us(), Ok(1000000));
        ps5_scheduler_test_clock(0, 1, 99);
        assert!(matches!(
            clock.now_us(),
            Err(ClockWaitError::ClockRegressed(_))
        ));
    }
}
