// SPDX-License-Identifier: GPL-3.0-or-later
//! Allocation-free control state and a narrow authenticated HTTP boundary.
#![no_std]
#![forbid(unsafe_code)]

pub mod http;
pub mod management;
pub mod pump;
pub mod service;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capability {
    Unsupported,
    Unavailable,
    Ready,
}

impl Capability {
    const fn name(self) -> &'static [u8] {
        match self {
            Self::Unsupported => b"unsupported",
            Self::Unavailable => b"unavailable",
            Self::Ready => b"ready",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkState {
    Disconnected,
    Connecting,
    Connected,
}

impl LinkState {
    const fn name(self) -> &'static [u8] {
        match self {
            Self::Disconnected => b"disconnected",
            Self::Connecting => b"connecting",
            Self::Connected => b"connected",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamState {
    Stopped,
    Starting,
    Streaming,
    Stopping,
}

impl StreamState {
    const fn name(self) -> &'static [u8] {
        match self {
            Self::Stopped => b"stopped",
            Self::Starting => b"starting",
            Self::Streaming => b"streaming",
            Self::Stopping => b"stopping",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Status {
    pub transport: Capability,
    pub capture: Capability,
    pub codec: Capability,
    pub playback: Capability,
    pub link: LinkState,
    pub stream: StreamState,
    /// Stable local error category or exact backend status. Never a key/address.
    pub last_error: Option<i32>,
    pub revision: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Connect,
    Start,
    Stop,
}

impl Command {
    const fn name(self) -> &'static [u8] {
        match self {
            Self::Connect => b"connect",
            Self::Start => b"start",
            Self::Stop => b"stop",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PendingCommand {
    pub id: u64,
    pub command: Command,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmitError {
    Busy,
    Unavailable,
    InvalidState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FinishError {
    NoPendingCommand,
    StaleCommand,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandOutcome {
    Success,
    Failed(i32),
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusWriteError {
    BufferTooSmall,
}

/// State shared conceptually with the single Bluetooth owner.
///
/// Submitting changes the visible state to an in-progress state but never runs
/// work. The one transport owner observes `pending`, executes it, then finishes
/// the same ticket exactly once. A second command cannot replace it.
pub struct Control {
    status: Status,
    pending: Option<PendingCommand>,
    next_id: u64,
}

impl Control {
    pub const fn new(status: Status) -> Self {
        Self {
            status,
            pending: None,
            next_id: 1,
        }
    }

    pub const fn status(&self) -> Status {
        self.status
    }

    pub const fn pending(&self) -> Option<PendingCommand> {
        self.pending
    }

    /// Replace observed backend state only while no command is owned.
    pub fn observe(&mut self, mut status: Status) -> Result<(), SubmitError> {
        if self.pending.is_some() {
            return Err(SubmitError::Busy);
        }
        status.revision = self.status.revision.wrapping_add(1);
        self.status = status;
        Ok(())
    }

    pub fn submit(&mut self, command: Command) -> Result<PendingCommand, SubmitError> {
        if self.pending.is_some() {
            return Err(SubmitError::Busy);
        }
        match command {
            Command::Connect => {
                if self.status.transport != Capability::Ready {
                    return Err(SubmitError::Unavailable);
                }
                if self.status.link != LinkState::Disconnected
                    || self.status.stream != StreamState::Stopped
                {
                    return Err(SubmitError::InvalidState);
                }
                self.status.link = LinkState::Connecting;
            }
            Command::Start => {
                if self.status.transport != Capability::Ready
                    || self.status.playback != Capability::Ready
                    || self.status.codec != Capability::Ready
                {
                    return Err(SubmitError::Unavailable);
                }
                if self.status.link != LinkState::Connected
                    || self.status.stream != StreamState::Stopped
                {
                    return Err(SubmitError::InvalidState);
                }
                self.status.stream = StreamState::Starting;
            }
            Command::Stop => {
                if self.status.stream != StreamState::Streaming {
                    return Err(SubmitError::InvalidState);
                }
                self.status.stream = StreamState::Stopping;
            }
        }
        let pending = PendingCommand {
            id: self.next_id,
            command,
        };
        self.next_id = self.next_id.wrapping_add(1);
        self.status.last_error = None;
        self.status.revision = self.status.revision.wrapping_add(1);
        self.pending = Some(pending);
        Ok(pending)
    }

    pub fn finish(
        &mut self,
        pending: PendingCommand,
        outcome: CommandOutcome,
    ) -> Result<(), FinishError> {
        match self.pending {
            None => return Err(FinishError::NoPendingCommand),
            Some(current) if current != pending => return Err(FinishError::StaleCommand),
            Some(_) => {}
        }
        match (pending.command, outcome) {
            (Command::Connect, CommandOutcome::Success) => {
                self.status.link = LinkState::Connected;
            }
            (Command::Connect, CommandOutcome::Failed(code)) => {
                self.status.link = LinkState::Disconnected;
                self.status.last_error = Some(code);
            }
            (Command::Connect, CommandOutcome::Cancelled) => {
                self.status.link = LinkState::Disconnected;
            }
            (Command::Start, CommandOutcome::Success) => {
                self.status.stream = StreamState::Streaming;
            }
            (Command::Start, CommandOutcome::Failed(code)) => {
                self.status.stream = StreamState::Stopped;
                self.status.last_error = Some(code);
            }
            (Command::Start, CommandOutcome::Cancelled) => {
                self.status.stream = StreamState::Stopped;
            }
            (Command::Stop, CommandOutcome::Success) => {
                self.status.stream = StreamState::Stopped;
            }
            (Command::Stop, CommandOutcome::Cancelled) => {
                self.status.stream = StreamState::Streaming;
            }
            (Command::Stop, CommandOutcome::Failed(code)) => {
                // A failed stop is not reported as stopped. The owner must
                // reconcile or tear down before accepting another command.
                self.status.stream = StreamState::Streaming;
                self.status.last_error = Some(code);
            }
        }
        self.status.revision = self.status.revision.wrapping_add(1);
        self.pending = None;
        Ok(())
    }

    /// Write compact JSON made only from closed enums and integers.
    /// Failure is atomic and never exposes addresses, tokens, keys or PCM.
    pub fn write_status(&self, output: &mut [u8]) -> Result<usize, StatusWriteError> {
        let mut length = 0;
        length += b"{\"transport\":\"".len() + self.status.transport.name().len();
        length += b"\",\"capture\":\"".len() + self.status.capture.name().len();
        length += b"\",\"codec\":\"".len() + self.status.codec.name().len();
        length += b"\",\"playback\":\"".len() + self.status.playback.name().len();
        length += b"\",\"link\":\"".len() + self.status.link.name().len();
        length += b"\",\"stream\":\"".len() + self.status.stream.name().len();
        length += b"\",\"pending\":".len();
        length += match self.pending {
            Some(value) => value.command.name().len() + 2,
            None => b"null".len(),
        };
        length += b",\"last_error\":".len();
        length += decimal_i32_len(self.status.last_error);
        length += b",\"revision\":".len() + decimal_u64_len(self.status.revision) + 1;
        if output.len() < length {
            return Err(StatusWriteError::BufferTooSmall);
        }

        let mut writer = Writer::new(output);
        writer.bytes(b"{\"transport\":\"");
        writer.bytes(self.status.transport.name());
        writer.bytes(b"\",\"capture\":\"");
        writer.bytes(self.status.capture.name());
        writer.bytes(b"\",\"codec\":\"");
        writer.bytes(self.status.codec.name());
        writer.bytes(b"\",\"playback\":\"");
        writer.bytes(self.status.playback.name());
        writer.bytes(b"\",\"link\":\"");
        writer.bytes(self.status.link.name());
        writer.bytes(b"\",\"stream\":\"");
        writer.bytes(self.status.stream.name());
        writer.bytes(b"\",\"pending\":");
        if let Some(value) = self.pending {
            writer.bytes(b"\"");
            writer.bytes(value.command.name());
            writer.bytes(b"\"");
        } else {
            writer.bytes(b"null");
        }
        writer.bytes(b",\"last_error\":");
        match self.status.last_error {
            Some(value) => writer.i32(value),
            None => writer.bytes(b"null"),
        }
        writer.bytes(b",\"revision\":");
        writer.u64(self.status.revision);
        writer.bytes(b"}");
        debug_assert_eq!(writer.offset, length);
        Ok(length)
    }
}

fn decimal_u64_len(mut value: u64) -> usize {
    let mut length = 1;
    while value >= 10 {
        value /= 10;
        length += 1;
    }
    length
}

fn decimal_i32_len(value: Option<i32>) -> usize {
    match value {
        None => 4,
        Some(value) if value < 0 => decimal_u64_len(u64::from(value.unsigned_abs())) + 1,
        Some(value) => decimal_u64_len(value as u64),
    }
}

struct Writer<'a> {
    output: &'a mut [u8],
    offset: usize,
}

impl<'a> Writer<'a> {
    fn new(output: &'a mut [u8]) -> Self {
        Self { output, offset: 0 }
    }

    fn bytes(&mut self, bytes: &[u8]) {
        let end = self.offset + bytes.len();
        self.output[self.offset..end].copy_from_slice(bytes);
        self.offset = end;
    }

    fn u64(&mut self, mut value: u64) {
        let length = decimal_u64_len(value);
        let start = self.offset;
        self.offset += length;
        for byte in self.output[start..self.offset].iter_mut().rev() {
            *byte = b'0' + (value % 10) as u8;
            value /= 10;
        }
    }

    fn i32(&mut self, value: i32) {
        if value < 0 {
            self.bytes(b"-");
            self.u64(u64::from(value.unsigned_abs()));
        } else {
            self.u64(value as u64);
        }
    }
}
