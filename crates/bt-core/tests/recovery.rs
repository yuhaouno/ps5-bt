// SPDX-License-Identifier: GPL-3.0-or-later
use bt_core::recovery::{
    Action, AttemptOutcome, AttemptTicket, Exhaustion, Failure, Policy, Recovery, RecoveryError,
    Requirement, SafetyTicket,
};

fn policy() -> Policy {
    Policy {
        max_attempts: 3,
        episode_timeout_us: 1_000,
        initial_backoff_us: 10,
        max_backoff_us: 25,
        stable_success_us: 100,
    }
}

fn safety(controller: &mut Recovery, now: u64, requirement: Requirement) -> SafetyTicket {
    match controller.poll(now).unwrap() {
        Action::AwaitSafety {
            ticket,
            requirement: actual,
        } => {
            assert_eq!(actual, requirement);
            ticket
        }
        other => panic!("expected external safety work, got {other:?}"),
    }
}

fn attempt(controller: &mut Recovery, now: u64) -> AttemptTicket {
    match controller.poll(now).unwrap() {
        Action::Attempt(ticket) => ticket,
        other => panic!("expected attempt, got {other:?}"),
    }
}

fn prepare(controller: &mut Recovery, now: u64, requirement: Requirement) {
    let ticket = safety(controller, now, requirement);
    controller.confirm_safety(now, ticket, requirement).unwrap();
}

#[test]
fn retries_spend_one_episode_budget_and_need_cleanup_each_time() {
    let mut controller = Recovery::new(policy()).unwrap();
    controller.trigger(0, Failure::Retryable).unwrap();
    prepare(&mut controller, 0, Requirement::Cleanup);
    let first = attempt(&mut controller, 0);
    // Observing the same offer does not consume attempts or execute anything.
    for _ in 0..5 {
        assert_eq!(attempt(&mut controller, 0), first);
    }
    assert_eq!(controller.snapshot().attempts, 0);
    controller.accept_attempt(0, first).unwrap();
    assert_eq!(
        controller.accept_attempt(0, first),
        Err(RecoveryError::StaleTicket)
    );
    controller
        .finish_attempt(1, first, AttemptOutcome::Failed(Failure::Retryable))
        .unwrap();
    let cleanup = safety(&mut controller, 1, Requirement::Cleanup);
    controller.trigger(2, Failure::Retryable).unwrap();
    assert_eq!(safety(&mut controller, 2, Requirement::Cleanup), cleanup);
    assert_eq!(controller.snapshot().episode_deadline_us, Some(1_000));
    controller
        .confirm_safety(2, cleanup, Requirement::Cleanup)
        .unwrap();
    assert_eq!(controller.poll(10), Ok(Action::WaitUntil(11)));
    let second = attempt(&mut controller, 11);
    controller.accept_attempt(11, second).unwrap();
    controller
        .finish_attempt(12, second, AttemptOutcome::Failed(Failure::Retryable))
        .unwrap();
    prepare(&mut controller, 12, Requirement::Cleanup);
    assert_eq!(controller.poll(31), Ok(Action::WaitUntil(32)));
    let third = attempt(&mut controller, 32);
    controller.accept_attempt(32, third).unwrap();
    controller
        .finish_attempt(33, third, AttemptOutcome::Failed(Failure::Retryable))
        .unwrap();
    assert_eq!(
        controller.poll(33),
        Ok(Action::Exhausted(Exhaustion::Attempts))
    );
    controller.trigger(34, Failure::Retryable).unwrap();
    assert_eq!(
        controller.poll(500),
        Ok(Action::Exhausted(Exhaustion::Attempts))
    );
    assert_eq!(controller.snapshot().attempts, 3);
    assert_eq!(
        controller.snapshot().outstanding_requirement,
        Some(Requirement::Cleanup)
    );
}

