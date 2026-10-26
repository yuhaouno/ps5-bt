// SPDX-License-Identifier: GPL-3.0-or-later
use crate::wire::CompletionResult;
use crate::{Event, Peer, SecretKey, WireError};
use bt_hci::ConnectionHandle;

const CREATE: u16 = 0x0405;
const DISCONNECT: u16 = 0x0406;
const CANCEL: u16 = 0x0408;
const KEY_REPLY: u16 = 0x040b;
const KEY_NEGATIVE: u16 = 0x040c;
const PIN_NEGATIVE: u16 = 0x040e;
const AUTH: u16 = 0x0411;
const ENCRYPT: u16 = 0x0413;
const IO_REPLY: u16 = 0x042b;
const CONFIRM: u16 = 0x042c;
const CONFIRM_NEGATIVE: u16 = 0x042d;
const PASSKEY_NEGATIVE: u16 = 0x042f;
const OOB_NEGATIVE: u16 = 0x0433;
const KEY_SIZE: u16 = 0x1408;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Connection,
    Security,
    Cleanup,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Connecting,
    Authenticating,
    Encrypting,
    CheckingKeySize,
    Ready,
    Stopping,
    Cancelling,
    Disconnecting,
    Closed,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    Controller { opcode: u16, status: u8 },
    ControllerEvent { event: u8, status: u8 },
    Timeout(Phase),
    Transport(bt_core::Error),
    EncryptionDisabled,
    UnprovenKey,
    WeakKey(u8),
    UnsupportedKeyType(u8),
    UnsupportedPairing(u8),
    UserRejected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidConfig,
    InvalidState,
    CommandBusy,
    OutputTooSmall,
    WrongTicket,
    CommandNotAccepted,
    ForeignEpoch,
    ForeignPeer,
    ForeignHandle,
    UnexpectedCommand,
    UnexpectedEvent,
    StaleRequest,
    ClockRegression,
    CounterExhausted,
    Wire(WireError),
    Failed(Failure),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandTicket {
    epoch: u64,
    serial: u32,
    opcode: u16,
}
impl CommandTicket {
    pub const fn opcode(self) -> u16 {
        self.opcode
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequestId {
    epoch: u64,
    serial: u32,
}

/// All values are caller policy, not discovered controller capabilities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// Only types actually supported by this controller may be enabled.
    pub packet_types: u16,
    /// Most recent discovery result: R0=0, R1=1, R2=2.
    pub page_scan_repetition: u8,
    pub allow_role_switch: bool,
    /// Finite budgets in monotonic microseconds. Security spans auth + encryption.
    pub connection_timeout_us: u64,
    pub security_timeout_us: u64,
    pub cleanup_timeout_us: u64,
    /// 7..=16 bytes. This verifies size, not MITM or Secure Connections support.
    pub minimum_key_size: u8,
}

/// No Debug: a bond notification owns key material. ProvisionalBond must remain
/// protected and uncommitted to a durable bond store until this session is Ready.
pub enum Notice {
    None,
    NeedKey {
        request: RequestId,
    },
    NeedConfirmation {
        request: RequestId,
        numeric_value: u32,
    },
    ProvisionalBond {
        key: SecretKey,
        key_type: u8,
    },
    Ready,
    Closed {
        reason: u8,
    },
    PairingRejected {
        event: u8,
    },
    CancellationRace {
        status: u8,
    },
}

struct Pending {
    ticket: CommandTicket,
    bytes: [u8; 25],
    len: usize,
    accepted: bool,
    expected_handle: Option<ConnectionHandle>,
}
impl Drop for Pending {
    fn drop(&mut self) {
        self.bytes.fill(0);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Challenge {
    Key(RequestId),
    Confirmation(RequestId),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum KeyAuthorization {
    None,
    Stored,
    NewSsp,
}

/// Caller-driven single-peer initiator. Provides no HCI ownership or credits.
/// The enclosing dispatcher must retain/retry a frame rejected with CommandBusy,
/// and must attach an externally established epoch before receive (not on wire).
pub struct LinkSession {
    peer: Peer,
    epoch: u64,
    config: Config,
    state: State,
    handle: Option<ConnectionHandle>,
    pending: Option<Pending>,
    challenge: Option<Challenge>,
    serial: u32,
    request_serial: u32,
    last_us: u64,
    deadline_us: u64,
    phase: Phase,
    stopping: bool,
    failure: Option<Failure>,
    confirmation_approved: bool,
    pairing_seen: bool,
    connecting_on_controller: bool,
    transport_uncertain: bool,
    key_authorization: KeyAuthorization,
}

impl LinkSession {
    pub fn new(peer: Peer, epoch: u64, now_us: u64, config: Config) -> Result<Self, Error> {
        // Bits absent from this mask are reserved. Bit 3 is ignored by Core.
        if epoch == 0
            || config.packet_types & !0xff1e != 0
            || config.page_scan_repetition > 2
            || !(7..=16).contains(&config.minimum_key_size)
            || config.connection_timeout_us == 0
            || config.security_timeout_us == 0
            || config.cleanup_timeout_us == 0
        {
            return Err(Error::InvalidConfig);
        }
        let deadline_us = now_us
            .checked_add(config.connection_timeout_us)
            .ok_or(Error::InvalidConfig)?;
        // Reject impossible later budget additions now rather than after radio I/O.
        deadline_us
            .checked_add(config.security_timeout_us)
            .and_then(|n| n.checked_add(config.cleanup_timeout_us))
            .ok_or(Error::InvalidConfig)?;
        let mut s = Self {
            peer,
            epoch,
            config,
            state: State::Connecting,
            handle: None,
            pending: None,
            challenge: None,
            serial: 0,
            request_serial: 0,
            last_us: now_us,
            deadline_us,
            phase: Phase::Connection,
            stopping: false,
            failure: None,
            confirmation_approved: false,
            pairing_seen: false,
            connecting_on_controller: false,
            transport_uncertain: false,
            key_authorization: KeyAuthorization::None,
        };
        let mut p = [0; 13];
        p[..6].copy_from_slice(&peer.hci_bytes());
        p[6..8].copy_from_slice(&config.packet_types.to_le_bytes());
        p[8] = config.page_scan_repetition;
        // Reserved and clock offset remain zero (no invented valid clock offset).
        p[12] = u8::from(config.allow_role_switch);
        s.queue(CREATE, &p)?;
        Ok(s)
    }

    pub const fn state(&self) -> State {
        self.state
    }
    pub const fn failure(&self) -> Option<Failure> {
        self.failure
    }
    /// Known handle even during failed cleanup; it is not permission to send ACL.
    pub const fn owned_connection(&self) -> Option<(ConnectionHandle, u64)> {
        match self.handle {
            Some(h) => Some((h, self.epoch)),
            None => None,
        }
    }
    pub fn ready_connection(&self) -> Option<(ConnectionHandle, u64)> {
        if self.state == State::Ready && self.failure.is_none() {
            self.owned_connection()
        } else {
            None
        }
    }
    pub fn has_pending_command(&self) -> bool {
        self.pending.is_some()
    }

    /// Repeated preparation is byte-identical; Busy means retain and retry.
    /// Output may contain a bond key and must never enter traces or HTTP status.
    pub fn prepare_command(&self, output: &mut [u8]) -> Result<Option<CommandTicket>, Error> {
        if self.transport_uncertain {
            return Err(Error::Failed(self.failure.ok_or(Error::InvalidState)?));
        }
        if self.state == State::Failed {
            return Err(Error::Failed(self.failure.ok_or(Error::InvalidState)?));
        }
        let Some(p) = &self.pending else {
            return Ok(None);
        };
        if p.accepted {
            return Ok(None);
        }
        if output.len() < p.len {
            return Err(Error::OutputTooSmall);
        }
        output[..p.len].copy_from_slice(&p.bytes[..p.len]);
        Ok(Some(p.ticket))
    }

    pub fn command_len(&self) -> Option<usize> {
        self.pending.as_ref().filter(|p| !p.accepted).map(|p| p.len)
    }

    /// Invoke only after the complete command has entered owned transport storage.
    /// Controller acceptance is the separate correlated Status/Complete event.
    pub fn confirm_accepted(&mut self, ticket: CommandTicket) -> Result<(), Error> {
        let p = self.pending.as_mut().ok_or(Error::WrongTicket)?;
        if p.ticket != ticket || p.accepted {
            return Err(Error::WrongTicket);
        }
        p.accepted = true;
        if ticket.opcode == CREATE {
            self.connecting_on_controller = true;
        }
        // Accepted storage belongs to transport now; retain correlation only.
        p.bytes.fill(0);
        Ok(())
    }

    pub fn tick(&mut self, now_us: u64) -> Result<(), Error> {
        self.observe(now_us)
    }

    /// Non-Busy send errors have uncertain acceptance. No retry or credit refill.
    pub fn fail_transport(&mut self, error: bt_core::Error) -> Result<(), Error> {
        if error == bt_core::Error::Busy {
            return Err(Error::InvalidState);
        }
        self.transport_uncertain = true;
        self.fail(Failure::Transport(error))
    }

    pub fn supply_key(
        &mut self,
        request: RequestId,
        now_us: u64,
        key: Option<SecretKey>,
    ) -> Result<(), Error> {
        self.observe(now_us)?;
        if self.challenge != Some(Challenge::Key(request)) {
            return Err(Error::StaleRequest);
        }
        let mut p = [0; 22];
        p[..6].copy_from_slice(&self.peer.hci_bytes());
        let has_key = key.is_some();
        let result = if let Some(key) = key {
            p[6..].copy_from_slice(key.expose());
            self.queue(KEY_REPLY, &p)
        } else {
            self.queue(KEY_NEGATIVE, &p[..6])
        };
        p.fill(0);
        result?;
        self.challenge = None;
        self.key_authorization = if has_key {
            KeyAuthorization::Stored
        } else {
            KeyAuthorization::None
        };
        Ok(())
    }

    pub fn approve(
        &mut self,
        request: RequestId,
        now_us: u64,
        approved: bool,
    ) -> Result<(), Error> {
        self.observe(now_us)?;
        if self.challenge != Some(Challenge::Confirmation(request)) {
            return Err(Error::StaleRequest);
        }
        self.queue_peer(if approved { CONFIRM } else { CONFIRM_NEGATIVE })?;
        self.challenge = None;
        self.confirmation_approved = approved;
        if !approved {
            self.failure = Some(Failure::UserRejected);
        }
        Ok(())
    }

    /// One cleanup episode; repeated cancellation never extends its deadline.
    /// A pending accepted command must first receive its matching acknowledgement.
    /// If it never does, cleanup times out with state/handle retained for diagnosis.
    pub fn cancel(&mut self, now_us: u64) -> Result<(), Error> {
        self.check_clock(now_us)?;
        if self.transport_uncertain {
            return Err(Error::Failed(self.failure.ok_or(Error::InvalidState)?));
        }
        if self.state == State::Closed {
            return Ok(());
        }
        if self.stopping {
            return Ok(());
        }
        self.deadline_us = now_us
            .checked_add(self.config.cleanup_timeout_us)
            .ok_or(Error::InvalidConfig)?;
        self.phase = Phase::Cleanup;
        self.stopping = true;
        self.state = State::Stopping;
        self.challenge = None;
        if self.pending.as_ref().is_some_and(|p| !p.accepted) {
            let never_created = self
                .pending
                .as_ref()
                .is_some_and(|p| p.ticket.opcode == CREATE);
            self.pending = None;
            if never_created {
                self.state = State::Closed;
                return Ok(());
            }
        }
        self.drive_stop()
    }

    pub fn receive(&mut self, epoch: u64, now_us: u64, packet: &[u8]) -> Result<Notice, Error> {
        if epoch != self.epoch {
            return Err(Error::ForeignEpoch);
        }
        let event = Event::parse(packet).map_err(Error::Wire)?;
        self.observe(now_us)?;
        match event {
            Event::CommandStatus { status, opcode, .. } => {
                // Unsupported opcodes may report Unknown HCI Command using
                // either acknowledgement kind (Core Vol 4 E 4.5).
                if !matches!(opcode, CREATE | AUTH | ENCRYPT | DISCONNECT) && status != 1 {
                    return Err(Error::UnexpectedCommand);
                }
                self.ack(opcode)?;
                if status != 0 {
                    if opcode == CREATE {
                        self.connecting_on_controller = false;
                    }
                    return self.fail(Failure::Controller { opcode, status });
                }
                if self.stopping {
                    self.drive_stop()?;
                }
                Ok(Notice::None)
            }
            Event::CommandComplete {
                opcode,
                status,
                result,
                ..
            } => {
                if matches!(opcode, CREATE | AUTH | ENCRYPT | DISCONNECT) && status != 1 {
                    return Err(Error::UnexpectedCommand);
                }
                self.check_ack(opcode)?;
                // Mandatory routing parameters remain valid even on failure.
                match result {
                    CompletionResult::Peer(p) => self.check_peer(p)?,
                    CompletionResult::EncryptionKeySize { handle, .. } => {
                        if self.pending.as_ref().and_then(|p| p.expected_handle) != Some(handle) {
                            return Err(Error::ForeignHandle);
                        }
                    }
                    CompletionResult::UnsupportedCommand => {
                        if status != 1 || !matches!(opcode, CREATE | AUTH | ENCRYPT | DISCONNECT) {
                            return Err(Error::UnexpectedCommand);
                        }
                    }
                }
                self.pending = None;
                if opcode == CANCEL && status == 0x0b && self.stopping {
                    self.drive_stop()?;
                    return Ok(Notice::CancellationRace { status });
                }
                if status != 0 {
                    if opcode == CREATE {
                        self.connecting_on_controller = false;
                    }
                    return self.fail(Failure::Controller { opcode, status });
                }
                if opcode == CANCEL && (self.handle.is_some() || !self.connecting_on_controller) {
                    self.drive_stop()?;
                }
                if opcode == KEY_SIZE && !self.stopping {
                    let CompletionResult::EncryptionKeySize { size, .. } = result else {
                        return Err(Error::UnexpectedCommand);
                    };
                    return self.mark_ready(size.ok_or(Error::UnexpectedCommand)?);
                }
                if self.failure == Some(Failure::UserRejected)
                    || matches!(self.failure, Some(Failure::UnsupportedPairing(_)))
                {
                    self.cancel(now_us)?;
                } else if self.stopping && opcode != CANCEL && opcode != DISCONNECT {
                    self.drive_stop()?;
                }
                Ok(Notice::None)
            }
            Event::ConnectionComplete {
                status,
                handle,
                peer,
                ..
            } => {
                self.check_peer(peer)?;
                if !matches!(
                    self.state,
                    State::Connecting | State::Stopping | State::Cancelling
                ) || !self.connecting_on_controller
                {
                    return Err(Error::UnexpectedEvent);
                }
                if self
                    .pending
                    .as_ref()
                    .is_some_and(|p| p.ticket.opcode != CANCEL)
                {
                    return Err(Error::UnexpectedEvent);
                }
                self.connecting_on_controller = false;
                if self.pending.as_ref().is_some_and(|p| !p.accepted) {
                    self.pending = None;
                }
                if status != 0 {
                    self.state = if self.pending.is_some() {
                        State::Stopping
                    } else {
                        State::Closed
                    };
                    if self.stopping {
                        return Ok(Notice::Closed { reason: status });
                    }
                    return self.fail(Failure::ControllerEvent {
                        event: 0x03,
                        status,
                    });
                }
                self.handle = handle;
                if self.stopping {
                    self.drive_stop()?;
                    return Ok(Notice::None);
                }
                self.deadline_us = now_us
                    .checked_add(self.config.security_timeout_us)
                    .ok_or(Error::InvalidConfig)?;
                self.phase = Phase::Security;
                self.state = State::Authenticating;
                self.queue_handle(AUTH, &[])?;
                Ok(Notice::None)
            }
            Event::Disconnected {
                status,
                handle,
                reason,
            } => {
                self.check_handle(handle)?;
                if status != 0 {
                    return self.fail(Failure::ControllerEvent {
                        event: 0x05,
                        status,
                    });
                }
                self.handle = None;
                self.challenge = None;
                self.state = State::Closed;
                // An accepted command may still need its ack. Keep its correlation;
                // no new epoch may reuse it without external traffic quarantine.
                if self.pending.as_ref().is_some_and(|p| !p.accepted) {
                    self.pending = None;
                }
                if self.pending.is_some() {
                    self.stopping = true;
                    self.state = State::Stopping;
                    self.phase = Phase::Cleanup;
                    self.deadline_us = now_us
                        .checked_add(self.config.cleanup_timeout_us)
                        .ok_or(Error::InvalidConfig)?;
                }
                Ok(Notice::Closed { reason })
            }
            Event::AuthenticationComplete { status, handle } => {
                self.check_handle(handle)?;
                self.require_security(State::Authenticating)?;
                if self.pending.is_some() || self.challenge.is_some() {
                    return Err(Error::UnexpectedEvent);
                }
                if status != 0 {
                    return self.fail(Failure::ControllerEvent {
                        event: 0x06,
                        status,
                    });
                }
                if self.pairing_seen && !self.confirmation_approved {
                    return self.fail(Failure::UserRejected);
                }
                let required_key = if self.pairing_seen {
                    KeyAuthorization::NewSsp
                } else {
                    KeyAuthorization::Stored
                };
                if self.key_authorization != required_key {
                    return self.fail(Failure::UnprovenKey);
                }
                self.state = State::Encrypting;
                self.queue_handle(ENCRYPT, &[1])?;
                Ok(Notice::None)
            }
            Event::EncryptionChange {
                status,
                handle,
                enabled,
                ..
            } => {
                self.check_handle(handle)?;
                if !matches!(
                    self.state,
                    State::Encrypting | State::CheckingKeySize | State::Ready
                ) || self.stopping
                {
                    return Err(Error::UnexpectedEvent);
                }
                if status != 0 {
                    return self.fail(Failure::ControllerEvent {
                        event: 0x08,
                        status,
                    });
                }
                if enabled == 0 {
                    return self.fail(Failure::EncryptionDisabled);
                }
                if !matches!(self.state, State::Encrypting | State::Ready) || self.pending.is_some()
                {
                    return Err(Error::UnexpectedEvent);
                }
                if self.state == State::Ready {
                    self.deadline_us = now_us
                        .checked_add(self.config.security_timeout_us)
                        .ok_or(Error::InvalidConfig)?;
                    self.phase = Phase::Security;
                }
                self.state = State::CheckingKeySize;
                self.queue_handle(KEY_SIZE, &[])?;
                Ok(Notice::None)
            }
            Event::LinkKeyRequest(peer) => {
                self.check_peer(peer)?;
                self.require_security(State::Authenticating)?;
                self.require_challenge_slot()?;
                let request = self.request_id()?;
                self.challenge = Some(Challenge::Key(request));
                Ok(Notice::NeedKey { request })
            }
            Event::IoCapabilityRequest(peer) => {
                self.check_peer(peer)?;
                self.require_security(State::Authenticating)?;
                self.require_challenge_slot()?;
                let mut p = [0; 9];
                p[..6].copy_from_slice(&self.peer.hci_bytes());
                p[6] = 1; // DisplayYesNo: the protected LAN UI must actually provide it.
                p[7] = 0; // no OOB material
                p[8] = 4; // General Bonding, MITM not required (headset Just Works allowed)
                self.queue(IO_REPLY, &p)?;
                self.key_authorization = KeyAuthorization::None;
                self.pairing_seen = true;
                Ok(Notice::None)
            }
            Event::IoCapabilityResponse { peer, .. } => {
                self.check_peer(peer)?;
                self.require_security(State::Authenticating)?;
                Ok(Notice::None)
            }
            Event::UserConfirmation { peer, value } => {
                self.check_peer(peer)?;
                self.require_security(State::Authenticating)?;
                self.require_challenge_slot()?;
                let request = self.request_id()?;
                self.challenge = Some(Challenge::Confirmation(request));
                self.key_authorization = KeyAuthorization::None;
                self.pairing_seen = true;
                Ok(Notice::NeedConfirmation {
                    request,
                    numeric_value: value,
                })
            }
            Event::LinkKeyNotification {
                peer,
                key,
                key_type,
            } => {
                self.check_peer(peer)?;
                self.require_security(State::Authenticating)?;
                if !matches!(key_type, 4 | 5 | 7 | 8) {
                    return self.fail(Failure::UnsupportedKeyType(key_type));
                }
                if !self.confirmation_approved {
                    return self.fail(Failure::UserRejected);
                }
                self.key_authorization = KeyAuthorization::NewSsp;
                Ok(Notice::ProvisionalBond { key, key_type })
            }
            Event::SimplePairingComplete { peer, status } => {
                self.check_peer(peer)?;
                self.require_security(State::Authenticating)?;
                if status != 0 {
                    return self.fail(Failure::ControllerEvent {
                        event: 0x36,
                        status,
                    });
                }
                if !self.confirmation_approved {
                    return self.fail(Failure::UserRejected);
                }
                Ok(Notice::None)
            }
            Event::UnsupportedPairing { peer, event_code } => {
                self.check_peer(peer)?;
                self.require_security(State::Authenticating)?;
                self.require_challenge_slot()?;
                self.queue_peer(match event_code {
                    0x16 => PIN_NEGATIVE,
                    0x34 => PASSKEY_NEGATIVE,
                    0x35 => OOB_NEGATIVE,
                    _ => return Err(Error::UnexpectedEvent),
                })?;
                self.failure = Some(Failure::UnsupportedPairing(event_code));
                Ok(Notice::PairingRejected { event: event_code })
            }
        }
    }

    fn queue(&mut self, opcode: u16, parameters: &[u8]) -> Result<(), Error> {
        if self.pending.is_some() {
            return Err(Error::CommandBusy);
        }
        if parameters.len() > 22 {
            return Err(Error::InvalidConfig);
        }
        let serial = self.serial.checked_add(1).ok_or(Error::CounterExhausted)?;
        let mut bytes = [0; 25];
        bytes[..2].copy_from_slice(&opcode.to_le_bytes());
        bytes[2] = parameters.len() as u8;
        bytes[3..3 + parameters.len()].copy_from_slice(parameters);
        self.pending = Some(Pending {
            ticket: CommandTicket {
                epoch: self.epoch,
                serial,
                opcode,
            },
            bytes,
            len: 3 + parameters.len(),
            accepted: false,
            expected_handle: None,
        });
        self.serial = serial;
        Ok(())
    }
    fn queue_peer(&mut self, opcode: u16) -> Result<(), Error> {
        self.queue(opcode, &self.peer.hci_bytes())
    }
    fn queue_handle(&mut self, opcode: u16, tail: &[u8]) -> Result<(), Error> {
        let h = self.handle.ok_or(Error::InvalidState)?;
        let mut p = [0; 3];
        p[..2].copy_from_slice(&h.raw().to_le_bytes());
        if tail.len() > 1 {
            return Err(Error::InvalidConfig);
        }
        if let Some(v) = tail.first() {
            p[2] = *v;
        }
        self.queue(opcode, &p[..2 + tail.len()])?;
        if let Some(pending) = self.pending.as_mut() {
            pending.expected_handle = Some(h);
        }
        Ok(())
    }
    fn check_ack(&self, opcode: u16) -> Result<(), Error> {
        let p = self.pending.as_ref().ok_or(Error::UnexpectedCommand)?;
        if p.ticket.opcode != opcode {
            return Err(Error::UnexpectedCommand);
        }
        if !p.accepted {
            return Err(Error::CommandNotAccepted);
        }
        Ok(())
    }
    fn ack(&mut self, opcode: u16) -> Result<(), Error> {
        self.check_ack(opcode)?;
        self.pending = None;
        Ok(())
    }
    fn check_peer(&self, peer: Peer) -> Result<(), Error> {
        if self.peer == peer {
            Ok(())
        } else {
            Err(Error::ForeignPeer)
        }
    }
    fn check_handle(&self, handle: ConnectionHandle) -> Result<(), Error> {
        if self.handle == Some(handle) {
            Ok(())
        } else {
            Err(Error::ForeignHandle)
        }
    }
    fn require_security(&self, state: State) -> Result<(), Error> {
        if self.state == state && !self.stopping && self.failure.is_none() {
            Ok(())
        } else {
            Err(Error::InvalidState)
        }
    }
    fn require_challenge_slot(&self) -> Result<(), Error> {
        if self.pending.is_some() {
            return Err(Error::CommandBusy);
        }
        if self.challenge.is_some() {
            return Err(Error::UnexpectedEvent);
        }
        Ok(())
    }
    fn request_id(&mut self) -> Result<RequestId, Error> {
        self.request_serial = self
            .request_serial
            .checked_add(1)
            .ok_or(Error::CounterExhausted)?;
        Ok(RequestId {
            epoch: self.epoch,
            serial: self.request_serial,
        })
    }
    fn check_clock(&mut self, now: u64) -> Result<(), Error> {
        if now < self.last_us {
            return Err(Error::ClockRegression);
        }
        self.last_us = now;
        Ok(())
    }
    fn observe(&mut self, now: u64) -> Result<(), Error> {
        self.check_clock(now)?;
        if !matches!(self.state, State::Ready | State::Closed | State::Failed)
            && now >= self.deadline_us
        {
            return self.fail(Failure::Timeout(self.phase));
        }
        Ok(())
    }
    fn fail<T>(&mut self, failure: Failure) -> Result<T, Error> {
        if self.failure.is_none() {
            self.failure = Some(failure);
        }
        self.state = State::Failed;
        self.challenge = None;
        Err(Error::Failed(failure))
    }
    fn mark_ready(&mut self, size: u8) -> Result<Notice, Error> {
        if !(1..=16).contains(&size) || size < self.config.minimum_key_size {
            return self.fail(Failure::WeakKey(size));
        }
        if self.state != State::CheckingKeySize || self.failure.is_some() {
            return Err(Error::InvalidState);
        }
        self.state = State::Ready;
        Ok(Notice::Ready)
    }
    fn drive_stop(&mut self) -> Result<(), Error> {
        if self.pending.is_some() {
            return Ok(());
        }
        if self.handle.is_some() {
            if self.state == State::Disconnecting {
                return Ok(());
            }
            self.queue_handle(DISCONNECT, &[0x13])?;
            self.state = State::Disconnecting;
        } else if !self.connecting_on_controller {
            self.state = State::Closed;
        } else if self.state != State::Closed && self.state != State::Cancelling {
            self.queue_peer(CANCEL)?;
            self.state = State::Cancelling;
        }
        Ok(())
    }
}
