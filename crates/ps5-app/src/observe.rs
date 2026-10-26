// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded raw-byte observation; no PCM interpretation, encoding or native wait.
use bt_capture::{
    Backend, CaptureOwner, Cleanup, MAX_READ_BYTES, NativeTime, OwnerFailure, ReadKind,
};

pub const MAX_OBSERVE_READS: u32 = 20;
pub const MAX_OBSERVE_DURATION_US: u64 = 2_000_000;
pub const OBSERVE_INTERVAL_US: u64 = 100_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObserveConfig {
    pub max_reads: u32,
    /// Exclusive duration from workload creation, in the host monotonic domain.
    pub duration_us: u64,
    /// Zero for an immediate first read, or the reviewed 100 ms settle request.
    pub first_settle_us: u64,
    /// Exactly 100 ms. This workload suggests deadlines; it never sleeps.
    pub interval_us: u64,
}
impl ObserveConfig {
    pub const BOUNDED: Self = Self {
        max_reads: MAX_OBSERVE_READS,
        duration_us: MAX_OBSERVE_DURATION_US,
        first_settle_us: OBSERVE_INTERVAL_US,
        interval_us: OBSERVE_INTERVAL_US,
    };
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObserveStop {
    Cancelled,
    Deadline,
    ReadBudget,
    Ended,
    Failed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObserveError {
    InvalidConfig,
    InvalidBuffer,
    ClockRegressed,
    ClockOverflow,
    /// Some other caller consumed or replaced the capture owner between steps.
    OwnerProgressChanged,
    Owner(OwnerFailure),
    NativeRead {
        raw_status: i32,
        cleanup: Cleanup,
    },
    Cleanup(Cleanup),
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ObserveStatistics {
    /// Workload read_once invocations, including early owner-budget rejection.
    pub owner_calls: u32,
    /// Actual owner read-budget advances. Backend errors still consume one read.
    pub native_reads: u32,
    pub data: u32,
    pub empty: u32,
    pub zero: u32,
    pub overruns: u32,
    pub ended: u32,
    pub native_errors: u32,
    pub owner_failures: u32,
    pub discontinuities: u32,
    /// An event count only. No lost-sample count or sample position is inferred.
    pub unknown_gap_events: u32,
    pub total_bytes: u64,
    pub max_read_bytes: u32,
}
/// Public diagnostic metadata deliberately excludes raw audio and the sixteen
/// opaque native metadata bytes. Raw native time, when supplied by another
/// backend, stays in its original domain and is never converted or paced from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObserveRead {
    pub kind: ReadKind,
    pub raw_status: i32,
    pub bytes: u32,
    pub discontinuity: bool,
    /// Caller observation immediately before the synchronous read; not sample
    /// time, completion time, read duration or native timestamp conversion.
    pub host_observed_us: u64,
    pub native_time: Option<NativeTime>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObserveSnapshot {
    pub config: ObserveConfig,
    pub started_us: u64,
    pub deadline_us: u64,
    pub last_observed_us: u64,
    pub next_read_us: Option<u64>,
    pub stop_reason: Option<ObserveStop>,
    pub error: Option<ObserveError>,
    pub statistics: ObserveStatistics,
    pub last_read: Option<ObserveRead>,
    /// Already-consumed reads remain visible and are never reset by this owner.
    pub owner_reads_initial: Option<u32>,
    pub owner_reads_final: Option<u32>,
    /// Only what CaptureOwner has already attempted. Normal workload completion
    /// does not itself close; the enclosing resource factory still owns cleanup.
    pub owner_cleanup: Cleanup,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObserveProgress {
    pub progressed: bool,
    pub wake_at_us: Option<u64>,
    pub finished: bool,
}
/// Caller-driven state only; borrows an existing owner for each quantum and
/// performs at most one read. No initialization, restart, wait or cleanup loop.
/// Captured bytes are cleared before every return and never retained here.
pub struct ObserveWorkload {
    snapshot: ObserveSnapshot,
}
// Preserve complete primary/cleanup records without allocation on cold errors.
#[allow(clippy::result_large_err)]
impl ObserveWorkload {
    pub fn new(config: ObserveConfig, started_us: u64) -> Result<Self, ObserveError> {
        if config.max_reads == 0
            || config.max_reads > MAX_OBSERVE_READS
            || config.duration_us == 0
            || config.duration_us > MAX_OBSERVE_DURATION_US
            || !matches!(config.first_settle_us, 0 | OBSERVE_INTERVAL_US)
            || config.first_settle_us >= config.duration_us
            || config.interval_us != OBSERVE_INTERVAL_US
        {
            return Err(ObserveError::InvalidConfig);
        }
        let deadline_us = started_us
            .checked_add(config.duration_us)
            .ok_or(ObserveError::ClockOverflow)?;
        let next_read_us = started_us
            .checked_add(config.first_settle_us)
            .ok_or(ObserveError::ClockOverflow)?;
        Ok(Self {
            snapshot: ObserveSnapshot {
                config,
                started_us,
                deadline_us,
                last_observed_us: started_us,
                next_read_us: Some(next_read_us),
                stop_reason: None,
                error: None,
                statistics: ObserveStatistics::default(),
                last_read: None,
                owner_reads_initial: None,
                owner_reads_final: None,
                owner_cleanup: Cleanup::default(),
            },
        })
    }
    pub const fn snapshot(&self) -> ObserveSnapshot {
        self.snapshot
    }
    fn progress(&self, progressed: bool) -> ObserveProgress {
        ObserveProgress {
            progressed,
            wake_at_us: self.snapshot.next_read_us,
            finished: self.snapshot.stop_reason.is_some(),
        }
    }
    fn finish(&mut self, reason: ObserveStop, progressed: bool) -> ObserveProgress {
        self.snapshot.stop_reason = Some(reason);
        self.snapshot.next_read_us = None;
        self.progress(progressed)
    }
    fn fail(&mut self, error: ObserveError) -> ObserveError {
        self.snapshot.error = Some(error);
        self.finish(ObserveStop::Failed, false);
        error
    }
    /// Stop is checked before clock/buffer validation or a new native operation.
    /// The byte buffer is always cleared, including waiting, stop and error paths.
    /// A late caller performs one current read at most; it never catches up by
    /// replaying missed read deadlines or inventing lost samples.
    pub fn step<B: Backend>(
        &mut self,
        owner: &mut CaptureOwner<B>,
        bytes: &mut [u8],
        now_us: u64,
        stop: bool,
    ) -> Result<ObserveProgress, ObserveError> {
        bytes.fill(0);
        if let Some(error) = self.snapshot.error {
            return Err(error);
        }
        if self.snapshot.stop_reason.is_some() {
            return Ok(self.progress(false));
        }
        if stop {
            return Ok(self.finish(ObserveStop::Cancelled, false));
        }
        if now_us < self.snapshot.last_observed_us {
            return Err(self.fail(ObserveError::ClockRegressed));
        }
        self.snapshot.last_observed_us = now_us;
        if now_us >= self.snapshot.deadline_us {
            return Ok(self.finish(ObserveStop::Deadline, false));
        }
        if bytes.is_empty() || bytes.len() > MAX_READ_BYTES {
            return Err(self.fail(ObserveError::InvalidBuffer));
        }
        if self.snapshot.statistics.owner_calls >= self.snapshot.config.max_reads {
            return Ok(self.finish(ObserveStop::ReadBudget, false));
        }
        let before = owner.reads();
        if self
            .snapshot
            .owner_reads_final
            .is_some_and(|last| last != before)
        {
            return Err(self.fail(ObserveError::OwnerProgressChanged));
        }
        if now_us
            < self
                .snapshot
                .next_read_us
                .unwrap_or(self.snapshot.deadline_us)
        {
            return Ok(self.progress(false));
        }
        self.snapshot.owner_reads_initial.get_or_insert(before);
        self.snapshot.statistics.owner_calls += 1;
        let outcome = owner.read_once(now_us, bytes);
        bytes.fill(0);
        let after = owner.reads();
        self.snapshot.owner_reads_final = Some(after);
        self.snapshot.owner_cleanup = owner.cleanup();
        let Some(advanced) = after.checked_sub(before).filter(|delta| *delta <= 1) else {
            return Err(self.fail(ObserveError::OwnerProgressChanged));
        };
        self.snapshot.statistics.native_reads += advanced;
        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err(failure) => {
                self.snapshot.statistics.owner_failures += 1;
                return Err(self.fail(ObserveError::Owner(failure)));
            }
        };
        self.snapshot.last_read = Some(ObserveRead {
            kind: outcome.kind,
            raw_status: outcome.raw_status,
            bytes: outcome.bytes as u32,
            discontinuity: outcome.discontinuity,
            host_observed_us: now_us,
            native_time: outcome.native_time,
        });
        let stats = &mut self.snapshot.statistics;
        if outcome.discontinuity {
            stats.discontinuities += 1;
            stats.unknown_gap_events += 1;
        }
        match outcome.kind {
            ReadKind::Data => {
                stats.data += 1;
                stats.total_bytes += outcome.bytes as u64;
                stats.max_read_bytes = stats.max_read_bytes.max(outcome.bytes as u32);
            }
            ReadKind::Empty => stats.empty += 1,
            ReadKind::Zero => stats.zero += 1,
            ReadKind::Overrun => stats.overruns += 1,
            ReadKind::Ended => {
                stats.ended += 1;
                if self.snapshot.owner_cleanup.uncertain() {
                    return Err(self.fail(ObserveError::Cleanup(self.snapshot.owner_cleanup)));
                }
                return Ok(self.finish(ObserveStop::Ended, true));
            }
            ReadKind::Error => {
                stats.native_errors += 1;
                return Err(self.fail(ObserveError::NativeRead {
                    raw_status: outcome.raw_status,
                    cleanup: self.snapshot.owner_cleanup,
                }));
            }
        }
        if self.snapshot.statistics.owner_calls == self.snapshot.config.max_reads {
            return Ok(self.finish(ObserveStop::ReadBudget, true));
        }
        // Clamp the delta before addition: a valid absolute deadline near u64::MAX
        // need not fail merely because a full cadence would extend beyond it.
        let delay = self
            .snapshot
            .config
            .interval_us
            .min(self.snapshot.deadline_us - now_us);
        let next = now_us
            .checked_add(delay)
            .ok_or_else(|| self.fail(ObserveError::ClockOverflow))?;
        self.snapshot.next_read_us = Some(next);
        Ok(self.progress(true))
    }
}
