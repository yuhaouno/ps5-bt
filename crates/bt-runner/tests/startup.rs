// SPDX-License-Identifier: GPL-3.0-or-later
mod common;
use bt_device::{BondStore, StoreError};
use bt_runner::{
    DriveBudget, DriveFailure, DriveResources, LoopConfig, LoopPhase, LoopResources, MainLoop,
    Quantum, Resource, StoreSlot, drive_bounded,
};
use std::{cell::RefCell, rc::Rc};
#[derive(Default)]
struct Trace {
    acquired: Vec<Resource>,
    closed: Vec<Resource>,
    clocks: u32,
    waits: u32,
    aborts: u32,
    stop_calls: u32,
    stop: bool,
    clock_error: bool,
    wait_error: bool,
    now: u64,
    fail_at: Option<Resource>,
    delayed_stop: bool,
    normal_quanta: u32,
    latest: Option<u64>,
    jump_and_fail_clock: bool,
}
struct Resources(Rc<RefCell<Trace>>);
impl LoopResources for Resources {
    fn acquire(&mut self, r: Resource, _: u64) -> Result<(), i32> {
        let mut t = self.0.borrow_mut();
        if t.fail_at == Some(r) {
            return Err(-81);
        }
        t.acquired.push(r);
        Ok(())
    }
    fn take_stop(&mut self) -> bool {
        self.0.borrow().stop
    }
    fn latest_observed_us(&self) -> Option<u64> {
        self.0.borrow().latest
    }
    fn quantum(&mut self, _: bt_runner::Lane, _: u64) -> Result<Quantum, i32> {
        let mut trace = self.0.borrow_mut();
        trace.normal_quanta += 1;
        if trace.jump_and_fail_clock {
            trace.jump_and_fail_clock = false;
            trace.latest = Some(2_000_000);
            trace.now = 2_000_000;
            trace.clock_error = true;
        }
        Ok(Quantum::default())
    }
    fn request_stop(&mut self, _: u64) -> Result<(), i32> {
        self.0.borrow_mut().stop_calls += 1;
        Ok(())
    }
    fn poll_stop(&mut self, _: u64) -> Result<bool, i32> {
        Ok(!self.0.borrow().delayed_stop)
    }
    fn abort_stop(&mut self) {
        self.0.borrow_mut().aborts += 1;
    }
    fn cleanup(&mut self, r: Resource) -> Result<(), i32> {
        self.0.borrow_mut().closed.push(r);
        Ok(())
    }
}
impl DriveResources for Resources {
    fn now_us(&mut self) -> Result<u64, i32> {
        let mut t = self.0.borrow_mut();
        t.clocks += 1;
        if t.clock_error {
            return Err(-91);
        }
        t.now += 1;
        Ok(t.now)
    }
    fn wait_slice(&mut self, _: u64, cancelled: bool) -> Result<(), i32> {
        assert!(!cancelled);
        let mut t = self.0.borrow_mut();
        t.waits += 1;
        if t.wait_error { Err(-92) } else { Ok(()) }
    }
}
fn config() -> LoopConfig {
    LoopConfig {
        quotas: [1; 6],
        max_wait_us: 1,
        stop_timeout_us: 10,
        deadline_us: Some(20),
    }
}
fn owner(t: Rc<RefCell<Trace>>) -> MainLoop<Resources> {
    MainLoop::new(Resources(t), config(), 0).unwrap()
}
#[test]
fn finite_driver_reaches_deadline_and_unwinds_without_clock_during_cleanup() {
    let trace = Rc::new(RefCell::new(Trace::default()));
    let mut owner = owner(trace.clone());
    let report = drive_bounded(
        &mut owner,
        DriveBudget {
            max_turns: 100,
            max_waits: 100,
        },
        0,
    );
    assert_eq!(report.failure, None);
    assert_eq!(report.outcome.phase, LoopPhase::Finished);
    assert_eq!(trace.borrow().closed.len(), 5);
    assert_eq!(trace.borrow().clocks, report.clock_calls);
    assert!(report.cleanup_turns <= 5);
    assert_eq!(trace.borrow().aborts, 0);
}
#[test]
fn pending_stop_preempts_clock_and_startup_acquisition() {
    let trace = Rc::new(RefCell::new(Trace {
        stop: true,
        clock_error: true,
        ..Trace::default()
    }));
    let mut owner = owner(trace.clone());
    let report = drive_bounded(
        &mut owner,
        DriveBudget {
            max_turns: 20,
            max_waits: 20,
        },
        0,
    );
    assert_eq!(report.failure, None);
    assert_eq!(report.outcome.phase, LoopPhase::Finished);
    assert_eq!(trace.borrow().clocks, 0);
    assert!(trace.borrow().acquired.is_empty());
    assert_eq!(trace.borrow().stop_calls, 1);
}
#[test]
fn clock_wait_and_operation_failures_remain_finite_and_visible() {
    for mode in 0..4 {
        let trace = Rc::new(RefCell::new(Trace {
            clock_error: mode == 0,
            wait_error: mode == 1,
            ..Trace::default()
        }));
        let mut owner = owner(trace.clone());
        let report = drive_bounded(
            &mut owner,
            DriveBudget {
                max_turns: if mode == 2 { 2 } else { 100 },
                max_waits: if mode == 3 { 1 } else { 100 },
            },
            0,
        );
        assert_eq!(
            report.failure,
            Some(match mode {
                0 => DriveFailure::Clock(-91),
                1 => DriveFailure::Wait(-92),
                2 => DriveFailure::TurnBudget,
                _ => DriveFailure::WaitBudget,
            })
        );
        assert_eq!(report.outcome.phase, LoopPhase::Finished);
        assert!(report.outcome.forced);
        assert_eq!(trace.borrow().aborts, 1);
        assert_eq!(trace.borrow().closed.len(), trace.borrow().acquired.len());
    }
}
#[test]
fn partial_startup_failure_never_opens_later_resources() {
    let trace = Rc::new(RefCell::new(Trace {
        fail_at: Some(Resource::Listener),
        ..Trace::default()
    }));
    let mut owner = owner(trace.clone());
    let report = drive_bounded(
        &mut owner,
        DriveBudget {
            max_turns: 100,
            max_waits: 100,
        },
        0,
    );
    assert_eq!(report.outcome.phase, LoopPhase::Finished);
    assert!(matches!(
        report.outcome.reason,
        Some(bt_runner::StopReason::Startup {
            resource: Resource::Listener,
            error: -81
        })
    ));
    assert_eq!(
        trace.borrow().closed,
        vec![Resource::Transport, Resource::Store]
    );
}
#[test]
fn store_slot_rejects_absence_and_preserves_actual_owner_through_lending() {
    let slot = StoreSlot::new();
    let mut lease = slot.lease();
    let mut out = [0; bt_device::BOND_RECORD_BYTES];
    let peer = bt_link::Peer::from_hci_bytes([1, 2, 3, 4, 5, 6]);
    assert_eq!(lease.load(peer, &mut out), Err(StoreError::Unavailable));
    assert!(slot.install(common::Store).is_ok());
    assert_eq!(
        lease.load(peer, &mut out),
        Ok(Some(bt_device::BOND_RECORD_BYTES))
    );
    assert!(slot.install(common::Store).is_err());
    assert!(slot.take().unwrap().is_some());
    assert_eq!(lease.load(peer, &mut out), Err(StoreError::Unavailable));
}