#[test]
fn integrity_loss_cannot_be_downgraded_by_another_fault_or_user_retry() {
    let mut controller = Recovery::new(Policy {
        max_attempts: 1,
        ..policy()
    })
    .unwrap();
    controller.trigger(0, Failure::IntegrityLost).unwrap();
    let ticket = safety(&mut controller, 0, Requirement::Reconciliation);
    controller.trigger(1, Failure::Retryable).unwrap();
    assert_eq!(
        controller.confirm_safety(1, ticket, Requirement::Cleanup),
        Err(RecoveryError::InvalidState)
    );
    assert_eq!(
        safety(&mut controller, 1, Requirement::Reconciliation),
        ticket
    );
    controller
        .confirm_safety(1, ticket, Requirement::Reconciliation)
        .unwrap();
    let first = attempt(&mut controller, 1);
    controller.accept_attempt(1, first).unwrap();
    controller
        .finish_attempt(2, first, AttemptOutcome::Failed(Failure::IntegrityLost))
        .unwrap();
    controller.retry_exhausted(3).unwrap();
    assert_eq!(controller.snapshot().attempts, 0);
    let new_ticket = safety(&mut controller, 3, Requirement::Reconciliation);
    assert_ne!(ticket, new_ticket);
    assert_eq!(
        controller.confirm_safety(3, ticket, Requirement::Reconciliation),
        Err(RecoveryError::StaleTicket)
    );
    assert_eq!(
        controller.confirm_safety(3, new_ticket, Requirement::Cleanup),
        Err(RecoveryError::InvalidState)
    );
}

#[test]
fn only_uninterrupted_stable_success_automatically_resets_budget() {
    let mut controller = Recovery::new(policy()).unwrap();
    controller.trigger(0, Failure::Retryable).unwrap();
    prepare(&mut controller, 0, Requirement::Cleanup);
    let first = attempt(&mut controller, 0);
    controller.accept_attempt(0, first).unwrap();
    controller
        .finish_attempt(5, first, AttemptOutcome::Success)
        .unwrap();
    assert_eq!(controller.poll(104), Ok(Action::StabilizingUntil(105)));
    controller.trigger(104, Failure::Retryable).unwrap();
    assert_eq!(controller.snapshot().attempts, 1);
    assert_eq!(controller.snapshot().episode_deadline_us, Some(1_000));
    prepare(&mut controller, 104, Requirement::Cleanup);
    let second = attempt(&mut controller, 114);
    controller.accept_attempt(114, second).unwrap();
    controller
        .finish_attempt(115, second, AttemptOutcome::Success)
        .unwrap();
    assert_eq!(controller.poll(214), Ok(Action::StabilizingUntil(215)));
    assert_eq!(controller.poll(215), Ok(Action::Healthy));
    assert_eq!(controller.snapshot().attempts, 0);
    assert_eq!(controller.snapshot().episode_deadline_us, None);
    controller.trigger(215, Failure::Retryable).unwrap();
    assert_eq!(controller.snapshot().episode_deadline_us, Some(1_215));
}

#[test]
fn episode_deadline_invalidates_hung_attempt_without_assuming_cleanup() {
    let mut controller = Recovery::new(policy()).unwrap();
    controller.trigger(0, Failure::Retryable).unwrap();
    prepare(&mut controller, 0, Requirement::Cleanup);
    let ticket = attempt(&mut controller, 0);
    controller.accept_attempt(0, ticket).unwrap();
    assert_eq!(controller.poll(999), Ok(Action::InFlight(ticket)));
    assert_eq!(
        controller.poll(1_000),
        Ok(Action::Exhausted(Exhaustion::Deadline))
    );
    assert_eq!(
        controller.finish_attempt(1_000, ticket, AttemptOutcome::Success),
        Err(RecoveryError::StaleTicket)
    );
    assert_eq!(
        controller.snapshot().outstanding_requirement,
        Some(Requirement::Cleanup)
    );
    controller.retry_exhausted(1_001).unwrap();
    prepare(&mut controller, 1_001, Requirement::Cleanup);
    let next = attempt(&mut controller, 1_001);
    assert_ne!(next, ticket);
    controller.accept_attempt(1_001, next).unwrap();
    assert_eq!(
        controller.finish_attempt(1_001, ticket, AttemptOutcome::Success),
        Err(RecoveryError::StaleTicket)
    );
    assert_eq!(controller.poll(1_001), Ok(Action::InFlight(next)));
}

#[test]
fn expired_safety_and_waiting_offers_cannot_start_operations() {
    let mut controller = Recovery::new(policy()).unwrap();
    controller.trigger(0, Failure::IntegrityLost).unwrap();
    let ticket = safety(&mut controller, 0, Requirement::Reconciliation);
    assert_eq!(
        controller.confirm_safety(1_000, ticket, Requirement::Reconciliation),
        Err(RecoveryError::StaleTicket)
    );
    controller.retry_exhausted(1_000).unwrap();
    prepare(&mut controller, 1_000, Requirement::Reconciliation);
    let ticket = attempt(&mut controller, 1_000);
    assert_eq!(
        controller.accept_attempt(2_000, ticket),
        Err(RecoveryError::StaleTicket)
    );
    assert_eq!(controller.snapshot().attempts, 0);
}

