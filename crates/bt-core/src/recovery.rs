// SPDX-License-Identifier: GPL-3.0-or-later
//! Caller-driven fault recovery budgets. This module owns no resource or credit.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Policy {
    pub max_attempts: u32,
    pub episode_timeout_us: u64,
    pub initial_backoff_us: u64,
    pub max_backoff_us: u64,
    pub stable_success_us: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryError {
    InvalidPolicy,
    ClockRegressed,
    Overflow,
    StaleTicket,
    InvalidState,
    TooEarly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    /// A recoverable failure with known ownership; owned resources still need cleanup.
    Retryable,
    /// Lost events, unknown ownership or uncertain credits. Cleanup alone is insufficient.
    IntegrityLost,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Requirement {
    Cleanup,
    /// Confirm both cleanup and externally established ownership/credit reconciliation.
    Reconciliation,
}

/// Opaque identity scoped to this controller instance; do not exchange controllers' tickets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SafetyTicket(u64);

/// Opaque identity for an offered/accepted attempt, scoped to this controller instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AttemptTicket(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttemptOutcome {
    Success,
    Failed(Failure),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exhaustion {
    Attempts,
    Deadline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Healthy,
    AwaitSafety {
        ticket: SafetyTicket,
        requirement: Requirement,
    },
    WaitUntil(u64),
    /// An offer, not permission to reopen automatically. Accept once before executing.
    Attempt(AttemptTicket),
    InFlight(AttemptTicket),
    /// Success is provisional until this deadline without another reported fault.
    StabilizingUntil(u64),
    Exhausted(Exhaustion),
    Stopped,
    Failed(RecoveryError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub action: Action,
    pub attempts: u32,
    pub episode_deadline_us: Option<u64>,
    pub last_failure: Option<Failure>,
    /// Preserved even when exhausted/stopped: ending a budget is not reconciliation.
    pub outstanding_requirement: Option<Requirement>,
}

#[derive(Clone, Copy, Debug)]
enum State {
    Healthy,
    Safety(SafetyTicket),
    Waiting { ticket: AttemptTicket, due: u64 },
    InFlight(AttemptTicket),
    Stabilizing(u64),
    Exhausted(Exhaustion),
    Stopped,
    Failed(RecoveryError),
}

/// Bounded per-fault policy, with explicit monotonic microseconds and no sleeping.
/// The caller remains responsible for finite individual operations and exactly-once cleanup.
/// A success report attests a working recovered resource, not merely a successful open call.
pub struct Recovery {
    policy: Policy,
    state: State,
    last_now: Option<u64>,
    next_identity: u64,
    attempts: u32,
    deadline: Option<u64>,
    retry_due: u64,
    last_failure: Option<Failure>,
    requirement: Option<Requirement>,
}

impl Recovery {
    pub fn new(policy: Policy) -> Result<Self, RecoveryError> {
        if policy.max_attempts == 0
            || policy.episode_timeout_us == 0
            || policy.initial_backoff_us == 0
            || policy.max_backoff_us < policy.initial_backoff_us
            || policy.stable_success_us == 0
        {
            return Err(RecoveryError::InvalidPolicy);
        }
        Ok(Self {
            policy,
            state: State::Healthy,
            last_now: None,
            next_identity: 0,
            attempts: 0,
            deadline: None,
            retry_due: 0,
            last_failure: None,
            requirement: None,
        })
    }

    pub fn snapshot(&self) -> Snapshot {
        let action = match self.state {
            State::Healthy => Action::Healthy,
            State::Safety(ticket) => Action::AwaitSafety {
                ticket,
                requirement: self.requirement.unwrap_or(Requirement::Cleanup),
            },
            State::Waiting { ticket, due } => {
                if self.last_now.is_some_and(|now| now >= due) {
                    Action::Attempt(ticket)
                } else {
                    Action::WaitUntil(self.deadline.map_or(due, |deadline| due.min(deadline)))
                }
            }
            State::InFlight(ticket) => Action::InFlight(ticket),
            State::Stabilizing(until) => Action::StabilizingUntil(until),
            State::Exhausted(reason) => Action::Exhausted(reason),
            State::Stopped => Action::Stopped,
            State::Failed(error) => Action::Failed(error),
        };
        Snapshot {
            action,
            attempts: self.attempts,
            episode_deadline_us: self.deadline,
            last_failure: self.last_failure,
            outstanding_requirement: self.requirement,
        }
    }

    fn fail<T>(&mut self, error: RecoveryError) -> Result<T, RecoveryError> {
        self.state = State::Failed(error);
        Err(error)
    }

    fn time(&mut self, now_us: u64) -> Result<(), RecoveryError> {
        if matches!(self.state, State::Stopped) {
            return Ok(());
        }
        if let State::Failed(error) = self.state {
            return Err(error);
        }
        if self.last_now.is_some_and(|last| now_us < last) {
            return self.fail(RecoveryError::ClockRegressed);
        }
        self.last_now = Some(now_us);
        // Stable success is the sole automatic budget reset. The episode timeout
        // bounds attempts, not the observation of an already successful recovery.
        if let State::Stabilizing(until) = self.state {
            if now_us >= until {
                self.state = State::Healthy;
                self.attempts = 0;
                self.deadline = None;
                self.last_failure = None;
            }
        } else if matches!(
            self.state,
            State::Safety(_) | State::Waiting { .. } | State::InFlight(_)
        ) && self.deadline.is_some_and(|deadline| now_us >= deadline)
        {
            self.state = State::Exhausted(Exhaustion::Deadline);
        }
        Ok(())
    }

    fn identity(&mut self) -> Result<u64, RecoveryError> {
        match self.next_identity.checked_add(1) {
            Some(identity) => {
                self.next_identity = identity;
                Ok(identity)
            }
            None => self.fail(RecoveryError::Overflow),
        }
    }

    fn add_time(&mut self, now: u64, duration: u64) -> Result<u64, RecoveryError> {
        match now.checked_add(duration) {
            Some(value) => Ok(value),
            None => self.fail(RecoveryError::Overflow),
        }
    }

    fn note_failure(&mut self, failure: Failure) {
        self.last_failure = Some(failure);
        self.requirement = Some(match (self.requirement, failure) {
            (Some(Requirement::Reconciliation), _) | (_, Failure::IntegrityLost) => {
                Requirement::Reconciliation
            }
            _ => Requirement::Cleanup,
        });
    }

    fn require_safety(&mut self, now: u64) -> Result<(), RecoveryError> {
        if self.attempts >= self.policy.max_attempts {
            self.state = State::Exhausted(Exhaustion::Attempts);
            return Ok(());
        }
        // First attempt is immediate after safety confirmation; subsequent ones
        // use capped exponential backoff from the latest actual attempt failure.
        let mut delay = 0;
        if self.attempts > 0 {
            delay = self.policy.initial_backoff_us;
            for _ in 1..self.attempts {
                if delay >= self.policy.max_backoff_us - delay {
                    delay = self.policy.max_backoff_us;
                    break;
                }
                delay *= 2;
            }
        }
        self.retry_due = self.add_time(now, delay)?;
        let identity = self.identity()?;
        self.state = State::Safety(SafetyTicket(identity));
        Ok(())
    }

    /// Report an external fault. Repeated reports retain the episode deadline,
    /// attempts and pending safety ticket; they can escalate safety, never reduce it.
    /// A fault during an accepted attempt invalidates its eventual result ticket.
    pub fn trigger(&mut self, now_us: u64, failure: Failure) -> Result<(), RecoveryError> {
        self.time(now_us)?;
        if matches!(self.state, State::Stopped) {
            return Err(RecoveryError::InvalidState);
        }
        self.note_failure(failure);
        match self.state {
            State::Healthy => {
                self.deadline = Some(self.add_time(now_us, self.policy.episode_timeout_us)?);
                self.require_safety(now_us)
            }
            State::Safety(_) | State::Exhausted(_) => Ok(()),
            State::Waiting { .. } | State::InFlight(_) | State::Stabilizing(_) => {
                // A repeated fault cannot evade an expired episode deadline.
                if self.deadline.is_some_and(|deadline| now_us >= deadline) {
                    self.state = State::Exhausted(Exhaustion::Deadline);
                    Ok(())
                } else {
                    self.require_safety(now_us)
                }
            }
            State::Stopped | State::Failed(_) => Err(RecoveryError::InvalidState),
        }
    }

    /// Refresh observable timers. Never executes an operation or restores credit.
    pub fn poll(&mut self, now_us: u64) -> Result<Action, RecoveryError> {
        self.time(now_us)?;
        Ok(self.snapshot().action)
    }

    /// Attest the exact requested safety work has actually completed externally.
    /// `Reconciliation` includes cleanup and a verified ownership/credit mechanism;
    /// elapsed time, ordinary stop or a local reset is not sufficient evidence.
    pub fn confirm_safety(
        &mut self,
        now_us: u64,
        ticket: SafetyTicket,
        confirmation: Requirement,
    ) -> Result<(), RecoveryError> {
        self.time(now_us)?;
        if !matches!(self.state, State::Safety(current) if current == ticket) {
            return Err(RecoveryError::StaleTicket);
        }
        if self.requirement == Some(Requirement::Reconciliation)
            && confirmation != Requirement::Reconciliation
        {
            return Err(RecoveryError::InvalidState);
        }
        let identity = self.identity()?;
        self.requirement = None;
        self.state = State::Waiting {
            ticket: AttemptTicket(identity),
            due: self.retry_due,
        };
        Ok(())
    }

    /// Accept exactly once before executing the operation; polling does not spend a budget.
    pub fn accept_attempt(
        &mut self,
        now_us: u64,
        ticket: AttemptTicket,
    ) -> Result<(), RecoveryError> {
        self.time(now_us)?;
        let State::Waiting {
            ticket: current,
            due,
        } = self.state
        else {
            return Err(RecoveryError::StaleTicket);
        };
        if ticket != current {
            return Err(RecoveryError::StaleTicket);
        }
        if now_us < due {
            return Err(RecoveryError::TooEarly);
        }
        self.attempts += 1; // Bounded by the validated max_attempts and require_safety.
        // Until a result is known, an accepted operation might own resources.
        self.requirement = Some(Requirement::Cleanup);
        self.state = State::InFlight(ticket);
        Ok(())
    }

    /// Report only the matching accepted operation. A late or duplicate result
    /// cannot revive an expired/cancelled attempt or reset another attempt's budget.
    pub fn finish_attempt(
        &mut self,
        now_us: u64,
        ticket: AttemptTicket,
        outcome: AttemptOutcome,
    ) -> Result<(), RecoveryError> {
        self.time(now_us)?;
        if !matches!(self.state, State::InFlight(current) if current == ticket) {
            return Err(RecoveryError::StaleTicket);
        }
        match outcome {
            AttemptOutcome::Success => {
                let until = self.add_time(now_us, self.policy.stable_success_us)?;
                self.requirement = None;
                self.state = State::Stabilizing(until);
                Ok(())
            }
            AttemptOutcome::Failed(failure) => {
                self.note_failure(failure);
                self.require_safety(now_us)
            }
        }
    }

    /// A deliberate new user retry/new documented external trigger after exhaustion.
    /// Never call this from a timer, failure callback or automatic polling loop.
    /// It starts a new budget but preserves outstanding safety requirements.
    pub fn retry_exhausted(&mut self, now_us: u64) -> Result<(), RecoveryError> {
        self.time(now_us)?;
        if !matches!(self.state, State::Exhausted(_)) {
            return Err(RecoveryError::InvalidState);
        }
        self.deadline = Some(self.add_time(now_us, self.policy.episode_timeout_us)?);
        self.attempts = 0;
        self.requirement = Some(self.requirement.unwrap_or(Requirement::Cleanup));
        self.require_safety(now_us)
    }

    /// Always available, including after bad clocks/overflow. This cancels policy
    /// only: it neither closes resources nor clears outstanding uncertainty.
    /// This controller remains stopped; a new session requires its own lifecycle.
    pub fn stop(&mut self) {
        self.state = State::Stopped;
    }
}
