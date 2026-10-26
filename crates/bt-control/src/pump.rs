// SPDX-License-Identifier: GPL-3.0-or-later
//! A single owned nonblocking connection, fixed deadline and caller buffers.
use crate::{
    Control,
    http::{MAX_REQUEST_BYTES, Policy, RequestError},
    management::Manager,
    service,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IoError {
    WouldBlock,
    Failed(i32),
}
/// Implement using a reviewed nonblocking socket adapter. Calls must return
/// promptly, never retain slices, and report exactly the consumed byte count.
/// read(0) is EOF; write WouldBlock consumes nothing; close consumes ownership
/// even on failure. This trait does not open, bind, accept or guess PS5 APIs.
pub trait SocketIo {
    fn read(&mut self, output: &mut [u8]) -> Result<usize, IoError>;
    fn write(&mut self, bytes: &[u8]) -> Result<usize, IoError>;
    fn close(&mut self) -> Result<(), i32>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cause {
    InvalidConfig,
    DeadlineOverflow,
    ClockRegression,
    Timeout,
    Read(i32),
    Write(i32),
    Close(i32),
    Eof,
    WriteZero,
    InvalidCount,
    ResponseCapacity,
    Cancelled,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Failure {
    pub cause: Cause,
    pub cleanup: Option<i32>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Progress {
    Reading,
    Writing { remaining: usize },
    Closed,
}
enum State {
    Reading,
    Writing,
    Closed,
}

pub struct ConnectionPump<'buf, I: SocketIo> {
    io: I,
    request: &'buf mut [u8],
    response: &'buf mut [u8],
    read: usize,
    written: usize,
    response_len: usize,
    state: State,
    last_us: u64,
    deadline: u64,
    outcome: Option<Result<(), Failure>>,
}
impl<'buf, I: SocketIo> ConnectionPump<'buf, I> {
    /// Takes one already accepted socket, closing it even on invalid construction.
    pub fn new(
        mut io: I,
        now: u64,
        timeout_us: u64,
        request: &'buf mut [u8],
        response: &'buf mut [u8],
    ) -> Result<Self, Failure> {
        let cause = if timeout_us == 0
            || request.len() < MAX_REQUEST_BYTES
            || response.len() < service::MIN_RESPONSE_CAPACITY
        {
            Some(Cause::InvalidConfig)
        } else if now.checked_add(timeout_us).is_none() {
            Some(Cause::DeadlineOverflow)
        } else {
            None
        };
        if let Some(cause) = cause {
            request.fill(0);
            response.fill(0);
            return Err(Failure {
                cause,
                cleanup: io.close().err(),
            });
        }
        Ok(Self {
            io,
            request,
            response,
            read: 0,
            written: 0,
            response_len: 0,
            state: State::Reading,
            last_us: now,
            deadline: now + timeout_us,
            outcome: None,
        })
    }
    pub const fn outcome(&self) -> Option<Result<(), Failure>> {
        self.outcome
    }
    pub fn is_closed(&self) -> bool {
        matches!(self.state, State::Closed)
    }
    /// At most one read or write syscall per step; no internal retry/sleep loop.
    pub fn step<const N: usize>(
        &mut self,
        now: u64,
        policy: Policy<'_>,
        legacy: &mut Control,
        manager: &mut Manager<N>,
    ) -> Result<Progress, Failure> {
        if let Some(outcome) = self.outcome {
            return outcome.map(|()| Progress::Closed);
        }
        if now < self.last_us {
            return self.fail(Cause::ClockRegression);
        }
        self.last_us = now;
        if now >= self.deadline {
            return self.fail(Cause::Timeout);
        }
        match self.state {
            State::Reading => {
                let remaining = MAX_REQUEST_BYTES - self.read;
                let count = match self
                    .io
                    .read(&mut self.request[self.read..MAX_REQUEST_BYTES])
                {
                    Ok(0) => return self.fail(Cause::Eof),
                    Ok(count) if count <= remaining => count,
                    Ok(_) => return self.fail(Cause::InvalidCount),
                    Err(IoError::WouldBlock) => return Ok(Progress::Reading),
                    Err(IoError::Failed(code)) => return self.fail(Cause::Read(code)),
                };
                self.read += count;
                let complete = self.request[..self.read]
                    .windows(4)
                    .any(|w| w == b"\r\n\r\n");
                if !complete && self.read < MAX_REQUEST_BYTES {
                    return Ok(Progress::Reading);
                }
                let result = if complete {
                    service::handle(
                        &self.request[..self.read],
                        policy,
                        legacy,
                        manager,
                        self.response,
                    )
                } else {
                    service::error_response(RequestError::TooLarge, self.response)
                };
                match result {
                    Ok(response) => self.response_len = response.len,
                    Err(_) => return self.fail(Cause::ResponseCapacity),
                }
                self.state = State::Writing;
                Ok(Progress::Writing {
                    remaining: self.response_len,
                })
            }
            State::Writing => {
                let remaining = self.response_len - self.written;
                match self
                    .io
                    .write(&self.response[self.written..self.response_len])
                {
                    Ok(0) => return self.fail(Cause::WriteZero),
                    Ok(count) if count <= remaining => self.written += count,
                    Ok(_) => return self.fail(Cause::InvalidCount),
                    Err(IoError::WouldBlock) => return Ok(Progress::Writing { remaining }),
                    Err(IoError::Failed(code)) => return self.fail(Cause::Write(code)),
                }
                if self.written == self.response_len {
                    self.state = State::Closed;
                    let result = self.io.close().map_err(|code| Failure {
                        cause: Cause::Close(code),
                        cleanup: Some(code),
                    });
                    self.request.fill(0);
                    self.response.fill(0);
                    self.outcome = Some(result);
                    result.map(|()| Progress::Closed)
                } else {
                    Ok(Progress::Writing {
                        remaining: self.response_len - self.written,
                    })
                }
            }
            State::Closed => Ok(Progress::Closed),
        }
    }
    pub fn cancel(&mut self) -> Result<(), Failure> {
        if let Some(outcome) = self.outcome {
            return outcome;
        }
        self.fail::<()>(Cause::Cancelled)
    }
    fn fail<T>(&mut self, cause: Cause) -> Result<T, Failure> {
        self.state = State::Closed;
        let failure = Failure {
            cause,
            cleanup: self.io.close().err(),
        };
        self.request.fill(0);
        self.response.fill(0);
        self.outcome = Some(Err(failure));
        Err(failure)
    }
}
impl<I: SocketIo> Drop for ConnectionPump<'_, I> {
    fn drop(&mut self) {
        if !matches!(self.state, State::Closed) {
            let _ = self.io.close();
            self.request.fill(0);
            self.response.fill(0);
            self.state = State::Closed;
        }
    }
}
