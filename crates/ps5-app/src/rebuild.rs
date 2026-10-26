// SPDX-License-Identifier: GPL-3.0-or-later
//! Scoped concrete native resource rebuilding on one persistent recovery episode.
use crate::{
    Config, ConfigError, FactoryReport, NativeFactory, NativeStorage, Resolvers, ResourceError,
    ResourceKind,
};
use bt_core::recovery::{
    Action, AttemptOutcome, AttemptTicket, Failure, Recovery, RecoveryError, Requirement, Snapshot,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeRebuildError {
    InvalidEpoch,
    EpochExhausted,
    InvalidBudget,
    NotReady(Action),
    CleanupUnknown,
    Config(ConfigError),
    Native(ResourceError),
    Recovery(RecoveryError),
    Stopped,
    Deadline,
    OperationBudget,
    InvalidState,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeAttemptBudget {
    pub deadline_us: u64,
    pub max_operations: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeRebuildSnapshot {
    /// Local store/listener/capture/codec generation, NEVER a Bluetooth epoch.
    pub local_epoch: u64,
    pub recovery: Snapshot,
    pub cleanup: Option<FactoryReport>,
    pub cleanup_uncertain: bool,
    pub scope_incomplete: bool,
    pub prior_cleanup: Option<ResourceError>,
    pub error: Option<NativeRebuildError>,
}
pub enum NativeSessionExit<T> {
    /// A bounded fault observation while the actual resource scope was live.
    Fault { value: T, failure: Failure },
    /// Explicit operator/candidate stop; no later automatic retry is permitted.
    Stop(T),
}
pub struct NativeAttemptReport<T> {
    pub local_epoch: u64,
    pub ticket: AttemptTicket,
    pub ready: bool,
    pub operations: u32,
    pub value: Option<T>,
    pub before_cleanup: Snapshot,
    pub after_cleanup: Snapshot,
    pub cleanup: FactoryReport,
    pub cleanup_uncertain: bool,
    pub prior_cleanup: Option<ResourceError>,
    pub error: Option<NativeRebuildError>,
}
/// The callback may observe/use native resources only through this bounded live
/// scope. A successful open never attests working service or stable success.
/// Method errors are sticky. No resource is available outside the HRTB callback.
pub struct LiveNativeAttempt<'scope, 'native, 'config, S: FnMut() -> bool> {
    factory: &'scope mut NativeFactory<'native, 'config>,
    recovery: &'scope mut Recovery,
    stop: &'scope mut S,
    ticket: AttemptTicket,
    budget: NativeAttemptBudget,
    local_epoch: u64,
    last_us: u64,
    operations: u32,
    error: Option<NativeRebuildError>,
    prior_cleanup: Option<ResourceError>,
}
impl<S: FnMut() -> bool> LiveNativeAttempt<'_, '_, '_, S> {
    pub const fn local_epoch(&self) -> u64 {
        self.local_epoch
    }
    pub fn recovery(&self) -> Snapshot {
        self.recovery.snapshot()
    }
    pub const fn operations(&self) -> u32 {
        self.operations
    }
    fn fail<T>(&mut self, error: NativeRebuildError) -> Result<T, NativeRebuildError> {
        self.error.get_or_insert(error);
        Err(error)
    }
    fn resources_ready(&mut self) -> bool {
        let report = self.factory.report();
        if report.phase != crate::FactoryPhase::Ready
            || report
                .required
                .iter()
                .zip(report.acquired)
                .any(|(required, acquired)| *required && !acquired)
        {
            return false;
        }
        let parts = self.factory.parts_mut();
        if (report.required[ResourceKind::Clock as usize] && parts.clock.is_none())
            || (report.required[ResourceKind::Store as usize] && parts.store.is_none())
            || (report.required[ResourceKind::Listener as usize] && parts.listener.is_none())
            || (report.required[ResourceKind::Capture as usize] && parts.capture.is_none())
        {
            return false;
        }
        #[cfg(any(feature = "host-libsbc", feature = "target-libsbc"))]
        if report.required[ResourceKind::Encoder as usize] && parts.encoder.is_none() {
            return false;
        }
        true
    }
    fn checkpoint(&mut self) -> Result<u64, NativeRebuildError> {
        if let Some(error) = self.error {
            return Err(error);
        }
        if (self.stop)() {
            self.recovery.stop();
            return self.fail(NativeRebuildError::Stopped);
        }
        if !self.resources_ready() {
            self.recovery.stop();
            return self.fail(NativeRebuildError::InvalidState);
        }
        if self.operations == self.budget.max_operations {
            self.recovery.stop();
            return self.fail(NativeRebuildError::OperationBudget);
        }
        self.operations += 1;
        let now = match self.factory.now_us() {
            Ok(now) => now,
            Err(error) => {
                self.recovery.stop();
                return self.fail(NativeRebuildError::Native(error));
            }
        };
        self.last_us = now;
        if now >= self.budget.deadline_us {
            self.recovery.stop();
            return self.fail(NativeRebuildError::Deadline);
        }
        let action = match self.recovery.poll(now) {
            Ok(action) => action,
            Err(error) => return self.fail(NativeRebuildError::Recovery(error)),
        };
        if matches!(
            action,
            Action::Exhausted(_) | Action::Stopped | Action::Failed(_)
        ) {
            return self.fail(NativeRebuildError::NotReady(action));
        }
        Ok(now)
    }
    /// Exactly one caller-bounded operation; native callbacks must not hide a
    /// retry loop. The operation cannot borrow the factory beyond this call.
    /// ```compile_fail
    /// use ps5_app::{NativeStore,rebuild::LiveNativeAttempt};
    /// fn escape<'a,'b,S:FnMut()->bool>(live:&mut LiveNativeAttempt<'_,'a,'b,S>)->NativeStore<'a> {
    ///     live.operate(|factory|Ok(factory.take_store().unwrap())).unwrap()
    /// }
    /// ```
    pub fn operate<T>(
        &mut self,
        operation: impl FnOnce(&mut NativeFactory<'_, '_>) -> Result<T, ResourceError>,
    ) -> Result<T, NativeRebuildError> {
        self.checkpoint()?;
        let result = operation(self.factory);
        if (self.stop)() {
            self.recovery.stop();
            return self.fail(NativeRebuildError::Stopped);
        }
        match result {
            Ok(value) => {
                if !self.resources_ready() {
                    self.recovery.stop();
                    return self.fail(NativeRebuildError::InvalidState);
                }
                Ok(value)
            }
            Err(error) => {
                if error.flags != 0 || error.cleanup.iter().any(|raw| *raw != 0) {
                    self.prior_cleanup = Some(error);
                }
                self.fail(NativeRebuildError::Native(error))
            }
        }
    }
    /// Retain an earlier client/transfer cleanup failure that a later successful
    /// factory shutdown cannot erase. This never authorizes reconciliation.
    pub fn note_cleanup_uncertainty(&mut self, error: ResourceError) {
        self.prior_cleanup = Some(error);
        self.error.get_or_insert(NativeRebuildError::CleanupUnknown);
    }
    /// Explicit external observation of functioning LOCAL resources, beyond
    /// constructor success. Audio/headset success is not implied or accepted here.
    pub fn confirm_working(&mut self) -> Result<(), NativeRebuildError> {
        let now = self.checkpoint()?;
        self.recovery
            .finish_attempt(now, self.ticket, AttemptOutcome::Success)
            .map_err(|error| {
                let error = NativeRebuildError::Recovery(error);
                self.error.get_or_insert(error);
                error
            })
    }
    /// Advance stable-success observation only while native resources remain
    /// actually acquired. Polling after the scope closes cannot reset a budget.
    pub fn poll(&mut self) -> Result<Action, NativeRebuildError> {
        self.checkpoint()?;
        Ok(self.recovery.snapshot().action)
    }
}
/// Concrete native close/reopen coordinator. Recovery is borrowed structurally
/// for the coordinator's lifetime; it cannot be swapped/reset per attempt.
/// Storage is lent anew for each scope after the previous factory is consumed.
/// No self-reference, unsafe alias/lifetime extension, allocation, transport
/// constructor or controller-credit operation is involved.
pub struct NativeRebuild<'recovery> {
    recovery: &'recovery mut Recovery,
    local_epoch: u64,
    cleanup: Option<FactoryReport>,
    cleanup_uncertain: bool,
    scope_incomplete: bool,
    prior_cleanup: Option<ResourceError>,
    error: Option<NativeRebuildError>,
}
impl<'recovery> NativeRebuild<'recovery> {
    pub fn new(
        recovery: &'recovery mut Recovery,
        local_epoch: u64,
    ) -> Result<Self, NativeRebuildError> {
        if local_epoch == 0 {
            return Err(NativeRebuildError::InvalidEpoch);
        }
        Ok(Self {
            recovery,
            local_epoch,
            cleanup: None,
            cleanup_uncertain: false,
            scope_incomplete: false,
            prior_cleanup: None,
            error: None,
        })
    }
    pub fn snapshot(&self) -> NativeRebuildSnapshot {
        NativeRebuildSnapshot {
            local_epoch: self.local_epoch,
            recovery: self.recovery.snapshot(),
            cleanup: self.cleanup,
            cleanup_uncertain: self.cleanup_uncertain || self.scope_incomplete,
            scope_incomplete: self.scope_incomplete,
            prior_cleanup: self.prior_cleanup,
            error: self.error,
        }
    }
    /// Retire the actual old factory before touching its backing storage again.
    /// Restore any handed-off store first. prior_cleanup is earlier uncertainty
    /// (for example client close), not an ordinary request error.
    pub fn retire(
        &mut self,
        mut factory: NativeFactory<'_, '_>,
        now_us: u64,
        failure: Failure,
        prior_cleanup: Option<ResourceError>,
    ) -> Result<FactoryReport, NativeRebuildError> {
        let cleanup = factory.shutdown();
        self.cleanup_uncertain |= cleanup.cleanup_uncertain() || prior_cleanup.is_some();
        if let Some(error) = prior_cleanup {
            self.prior_cleanup.get_or_insert(error);
        }
        self.cleanup = Some(cleanup);
        let failure = if self.cleanup_uncertain {
            Failure::IntegrityLost
        } else {
            failure
        };
        self.recovery
            .trigger(now_us, failure)
            .map_err(NativeRebuildError::Recovery)?;
        Ok(cleanup)
    }
    /// Acknowledge only successful local cleanup. Reconciliation remains a
    /// visible blocked requirement; this candidate provides no radio authority.
    pub fn poll(&mut self, now_us: u64) -> Result<Action, NativeRebuildError> {
        if self.scope_incomplete {
            self.recovery.stop();
            return Err(NativeRebuildError::CleanupUnknown);
        }
        let action = self
            .recovery
            .poll(now_us)
            .map_err(NativeRebuildError::Recovery)?;
        if self.cleanup_uncertain || self.scope_incomplete {
            return Err(NativeRebuildError::CleanupUnknown);
        }
        if let Action::AwaitSafety {
            ticket,
            requirement: Requirement::Cleanup,
        } = action
            && self.cleanup.is_some()
        {
            self.recovery
                .confirm_safety(now_us, ticket, Requirement::Cleanup)
                .map_err(NativeRebuildError::Recovery)?;
            return self
                .recovery
                .poll(now_us)
                .map_err(NativeRebuildError::Recovery);
        }
        Ok(action)
    }
    pub fn stop(&mut self) {
        self.recovery.stop();
    }
    /// # Safety
    /// Resolvers/imports must meet NativeFactory::create's reviewed signatures
    /// and lifetimes for this whole scope. The callback performs bounded local
    /// operations, reports earlier cleanup uncertainty, and does not reset the
    /// native capture uncertainty latch. This does not authorize a PS5 run.
    ///
    /// Higher-ranked native lifetime prevents a borrowed/owned native handle
    /// escaping in T. After return, the SAME NativeStorage can be safely lent to
    /// another attempt; every actual old owner has been consumed first.
    #[allow(clippy::too_many_arguments, clippy::result_large_err)]
    pub unsafe fn with_attempt<'config, T, S, F>(
        &mut self,
        storage: &mut NativeStorage,
        config: Config<'config>,
        resolvers: Resolvers,
        now_us: u64,
        budget: NativeAttemptBudget,
        mut stop: S,
        use_ready: F,
    ) -> Result<NativeAttemptReport<T>, NativeRebuildError>
    where
        S: FnMut() -> bool,
        F: for<'scope, 'native> FnOnce(
            &mut LiveNativeAttempt<'scope, 'native, 'config, S>,
        ) -> NativeSessionExit<T>,
    {
        if stop() {
            self.stop();
            return Err(NativeRebuildError::Stopped);
        }
        if budget.max_operations == 0 || budget.deadline_us <= now_us {
            return Err(NativeRebuildError::InvalidBudget);
        }
        config.validate().map_err(NativeRebuildError::Config)?;
        let Action::Attempt(ticket) = self.poll(now_us)? else {
            return Err(NativeRebuildError::NotReady(
                self.recovery.snapshot().action,
            ));
        };
        let Some(epoch) = self.local_epoch.checked_add(1) else {
            self.stop();
            return Err(NativeRebuildError::EpochExhausted);
        };
        self.recovery
            .accept_attempt(now_us, ticket)
            .map_err(NativeRebuildError::Recovery)?;
        self.local_epoch = epoch;
        self.scope_incomplete = true;
        self.error = None;
        // SAFETY: caller establishes the existing native resolver/import ABI;
        // storage remains exclusively and address-stably borrowed inside scope.
        let mut factory = match unsafe { NativeFactory::create(storage, config, resolvers) } {
            Ok(factory) => factory,
            Err(error) => {
                let _ = self.recovery.finish_attempt(
                    now_us,
                    ticket,
                    AttemptOutcome::Failed(Failure::Retryable),
                );
                self.scope_incomplete = false;
                self.error = Some(NativeRebuildError::Config(error));
                return Err(NativeRebuildError::Config(error));
            }
        };
        let mut error = None;
        let mut last_us = now_us;
        let mut ready = true;
        for resource in [
            ResourceKind::Clock,
            ResourceKind::Store,
            ResourceKind::Listener,
            ResourceKind::Capture,
            ResourceKind::Encoder,
        ] {
            if stop() {
                self.stop();
                error = Some(NativeRebuildError::Stopped);
                ready = false;
                break;
            }
            if resource != ResourceKind::Clock {
                match factory.now_us() {
                    Ok(now) => last_us = now,
                    Err(native) => {
                        self.stop();
                        error = Some(NativeRebuildError::Native(native));
                        ready = false;
                        break;
                    }
                }
                if last_us >= budget.deadline_us {
                    self.stop();
                    error = Some(NativeRebuildError::Deadline);
                    ready = false;
                    break;
                }
                match self.recovery.poll(last_us) {
                    Ok(Action::InFlight(current)) if current == ticket => {}
                    Ok(action) => {
                        error = Some(NativeRebuildError::NotReady(action));
                        ready = false;
                        break;
                    }
                    Err(recovery) => {
                        error = Some(NativeRebuildError::Recovery(recovery));
                        ready = false;
                        break;
                    }
                }
            }
            if let Err(failure) = factory.acquire(resource, last_us) {
                error = Some(NativeRebuildError::Native(failure.error));
                ready = false;
                break;
            }
            if stop() {
                self.stop();
                error = Some(NativeRebuildError::Stopped);
                ready = false;
                break;
            }
        }
        let mut operations = 0;
        let mut prior_cleanup = None;
        let exit = if ready {
            let mut active = LiveNativeAttempt {
                factory: &mut factory,
                recovery: self.recovery,
                stop: &mut stop,
                ticket,
                budget,
                local_epoch: epoch,
                last_us,
                operations: 0,
                error: None,
                prior_cleanup: None,
            };
            let exit = use_ready(&mut active);
            last_us = active.last_us;
            operations = active.operations;
            error = active.error;
            prior_cleanup = active.prior_cleanup;
            Some(exit)
        } else {
            None
        };
        if stop() {
            self.stop();
            error.get_or_insert(NativeRebuildError::Stopped);
        }
        let before_cleanup = self.recovery.snapshot();
        let cleanup = factory.shutdown();
        if stop() {
            self.stop();
            error.get_or_insert(NativeRebuildError::Stopped);
        }
        self.cleanup_uncertain |= cleanup.cleanup_uncertain() || prior_cleanup.is_some();
        if let Some(error) = prior_cleanup {
            self.prior_cleanup.get_or_insert(error);
        }
        self.cleanup = Some(cleanup);
        self.scope_incomplete = false;
        self.error = error;
        let mut value = None;
        let failure = match exit {
            Some(NativeSessionExit::Stop(result)) => {
                value = Some(result);
                self.stop();
                None
            }
            Some(NativeSessionExit::Fault {
                value: result,
                failure,
            }) => {
                value = Some(result);
                Some(failure)
            }
            None => Some(Failure::Retryable),
        };
        if !matches!(
            self.recovery.snapshot().action,
            Action::Stopped | Action::Exhausted(_) | Action::Failed(_)
        ) {
            let failure = if self.cleanup_uncertain {
                Failure::IntegrityLost
            } else {
                failure.unwrap_or(Failure::Retryable)
            };
            // Even after working attestation, closing a not-yet-stable resource
            // is a fault in the SAME episode, never stable time after close.
            if let Err(recovery) = self.recovery.trigger(last_us, failure) {
                self.error
                    .get_or_insert(NativeRebuildError::Recovery(recovery));
            }
        }
        Ok(NativeAttemptReport {
            local_epoch: epoch,
            ticket,
            ready,
            operations,
            value,
            before_cleanup,
            after_cleanup: self.recovery.snapshot(),
            cleanup,
            cleanup_uncertain: self.cleanup_uncertain,
            prior_cleanup: self.prior_cleanup,
            error: self.error,
        })
    }
}
