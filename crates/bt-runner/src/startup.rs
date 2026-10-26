// SPDX-License-Identifier: GPL-3.0-or-later
//! Caller-owned store lending and finite native-clock loop driving.
use crate::{LoopPhase, LoopReport, LoopResources, MainLoop, StopReason};
use bt_device::{BOND_RECORD_BYTES, BondStore, StoreError};
use bt_link::Peer;
use core::cell::RefCell;

/// Stable caller-owned store slot. Its lease contains a reference to this slot,
/// never a pointer into a movable factory/application aggregate. Cleanup takes
/// the actual store only after native command/audio admission has been revoked.
pub struct StoreSlot<S: BondStore> {
    store: RefCell<Option<S>>,
}
impl<S: BondStore> Default for StoreSlot<S> {
    fn default() -> Self {
        Self::new()
    }
}
impl<S: BondStore> StoreSlot<S> {
    pub const fn new() -> Self {
        Self {
            store: RefCell::new(None),
        }
    }
    /// Return the supplied owner unchanged if already installed/borrowed.
    pub fn install(&self, store: S) -> Result<(), S> {
        let Ok(mut slot) = self.store.try_borrow_mut() else {
            return Err(store);
        };
        if slot.is_some() {
            return Err(store);
        }
        *slot = Some(store);
        Ok(())
    }
    pub fn take(&self) -> Result<Option<S>, StoreError> {
        Ok(self
            .store
            .try_borrow_mut()
            .map_err(|_| StoreError::Unavailable)?
            .take())
    }
    pub fn lease(&self) -> StoreLease<'_, S> {
        StoreLease { slot: self }
    }
    pub fn is_installed(&self) -> bool {
        self.store.try_borrow().is_ok_and(|store| store.is_some())
    }
}
/// Exclusive application access is still enforced by the enclosing owner;
/// dynamic reentry rejects with Unavailable instead of panicking or aliasing.
pub struct StoreLease<'a, S: BondStore> {
    slot: &'a StoreSlot<S>,
}
impl<S: BondStore> BondStore for StoreLease<'_, S> {
    fn load(
        &mut self,
        peer: Peer,
        out: &mut [u8; BOND_RECORD_BYTES],
    ) -> Result<Option<usize>, StoreError> {
        let mut slot = self
            .slot
            .store
            .try_borrow_mut()
            .map_err(|_| StoreError::Unavailable)?;
        slot.as_mut()
            .ok_or(StoreError::Unavailable)?
            .load(peer, out)
    }
    fn save_atomic(
        &mut self,
        peer: Peer,
        record: &[u8; BOND_RECORD_BYTES],
    ) -> Result<(), StoreError> {
        let mut slot = self
            .slot
            .store
            .try_borrow_mut()
            .map_err(|_| StoreError::Unavailable)?;
        slot.as_mut()
            .ok_or(StoreError::Unavailable)?
            .save_atomic(peer, record)
    }
    fn delete_atomic(&mut self, peer: Peer) -> Result<(), StoreError> {
        let mut slot = self
            .slot
            .store
            .try_borrow_mut()
            .map_err(|_| StoreError::Unavailable)?;
        slot.as_mut()
            .ok_or(StoreError::Unavailable)?
            .delete_atomic(peer)
    }
}
/// Fallible native port on the SAME owner as LoopResources. wait_slice performs
/// at most one reviewed finite wait and checks cancelled before native calls.
/// Implementations preserve full native diagnostics in their own report; i32 is
/// a redacted correlation code, never a fallback timestamp or readiness proof.
pub trait DriveResources: LoopResources {
    fn now_us(&mut self) -> Result<u64, i32>;
    fn wait_slice(&mut self, deadline_us: u64, cancelled: bool) -> Result<(), i32>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DriveBudget {
    pub max_turns: u32,
    pub max_waits: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DriveFailure {
    InvalidBudget,
    Clock(i32),
    Wait(i32),
    TurnBudget,
    WaitBudget,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DriveReport {
    pub outcome: LoopReport,
    pub failure: Option<DriveFailure>,
    pub clock_calls: u32,
    pub waits: u32,
    pub turns: u32,
    pub cleanup_turns: u8,
    pub last_observed_us: u64,
}
/// Drives concrete MainLoop resources with finite iteration and wait budgets.
/// Stop is checked before clock reads, after reads and before every wait. A
/// committed Stop advances only bounded teardown clocks/waits, allowing its
/// aggregate deadline to expire under a sticky latch. A failed clock/wait or
/// exhausted operation budget enters terminal abort then
/// runs at most five consume-once cleanup quanta without ANY further native
/// clock/wait call. Normal Stop still receives its aggregate drain deadline.
pub fn drive_bounded<R: DriveResources>(
    owner: &mut MainLoop<R>,
    budget: DriveBudget,
    initial_us: u64,
) -> DriveReport {
    let mut report = DriveReport {
        outcome: owner.report(),
        failure: None,
        clock_calls: 0,
        waits: 0,
        turns: 0,
        cleanup_turns: 0,
        last_observed_us: initial_us.max(owner.accepted_us()),
    };
    if budget.max_turns == 0 || budget.max_waits == 0 {
        report.failure = Some(DriveFailure::InvalidBudget);
        owner.abort(StopReason::StopFailed(-300));
    } else {
        let mut cleanup_only = false;
        for _ in 0..budget.max_turns {
            if owner.report().phase == LoopPhase::Finished {
                break;
            }
            let cancelled_before_clock = owner.resources_mut().take_stop();
            if cancelled_before_clock {
                owner.request_stop();
            }
            // The first Stop preempts normal/startup time work. Once committed
            // to Stopping, advance only its finite teardown clock/deadline even
            // if the external priority latch remains permanently asserted.
            let now = if cancelled_before_clock && owner.report().phase != LoopPhase::Stopping {
                report.last_observed_us
            } else {
                report.clock_calls += 1;
                match owner.resources_mut().now_us() {
                    Ok(now) => now,
                    Err(error) => {
                        report.failure = Some(DriveFailure::Clock(error));
                        owner.abort(StopReason::StopFailed(error));
                        break;
                    }
                }
            };
            report.last_observed_us = now.max(report.last_observed_us);
            if owner.resources_mut().take_stop() {
                owner.request_stop();
            }
            let turn = owner.step(now);
            report.last_observed_us = report.last_observed_us.max(owner.accepted_us());
            report.turns += 1;
            if turn.phase == LoopPhase::Finished {
                break;
            }
            if turn.phase == LoopPhase::Cleaning {
                cleanup_only = true;
                break;
            }
            if let Some(wake) = turn.wake_at_us
                && wake > report.last_observed_us
            {
                if report.waits == budget.max_waits {
                    report.failure = Some(DriveFailure::WaitBudget);
                    owner.abort(StopReason::StopFailed(-302));
                    break;
                }
                let cancelled = owner.resources_mut().take_stop();
                if cancelled {
                    owner.request_stop();
                    if owner.report().phase != LoopPhase::Stopping {
                        continue;
                    }
                }
                report.waits += 1;
                if let Err(error) = owner.resources_mut().wait_slice(wake, false) {
                    report.failure = Some(DriveFailure::Wait(error));
                    owner.abort(StopReason::StopFailed(error));
                    break;
                }
            }
        }
        if owner.report().phase != LoopPhase::Finished && report.failure.is_none() && !cleanup_only
        {
            report.failure = Some(DriveFailure::TurnBudget);
            owner.abort(StopReason::StopFailed(-301));
        }
    }
    // Exactly five resource slots. One extra iteration observes Finished when
    // no resource had ever been acquired. No policy/native retries occur here.
    report.last_observed_us = report.last_observed_us.max(owner.accepted_us());
    for _ in 0..6 {
        if owner.report().phase == LoopPhase::Finished {
            break;
        }
        owner.step(report.last_observed_us);
        report.last_observed_us = report.last_observed_us.max(owner.accepted_us());
        report.cleanup_turns += 1;
    }
    report.outcome = owner.report();
    report
}
