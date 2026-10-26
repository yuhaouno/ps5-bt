// SPDX-License-Identifier: GPL-3.0-or-later
//! Resource reopening tied to the existing persistent fault-episode controller.
use bt_audio::FinishEncoder;
use bt_capture::{Backend, CaptureOwner, Cleanup};
use bt_core::recovery::{
    Action, AttemptOutcome, AttemptTicket, Failure, Recovery, RecoveryError, Requirement,
    SafetyTicket,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResourceCleanup {
    pub confirmed: bool,
    pub capture: Option<Cleanup>,
    pub encoder: Option<Result<(), bt_core::Error>>,
    pub other_error: Option<i32>,
}
/// close consumes this resource even on error. Unknown native outcomes must
/// return confirmed=false and retain enough external evidence for reconciliation.
pub trait RebuildResource {
    fn close(self) -> ResourceCleanup;
}
/// A real capture/codec resource pair suitable for factory-based reconstruction.
/// Fixed PCM/scheduler storage is assembled separately for the new stream epoch.
pub struct CaptureEncoderResources<B: Backend, E: FinishEncoder> {
    pub capture: CaptureOwner<B>,
    pub encoder: E,
}
impl<B: Backend, E: FinishEncoder> RebuildResource for CaptureEncoderResources<B, E> {
    fn close(mut self) -> ResourceCleanup {
        let capture = self.capture.close();
        let encoder = self.encoder.finish();
        ResourceCleanup {
            confirmed: !capture.uncertain() && encoder.is_ok(),
            capture: Some(capture),
            encoder: Some(encoder),
            other_error: None,
        }
    }
}
pub struct ReopenFailure<R> {
    pub error: i32,
    pub failure: Failure,
    /// Return every partially acquired resource; the coordinator closes it once.
    pub partial: Option<R>,
}
/// Reopen performs one bounded acquisition attempt for this exact newer epoch.
/// It may construct a fresh CaptureOwner and persistent target SBC encoder.
/// It must never create a new Recovery, refill unknown credits, hide retries or
/// report success merely by returning an old failed handle. Failed acquisition
/// with no partial resource attests it left no new resource owned.
pub trait RebuildFactory {
    type Resource: RebuildResource;
    fn reopen(
        &mut self,
        epoch: u64,
        now_us: u64,
    ) -> Result<Self::Resource, ReopenFailure<Self::Resource>>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RebuildError {
    InvalidEpoch,
    EpochExhausted,
    Recovery(RecoveryError),
    InvalidState,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RebuildSnapshot {
    pub epoch: u64,
    pub resource_present: bool,
    pub cleanup: Option<ResourceCleanup>,
    pub open_error: Option<i32>,
    pub attempt: Option<AttemptTicket>,
}
/// Owns actual acquired resources, while Recovery stays with the application
/// across reconstruction. Use the SAME Recovery for every method in an episode.
/// Before calling step after app failure, retire the old protocol application
/// and preserve its cancellation evidence; cleanup here concerns factory-owned
/// native capture/encoder/transport resources only.
pub struct RebuildOwner<F: RebuildFactory> {
    factory: F,
    resource: Option<F::Resource>,
    epoch: u64,
    cleanup: Option<ResourceCleanup>,
    open_error: Option<i32>,
    attempt: Option<AttemptTicket>,
    stopped: bool,
}
impl<F: RebuildFactory> RebuildOwner<F> {
    pub fn new(
        factory: F,
        epoch: u64,
        resource: Option<F::Resource>,
    ) -> Result<Self, RebuildError> {
        if epoch == 0 {
            if let Some(resource) = resource {
                resource.close();
            }
            return Err(RebuildError::InvalidEpoch);
        }
        Ok(Self {
            factory,
            resource,
            epoch,
            cleanup: None,
            open_error: None,
            attempt: None,
            stopped: false,
        })
    }
    pub const fn snapshot(&self) -> RebuildSnapshot {
        RebuildSnapshot {
            epoch: self.epoch,
            resource_present: self.resource.is_some(),
            cleanup: self.cleanup,
            open_error: self.open_error,
            attempt: self.attempt,
        }
    }
    pub fn resource_mut(&mut self) -> Option<&mut F::Resource> {
        self.resource.as_mut()
    }
    fn close_once(&mut self) {
        if let Some(resource) = self.resource.take() {
            self.cleanup = Some(resource.close());
        }
        self.attempt = None;
    }
    /// At most one close or reopen per call. Cleanup-only safety can be
    /// acknowledged from the recorded successful close. Integrity-loss safety
    /// ALWAYS requires a separate explicit reconciliation attestation.
    pub fn step(&mut self, recovery: &mut Recovery, now_us: u64) -> Result<Action, RebuildError> {
        if self.stopped {
            return Ok(Action::Stopped);
        }
        let action = match recovery.poll(now_us) {
            Ok(action) => action,
            Err(error) => {
                self.close_once();
                self.stopped = true;
                return Err(RebuildError::Recovery(error));
            }
        };
        match action {
            Action::AwaitSafety {
                ticket,
                requirement,
            } => {
                if self.resource.is_some() {
                    self.close_once();
                    return Ok(action);
                }
                let clean = self.cleanup.is_none_or(|result| result.confirmed);
                if clean && requirement == Requirement::Cleanup {
                    recovery
                        .confirm_safety(now_us, ticket, Requirement::Cleanup)
                        .map_err(RebuildError::Recovery)?;
                    return recovery.poll(now_us).map_err(RebuildError::Recovery);
                }
            }
            Action::Attempt(ticket) => {
                if self.resource.is_some() {
                    return Err(RebuildError::InvalidState);
                }
                let Some(epoch) = self.epoch.checked_add(1) else {
                    recovery.stop();
                    self.stopped = true;
                    return Err(RebuildError::EpochExhausted);
                };
                recovery
                    .accept_attempt(now_us, ticket)
                    .map_err(RebuildError::Recovery)?;
                self.epoch = epoch;
                self.cleanup = None;
                self.open_error = None;
                self.attempt = Some(ticket);
                match self.factory.reopen(epoch, now_us) {
                    Ok(resource) => self.resource = Some(resource),
                    Err(failure) => {
                        self.open_error = Some(failure.error);
                        self.resource = failure.partial;
                        // A partial resource is retained until the next bounded
                        // cleanup quantum. Its failure never starts a fresh budget.
                        recovery
                            .finish_attempt(now_us, ticket, AttemptOutcome::Failed(failure.failure))
                            .map_err(RebuildError::Recovery)?;
                        self.attempt = None;
                    }
                }
                return recovery.poll(now_us).map_err(RebuildError::Recovery);
            }
            Action::Exhausted(_) | Action::Stopped | Action::Failed(_) => {
                self.close_once();
            }
            _ => {}
        }
        Ok(action)
    }
    /// Call only after an independently verified mechanism has completed native
    /// cleanup AND re-established routing/ownership/credits for this exact old
    /// epoch. Time passage, a new handle, and resetting a local ledger cannot
    /// supply this evidence. Stale epoch/tickets are rejected by both owners.
    pub fn confirm_reconciled(
        &mut self,
        recovery: &mut Recovery,
        now_us: u64,
        ticket: SafetyTicket,
        old_epoch: u64,
    ) -> Result<(), RebuildError> {
        if self.stopped || old_epoch != self.epoch || self.resource.is_some() {
            return Err(RebuildError::InvalidState);
        }
        recovery
            .confirm_safety(now_us, ticket, Requirement::Reconciliation)
            .map_err(RebuildError::Recovery)?;
        if let Some(cleanup) = self.cleanup.as_mut() {
            cleanup.confirmed = true;
        }
        Ok(())
    }
    /// Explicit attestation of functioning recovered resources. For an audio
    /// application, call only after its own Streaming/accepted-media requirements
    /// and external observations are satisfied. A successful reopen is insufficient.
    pub fn confirm_working(
        &mut self,
        recovery: &mut Recovery,
        now_us: u64,
        ticket: AttemptTicket,
    ) -> Result<(), RebuildError> {
        if self.stopped || self.resource.is_none() || self.attempt != Some(ticket) {
            return Err(RebuildError::InvalidState);
        }
        recovery
            .finish_attempt(now_us, ticket, AttemptOutcome::Success)
            .map_err(RebuildError::Recovery)?;
        self.attempt = None;
        Ok(())
    }
    pub fn stop(&mut self, recovery: &mut Recovery) {
        self.stopped = true;
        recovery.stop();
        self.close_once();
    }
}
impl<F: RebuildFactory> Drop for RebuildOwner<F> {
    fn drop(&mut self) {
        self.close_once();
    }
}
