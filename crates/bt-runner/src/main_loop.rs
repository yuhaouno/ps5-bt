// SPDX-License-Identifier: GPL-3.0-or-later
//! Finite single-owner scheduling and consume-once resource unwinding.

/// Acquisition order. Cleanup always uses its reverse, including failed startup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum Resource {
    Store,
    Transport,
    Listener,
    Capture,
    Encoder,
}
const RESOURCES: [Resource; 5] = [
    Resource::Store,
    Resource::Transport,
    Resource::Listener,
    Resource::Capture,
    Resource::Encoder,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum Lane {
    Network,
    Control,
    Input,
    Capture,
    Encode,
    Send,
}
const LANES: [Lane; 6] = [
    Lane::Network,
    Lane::Control,
    Lane::Input,
    Lane::Capture,
    Lane::Encode,
    Lane::Send,
];

/// Each callback must perform at most one bounded operation. No callback may
/// hide blocking I/O, an unbounded poll/retry loop or retain a borrowed buffer.
/// The implementation owns its resources and fixed storage; only this owner
/// invokes it. An acquisition error MUST unwind any partial acquisition of that
/// one resource. Successful acquisitions are cleaned exactly once by MainLoop.
/// Cleanup consumes ownership even on error; it must reap/cancel native work or
/// report uncertainty, never infer cleanup from dropping protocol state.
///
/// Stop disables new capture/encoding/requests immediately. poll_stop drives
/// only already-owned protocol/transfer teardown (preserving healthy partial
/// PDUs). abort_stop reports terminal cancellation and accepted prefixes without
/// restoring credits. Neither callback may grant ownership or reconcile credits.
pub trait LoopResources {
    fn acquire(&mut self, resource: Resource, now_us: u64) -> Result<(), i32>;
    fn take_stop(&mut self) -> bool;
    fn quantum(&mut self, lane: Lane, now_us: u64) -> Result<Quantum, i32>;
    fn request_stop(&mut self, now_us: u64) -> Result<(), i32>;
    fn poll_stop(&mut self, now_us: u64) -> Result<bool, i32>;
    fn abort_stop(&mut self);
    fn cleanup(&mut self, resource: Resource) -> Result<(), i32>;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Continuity {
    pub capture_empty: u64,
    pub capture_overruns: u64,
    pub capture_gap_frames: u64,
    pub queue_frames: Option<usize>,
    pub encode_failures: u64,
    pub send_waits: u64,
    pub rebuffer_episodes: u64,
    pub reconnects: u64,
    /// Stable platform-defined, redacted reason; never native pointers or keys.
    pub last_reason: Option<i32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Quantum {
    /// false means this lane has no immediately runnable work this turn.
    pub progressed: bool,
    /// Absolute microseconds, independent of native capture timestamp units.
    pub wake_at_us: Option<u64>,
    /// Deltas except queue_frames (the current backlog observation).
    pub continuity: Continuity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoopConfig {
    /// Per-turn limits in Lane order; each is 1..=64, including control.
    pub quotas: [u8; 6],
    pub max_wait_us: u64,
    pub stop_timeout_us: u64,
    /// Optional exclusive deadline for an independently selected experiment.
    pub deadline_us: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopReason {
    Requested,
    Deadline,
    Startup { resource: Resource, error: i32 },
    Work { lane: Lane, error: i32 },
    ClockRegressed,
    ClockOverflow,
    StopFailed(i32),
    StopTimeout,
    Dropped,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopPhase {
    Starting,
    Running,
    Stopping,
    Cleaning,
    Finished,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LoopStatistics {
    pub turns: u64,
    pub quanta: [u64; 6],
    pub continuity: Continuity,
    pub queue_high_water_frames: usize,
    pub counters_saturated: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoopReport {
    pub phase: LoopPhase,
    pub reason: Option<StopReason>,
    pub forced: bool,
    pub stop_error: Option<i32>,
    /// None means never acquired or cleanup has not yet been attempted.
    pub cleanup: [Option<Result<(), i32>>; 5],
    pub acquired: [bool; 5],
    pub statistics: LoopStatistics,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Turn {
    pub phase: LoopPhase,
    pub quanta: [u8; 6],
    /// A bounded suggested wait only. Native wait must remain interruptible by
    /// Stop and readiness. No waiting is done inside the owner.
    pub wake_at_us: Option<u64>,
}

/// Owns all callbacks and acquisition state; no allocation or per-frame storage.
/// One step initializes/cleans at most one resource, or runs at most sum(quotas)
/// work quanta. Stop is checked before and after every work quantum.
pub struct MainLoop<R: LoopResources> {
    resources: R,
    config: LoopConfig,
    report: LoopReport,
    next_resource: usize,
    last_us: u64,
    stop_deadline: Option<u64>,
    stop_requested: bool,
}
impl<R: LoopResources> MainLoop<R> {
    pub fn new(resources: R, config: LoopConfig, now_us: u64) -> Result<Self, i32> {
        Self::new_retaining(resources, config, now_us).map_err(|(error, _)| error)
    }
    /// Preserve caller-owned preinitialized resources on configuration failure,
    /// allowing explicit native cleanup and its report before releasing storage.
    #[allow(clippy::result_large_err)]
    pub fn new_retaining(resources: R, config: LoopConfig, now_us: u64) -> Result<Self, (i32, R)> {
        if config.quotas.iter().any(|&q| q == 0 || q > 64)
            || config.max_wait_us == 0
            || config.stop_timeout_us == 0
            || config.deadline_us.is_some_and(|end| end <= now_us)
            || now_us.checked_add(config.max_wait_us).is_none()
            || now_us.checked_add(config.stop_timeout_us).is_none()
        {
            return Err((-1, resources));
        }
        Ok(Self {
            resources,
            config,
            report: LoopReport {
                phase: LoopPhase::Starting,
                reason: None,
                forced: false,
                stop_error: None,
                cleanup: [None; 5],
                acquired: [false; 5],
                statistics: LoopStatistics::default(),
            },
            next_resource: 0,
            last_us: now_us,
            stop_deadline: None,
            stop_requested: false,
        })
    }
    pub const fn report(&self) -> LoopReport {
        self.report
    }
    pub const fn resources(&self) -> &R {
        &self.resources
    }
    /// Borrow the same single owner for its fallible clock/wait port. Callers
    /// must not bypass acquisition/cleanup or independently drive protocol lanes.
    pub fn resources_mut(&mut self) -> &mut R {
        &mut self.resources
    }
    /// Terminal bounded abort for native clock/wait or outer execution-budget
    /// failure. Cleanup remains observable through ordinary finite steps.
    pub fn abort(&mut self, reason: StopReason) {
        if self.report.phase != LoopPhase::Finished {
            self.force(reason);
        }
    }
    /// An out-of-band stop latch; no valid clock is needed to request it.
    pub fn request_stop(&mut self) {
        self.stop_requested = true;
    }
    fn reason(&mut self, reason: StopReason) {
        if self.report.reason.is_none() {
            self.report.reason = Some(reason);
        }
    }
    fn force(&mut self, reason: StopReason) {
        self.reason(reason);
        if !self.report.forced {
            self.resources.abort_stop();
            self.report.forced = true;
        }
        self.report.phase = LoopPhase::Cleaning;
    }
    fn begin_stop(&mut self, now: u64, reason: StopReason) {
        self.reason(reason);
        let Some(end) = now.checked_add(self.config.stop_timeout_us) else {
            self.force(StopReason::ClockOverflow);
            return;
        };
        self.stop_deadline = Some(end);
        match self.resources.request_stop(now) {
            Ok(()) => self.report.phase = LoopPhase::Stopping,
            Err(error) => {
                self.report.stop_error = Some(error);
                self.force(StopReason::StopFailed(error));
            }
        }
    }
    fn stop_pending(&mut self) -> bool {
        // Always poll the platform latch even if the external latch is set.
        self.resources.take_stop() || self.stop_requested
    }
    fn add(&mut self, value: u64, delta: u64) -> u64 {
        match value.checked_add(delta) {
            Some(value) => value,
            None => {
                self.report.statistics.counters_saturated = true;
                u64::MAX
            }
        }
    }
    fn record(&mut self, lane: Lane, q: Quantum) {
        let index = lane as usize;
        self.report.statistics.quanta[index] = self.add(self.report.statistics.quanta[index], 1);
        let delta = q.continuity;
        let old = self.report.statistics.continuity;
        self.report.statistics.continuity = Continuity {
            capture_empty: self.add(old.capture_empty, delta.capture_empty),
            capture_overruns: self.add(old.capture_overruns, delta.capture_overruns),
            capture_gap_frames: self.add(old.capture_gap_frames, delta.capture_gap_frames),
            queue_frames: delta.queue_frames.or(old.queue_frames),
            encode_failures: self.add(old.encode_failures, delta.encode_failures),
            send_waits: self.add(old.send_waits, delta.send_waits),
            rebuffer_episodes: self.add(old.rebuffer_episodes, delta.rebuffer_episodes),
            reconnects: self.add(old.reconnects, delta.reconnects),
            last_reason: delta.last_reason.or(old.last_reason),
        };
        self.report.statistics.queue_high_water_frames = self
            .report
            .statistics
            .queue_high_water_frames
            .max(delta.queue_frames.unwrap_or(0));
    }
    pub fn step(&mut self, now_us: u64) -> Turn {
        let mut turn = Turn {
            phase: self.report.phase,
            quanta: [0; 6],
            wake_at_us: None,
        };
        if self.report.phase == LoopPhase::Finished {
            return turn;
        }
        self.report.statistics.turns = self.add(self.report.statistics.turns, 1);
        if now_us < self.last_us {
            self.force(StopReason::ClockRegressed);
        } else {
            self.last_us = now_us;
        }
        if matches!(self.report.phase, LoopPhase::Starting | LoopPhase::Running) {
            if self.stop_pending() {
                self.begin_stop(self.last_us, StopReason::Requested);
            } else if self.config.deadline_us.is_some_and(|end| now_us >= end) {
                self.begin_stop(self.last_us, StopReason::Deadline);
            }
        }
        match self.report.phase {
            LoopPhase::Starting => {
                let resource = RESOURCES[self.next_resource];
                match self.resources.acquire(resource, now_us) {
                    Ok(()) => {
                        self.report.acquired[resource as usize] = true;
                        self.next_resource += 1;
                        if self.next_resource == RESOURCES.len() {
                            self.report.phase = LoopPhase::Running;
                        }
                    }
                    Err(error) => {
                        self.reason(StopReason::Startup { resource, error });
                        self.report.phase = LoopPhase::Cleaning;
                    }
                }
            }
            LoopPhase::Running => {
                let Some(mut wake) = now_us.checked_add(self.config.max_wait_us) else {
                    self.force(StopReason::ClockOverflow);
                    turn.phase = self.report.phase;
                    return turn;
                };
                if let Some(end) = self.config.deadline_us {
                    wake = wake.min(end);
                }
                'work: for lane in LANES {
                    for _ in 0..self.config.quotas[lane as usize] {
                        if self.stop_pending() {
                            self.begin_stop(now_us, StopReason::Requested);
                            break 'work;
                        }
                        match self.resources.quantum(lane, now_us) {
                            Ok(quantum) => {
                                turn.quanta[lane as usize] += 1;
                                self.record(lane, quantum);
                                if let Some(due) = quantum.wake_at_us {
                                    wake = wake.min(due.max(now_us));
                                }
                                if self.stop_pending() {
                                    self.begin_stop(now_us, StopReason::Requested);
                                    break 'work;
                                }
                                if !quantum.progressed {
                                    break;
                                }
                                // A quota hit means work may remain; yield to the
                                // next lane and do not sleep before another turn.
                                wake = now_us;
                            }
                            Err(error) => {
                                self.begin_stop(now_us, StopReason::Work { lane, error });
                                break 'work;
                            }
                        }
                    }
                }
                if self.report.phase == LoopPhase::Running {
                    turn.wake_at_us = Some(wake);
                }
            }
            LoopPhase::Stopping => {
                if self.stop_deadline.is_some_and(|end| now_us >= end) {
                    self.report.stop_error.get_or_insert(-2);
                    self.force(StopReason::StopTimeout);
                } else {
                    match self.resources.poll_stop(now_us) {
                        Ok(true) => self.report.phase = LoopPhase::Cleaning,
                        Ok(false) => {
                            turn.wake_at_us = self
                                .stop_deadline
                                .map(|end| now_us.saturating_add(self.config.max_wait_us).min(end));
                        }
                        Err(error) => {
                            self.report.stop_error = Some(error);
                            self.force(StopReason::StopFailed(error));
                        }
                    }
                }
            }
            LoopPhase::Cleaning => {
                if let Some(index) = self.report.acquired.iter().rposition(|owned| *owned) {
                    // Consume before calling; errors and Drop must never retry.
                    self.report.acquired[index] = false;
                    self.report.cleanup[index] = Some(self.resources.cleanup(RESOURCES[index]));
                }
                if !self.report.acquired.iter().any(|owned| *owned) {
                    self.report.phase = LoopPhase::Finished;
                }
            }
            LoopPhase::Finished => {}
        }
        turn.phase = self.report.phase;
        turn
    }
}
impl<R: LoopResources> Drop for MainLoop<R> {
    fn drop(&mut self) {
        if self.report.phase == LoopPhase::Finished {
            return;
        }
        self.force(StopReason::Dropped);
        for (index, resource) in RESOURCES.iter().enumerate().rev() {
            if self.report.acquired[index] {
                self.report.acquired[index] = false;
                self.report.cleanup[index] = Some(self.resources.cleanup(*resource));
            }
        }
        self.report.phase = LoopPhase::Finished;
    }
}
