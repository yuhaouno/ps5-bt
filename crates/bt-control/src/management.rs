// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded protected management state. The transport owner supplies observations.
use crate::{CommandOutcome, Status};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Generation {
    pub session: u64,
    pub revision: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Name {
    bytes: [u8; 64],
    len: u8,
}
impl Name {
    pub fn new(value: &str) -> Result<Self, Error> {
        if value.len() > 64 {
            return Err(Error::Invalid);
        }
        let mut name = Self {
            bytes: [0; 64],
            len: value.len() as u8,
        };
        name.bytes[..value.len()].copy_from_slice(value.as_bytes());
        Ok(name)
    }
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..usize::from(self.len)]).expect("validated UTF-8")
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Device {
    pub id: DeviceId,
    pub name: Name,
    /// None means the owner has not established protected-store metadata.
    pub bonded: Option<bool>,
    pub audio_sink: Option<bool>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Approval {
    pub token: u64,
    pub device: DeviceId,
    pub number: Option<u32>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Scan,
    CancelScan,
    Select(DeviceId),
    Pair,
    Approve { token: u64 },
    Reject { token: u64 },
    Connect,
    Start,
    Stop,
    Disconnect,
    Forget,
}
impl Action {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Scan => "scan",
            Self::CancelScan => "cancel-scan",
            Self::Select(_) => "select",
            Self::Pair => "pair",
            Self::Approve { .. } => "approve",
            Self::Reject { .. } => "reject",
            Self::Connect => "connect",
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Disconnect => "disconnect",
            Self::Forget => "forget",
        }
    }
    const fn bit(self) -> u16 {
        1 << match self {
            Self::Scan => 0,
            Self::CancelScan => 1,
            Self::Select(_) => 2,
            Self::Pair => 3,
            Self::Approve { .. } => 4,
            Self::Reject { .. } => 5,
            Self::Connect => 6,
            Self::Start => 7,
            Self::Stop => 8,
            Self::Disconnect => 9,
            Self::Forget => 10,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Actions(u16);
impl Actions {
    pub const fn none() -> Self {
        Self(0)
    }
    pub const fn with(self, action: Action) -> Self {
        Self(self.0 | action.bit())
    }
    pub const fn contains(self, action: Action) -> bool {
        self.0 & action.bit() != 0
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnerPhase {
    Idle,
    Scanning,
    Connecting,
    Pairing,
    Connected,
    Starting,
    Streaming,
    Stopping,
    Disconnecting,
    RequiresCleanup,
    Failed,
}
impl OwnerPhase {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Scanning => "scanning",
            Self::Connecting => "connecting",
            Self::Pairing => "pairing",
            Self::Connected => "connected",
            Self::Starting => "starting",
            Self::Streaming => "streaming",
            Self::Stopping => "stopping",
            Self::Disconnecting => "disconnecting",
            Self::RequiresCleanup => "requires-cleanup",
            Self::Failed => "failed",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub status: Status,
    pub owner_phase: OwnerPhase,
    pub cleanup_required: bool,
    pub scanning: bool,
    pub selected: Option<DeviceId>,
    pub approval: Option<Approval>,
    /// Actual backend capabilities/state, supplied by the single owner.
    pub available: Actions,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ticket {
    pub id: u64,
    pub generation: Generation,
    pub action: Action,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Owned {
    pub ticket: Ticket,
    pub claimed: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Completion {
    pub ticket: Ticket,
    pub outcome: CommandOutcome,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Invalid,
    Capacity,
    Busy,
    StaleGeneration,
    Unavailable,
    StaleTicket,
    NotClaimed,
    Exhausted,
}

pub struct Manager<const N: usize> {
    snapshot: Snapshot,
    devices: [Option<Device>; N],
    len: usize,
    generation: Generation,
    next_id: u64,
    regular: Option<Owned>,
    urgent: Option<Owned>,
    completion: Option<Completion>,
}
impl<const N: usize> Manager<N> {
    /// `session` must be nonzero and never reused by the owning service instance.
    pub fn new(session: u64, snapshot: Snapshot) -> Result<Self, Error> {
        if session == 0 || snapshot.selected.is_some() || snapshot.approval.is_some() {
            return Err(Error::Invalid);
        }
        Ok(Self {
            snapshot,
            devices: [None; N],
            len: 0,
            generation: Generation {
                session,
                revision: 0,
            },
            next_id: 1,
            regular: None,
            urgent: None,
            completion: None,
        })
    }
    pub const fn generation(&self) -> Generation {
        self.generation
    }
    pub const fn snapshot(&self) -> Snapshot {
        self.snapshot
    }
    pub fn devices(&self) -> impl Iterator<Item = &Device> {
        self.devices[..self.len].iter().flatten()
    }
    pub const fn pending(&self) -> Option<Owned> {
        self.regular
    }
    pub const fn urgent(&self) -> Option<Owned> {
        self.urgent
    }
    pub const fn completion(&self) -> Option<Completion> {
        self.completion
    }
    /// Refresh observations even while a command is owned. Its ticket stays valid.
    pub fn observe(&mut self, snapshot: Snapshot, devices: &[Device]) -> Result<(), Error> {
        if devices.len() > N {
            return Err(Error::Capacity);
        }
        for (index, device) in devices.iter().enumerate() {
            if device.id.0 == 0 || devices[..index].iter().any(|old| old.id == device.id) {
                return Err(Error::Invalid);
            }
        }
        let selected = snapshot
            .selected
            .and_then(|id| devices.iter().find(|d| d.id == id));
        if snapshot.selected.is_some() && selected.is_none() {
            return Err(Error::Invalid);
        }
        if snapshot.approval.is_some_and(|a| {
            a.token == 0
                || Some(a.device) != snapshot.selected
                || a.number.is_some_and(|n| n > 999999)
        }) {
            return Err(Error::Invalid);
        }
        if self.snapshot == snapshot
            && self.len == devices.len()
            && self.devices().copied().eq(devices.iter().copied())
        {
            return Ok(());
        }
        let revision = self.next_revision()?;
        self.devices.fill(None);
        for (slot, device) in self.devices.iter_mut().zip(devices) {
            *slot = Some(*device);
        }
        self.len = devices.len();
        self.snapshot = snapshot;
        self.generation.revision = revision;
        Ok(())
    }
    fn next_revision(&self) -> Result<u64, Error> {
        self.generation
            .revision
            .checked_add(1)
            .ok_or(Error::Exhausted)
    }
    fn is_urgent(&self, action: Action) -> bool {
        let Some(current) = self.regular else {
            return false;
        };
        matches!(
            (action, current.ticket.action),
            (Action::CancelScan, Action::Scan)
                | (Action::Stop, Action::Start)
                | (
                    Action::Disconnect,
                    Action::Connect | Action::Pair | Action::Start
                )
                | (
                    Action::Approve { .. } | Action::Reject { .. },
                    Action::Pair | Action::Connect
                )
        )
    }
    pub fn can_submit(&self, action: Action) -> bool {
        self.check_action(action).is_ok()
    }
    fn check_action(&self, action: Action) -> Result<(), Error> {
        if !self.snapshot.available.contains(action) {
            return Err(Error::Unavailable);
        }
        if self.urgent.is_some() || (self.regular.is_some() && !self.is_urgent(action)) {
            return Err(Error::Busy);
        }
        match action {
            Action::Select(id) if !self.devices().any(|d| d.id == id) => {
                return Err(Error::Invalid);
            }
            Action::Pair
                if !self
                    .devices()
                    .any(|d| Some(d.id) == self.snapshot.selected && d.bonded == Some(false)) =>
            {
                return Err(Error::Unavailable);
            }
            Action::Connect | Action::Forget if self.snapshot.selected.is_none() => {
                return Err(Error::Unavailable);
            }
            Action::Approve { token } | Action::Reject { token }
                if self.snapshot.approval.is_none_or(|a| a.token != token) =>
            {
                return Err(Error::Invalid);
            }
            _ => {}
        }
        Ok(())
    }
    pub fn submit(&mut self, expected: Generation, action: Action) -> Result<Ticket, Error> {
        if expected != self.generation {
            return Err(Error::StaleGeneration);
        }
        self.check_action(action)?;
        let next_id = self.next_id.checked_add(1).ok_or(Error::Exhausted)?;
        let revision = self.next_revision()?;
        let ticket = Ticket {
            id: self.next_id,
            generation: self.generation,
            action,
        };
        let urgent = self.is_urgent(action);
        if urgent {
            self.urgent = Some(Owned {
                ticket,
                claimed: false,
            });
        } else {
            self.regular = Some(Owned {
                ticket,
                claimed: false,
            });
        }
        self.next_id = next_id;
        self.generation.revision = revision;
        Ok(ticket)
    }
    /// Claims an urgent command first. Repeated calls cannot execute it twice.
    pub fn take_pending(&mut self) -> Option<Ticket> {
        for slot in [&mut self.urgent, &mut self.regular].into_iter().flatten() {
            if !slot.claimed {
                slot.claimed = true;
                return Some(slot.ticket);
            }
        }
        None
    }
    /// Finish only after the owner has a terminal result, including cleanup.
    /// `snapshot` is the owner's actual current state, never an optimistic state.
    pub fn finish(
        &mut self,
        ticket: Ticket,
        outcome: CommandOutcome,
        mut snapshot: Snapshot,
    ) -> Result<(), Error> {
        let slot = if self.urgent.is_some_and(|o| o.ticket == ticket) {
            &mut self.urgent
        } else if self.regular.is_some_and(|o| o.ticket == ticket) {
            &mut self.regular
        } else {
            return Err(Error::StaleTicket);
        };
        if !slot.is_some_and(|o| o.claimed) {
            return Err(Error::NotClaimed);
        }
        // Validate before relinquishing ownership, preserving atomic rejection.
        if snapshot.selected.is_some_and(|id| {
            !self.devices[..self.len]
                .iter()
                .flatten()
                .any(|d| d.id == id)
        }) || snapshot.approval.is_some_and(|a| {
            a.token == 0
                || Some(a.device) != snapshot.selected
                || a.number.is_some_and(|n| n > 999999)
        }) {
            return Err(Error::Invalid);
        }
        let revision = self
            .generation
            .revision
            .checked_add(1)
            .ok_or(Error::Exhausted)?;
        if let CommandOutcome::Failed(code) = outcome {
            snapshot.status.last_error = Some(code);
        }
        *slot = None;
        self.snapshot = snapshot;
        self.generation.revision = revision;
        self.completion = Some(Completion { ticket, outcome });
        Ok(())
    }
}
