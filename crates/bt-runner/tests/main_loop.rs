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
    latest: Option<u64>,
    jump_on: Option<(Lane, u64)>,
    idle: bool,
    work_times: Vec<(Lane, u64)>,
    stop_times: Vec<u64>,
    poll_times: Vec<u64>,
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
            latest: None,
            jump_on: None,
            idle: false,
            work_times: Vec::new(),
            stop_times: Vec::new(),
            poll_times: Vec::new(),
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
    fn quantum(&mut self, lane: Lane, now: u64) -> Result<Quantum, i32> {
        self.calls.borrow_mut().push(Call::Work(lane));
        self.work_times.push((lane, now));
        if self.jump_on.is_some_and(|(at, _)| at == lane) {
            self.latest = self.jump_on.take().map(|(_, now)| now);
        }
        if self.stop_at == Some(lane) {
            self.stop = true;
        }
        if self.fail_work == Some(lane) {
            return Err(-11);
        }
        Ok(Quantum {
            progressed: !self.idle,
            wake_at_us: Some(9999),
            continuity: Continuity {
                capture_empty: if self.saturation { u64::MAX } else { 1 },
                queue_frames: Some(42),
                ..Continuity::default()
            },
        })
    }
    fn request_stop(&mut self, now: u64) -> Result<(), i32> {
        self.calls.borrow_mut().push(Call::Stop);
        self.stop_times.push(now);
        Ok(())
    }
    fn poll_stop(&mut self, now: u64) -> Result<bool, i32> {
        self.calls.borrow_mut().push(Call::Poll);
        self.poll_times.push(now);
        Ok(self.drained)
    }
    fn abort_stop(&mut self) {
        self.calls.borrow_mut().push(Call::Abort);
    }
    fn latest_observed_us(&self) -> Option<u64> {
        self.latest
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

fn started_with_time(r: Resources, deadline: Option<u64>) -> MainLoop<Resources> {
    let mut cfg = config();
    cfg.deadline_us = deadline;
    cfg.stop_timeout_us = 100_000;
    let mut owner = MainLoop::new(r, cfg, 0).unwrap();
    for _ in 0..5 {
        owner.step(0);
    }
    owner
}
#[test]
fn stop_after_fresh_intraquantum_observation_receives_full_drain_budget() {
    for failed in [false, true] {
        let mut r = Resources::new();
        r.jump_on = Some((Lane::Encode, 2_000_000));
        if failed {
            r.fail_work = Some(Lane::Encode);
        } else {
            r.stop_at = Some(Lane::Encode);
        }
        let mut owner = started_with_time(r, Some(5_000_000));
        let turn = owner.step(1);
        assert_eq!(turn.phase, LoopPhase::Stopping);
        assert_eq!(owner.accepted_us(), 2_000_000);
        assert_eq!(owner.resources().stop_times, vec![2_000_000]);
        assert_eq!(turn.quanta[Lane::Send as usize], 0);
        if failed {
            assert!(matches!(
                owner.report().reason,
                Some(StopReason::Work {
                    lane: Lane::Encode,
                    error: -11
                })
            ));
        } else {
            assert_eq!(owner.report().reason, Some(StopReason::Requested));
        }
        owner.step(2_099_999);
        assert!(!owner.report().forced);
        assert_eq!(owner.resources().poll_times, vec![2_099_999]);
        owner.resources_mut().drained = true;
        owner.step(2_099_999);
        finish(&mut owner, 2_099_999);
        assert!(!owner.report().forced);
        assert_eq!(owner.report().stop_error, None);
    }
}
#[test]
fn global_deadline_is_rechecked_after_work_even_when_lane_returns_idle() {
    for idle in [false, true] {
        let mut r = Resources::new();
        r.jump_on = Some((Lane::Capture, 2_000_000));
        r.idle = idle;
        let mut owner = started_with_time(r, Some(1_000_000));
        let turn = owner.step(1);
        assert_eq!(turn.phase, LoopPhase::Stopping);
        assert_eq!(owner.report().reason, Some(StopReason::Deadline));
        assert_eq!(owner.resources().stop_times, vec![2_000_000]);
        assert_eq!(turn.quanta[Lane::Capture as usize], 1);
        assert_eq!(turn.quanta[Lane::Encode as usize], 0);
        assert_eq!(turn.quanta[Lane::Send as usize], 0);
    }
}
#[test]
fn subsequent_quanta_and_wake_use_accepted_time_stale_cache_is_not_regression() {
    let mut r = Resources::new();
    r.jump_on = Some((Lane::Network, 2_000_000));
    let mut owner = started_with_time(r, Some(5_000_000));
    let turn = owner.step(1);
    assert_eq!(turn.quanta, config().quotas);
    assert_eq!(turn.wake_at_us, Some(2_000_000));
    assert_eq!(owner.resources().work_times[0], (Lane::Network, 1));
    assert!(
        owner.resources().work_times[1..]
            .iter()
            .all(|(_, time)| *time == 2_000_000)
    );
    owner.resources_mut().latest = Some(100);
    owner.step(2_000_000);
    assert_eq!(owner.accepted_us(), 2_000_000);
    assert!(!owner.report().forced);
    owner.step(1_999_999);
    assert_eq!(owner.report().reason, Some(StopReason::ClockRegressed));
    assert!(owner.report().forced);
}