#[test]
fn old_results_cannot_complete_unaccepted_or_superseded_attempts() {
    let mut controller = Recovery::new(policy()).unwrap();
    controller.trigger(0, Failure::Retryable).unwrap();
    prepare(&mut controller, 0, Requirement::Cleanup);
    let ticket = attempt(&mut controller, 0);
    assert_eq!(
        controller.finish_attempt(0, ticket, AttemptOutcome::Success),
        Err(RecoveryError::StaleTicket)
    );
    controller.accept_attempt(0, ticket).unwrap();
    controller.trigger(1, Failure::IntegrityLost).unwrap();
    assert_eq!(
        controller.finish_attempt(1, ticket, AttemptOutcome::Success),
        Err(RecoveryError::StaleTicket)
    );
    assert!(matches!(
        controller.poll(1),
        Ok(Action::AwaitSafety {
            requirement: Requirement::Reconciliation,
            ..
        })
    ));
    assert_eq!(controller.snapshot().attempts, 1);
}

#[test]
fn capped_backoff_and_delayed_cleanup_do_not_extend_episode_deadline() {
    let mut controller = Recovery::new(Policy {
        max_attempts: 5,
        ..policy()
    })
    .unwrap();
    controller.trigger(0, Failure::Retryable).unwrap();
    let mut now = 0;
    prepare(&mut controller, now, Requirement::Cleanup);
    for expected_delay in [10, 20, 25, 25] {
        let ticket = attempt(&mut controller, now);
        controller.accept_attempt(now, ticket).unwrap();
        controller
            .finish_attempt(now, ticket, AttemptOutcome::Failed(Failure::Retryable))
            .unwrap();
        prepare(&mut controller, now, Requirement::Cleanup);
        assert_eq!(
            controller.poll(now),
            Ok(Action::WaitUntil(now + expected_delay))
        );
        now += expected_delay;
    }
    assert_eq!(controller.snapshot().episode_deadline_us, Some(1_000));
}

#[test]
fn stop_invalidates_tokens_and_never_claims_resource_cleanup() {
    let mut controller = Recovery::new(policy()).unwrap();
    controller.trigger(20, Failure::IntegrityLost).unwrap();
    let ticket = safety(&mut controller, 20, Requirement::Reconciliation);
    controller.stop();
    controller.stop();
    assert_eq!(controller.poll(0), Ok(Action::Stopped));
    assert_eq!(
        controller.confirm_safety(20, ticket, Requirement::Reconciliation),
        Err(RecoveryError::StaleTicket)
    );
    assert_eq!(
        controller.trigger(20, Failure::Retryable),
        Err(RecoveryError::InvalidState)
    );
    assert_eq!(
        controller.retry_exhausted(20),
        Err(RecoveryError::InvalidState)
    );
    assert_eq!(
        controller.snapshot().outstanding_requirement,
        Some(Requirement::Reconciliation)
    );
}

#[test]
fn bad_clock_and_deadline_overflow_fail_closed_but_stop_is_available() {
    let mut controller = Recovery::new(policy()).unwrap();
    controller.trigger(20, Failure::Retryable).unwrap();
    assert_eq!(controller.poll(19), Err(RecoveryError::ClockRegressed));
    assert_eq!(controller.poll(21), Err(RecoveryError::ClockRegressed));
    assert_eq!(
        controller.snapshot().action,
        Action::Failed(RecoveryError::ClockRegressed)
    );
    controller.stop();
    assert_eq!(controller.poll(21), Ok(Action::Stopped));
    let mut controller = Recovery::new(policy()).unwrap();
    assert_eq!(
        controller.trigger(u64::MAX, Failure::Retryable),
        Err(RecoveryError::Overflow)
    );
    assert_eq!(
        controller.snapshot().action,
        Action::Failed(RecoveryError::Overflow)
    );
    controller.stop();
    assert_eq!(controller.snapshot().action, Action::Stopped);
}

