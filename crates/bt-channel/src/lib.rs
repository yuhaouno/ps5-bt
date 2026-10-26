// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded selected-connection L2CAP/SDP coordinator. No HCI authority or transport.
#![no_std]
#![forbid(unsafe_code)]

use bt_l2cap::{
    BasicFrame, ChannelError, ChannelEvent, ChannelPhase, ClassicChannel, Connection,
    ConnectionResult, Signal, SignalCommand, SignalError,
};
use bt_sdp::{A2dpSinkDiscovery, DiscoveryError, DiscoveryLimits, DiscoveryProgress, SinkService};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelRole {
    Sdp,
    Signaling,
    Media,
}
impl ChannelRole {
    const fn index(self) -> usize {
        match self {
            Self::Sdp => 0,
            Self::Signaling => 1,
            Self::Media => 2,
        }
    }
}
const ROLES: [ChannelRole; 3] = [ChannelRole::Sdp, ChannelRole::Signaling, ChannelRole::Media];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChannelBinding {
    pub connection: Connection,
    pub local_cid: u16,
    pub remote_cid: u16,
    pub local_mtu: u16,
    pub peer_mtu: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChannelConfig {
    pub local_cids: [u16; 3],
    pub local_mtu: u16,
    pub channel_timeout_us: u64,
    pub discovery_timeout_us: u64,
    pub cleanup_timeout_us: u64,
    pub discovery_limits: DiscoveryLimits,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoordinatorPhase {
    Idle,
    Discovering,
    ClosingDiscovery,
    Discovered,
    OpeningSignaling,
    SignalingReady,
    OpeningMedia,
    Ready,
    ClosingMedia,
    ClosingSignaling,
    Cancelling,
    Closed,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidConfig,
    InvalidState,
    Busy,
    OutputTooSmall,
    WrongTicket,
    ForeignConnection,
    ForeignChannel,
    ResponseBeforeAcceptance,
    UnexpectedResponse,
    MtuExceeded,
    RemoteCidCollision,
    ClockRegression,
    DeadlineOverflow,
    Timeout(CoordinatorPhase),
    CounterExhausted,
    Failed,
    Rejected(ChannelEvent),
    PeerClosed(ChannelRole),
    Basic(bt_l2cap::ParseError),
    Signal(SignalError),
    Channel(ChannelError),
    Discovery(DiscoveryError),
    Transport(bt_core::Error),
}
impl From<ChannelError> for Error {
    fn from(e: ChannelError) -> Self {
        Self::Channel(e)
    }
}
impl From<DiscoveryError> for Error {
    fn from(e: DiscoveryError) -> Self {
        Self::Discovery(e)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SduTicket {
    connection: Connection,
    serial: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SduKind {
    ChannelSignal(ChannelRole),
    SdpRequest,
    Application(ChannelRole),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreparedSdu {
    pub ticket: SduTicket,
    pub connection: Connection,
    pub destination_cid: u16,
    /// Complete Basic PDU length, including its four-byte length/CID header.
    pub len: usize,
    pub kind: SduKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notice<'a> {
    None,
    ServiceDiscovered(SinkService),
    Opened {
        role: ChannelRole,
        binding: ChannelBinding,
    },
    Closed {
        role: ChannelRole,
    },
    Data {
        role: ChannelRole,
        binding: ChannelBinding,
        payload: &'a [u8],
    },
}

struct Slot {
    channel: ClassicChannel,
    used: bool,
    radio_visible: bool,
    published: bool,
    awaiting: Option<u8>,
}
#[derive(Clone, Copy)]
enum PendingKind {
    Request(ChannelRole, u8),
    Reply(ChannelRole),
    Sdp,
    Application(ChannelRole),
}
#[derive(Clone, Copy)]
struct Pending {
    prepared: PreparedSdu,
    kind: PendingKind,
}

/// Owns protocol scheduling, not controller/ACL credits or native traffic.
/// The external owner must accept complete PDUs in FIFO order and retain every
/// received packet on Busy. Do not use two coordinators for the same channel CIDs.
pub struct ChannelCoordinator<'buf> {
    connection: Connection,
    config: ChannelConfig,
    slots: [Slot; 3],
    discovery: A2dpSinkDiscovery<'buf>,
    tx_storage: &'buf mut [u8],
    pending: Option<Pending>,
    serial: u32,
    next_identifier: u16,
    sdp_transaction: u16,
    sdp_awaiting: bool,
    service: Option<SinkService>,
    phase: CoordinatorPhase,
    deadline_us: Option<u64>,
    last_us: u64,
    stopping: bool,
    failure: Option<Error>,
    uncertain: bool,
}

impl<'buf> ChannelCoordinator<'buf> {
    pub fn new(
        connection: Connection,
        now_us: u64,
        config: ChannelConfig,
        tx_storage: &'buf mut [u8],
        sdp_storage: &'buf mut [u8],
    ) -> Result<Self, Error> {
        if connection.epoch == 0
            || config.local_mtu < 48
            || tx_storage.len() < 47
            || config.channel_timeout_us == 0
            || config.discovery_timeout_us == 0
            || config.cleanup_timeout_us == 0
            || u32::from(config.discovery_limits.max_chunk_bytes) + 24 > u32::from(config.local_mtu)
            || config.local_cids.iter().any(|cid| *cid < 0x40)
            || config.local_cids[0] == config.local_cids[1]
            || config.local_cids[0] == config.local_cids[2]
            || config.local_cids[1] == config.local_cids[2]
        {
            return Err(Error::InvalidConfig);
        }
        for budget in [
            config.channel_timeout_us,
            config.discovery_timeout_us,
            config.cleanup_timeout_us,
        ] {
            now_us.checked_add(budget).ok_or(Error::DeadlineOverflow)?;
        }
        let slot = |index: usize| -> Result<Slot, Error> {
            Ok(Slot {
                channel: ClassicChannel::new(
                    connection,
                    if index == 0 { 1 } else { 0x19 },
                    config.local_cids[index],
                    config.local_mtu,
                    (index as u8) * 4 + 1,
                )?,
                used: false,
                radio_visible: false,
                published: false,
                awaiting: None,
            })
        };
        Ok(Self {
            connection,
            config,
            slots: [slot(0)?, slot(1)?, slot(2)?],
            discovery: A2dpSinkDiscovery::new(sdp_storage, config.discovery_limits)?,
            tx_storage,
            pending: None,
            serial: 0,
            next_identifier: 1,
            sdp_transaction: 0,
            sdp_awaiting: false,
            service: None,
            phase: CoordinatorPhase::Idle,
            deadline_us: None,
            last_us: now_us,
            stopping: false,
            failure: None,
            uncertain: false,
        })
    }

    pub const fn connection(&self) -> Connection {
        self.connection
    }
    pub const fn phase(&self) -> CoordinatorPhase {
        self.phase
    }
    pub const fn failure(&self) -> Option<Error> {
        self.failure
    }
    pub const fn service(&self) -> Option<SinkService> {
        self.service
    }
    pub const fn has_pending(&self) -> bool {
        self.pending.is_some()
    }
    pub fn channel_phase(&self, role: ChannelRole) -> Option<ChannelPhase> {
        let slot = &self.slots[role.index()];
        slot.used.then_some(slot.channel.phase())
    }
    pub fn needs_cleanup(&self) -> bool {
        self.uncertain
            || self.slots.iter().any(|slot| slot.radio_visible)
            || self
                .pending
                .is_some_and(|p| matches!(p.kind, PendingKind::Reply(_)))
    }
    pub fn is_closed(&self) -> bool {
        self.phase == CoordinatorPhase::Closed && self.pending.is_none() && !self.needs_cleanup()
    }
    pub fn sdp_closed(&self) -> bool {
        self.role_closed(ChannelRole::Sdp)
    }
    pub fn media_closed(&self) -> bool {
        self.role_closed(ChannelRole::Media)
    }
    pub fn signaling_closed(&self) -> bool {
        self.role_closed(ChannelRole::Signaling)
    }
    pub fn binding(&self, role: ChannelRole) -> Option<ChannelBinding> {
        if self.stopping || self.phase == CoordinatorPhase::Failed {
            return None;
        }
        let slot = &self.slots[role.index()];
        if !slot.published || slot.channel.phase() != ChannelPhase::Open {
            return None;
        }
        Some(ChannelBinding {
            connection: self.connection,
            local_cid: slot.channel.local_channel(),
            remote_cid: slot.channel.remote_channel()?,
            local_mtu: slot.channel.local_mtu(),
            peer_mtu: slot.channel.peer_mtu(),
        })
    }

    pub fn begin_discovery(&mut self, now_us: u64) -> Result<(), Error> {
        self.observe(now_us)?;
        if self.phase != CoordinatorPhase::Idle {
            return Err(Error::InvalidState);
        }
        self.open_role(ChannelRole::Sdp, now_us, CoordinatorPhase::Discovering)
    }
    pub fn open_signaling(&mut self, now_us: u64) -> Result<(), Error> {
        self.observe(now_us)?;
        if self.phase != CoordinatorPhase::Discovered
            || !self.sdp_closed()
            || self.service.is_none()
        {
            return Err(Error::InvalidState);
        }
        self.open_role(
            ChannelRole::Signaling,
            now_us,
            CoordinatorPhase::OpeningSignaling,
        )
    }
    /// The application must first obtain AVDTP Open acceptance for this epoch.
    pub fn open_media(&mut self, now_us: u64) -> Result<(), Error> {
        self.observe(now_us)?;
        if self.phase != CoordinatorPhase::SignalingReady
            || self.binding(ChannelRole::Signaling).is_none()
        {
            return Err(Error::InvalidState);
        }
        self.open_role(ChannelRole::Media, now_us, CoordinatorPhase::OpeningMedia)
    }
    pub fn close_media(&mut self, now_us: u64) -> Result<(), Error> {
        self.observe(now_us)?;
        if self.phase != CoordinatorPhase::Ready {
            return Err(Error::InvalidState);
        }
        self.close_role(ChannelRole::Media, now_us, CoordinatorPhase::ClosingMedia)
    }
    pub fn close_signaling(&mut self, now_us: u64) -> Result<(), Error> {
        self.observe(now_us)?;
        if self.phase != CoordinatorPhase::SignalingReady {
            return Err(Error::InvalidState);
        }
        self.close_role(
            ChannelRole::Signaling,
            now_us,
            CoordinatorPhase::ClosingSignaling,
        )
    }

    /// Optional serialized application-SDU path. The caller correlates AVDTP
    /// command acceptance with this returned queue's later acceptance ticket.
    pub fn queue_data(
        &mut self,
        role: ChannelRole,
        now_us: u64,
        payload: &[u8],
    ) -> Result<(), Error> {
        self.observe(now_us)?;
        self.ensure_free()?;
        if role == ChannelRole::Sdp {
            return Err(Error::InvalidState);
        }
        let binding = self.binding(role).ok_or(Error::InvalidState)?;
        if payload.len() > usize::from(binding.peer_mtu) {
            return Err(Error::MtuExceeded);
        }
        self.queue(binding.remote_cid, payload, PendingKind::Application(role))
    }

    /// Copy one complete Basic PDU only after checking the caller's capacity.
    /// Do not confirm until the owner copied all bytes into bounded FIFO storage.
    pub fn prepare_sdu(
        &mut self,
        now_us: u64,
        output: &mut [u8],
    ) -> Result<Option<PreparedSdu>, Error> {
        self.observe(now_us)?;
        let Some(pending) = self.pending else {
            return Ok(None);
        };
        if output.len() < pending.prepared.len {
            return Err(Error::OutputTooSmall);
        }
        output[..pending.prepared.len].copy_from_slice(&self.tx_storage[..pending.prepared.len]);
        Ok(Some(pending.prepared))
    }
    pub fn confirm_accepted(
        &mut self,
        ticket: SduTicket,
        now_us: u64,
    ) -> Result<Notice<'static>, Error> {
        let pending = self.pending.ok_or(Error::WrongTicket)?;
        if pending.prepared.ticket != ticket {
            return Err(Error::WrongTicket);
        }
        self.pending = None;
        match pending.kind {
            PendingKind::Request(role, identifier) => {
                let slot = &mut self.slots[role.index()];
                slot.awaiting = Some(identifier);
                slot.radio_visible = true;
            }
            PendingKind::Sdp => self.sdp_awaiting = true,
            PendingKind::Reply(_) | PendingKind::Application(_) => {}
        }
        // A matching callback is evidence of an already completed owner handoff.
        // Record it before checking time so a late callback cannot replay an SDU
        // or erase the remote allocation that now requires reconciliation.
        if let Err(error) = self.observe(now_us) {
            return self.fail(error);
        }
        if self.stopping {
            self.advance_stop()?;
            return Ok(Notice::None);
        }
        if let PendingKind::Reply(role) = pending.kind {
            return self.advance_role(role, now_us);
        }
        Ok(Notice::None)
    }

    pub fn receive<'packet>(
        &mut self,
        connection: Connection,
        now_us: u64,
        bytes: &'packet [u8],
        scratch: &mut [u8],
    ) -> Result<Notice<'packet>, Error> {
        if connection != self.connection {
            return Err(Error::ForeignConnection);
        }
        self.observe(now_us)?;
        let basic = BasicFrame::parse(bytes).map_err(Error::Basic)?;
        if basic.payload().len() > usize::from(self.config.local_mtu) {
            return Err(Error::MtuExceeded);
        }
        if basic.channel_id() == 1 {
            return self.receive_signal(now_us, basic.payload());
        }
        let role = ROLES
            .into_iter()
            .find(|role| {
                self.slots[role.index()].used
                    && self.slots[role.index()].channel.local_channel() == basic.channel_id()
            })
            .ok_or(Error::ForeignChannel)?;
        if basic.payload().len() > usize::from(self.config.local_mtu) {
            return Err(Error::MtuExceeded);
        }
        if role == ChannelRole::Sdp {
            if self.stopping {
                return Err(Error::InvalidState);
            }
            if self.pending.is_some() {
                return Err(Error::ResponseBeforeAcceptance);
            }
            if !self.sdp_awaiting {
                return Err(Error::UnexpectedResponse);
            }
            self.ensure_free()?;
            let progress = self.discovery.accept_response(
                connection.epoch,
                now_us,
                basic.payload(),
                scratch,
            )?;
            self.sdp_awaiting = false;
            match progress {
                DiscoveryProgress::ContinuationRequired { .. } => {
                    self.queue_discovery(false)?;
                    Ok(Notice::None)
                }
                DiscoveryProgress::Complete(service) => {
                    self.service = Some(service);
                    self.close_role(ChannelRole::Sdp, now_us, CoordinatorPhase::ClosingDiscovery)?;
                    Ok(Notice::ServiceDiscovered(service))
                }
            }
        } else {
            let binding = self.binding(role).ok_or(Error::InvalidState)?;
            Ok(Notice::Data {
                role,
                binding,
                payload: basic.payload(),
            })
        }
    }

    pub fn tick(&mut self, now_us: u64) -> Result<(), Error> {
        self.observe(now_us)
    }
    /// Clock-independent terminal invalidation. No queued SDU may be replayed.
    pub fn fail_transport(&mut self, error: bt_core::Error) -> Result<(), Error> {
        if error == bt_core::Error::Busy {
            return Err(Error::InvalidState);
        }
        self.uncertain = true;
        self.fail(Error::Transport(error))
    }
    pub fn cancel(&mut self, now_us: u64) -> Result<(), Error> {
        self.check_clock(now_us)?;
        if self.phase == CoordinatorPhase::Failed {
            return Err(Error::Failed);
        }
        if self.stopping {
            return self.observe(now_us);
        }
        self.deadline_us = Some(
            now_us
                .checked_add(self.config.cleanup_timeout_us)
                .ok_or(Error::DeadlineOverflow)?,
        );
        self.stopping = true;
        self.phase = CoordinatorPhase::Cancelling;
        if let Some(pending) = self.pending {
            match pending.kind {
                PendingKind::Request(role, _)
                    if self.slots[role.index()].channel.phase() == ChannelPhase::WaitConnection =>
                {
                    self.pending = None;
                    let mut out = [0; 32];
                    self.slots[role.index()]
                        .channel
                        .cancel(self.connection, &mut out)?;
                }
                PendingKind::Sdp | PendingKind::Application(_) => self.pending = None,
                PendingKind::Request(role, _)
                    if self.slots[role.index()].channel.phase() == ChannelPhase::Configuring =>
                {
                    self.pending = None;
                }
                _ => {}
            }
        }
        if self.discovery.epoch().is_some() {
            self.discovery.cancel(self.connection.epoch)?;
        }
        self.advance_stop()
    }

    fn open_role(
        &mut self,
        role: ChannelRole,
        now: u64,
        phase: CoordinatorPhase,
    ) -> Result<(), Error> {
        self.ensure_free()?;
        let previous = &self.slots[role.index()];
        if previous.used && (role != ChannelRole::Media || !self.media_closed()) {
            return Err(Error::InvalidState);
        }
        if self.next_identifier > 253 {
            return Err(Error::CounterExhausted);
        }
        let mut cid = self.config.local_cids[role.index()];
        if previous.used {
            cid = previous
                .channel
                .local_channel()
                .checked_add(1)
                .ok_or(Error::CounterExhausted)?;
            while self.config.local_cids[..2].contains(&cid) {
                cid = cid.checked_add(1).ok_or(Error::CounterExhausted)?;
            }
        }
        let deadline = now
            .checked_add(self.config.channel_timeout_us)
            .ok_or(Error::DeadlineOverflow)?;
        let mut output = [0; 32];
        let mut channel = ClassicChannel::new(
            self.connection,
            if role == ChannelRole::Sdp { 1 } else { 0x19 },
            cid,
            self.config.local_mtu,
            self.next_identifier as u8,
        )?;
        let progress = channel.connect(self.connection, &mut output)?;
        self.queue_signal(role, &output[..progress.output_len])?;
        self.slots[role.index()] = Slot {
            channel,
            used: true,
            radio_visible: false,
            published: false,
            awaiting: None,
        };
        self.next_identifier += 4;
        self.phase = phase;
        self.deadline_us = Some(deadline);
        Ok(())
    }
    fn close_role(
        &mut self,
        role: ChannelRole,
        now: u64,
        phase: CoordinatorPhase,
    ) -> Result<(), Error> {
        self.ensure_free()?;
        let slot = &self.slots[role.index()];
        if slot.awaiting.is_some() {
            return Err(Error::Busy);
        }
        let deadline = now
            .checked_add(self.config.cleanup_timeout_us)
            .ok_or(Error::DeadlineOverflow)?;
        let mut output = [0; 32];
        let progress = self.slots[role.index()]
            .channel
            .disconnect(self.connection, &mut output)?;
        self.queue_signal(role, &output[..progress.output_len])?;
        self.slots[role.index()].published = false;
        self.phase = phase;
        self.deadline_us = Some(deadline);
        Ok(())
    }
    fn receive_signal(&mut self, now: u64, bytes: &[u8]) -> Result<Notice<'static>, Error> {
        let command = SignalCommand::parse(bytes).map_err(Error::Signal)?;
        let role = match command.signal {
            Signal::ConnectionResponse { source_channel, .. }
            | Signal::DisconnectionResponse { source_channel, .. } => {
                self.find_local(source_channel)?
            }
            Signal::ConfigurationRequest {
                destination_channel,
                ..
            }
            | Signal::DisconnectionRequest {
                destination_channel,
                ..
            } => self.find_local(destination_channel)?,
            Signal::ConfigurationResponse { source_channel, .. } => ROLES
                .into_iter()
                .find(|role| {
                    self.slots[role.index()].channel.remote_channel() == Some(source_channel)
                })
                .ok_or(Error::ForeignChannel)?,
            Signal::ConnectionRequest { .. } => return Err(Error::ForeignChannel),
        };
        let is_response = matches!(
            command.signal,
            Signal::ConnectionResponse { .. }
                | Signal::ConfigurationResponse { .. }
                | Signal::DisconnectionResponse { .. }
        );
        if is_response && self.slots[role.index()].awaiting != Some(command.identifier) {
            if self.pending.is_some_and(|p| matches!(p.kind, PendingKind::Request(r, id) if r == role && id == command.identifier)) {
                return Err(Error::ResponseBeforeAcceptance);
            }
            return Err(Error::UnexpectedResponse);
        }
        self.ensure_free()?;
        if let Signal::ConnectionResponse {
            destination_channel,
            result: ConnectionResult::Success,
            ..
        } = command.signal
            && ROLES.into_iter().any(|other| {
                other != role
                    && self.slots[other.index()].channel.remote_channel()
                        == Some(destination_channel)
            })
        {
            return Err(Error::RemoteCidCollision);
        }
        if matches!(
            command.signal,
            Signal::ConnectionResponse {
                result: ConnectionResult::Pending,
                ..
            }
        ) || matches!(
            command.signal,
            Signal::ConfigurationResponse {
                result: bt_l2cap::ConfigurationResult::Pending,
                ..
            }
        ) {
            return Ok(Notice::None);
        }
        let peer_close_deadline =
            if !self.stopping && matches!(command.signal, Signal::DisconnectionRequest { .. }) {
                Some(
                    now.checked_add(self.config.cleanup_timeout_us)
                        .ok_or(Error::DeadlineOverflow)?,
                )
            } else {
                None
            };
        let mut output = [0; 32];
        let progress =
            self.slots[role.index()]
                .channel
                .receive(self.connection, command, &mut output)?;
        if let Some(deadline) = peer_close_deadline {
            self.deadline_us = Some(self.deadline_us.map_or(deadline, |old| old.min(deadline)));
            self.phase = match role {
                ChannelRole::Sdp => CoordinatorPhase::ClosingDiscovery,
                ChannelRole::Signaling => CoordinatorPhase::ClosingSignaling,
                ChannelRole::Media => CoordinatorPhase::ClosingMedia,
            };
        }
        if is_response {
            self.slots[role.index()].awaiting = None;
        }
        if matches!(
            progress.event,
            ChannelEvent::ConnectionRejected(_) | ChannelEvent::ConfigurationRejected(_)
        ) {
            if self.slots[role.index()].channel.phase() == ChannelPhase::Closed {
                self.slots[role.index()].radio_visible = false;
            }
            return self.fail(Error::Rejected(progress.event));
        }
        if self.slots[role.index()].channel.phase() == ChannelPhase::Closed {
            let slot = &mut self.slots[role.index()];
            slot.published = false;
            slot.radio_visible = false;
            slot.awaiting = None;
        }
        if progress.output_len != 0 {
            if self.stopping && matches!(progress.event, ChannelEvent::ConfigurationStarted) {
                // The newly known remote CID must be closed, not configured.
                self.advance_stop()?;
                return Ok(Notice::None);
            }
            self.queue_signal(role, &output[..progress.output_len])?;
        }
        if self.stopping {
            self.advance_stop()?;
            return Ok(Notice::None);
        }
        self.advance_role(role, now)
    }
    fn advance_role(&mut self, role: ChannelRole, now: u64) -> Result<Notice<'static>, Error> {
        if self.pending.is_some() {
            return Ok(Notice::None);
        }
        let slot = &mut self.slots[role.index()];
        if slot.channel.phase() == ChannelPhase::Open && !slot.published {
            slot.published = true;
            if role == ChannelRole::Sdp {
                self.deadline_us = Some(
                    now.checked_add(self.config.discovery_timeout_us)
                        .ok_or(Error::DeadlineOverflow)?,
                );
                self.queue_discovery(true)?;
                return Ok(Notice::None);
            }
            self.deadline_us = None;
            self.phase = if role == ChannelRole::Media {
                CoordinatorPhase::Ready
            } else {
                CoordinatorPhase::SignalingReady
            };
            return Ok(Notice::Opened {
                role,
                binding: self.binding(role).ok_or(Error::InvalidState)?,
            });
        }
        if slot.used && slot.channel.phase() == ChannelPhase::Closed {
            if (role == ChannelRole::Sdp && self.service.is_none())
                || (role == ChannelRole::Signaling && self.slots[2].radio_visible)
            {
                return self.fail(Error::PeerClosed(role));
            }
            self.deadline_us = None;
            self.phase = if role == ChannelRole::Media {
                CoordinatorPhase::SignalingReady
            } else {
                CoordinatorPhase::Discovered
            };
            return Ok(Notice::Closed { role });
        }
        Ok(Notice::None)
    }
    fn queue_discovery(&mut self, first: bool) -> Result<(), Error> {
        let transaction = self
            .sdp_transaction
            .checked_add(1)
            .ok_or(Error::CounterExhausted)?;
        let mut output = [0; 43];
        let length = if first {
            self.discovery.start(
                self.connection.epoch,
                transaction,
                self.deadline_us.ok_or(Error::InvalidState)?,
                &mut output,
            )?
        } else {
            self.discovery
                .continue_request(self.connection.epoch, transaction, &mut output)?
        };
        let remote = self.slots[0]
            .channel
            .remote_channel()
            .ok_or(Error::InvalidState)?;
        if length > usize::from(self.slots[0].channel.peer_mtu()) {
            return self.fail(Error::MtuExceeded);
        }
        self.queue(remote, &output[..length], PendingKind::Sdp)?;
        self.sdp_transaction = transaction;
        Ok(())
    }
    fn advance_stop(&mut self) -> Result<(), Error> {
        if self.pending.is_some() {
            return Ok(());
        }
        for role in [ChannelRole::Media, ChannelRole::Signaling, ChannelRole::Sdp] {
            let slot = &self.slots[role.index()];
            if !slot.used || slot.channel.phase() == ChannelPhase::Closed {
                continue;
            }
            if slot.awaiting.is_some() {
                return Ok(());
            }
            let mut output = [0; 32];
            let progress = self.slots[role.index()]
                .channel
                .cancel(self.connection, &mut output)?;
            self.slots[role.index()].published = false;
            if progress.output_len != 0 {
                self.queue_signal(role, &output[..progress.output_len])?;
                return Ok(());
            }
            self.slots[role.index()].radio_visible = false;
        }
        self.phase = CoordinatorPhase::Closed;
        self.deadline_us = None;
        Ok(())
    }
    fn role_closed(&self, role: ChannelRole) -> bool {
        let slot = &self.slots[role.index()];
        slot.used
            && slot.channel.phase() == ChannelPhase::Closed
            && !slot.radio_visible
            && !self
                .pending
                .is_some_and(|p| matches!(p.kind, PendingKind::Reply(r) if r == role))
    }
    fn find_local(&self, cid: u16) -> Result<ChannelRole, Error> {
        ROLES
            .into_iter()
            .find(|role| {
                self.slots[role.index()].used
                    && self.slots[role.index()].channel.local_channel() == cid
            })
            .ok_or(Error::ForeignChannel)
    }
    fn queue_signal(&mut self, role: ChannelRole, bytes: &[u8]) -> Result<(), Error> {
        let command = SignalCommand::parse(bytes).map_err(Error::Signal)?;
        let request = matches!(
            command.signal,
            Signal::ConnectionRequest { .. }
                | Signal::ConfigurationRequest { .. }
                | Signal::DisconnectionRequest { .. }
        );
        self.queue(
            1,
            bytes,
            if request {
                PendingKind::Request(role, command.identifier)
            } else {
                PendingKind::Reply(role)
            },
        )
    }
    fn queue(&mut self, cid: u16, bytes: &[u8], kind: PendingKind) -> Result<(), Error> {
        self.ensure_free()?;
        let serial = self.serial.checked_add(1).ok_or(Error::CounterExhausted)?;
        let length = BasicFrame::new(cid, bytes)
            .map_err(Error::Transport)?
            .encode(self.tx_storage)
            .map_err(|_| Error::OutputTooSmall)?;
        let public_kind = match kind {
            PendingKind::Request(role, _) | PendingKind::Reply(role) => {
                SduKind::ChannelSignal(role)
            }
            PendingKind::Sdp => SduKind::SdpRequest,
            PendingKind::Application(role) => SduKind::Application(role),
        };
        self.pending = Some(Pending {
            prepared: PreparedSdu {
                ticket: SduTicket {
                    connection: self.connection,
                    serial,
                },
                connection: self.connection,
                destination_cid: cid,
                len: length,
                kind: public_kind,
            },
            kind,
        });
        self.serial = serial;
        Ok(())
    }
    fn ensure_free(&self) -> Result<(), Error> {
        if self.pending.is_some() {
            return Err(Error::Busy);
        }
        if self.serial == u32::MAX {
            return Err(Error::CounterExhausted);
        }
        Ok(())
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
        if self.phase == CoordinatorPhase::Failed {
            return Err(Error::Failed);
        }
        if self.deadline_us.is_some_and(|deadline| now >= deadline) {
            return self.fail(Error::Timeout(self.phase));
        }
        Ok(())
    }
    fn fail<T>(&mut self, error: Error) -> Result<T, Error> {
        if self.failure.is_none() {
            self.failure = Some(error);
        }
        self.phase = CoordinatorPhase::Failed;
        Err(error)
    }
}