#[test]
fn sticky_stop_advances_only_teardown_until_aggregate_timeout() {
    let trace = Rc::new(RefCell::new(Trace::default()));
    let mut owner = owner(trace.clone());
    for _ in 0..5 {
        owner.step(0);
    }
    trace.borrow_mut().stop = true;
    trace.borrow_mut().delayed_stop = true;
    let report = drive_bounded(
        &mut owner,
        DriveBudget {
            max_turns: 100,
            max_waits: 100,
        },
        0,
    );
    assert_eq!(report.failure, None);
    assert_eq!(report.outcome.stop_error, Some(-2));
    assert!(report.outcome.forced);
    assert_eq!(report.outcome.phase, LoopPhase::Finished);
    assert!(report.turns < 100);
    assert_eq!(trace.borrow().stop_calls, 1);
    assert_eq!(trace.borrow().normal_quanta, 0);
    assert_eq!(trace.borrow().aborts, 1);
    assert_eq!(trace.borrow().closed.len(), 5);
    assert!(trace.borrow().clocks > 0);
    assert!(trace.borrow().waits > 0);
}

#[test]
fn sticky_stop_teardown_clock_or_wait_failure_aborts_and_cleans_once() {
    for clock_error in [true, false] {
        let trace = Rc::new(RefCell::new(Trace::default()));
        let mut owner = owner(trace.clone());
        for _ in 0..5 {
            owner.step(0);
        }
        {
            let mut t = trace.borrow_mut();
            t.stop = true;
            t.delayed_stop = true;
            t.clock_error = clock_error;
            t.wait_error = !clock_error;
        }
        let report = drive_bounded(
            &mut owner,
            DriveBudget {
                max_turns: 100,
                max_waits: 100,
            },
            0,
        );
        assert_eq!(
            report.failure,
            Some(if clock_error {
                DriveFailure::Clock(-91)
            } else {
                DriveFailure::Wait(-92)
            })
        );
        assert_eq!(report.outcome.phase, LoopPhase::Finished);
        assert!(report.outcome.forced);
        assert_eq!(trace.borrow().normal_quanta, 0);
        assert_eq!(trace.borrow().aborts, 1);
        assert_eq!(trace.borrow().closed.len(), 5);
    }
}

#[test]
fn driver_retains_intraquantum_time_for_clock_failure_and_clock_free_cleanup() {
    let trace = Rc::new(RefCell::new(Trace {
        jump_and_fail_clock: true,
        ..Trace::default()
    }));
    let mut cfg = config();
    cfg.deadline_us = Some(5_000_000);
    let mut owner = MainLoop::new(Resources(trace.clone()), cfg, 0).unwrap();
    let report = drive_bounded(
        &mut owner,
        DriveBudget {
            max_turns: 100,
            max_waits: 100,
        },
        0,
    );
    assert_eq!(report.failure, Some(DriveFailure::Clock(-91)));
    assert_eq!(report.last_observed_us, 2_000_000);
    assert_eq!(report.waits, 0); // the returned wake was already due at accepted time
    assert_eq!(owner.accepted_us(), 2_000_000);
    assert_eq!(report.outcome.phase, LoopPhase::Finished);
    assert_eq!(trace.borrow().closed.len(), 5);
    assert_eq!(trace.borrow().aborts, 1);
    assert_eq!(
        report.outcome.reason,
        Some(bt_runner::StopReason::StopFailed(-91))
    );
}
