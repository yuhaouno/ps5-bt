// SPDX-License-Identifier: GPL-3.0-or-later
use bt_runner::{
    Continuity, Lane, LoopConfig, LoopPhase, LoopResources, MainLoop, Quantum, Resource, StopReason,
};
use std::{cell::RefCell, rc::Rc};
#[derive(Clone, Debug, PartialEq, Eq)]
enum Call {
    Acquire(Resource),
    Work(Lane),
    Stop,
    Poll,
    Abort,
    Cleanup(Resource),
}
struct Resources {
    calls: Rc<RefCell<Vec<Call>>>,
    fail_acquire: Option<Resource>,
    fail_work: Option<Lane>,
    fail_cleanup: Option<Resource>,
    stop_at: Option<Lane>,
    stop: bool,
    drained: bool,
    saturation: bool,
}
impl Resources {
    fn new() -> Self {
        Self {
            calls: Rc::default(),
            fail_acquire: None,
            fail_work: None,
            fail_cleanup: None,
            stop_at: None,
            stop: false,
            drained: false,
            saturation: false,
        }
    }
}
impl LoopResources for Resources {
    fn acquire(&mut self, r: Resource, _: u64) -> Result<(), i32> {
        self.calls.borrow_mut().push(Call::Acquire(r));
        if self.fail_acquire == Some(r) {
            Err(-10)
        } else {
            Ok(())
        }
    }
    fn take_stop(&mut self) -> bool {
        std::mem::take(&mut self.stop)
    }
    fn quantum(&mut self, lane: Lane, _: u64) -> Result<Quantum, i32> {
        self.calls.borrow_mut().push(Call::Work(lane));
        if self.stop_at == Some(lane) {
            self.stop = true;
        }
        if self.fail_work == Some(lane) {
            return Err(-11);
        }
        Ok(Quantum {
            progressed: true,
            wake_at_us: Some(9999),
            continuity: Continuity {
                capture_empty: if self.saturation { u64::MAX } else { 1 },
                queue_frames: Some(42),
                ..Continuity::default()
            },
        })
    }
    fn request_stop(&mut self, _: u64) -> Result<(), i32> {
        self.calls.borrow_mut().push(Call::Stop);
        Ok(())
    }
    fn poll_stop(&mut self, _: u64) -> Result<bool, i32> {
        self.calls.borrow_mut().push(Call::Poll);
        Ok(self.drained)
    }
    fn abort_stop(&mut self) {
        self.calls.borrow_mut().push(Call::Abort);
    }
    fn cleanup(&mut self, r: Resource) -> Result<(), i32> {
        self.calls.borrow_mut().push(Call::Cleanup(r));
        if self.fail_cleanup == Some(r) {
            Err(-12)
        } else {
            Ok(())
        }
    }
}
fn config() -> LoopConfig {
    LoopConfig {
        quotas: [2, 3, 2, 2, 2, 2],
        max_wait_us: 10,
        stop_timeout_us: 20,
        deadline_us: Some(1000),
    }
}
fn started(r: Resources) -> MainLoop<Resources> {
    let mut owner = MainLoop::new(r, config(), 0).unwrap();
    for _ in 0..5 {
        owner.step(0);
    }
    assert_eq!(owner.report().phase, LoopPhase::Running);
    owner
}
fn finish(owner: &mut MainLoop<Resources>, now: u64) {
    for _ in 0..8 {
        owner.step(now);
    }
    assert_eq!(owner.report().phase, LoopPhase::Finished);
}
#[test]
fn startup_failure_unwinds_only_acquired_resources_once_in_reverse_order() {
    for failure in [
        Resource::Store,
        Resource::Transport,
        Resource::Listener,
        Resource::Capture,
        Resource::Encoder,
    ] {
        let mut r = Resources::new();
        r.fail_acquire = Some(failure);
        let calls = r.calls.clone();
        let mut owner = MainLoop::new(r, config(), 0).unwrap();
        for _ in 0..12 {
            owner.step(0);
        }
        assert_eq!(
            owner.report().reason,
            Some(StopReason::Startup {
                resource: failure,
                error: -10
            })
        );
        let cleanup: Vec<_> = calls
            .borrow()
            .iter()
            .filter_map(|c| {
                if let Call::Cleanup(r) = c {
                    Some(*r)
                } else {
                    None
                }
            })
            .collect();
        let mut expected = vec![
            Resource::Store,
            Resource::Transport,
            Resource::Listener,
            Resource::Capture,
            Resource::Encoder,
        ];
        expected.truncate(failure as usize);
        expected.reverse();
        assert_eq!(cleanup, expected);
        let count = calls.borrow().len();
        drop(owner);
        assert_eq!(calls.borrow().len(), count);
    }
}
#[test]
fn busy_lanes_have_finite_quotas_and_cannot_starve_later_work() {
    let mut owner = started(Resources::new());
    let turn = owner.step(1);
    assert_eq!(turn.quanta, config().quotas);
    assert_eq!(turn.wake_at_us, Some(1));
    assert_eq!(owner.report().statistics.queue_high_water_frames, 42);
    assert_eq!(owner.report().statistics.continuity.capture_empty, 13);
}
#[test]
fn stop_latch_is_checked_between_every_quantum_before_capture_encode_or_send() {
    let mut r = Resources::new();
    r.stop_at = Some(Lane::Network);
    let calls = r.calls.clone();
    let mut owner = started(r);
    let turn = owner.step(1);
    assert_eq!(turn.quanta, [1, 0, 0, 0, 0, 0]);
    assert_eq!(turn.phase, LoopPhase::Stopping);
    assert_eq!(
        calls.borrow().iter().filter(|c| **c == Call::Stop).count(),
        1
    );
    owner.request_stop();
    owner.step(2);
    assert_eq!(
        calls.borrow().iter().filter(|c| **c == Call::Stop).count(),
        1
    );
}
#[test]
fn stop_uses_original_deadline_then_forces_terminal_cleanup_once() {
    let r = Resources::new();
    let calls = r.calls.clone();
    let mut owner = started(r);
    owner.request_stop();
    owner.step(1);
    owner.step(20);
    assert_eq!(owner.report().phase, LoopPhase::Stopping);
    owner.request_stop();
    owner.step(21);
    assert!(owner.report().forced);
    assert_eq!(owner.report().stop_error, Some(-2));
    finish(&mut owner, 21);
    drop(owner);
    assert_eq!(
        calls.borrow().iter().filter(|c| **c == Call::Abort).count(),
        1
    );
    assert_eq!(
        calls
            .borrow()
            .iter()
            .filter(|c| matches!(c, Call::Cleanup(_)))
            .count(),
        5
    );
}
#[test]
fn graceful_drain_keeps_cleanup_errors_observable_without_retry() {
    let mut r = Resources::new();
    r.drained = true;
    r.fail_cleanup = Some(Resource::Capture);
    let calls = r.calls.clone();
    let mut owner = started(r);
    owner.request_stop();
    owner.step(1);
    finish(&mut owner, 2);
    assert!(!owner.report().forced);
    assert_eq!(
        owner.report().cleanup[Resource::Capture as usize],
        Some(Err(-12))
    );
    drop(owner);
    assert!(!calls.borrow().contains(&Call::Abort));
}
#[test]
fn deadline_and_regressing_or_overflowing_clock_fail_closed() {
    let mut owner = started(Resources::new());
    owner.step(1000);
    assert_eq!(owner.report().reason, Some(StopReason::Deadline));
    let mut owner = started(Resources::new());
    owner.step(100);
    owner.step(99);
    assert_eq!(owner.report().reason, Some(StopReason::ClockRegressed));
    assert!(owner.report().forced);
    let mut cfg = config();
    cfg.deadline_us = None;
    let mut owner = MainLoop::new(Resources::new(), cfg, 0).unwrap();
    for _ in 0..5 {
        owner.step(0);
    }
    owner.step(u64::MAX);
    assert_eq!(owner.report().reason, Some(StopReason::ClockOverflow));
    assert!(owner.report().forced);
}
#[test]
fn work_error_stops_later_lanes_and_drop_unwinds_once() {
    let mut r = Resources::new();
    r.fail_work = Some(Lane::Control);
    let calls = r.calls.clone();
    let mut owner = started(r);
    let turn = owner.step(1);
    assert_eq!(turn.quanta, [2, 0, 0, 0, 0, 0]);
    assert_eq!(
        owner.report().reason,
        Some(StopReason::Work {
            lane: Lane::Control,
            error: -11
        })
    );
    drop(owner);
    assert_eq!(
        calls.borrow().iter().filter(|c| **c == Call::Abort).count(),
        1
    );
    assert_eq!(
        calls
            .borrow()
            .iter()
            .filter(|c| matches!(c, Call::Cleanup(_)))
            .count(),
        5
    );
}
#[test]
fn counter_saturation_is_explicit() {
    let mut r = Resources::new();
    r.saturation = true;
    let mut owner = started(r);
    owner.step(1);
    assert!(owner.report().statistics.counters_saturated);
    assert_eq!(owner.report().statistics.continuity.capture_empty, u64::MAX);
}
#[test]
fn stop_during_startup_does_not_acquire_more_resources() {
    let mut r = Resources::new();
    r.drained = true;
    let calls = r.calls.clone();
    let mut owner = MainLoop::new(r, config(), 0).unwrap();
    owner.step(0);
    owner.request_stop();
    owner.step(1);
    finish(&mut owner, 1);
    assert_eq!(
        calls
            .borrow()
            .iter()
            .filter(|c| matches!(c, Call::Acquire(_)))
            .count(),
        1
    );
    assert_eq!(
        calls
            .borrow()
            .iter()
            .filter(|c| matches!(c, Call::Cleanup(_)))
            .count(),
        1
    );
}
