// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{MAX_READ_BYTES, ReadKind, ReadOutcome};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Diagnostic {
    pub abi_version: u32,
    pub stage: u32,
    pub status: i32,
    pub raw_status: i32,
    pub cleanup_flags: u32,
    pub raw_stop: i32,
    pub raw_close: i32,
    pub raw_terminate: i32,
    pub raw_lookup: i64,
}
/// Calls are synchronous and non-reentrant, retaining no read buffer. A failed
/// open must leave no owned lifecycle resources. Stop/close consume their owned
/// phase even on failure; no method silently retries or restarts native capture.
pub trait Backend {
    fn open(&mut self) -> Result<(), Diagnostic>;
    fn start(&mut self) -> Result<(), Diagnostic>;
    fn read_once(&mut self, output: &mut [u8]) -> Result<ReadOutcome, Diagnostic>;
    fn stop(&mut self) -> Result<(), Diagnostic>;
    fn close(&mut self) -> Result<(), Diagnostic>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budget {
    pub max_reads: u32,
    /// Exclusive host-monotonic deadline; native call duration still needs hardware validation.
    pub deadline_us: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnerPhase {
    Open,
    Started,
    Ended,
    Failed,
    Closed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureError {
    InvalidBudget,
    InvalidState,
    InvalidBuffer,
    Backend(Diagnostic),
    InvalidOutcome,
    ClockRegressed,
    ReadBudget,
    Deadline,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cleanup {
    pub stop: Option<Result<(), Diagnostic>>,
    pub close: Option<Result<(), Diagnostic>>,
}
impl Cleanup {
    pub fn uncertain(self) -> bool {
        self.stop.is_some_and(|result| result.is_err())
            || self.close.is_some_and(|result| result.is_err())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OwnerFailure {
    pub error: CaptureError,
    pub cleanup: Cleanup,
}
pub struct CaptureOwner<B: Backend> {
    backend: B,
    budget: Budget,
    phase: OwnerPhase,
    opened: bool,
    started: bool,
    reads: u32,
    last_host_us: u64,
    cleanup: Cleanup,
}
// Keep the primary failure and both cleanup records without allocation. The
// bounded 144-byte error is intentional at this non-per-frame fault boundary.
#[allow(clippy::result_large_err)]
impl<B: Backend> CaptureOwner<B> {
    pub fn open(mut backend: B, budget: Budget, now_us: u64) -> Result<Self, OwnerFailure> {
        if budget.max_reads == 0 || budget.deadline_us <= now_us {
            return Err(OwnerFailure {
                error: CaptureError::InvalidBudget,
                cleanup: Cleanup::default(),
            });
        }
        backend.open().map_err(|diagnostic| OwnerFailure {
            error: CaptureError::Backend(diagnostic),
            cleanup: Cleanup::default(),
        })?;
        Ok(Self {
            backend,
            budget,
            phase: OwnerPhase::Open,
            opened: true,
            started: false,
            reads: 0,
            last_host_us: now_us,
            cleanup: Cleanup::default(),
        })
    }
    pub const fn phase(&self) -> OwnerPhase {
        self.phase
    }
    pub const fn reads(&self) -> u32 {
        self.reads
    }
    pub const fn cleanup(&self) -> Cleanup {
        self.cleanup
    }
    pub fn start(&mut self) -> Result<(), OwnerFailure> {
        if self.phase != OwnerPhase::Open {
            return Err(self.report(CaptureError::InvalidState));
        }
        if let Err(diagnostic) = self.backend.start() {
            return Err(self.fail(CaptureError::Backend(diagnostic)));
        }
        self.started = true;
        self.phase = OwnerPhase::Started;
        Ok(())
    }
    /// Exactly one native NOWAIT call. Empty/Overrun never cause an internal wait.
    pub fn read_once(
        &mut self,
        now_us: u64,
        output: &mut [u8],
    ) -> Result<ReadOutcome, OwnerFailure> {
        if self.phase != OwnerPhase::Started {
            return Err(self.report(CaptureError::InvalidState));
        }
        if output.is_empty() || output.len() > MAX_READ_BYTES {
            return Err(self.report(CaptureError::InvalidBuffer));
        }
        if now_us < self.last_host_us {
            return Err(self.fail(CaptureError::ClockRegressed));
        }
        self.last_host_us = now_us;
        if now_us >= self.budget.deadline_us {
            return Err(self.fail(CaptureError::Deadline));
        }
        if self.reads >= self.budget.max_reads {
            return Err(self.fail(CaptureError::ReadBudget));
        }
        self.reads += 1;
        let outcome = match self.backend.read_once(output) {
            Ok(outcome) => outcome,
            Err(diagnostic) => {
                output.fill(0);
                return Err(self.fail(CaptureError::Backend(diagnostic)));
            }
        };
        if (outcome.kind == ReadKind::Data && (outcome.bytes == 0 || outcome.bytes > output.len()))
            || (outcome.kind != ReadKind::Data && outcome.bytes != 0)
            || (outcome.kind == ReadKind::Overrun && !outcome.discontinuity)
        {
            output.fill(0);
            return Err(self.fail(CaptureError::InvalidOutcome));
        }
        if outcome.kind != ReadKind::Data {
            output.fill(0);
        }
        if matches!(outcome.kind, ReadKind::Ended | ReadKind::Error) {
            self.phase = if outcome.kind == ReadKind::Ended {
                OwnerPhase::Ended
            } else {
                OwnerPhase::Failed
            };
            self.cleanup_once();
        }
        Ok(outcome)
    }
    pub fn close(&mut self) -> Cleanup {
        self.cleanup_once();
        if !matches!(self.phase, OwnerPhase::Ended | OwnerPhase::Failed) {
            self.phase = OwnerPhase::Closed;
        }
        self.cleanup
    }
    fn report(&self, error: CaptureError) -> OwnerFailure {
        OwnerFailure {
            error,
            cleanup: self.cleanup,
        }
    }
    fn fail(&mut self, error: CaptureError) -> OwnerFailure {
        self.phase = OwnerPhase::Failed;
        self.cleanup_once();
        self.report(error)
    }
    fn cleanup_once(&mut self) {
        if self.started {
            self.started = false;
            self.cleanup.stop = Some(self.backend.stop());
        }
        if self.opened {
            self.opened = false;
            self.cleanup.close = Some(self.backend.close());
        }
    }
}
impl<B: Backend> Drop for CaptureOwner<B> {
    fn drop(&mut self) {
        self.cleanup_once();
    }
}
