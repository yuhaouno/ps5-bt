// SPDX-License-Identifier: GPL-3.0-or-later
//! Persistent one-client-at-a-time HTTP service with per-turn native lending.
use crate::{Diagnostic, Listener};
use bt_control::{
    Control,
    http::Policy,
    management::Manager,
    pump::{Cause, ConnectionState, Failure, Progress, SocketIo},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServiceConfig {
    pub request_timeout_us: u64,
    /// Total accepts, not simultaneous clients; never reset by a client fault.
    pub max_connections: u32,
    /// Optional exclusive total service deadline in the owner's clock domain.
    pub deadline_us: Option<u64>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServiceStop {
    Requested,
    Deadline,
    ConnectionBudget,
    Failure,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ServiceProgress {
    pub progressed: bool,
    pub wake_at_us: Option<u64>,
    pub stopped: bool,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ServiceSnapshot {
    pub active: bool,
    pub stopped: bool,
    pub stop_reason: Option<ServiceStop>,
    pub accepted: u32,
    pub completed: u32,
    pub failed: u32,
    pub last_client: Option<Result<(), Failure>>,
    pub last_failure: Option<Failure>,
    pub listener_error: Option<Diagnostic>,
    pub stop_outcome: Option<Result<(), Failure>>,
}
/// HTTP state owns only fixed external buffers. Accepted socket ownership stays
/// inside Listener, which closes it even if this state is abandoned. Call cancel
/// before dropping the service while retaining its listener; only that explicit
/// path can return cleanup errors. No self-reference or live socket pointer is
/// retained here, and no listener/client token is exposed to HTTP.
pub struct ServiceState<'buf> {
    connection: ConnectionState<'buf>,
    owner: [usize; 2],
    config: ServiceConfig,
    last_now: Option<u64>,
    stop_requested: bool,
    snapshot: ServiceSnapshot,
}
fn failure(cause: Cause) -> Failure {
    Failure {
        cause,
        cleanup: None,
    }
}
fn native_failure(cause: impl FnOnce(i32) -> Cause, r: Diagnostic) -> Failure {
    let code = if r.raw_errno > 0 {
        r.raw_errno
    } else {
        r.status
    };
    Failure {
        cause: cause(code),
        cleanup: if r.flags != 0 || r.cleanup_errno != 0 {
            Some(if r.cleanup_errno > 0 {
                r.cleanup_errno
            } else {
                -1000
            })
        } else {
            None
        },
    }
}
impl<'buf> ServiceState<'buf> {
    pub fn new(
        config: ServiceConfig,
        request: &'buf mut [u8],
        response: &'buf mut [u8],
    ) -> Result<Self, Failure> {
        if config.request_timeout_us == 0 || config.max_connections == 0 {
            request.fill(0);
            response.fill(0);
            return Err(failure(Cause::InvalidConfig));
        }
        let owner = [request.as_ptr() as usize, response.as_ptr() as usize];
        let connection = ConnectionState::new(request, response)?;
        Ok(Self {
            connection,
            owner,
            config,
            last_now: None,
            stop_requested: false,
            snapshot: ServiceSnapshot::default(),
        })
    }
    pub const fn snapshot(&self) -> ServiceSnapshot {
        self.snapshot
    }
    /// Clock-independent priority latch: the next step cancels before accepting
    /// or reading any bytes, so a buffered mutating request cannot be admitted.
    pub fn request_stop(&mut self) {
        self.stop_requested = true;
    }
    fn progress(&self, progressed: bool) -> ServiceProgress {
        let mut wake = self.connection.deadline_us();
        if let Some(end) = self.config.deadline_us {
            wake = Some(wake.map_or(end, |due| due.min(end)));
        }
        ServiceProgress {
            progressed,
            wake_at_us: if self.snapshot.stopped { None } else { wake },
            stopped: self.snapshot.stopped,
        }
    }
    fn remember(&mut self, outcome: Result<(), Failure>) {
        self.snapshot.active = false;
        self.snapshot.last_client = Some(outcome);
        if outcome.is_ok() {
            self.snapshot.completed += 1;
        } else {
            self.snapshot.failed += 1;
            self.snapshot.last_failure = outcome.err();
        }
    }
    fn terminal<const N: usize>(
        &mut self,
        listener: &mut Listener<'_, N>,
        cause: Cause,
        reason: ServiceStop,
    ) -> Failure {
        let cleanup = self.cancel(listener).err().and_then(|error| error.cleanup);
        let error = Failure { cause, cleanup };
        self.snapshot.stop_reason = Some(reason);
        self.snapshot.stop_outcome = Some(Err(error));
        error
    }
    /// One accept OR one HTTP read/write, followed by required owned cleanup.
    /// Ordinary per-client errors are retained and the next step may reaccept;
    /// uncertain cleanup, listener faults and clock failure stop the service.
    pub fn step<const L: usize, const M: usize>(
        &mut self,
        listener: &mut Listener<'_, L>,
        now: u64,
        policy: Policy<'_>,
        legacy: &mut Control,
        manager: &mut Manager<M>,
    ) -> Result<ServiceProgress, Failure> {
        if self.snapshot.stopped {
            return self
                .snapshot
                .stop_outcome
                .unwrap_or(Ok(()))
                .map(|()| self.progress(false));
        }
        if self.stop_requested {
            self.cancel(listener)?;
            return Ok(self.progress(false));
        }
        if self.last_now.is_some_and(|previous| now < previous) {
            return Err(self.terminal(listener, Cause::ClockRegression, ServiceStop::Failure));
        }
        self.last_now = Some(now);
        if self.config.deadline_us.is_some_and(|end| now >= end) {
            return Err(self.terminal(listener, Cause::Timeout, ServiceStop::Deadline));
        }
        let native_active = match listener.managed_for(self.owner) {
            Ok(active) => active,
            Err(record) => {
                self.snapshot.listener_error = Some(record);
                return Err(self.terminal(listener, Cause::ListenerChanged, ServiceStop::Failure));
            }
        };
        if native_active != self.snapshot.active {
            return Err(self.terminal(listener, Cause::ListenerChanged, ServiceStop::Failure));
        }
        if !self.snapshot.active {
            if self.snapshot.accepted >= self.config.max_connections {
                self.snapshot.stopped = true;
                self.snapshot.stop_reason = Some(ServiceStop::ConnectionBudget);
                self.snapshot.stop_outcome = Some(Ok(()));
                return Ok(self.progress(false));
            }
            match listener.accept_managed(self.owner) {
                Ok(false) => return Ok(self.progress(false)),
                Ok(true) => {
                    self.snapshot.active = true;
                    self.snapshot.accepted += 1;
                    if let Err(error) = self.connection.begin(now, self.config.request_timeout_us) {
                        return Err(self.terminal(listener, error.cause, ServiceStop::Failure));
                    }
                    return Ok(self.progress(true));
                }
                Err(record) => {
                    self.snapshot.listener_error = Some(record);
                    let error = native_failure(Cause::Accept, record);
                    self.snapshot.stopped = true;
                    self.snapshot.stop_reason = Some(ServiceStop::Failure);
                    self.snapshot.stop_outcome = Some(Err(error));
                    return Err(error);
                }
            }
        }
        let before = self.connection.transferred_bytes();
        let result = listener.with_managed(self.owner, |io| {
            self.connection.step(io, now, policy, legacy, manager)
        });
        let outcome = match result {
            Ok(result) => result,
            Err(record) => {
                self.snapshot.listener_error = Some(record);
                return Err(self.terminal(listener, Cause::ListenerChanged, ServiceStop::Failure));
            }
        };
        match outcome {
            Ok(Progress::Closed) => self.remember(Ok(())),
            Ok(_) => return Ok(self.progress(self.connection.transferred_bytes() != before)),
            Err(error) => {
                self.remember(Err(error));
                if error.cleanup.is_some() || error.cause == Cause::ClockRegression {
                    self.snapshot.stopped = true;
                    self.snapshot.stop_reason = Some(ServiceStop::Failure);
                    self.snapshot.stop_outcome = Some(Err(error));
                    return Err(error);
                }
            }
        }
        Ok(self.progress(true))
    }
    /// Latch stop first, then consume the one managed client at most once.
    /// A clean explicit cancellation is normal service shutdown; the original
    /// per-client Cancelled outcome remains in snapshot. Listener cleanup belongs
    /// to the enclosing factory and is never guessed from HTTP state alone.
    pub fn cancel<const N: usize>(
        &mut self,
        listener: &mut Listener<'_, N>,
    ) -> Result<(), Failure> {
        self.stop_requested = true;
        if self.snapshot.stopped {
            return self.snapshot.stop_outcome.unwrap_or(Ok(()));
        }
        self.snapshot.stopped = true;
        self.snapshot.stop_reason = Some(ServiceStop::Requested);
        let result = if self.snapshot.active {
            let result = listener.with_managed(self.owner, |io| {
                if self.connection.is_closed() {
                    io.close().map_err(|code| Failure {
                        cause: Cause::Close(code),
                        cleanup: Some(code),
                    })
                } else {
                    self.connection.cancel(io)
                }
            });
            match result {
                Ok(outcome) => {
                    self.remember(outcome);
                    match outcome {
                        Err(error)
                            if error.cause == Cause::Cancelled && error.cleanup.is_none() =>
                        {
                            Ok(())
                        }
                        other => other,
                    }
                }
                Err(record) => {
                    self.snapshot.listener_error = Some(record);
                    Err(native_failure(|_| Cause::ListenerChanged, record))
                }
            }
        } else {
            Ok(())
        };
        self.snapshot.stop_outcome = Some(result);
        result
    }
}