#[test]
fn invalid_policy_and_success_deadline_overflow_are_explicit() {
    for bad in [
        Policy {
            max_attempts: 0,
            ..policy()
        },
        Policy {
            episode_timeout_us: 0,
            ..policy()
        },
        Policy {
            initial_backoff_us: 0,
            ..policy()
        },
        Policy {
            max_backoff_us: 1,
            ..policy()
        },
        Policy {
            stable_success_us: 0,
            ..policy()
        },
    ] {
        assert!(matches!(
            Recovery::new(bad),
            Err(RecoveryError::InvalidPolicy)
        ));
    }
    let mut controller = Recovery::new(Policy {
        episode_timeout_us: 20,
        ..policy()
    })
    .unwrap();
    let now = u64::MAX - 30;
    controller.trigger(now, Failure::Retryable).unwrap();
    prepare(&mut controller, now, Requirement::Cleanup);
    let ticket = attempt(&mut controller, now);
    controller.accept_attempt(now, ticket).unwrap();
    assert_eq!(
        controller.finish_attempt(now, ticket, AttemptOutcome::Success),
        Err(RecoveryError::Overflow)
    );
    assert_eq!(
        controller.snapshot().outstanding_requirement,
        Some(Requirement::Cleanup)
    );
}

#[test]
fn backoff_wait_wakes_at_episode_deadline_and_late_cleanup_never_extends_it() {
    let mut controller = Recovery::new(Policy {
        episode_timeout_us: 15,
        ..policy()
    })
    .unwrap();
    controller.trigger(0, Failure::Retryable).unwrap();
    prepare(&mut controller, 0, Requirement::Cleanup);
    let ticket = attempt(&mut controller, 0);
    controller.accept_attempt(0, ticket).unwrap();
    controller
        .finish_attempt(10, ticket, AttemptOutcome::Failed(Failure::Retryable))
        .unwrap();
    prepare(&mut controller, 14, Requirement::Cleanup);
    // Backoff ends at 20, but the owner must wake at the earlier episode deadline.
    assert_eq!(controller.poll(14), Ok(Action::WaitUntil(15)));
    assert_eq!(
        controller.poll(15),
        Ok(Action::Exhausted(Exhaustion::Deadline))
    );
    assert_eq!(controller.snapshot().attempts, 1);

    controller.retry_exhausted(20).unwrap();
    prepare(&mut controller, 20, Requirement::Cleanup);
    let ticket = attempt(&mut controller, 20);
    controller.accept_attempt(20, ticket).unwrap();
    controller
        .finish_attempt(21, ticket, AttemptOutcome::Failed(Failure::Retryable))
        .unwrap();
    // Cleanup takes longer than the backoff, so no extra delay is added to it.
    prepare(&mut controller, 32, Requirement::Cleanup);
    assert!(matches!(controller.poll(32), Ok(Action::Attempt(_))));
    assert_eq!(controller.snapshot().episode_deadline_us, Some(35));
}

#[test]
fn integrity_escalation_requires_stronger_confirmation_for_the_same_request() {
    let mut controller = Recovery::new(policy()).unwrap();
    controller.trigger(0, Failure::Retryable).unwrap();
    let ticket = safety(&mut controller, 0, Requirement::Cleanup);
    controller.trigger(1, Failure::IntegrityLost).unwrap();
    assert_eq!(
        safety(&mut controller, 1, Requirement::Reconciliation),
        ticket
    );
    assert_eq!(
        controller.confirm_safety(1, ticket, Requirement::Cleanup),
        Err(RecoveryError::InvalidState)
    );
    controller
        .confirm_safety(1, ticket, Requirement::Reconciliation)
        .unwrap();
    let offer = attempt(&mut controller, 1);
    controller.trigger(2, Failure::IntegrityLost).unwrap();
    assert_eq!(
        controller.accept_attempt(2, offer),
        Err(RecoveryError::StaleTicket)
    );
    assert_eq!(controller.snapshot().attempts, 0);
    assert!(matches!(
        controller.poll(2),
        Ok(Action::AwaitSafety {
            requirement: Requirement::Reconciliation,
            ..
        })
    ));
}

#[test]
fn a_success_before_deadline_can_be_observed_stable_after_it() {
    let mut controller = Recovery::new(Policy {
        episode_timeout_us: 10,
        ..policy()
    })
    .unwrap();
    controller.trigger(0, Failure::Retryable).unwrap();
    prepare(&mut controller, 0, Requirement::Cleanup);
    let ticket = attempt(&mut controller, 0);
    controller.accept_attempt(0, ticket).unwrap();
    controller
        .finish_attempt(9, ticket, AttemptOutcome::Success)
        .unwrap();
    assert_eq!(controller.poll(10), Ok(Action::StabilizingUntil(109)));
    controller.trigger(11, Failure::Retryable).unwrap();
    assert_eq!(
        controller.poll(11),
        Ok(Action::Exhausted(Exhaustion::Deadline))
    );
}
