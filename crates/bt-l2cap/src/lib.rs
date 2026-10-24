// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded basic framing, ACL recombination and Classic signaling. See the crate README.
#![no_std]
#![forbid(unsafe_code)]

use bt_core::Error;
use bt_hci::{AclPacket, BroadcastFlag, ConnectionHandle, PacketBoundary};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    Length,
    NullChannel,
}

/// One complete basic-header-bearing PDU. CID validity beyond nonzero, channel
/// ownership, mode and negotiated MTU must be checked by the future channel layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BasicFrame<'a> {
    channel_id: u16,
    payload: &'a [u8],
}

impl<'a> BasicFrame<'a> {
    pub fn new(channel_id: u16, payload: &'a [u8]) -> Result<Self, Error> {
        if channel_id == 0 || payload.len() > usize::from(u16::MAX) {
            return Err(Error::Invalid);
        }
        Ok(Self {
            channel_id,
            payload,
        })
    }

    /// Exactly one frame. No trailing padding or concatenated PDUs are accepted.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, ParseError> {
        if bytes.len() < 4
            || bytes.len() != usize::from(u16::from_le_bytes([bytes[0], bytes[1]])) + 4
        {
            return Err(ParseError::Length);
        }
        let channel_id = u16::from_le_bytes([bytes[2], bytes[3]]);
        if channel_id == 0 {
            return Err(ParseError::NullChannel);
        }
        Ok(Self {
            channel_id,
            payload: &bytes[4..],
        })
    }

    pub const fn channel_id(self) -> u16 {
        self.channel_id
    }

    pub const fn payload(self) -> &'a [u8] {
        self.payload
    }

    /// Failure leaves output unchanged. The return length includes the header.
    pub fn encode(self, output: &mut [u8]) -> Result<usize, Error> {
        let length = self.payload.len() + 4;
        if output.len() < length {
            return Err(Error::BufferTooSmall);
        }
        output[..2].copy_from_slice(&(self.payload.len() as u16).to_le_bytes());
        output[2..4].copy_from_slice(&self.channel_id.to_le_bytes());
        output[4..length].copy_from_slice(self.payload);
        Ok(length)
    }
}

/// Caller-assigned receive context, not information carried on the HCI wire.
/// The dispatcher must attach it before handle reuse, preserving ordered events.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Connection {
    pub handle: ConnectionHandle,
    pub epoch: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReassemblyError {
    ForeignConnection,
    InvalidEpoch,
    InvalidFlags,
    MissingStart,
    UnexpectedStart,
    Length,
    NullChannel,
    BufferTooSmall,
}

/// One received, point-to-point PDU at a time for an explicit handle and epoch.
///
/// Storage includes the basic header. Headers may span ACL fragments; neither
/// heap allocation nor an assumed controller packet size is required. One
/// instance is required per concurrently reassembled connection. All errors
/// leave state and storage unchanged. Caller policy must surface integrity
/// failures; this type never retries, discards an unfinished PDU, or reopens.
pub struct Reassembler<'storage> {
    connection: Connection,
    storage: &'storage mut [u8],
    received: usize,
    expected: Option<usize>,
    active: bool,
}

impl<'storage> Reassembler<'storage> {
    pub fn new(connection: Connection, storage: &'storage mut [u8]) -> Result<Self, Error> {
        if storage.len() < 4 {
            return Err(Error::BufferTooSmall);
        }
        Ok(Self {
            connection,
            storage,
            received: 0,
            expected: None,
            active: false,
        })
    }

    pub const fn connection(&self) -> Connection {
        self.connection
    }

    pub const fn in_progress(&self) -> bool {
        self.active
    }

    /// Number of buffered octets, including any partial basic header.
    pub const fn pending_len(&self) -> usize {
        self.received
    }

    /// Full PDU length including header, once all four header octets arrived.
    pub const fn expected_len(&self) -> Option<usize> {
        self.expected
    }

    /// Abandon the previous context only after caller-managed disconnect/loss
    /// handling. Epochs must increase across reset, even if the handle changes;
    /// wrapping or reusing an epoch requires destroying this instance instead.
    /// Reset does not erase caller storage or permit stale packets to be relabeled.
    pub fn reset(&mut self, connection: Connection) -> Result<(), ReassemblyError> {
        if connection.epoch <= self.connection.epoch {
            return Err(ReassemblyError::InvalidEpoch);
        }
        self.connection = connection;
        self.received = 0;
        self.expected = None;
        self.active = false;
        Ok(())
    }

    /// Receive only ordinary controller-to-host point-to-point ACL fragments:
    /// PB=2 starts a PDU, PB=1 continues it. PB=0 and broadcasts are rejected.
    /// A completed frame borrows this instance until consumed by the caller.
    /// Timers, MTUs and traffic routing belong to the future connection layer.
    pub fn push(
        &mut self,
        connection: Connection,
        packet: AclPacket<'_>,
    ) -> Result<Option<BasicFrame<'_>>, ReassemblyError> {
        if connection != self.connection || packet.handle() != self.connection.handle {
            return Err(ReassemblyError::ForeignConnection);
        }
        if packet.broadcast() != BroadcastFlag::PointToPoint {
            return Err(ReassemblyError::InvalidFlags);
        }
        let offset = match packet.boundary() {
            PacketBoundary::StartFlushable if self.active => {
                return Err(ReassemblyError::UnexpectedStart);
            }
            PacketBoundary::StartFlushable => 0,
            PacketBoundary::Continuation if !self.active => {
                return Err(ReassemblyError::MissingStart);
            }
            PacketBoundary::Continuation => self.received,
            PacketBoundary::StartNonFlushable => return Err(ReassemblyError::InvalidFlags),
        };
        let fragment = packet.payload();
        let total = offset
            .checked_add(fragment.len())
            .ok_or(ReassemblyError::BufferTooSmall)?;
        if total > self.storage.len() {
            return Err(ReassemblyError::BufferTooSmall);
        }

        // Reconstruct at most four header octets locally before changing state.
        // This preflight also covers a header split at any ACL boundary.
        let mut header = [0; 4];
        let prefix = offset.min(4);
        header[..prefix].copy_from_slice(&self.storage[..prefix]);
        let added = fragment.len().min(4 - prefix);
        header[prefix..prefix + added].copy_from_slice(&fragment[..added]);
        let expected = if total >= 4 {
            let expected = usize::from(u16::from_le_bytes([header[0], header[1]])) + 4;
            if expected > self.storage.len() {
                return Err(ReassemblyError::BufferTooSmall);
            }
            if total > expected {
                return Err(ReassemblyError::Length);
            }
            if u16::from_le_bytes([header[2], header[3]]) == 0 {
                return Err(ReassemblyError::NullChannel);
            }
            Some(expected)
        } else {
            None
        };

        self.storage[offset..total].copy_from_slice(fragment);
        if expected == Some(total) {
            self.received = 0;
            self.expected = None;
            self.active = false;
            Ok(Some(BasicFrame {
                channel_id: u16::from_le_bytes([header[2], header[3]]),
                payload: &self.storage[4..total],
            }))
        } else {
            self.received = total;
            self.expected = expected;
            self.active = true;
            Ok(None)
        }
    }
}

/// BR/EDR signaling channel.
pub const SIGNALING_CHANNEL_ID: u16 = 0x0001;
/// Minimum MTU that a BR/EDR L2CAP implementation must accept.
pub const MINIMUM_MTU: u16 = 48;
/// Default MTU when no MTU option is present.
pub const DEFAULT_MTU: u16 = 672;
const FIRST_DYNAMIC_CHANNEL_ID: u16 = 0x0040;
const CONFIGURATION_MTU_OPTION: u8 = 0x01;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignalError {
    Length,
    Identifier,
    UnsupportedCode,
    InvalidPsm,
    InvalidChannel,
    InvalidResult,
    InvalidStatus,
    Continuation,
    UnsupportedOption,
    DuplicateOption,
    InvalidMtu,
    BufferTooSmall,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum ConnectionResult {
    Success = 0x0000,
    Pending = 0x0001,
    PsmNotSupported = 0x0002,
    SecurityBlock = 0x0003,
    NoResources = 0x0004,
    InvalidSourceChannel = 0x0006,
    SourceChannelAlreadyAllocated = 0x0007,
}

impl ConnectionResult {
    fn parse(raw: u16) -> Result<Self, SignalError> {
        match raw {
            0x0000 => Ok(Self::Success),
            0x0001 => Ok(Self::Pending),
            0x0002 => Ok(Self::PsmNotSupported),
            0x0003 => Ok(Self::SecurityBlock),
            0x0004 => Ok(Self::NoResources),
            0x0006 => Ok(Self::InvalidSourceChannel),
            0x0007 => Ok(Self::SourceChannelAlreadyAllocated),
            _ => Err(SignalError::InvalidResult),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum ConnectionStatus {
    NoFurtherInformation = 0x0000,
    AuthenticationPending = 0x0001,
    AuthorizationPending = 0x0002,
}

impl ConnectionStatus {
    fn parse(raw: u16) -> Result<Self, SignalError> {
        match raw {
            0x0000 => Ok(Self::NoFurtherInformation),
            0x0001 => Ok(Self::AuthenticationPending),
            0x0002 => Ok(Self::AuthorizationPending),
            _ => Err(SignalError::InvalidStatus),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum ConfigurationResult {
    Success = 0x0000,
    UnacceptableParameters = 0x0001,
    Rejected = 0x0002,
    UnknownOptions = 0x0003,
    Pending = 0x0004,
    FlowSpecificationRejected = 0x0005,
}

impl ConfigurationResult {
    fn parse(raw: u16) -> Result<Self, SignalError> {
        match raw {
            0x0000 => Ok(Self::Success),
            0x0001 => Ok(Self::UnacceptableParameters),
            0x0002 => Ok(Self::Rejected),
            0x0003 => Ok(Self::UnknownOptions),
            0x0004 => Ok(Self::Pending),
            0x0005 => Ok(Self::FlowSpecificationRejected),
            _ => Err(SignalError::InvalidResult),
        }
    }
}

/// Supported Basic-mode configuration. Unknown options, including hint options,
/// are rejected instead of silently changing the negotiated contract.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConfigurationOptions {
    pub mtu: Option<u16>,
}

impl ConfigurationOptions {
    pub fn parse(mut bytes: &[u8]) -> Result<Self, SignalError> {
        let mut options = Self::default();
        while !bytes.is_empty() {
            if bytes.len() < 2 {
                return Err(SignalError::Length);
            }
            let option_type = bytes[0];
            let length = usize::from(bytes[1]);
            if bytes.len() < length + 2 {
                return Err(SignalError::Length);
            }
            let value = &bytes[2..length + 2];
            match option_type {
                CONFIGURATION_MTU_OPTION if length == 2 => {
                    if options.mtu.is_some() {
                        return Err(SignalError::DuplicateOption);
                    }
                    let mtu = u16::from_le_bytes([value[0], value[1]]);
                    if mtu < MINIMUM_MTU {
                        return Err(SignalError::InvalidMtu);
                    }
                    options.mtu = Some(mtu);
                }
                CONFIGURATION_MTU_OPTION => return Err(SignalError::Length),
                _ => return Err(SignalError::UnsupportedOption),
            }
            bytes = &bytes[length + 2..];
        }
        Ok(options)
    }

    const fn encoded_len(self) -> usize {
        if self.mtu.is_some() { 4 } else { 0 }
    }

    fn validate(self) -> Result<(), SignalError> {
        if self.mtu.is_some_and(|mtu| mtu < MINIMUM_MTU) {
            return Err(SignalError::InvalidMtu);
        }
        Ok(())
    }

    fn encode(self, output: &mut [u8]) -> usize {
        if let Some(mtu) = self.mtu {
            output[..4].copy_from_slice(&[
                CONFIGURATION_MTU_OPTION,
                2,
                mtu.to_le_bytes()[0],
                mtu.to_le_bytes()[1],
            ]);
            4
        } else {
            0
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
    ConnectionRequest {
        psm: u16,
        source_channel: u16,
    },
    ConnectionResponse {
        destination_channel: u16,
        source_channel: u16,
        result: ConnectionResult,
        status: ConnectionStatus,
    },
    ConfigurationRequest {
        destination_channel: u16,
        options: ConfigurationOptions,
    },
    ConfigurationResponse {
        source_channel: u16,
        result: ConfigurationResult,
        options: ConfigurationOptions,
    },
    DisconnectionRequest {
        destination_channel: u16,
        source_channel: u16,
    },
    DisconnectionResponse {
        destination_channel: u16,
        source_channel: u16,
    },
}

impl Signal {
    const fn code(self) -> u8 {
        match self {
            Self::ConnectionRequest { .. } => 0x02,
            Self::ConnectionResponse { .. } => 0x03,
            Self::ConfigurationRequest { .. } => 0x04,
            Self::ConfigurationResponse { .. } => 0x05,
            Self::DisconnectionRequest { .. } => 0x06,
            Self::DisconnectionResponse { .. } => 0x07,
        }
    }

    const fn data_len(self) -> usize {
        match self {
            Self::ConnectionRequest { .. }
            | Self::DisconnectionRequest { .. }
            | Self::DisconnectionResponse { .. } => 4,
            Self::ConnectionResponse { .. } => 8,
            Self::ConfigurationRequest { options, .. } => 4 + options.encoded_len(),
            Self::ConfigurationResponse { options, .. } => 6 + options.encoded_len(),
        }
    }

    fn validate(self) -> Result<(), SignalError> {
        match self {
            Self::ConnectionRequest {
                psm,
                source_channel,
            } => {
                validate_psm(psm)?;
                validate_dynamic_channel(source_channel)
            }
            Self::ConnectionResponse {
                destination_channel,
                source_channel,
                result,
                status,
            } => {
                validate_dynamic_channel(source_channel)?;
                if result == ConnectionResult::Success {
                    validate_dynamic_channel(destination_channel)?;
                    if status != ConnectionStatus::NoFurtherInformation {
                        return Err(SignalError::InvalidStatus);
                    }
                } else if result == ConnectionResult::Pending {
                    if destination_channel != 0 {
                        validate_dynamic_channel(destination_channel)?;
                    }
                    if status == ConnectionStatus::NoFurtherInformation {
                        return Err(SignalError::InvalidStatus);
                    }
                } else if destination_channel != 0
                    || status != ConnectionStatus::NoFurtherInformation
                {
                    return Err(SignalError::InvalidStatus);
                }
                Ok(())
            }
            Self::ConfigurationRequest {
                destination_channel,
                options,
            } => {
                validate_dynamic_channel(destination_channel)?;
                options.validate()
            }
            Self::ConfigurationResponse {
                source_channel,
                options,
                ..
            } => {
                validate_dynamic_channel(source_channel)?;
                options.validate()
            }
            Self::DisconnectionRequest {
                destination_channel,
                source_channel,
            }
            | Self::DisconnectionResponse {
                destination_channel,
                source_channel,
            } => {
                validate_dynamic_channel(destination_channel)?;
                validate_dynamic_channel(source_channel)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SignalCommand {
    pub identifier: u8,
    pub signal: Signal,
}

impl SignalCommand {
    pub fn parse(bytes: &[u8]) -> Result<Self, SignalError> {
        if bytes.len() < 4 {
            return Err(SignalError::Length);
        }
        let identifier = bytes[1];
        if identifier == 0 {
            return Err(SignalError::Identifier);
        }
        let data_len = usize::from(u16::from_le_bytes([bytes[2], bytes[3]]));
        if bytes.len() != data_len + 4 {
            return Err(SignalError::Length);
        }
        let data = &bytes[4..];
        let word = |offset: usize| u16::from_le_bytes([data[offset], data[offset + 1]]);
        let signal = match bytes[0] {
            0x02 if data.len() == 4 => Signal::ConnectionRequest {
                psm: word(0),
                source_channel: word(2),
            },
            0x03 if data.len() == 8 => Signal::ConnectionResponse {
                destination_channel: word(0),
                source_channel: word(2),
                result: ConnectionResult::parse(word(4))?,
                status: ConnectionStatus::parse(word(6))?,
            },
            0x04 if data.len() >= 4 => {
                if word(2) != 0 {
                    return Err(SignalError::Continuation);
                }
                Signal::ConfigurationRequest {
                    destination_channel: word(0),
                    options: ConfigurationOptions::parse(&data[4..])?,
                }
            }
            0x05 if data.len() >= 6 => {
                if word(2) != 0 {
                    return Err(SignalError::Continuation);
                }
                Signal::ConfigurationResponse {
                    source_channel: word(0),
                    result: ConfigurationResult::parse(word(4))?,
                    options: ConfigurationOptions::parse(&data[6..])?,
                }
            }
            0x06 if data.len() == 4 => Signal::DisconnectionRequest {
                destination_channel: word(0),
                source_channel: word(2),
            },
            0x07 if data.len() == 4 => Signal::DisconnectionResponse {
                destination_channel: word(0),
                source_channel: word(2),
            },
            0x02..=0x07 => return Err(SignalError::Length),
            _ => return Err(SignalError::UnsupportedCode),
        };
        signal.validate()?;
        Ok(Self { identifier, signal })
    }

    /// Encode exactly one signaling command. Failure leaves output unchanged.
    pub fn encode(self, output: &mut [u8]) -> Result<usize, SignalError> {
        if self.identifier == 0 {
            return Err(SignalError::Identifier);
        }
        self.signal.validate()?;
        let data_len = self.signal.data_len();
        let total = data_len + 4;
        if output.len() < total {
            return Err(SignalError::BufferTooSmall);
        }
        output[0] = self.signal.code();
        output[1] = self.identifier;
        output[2..4].copy_from_slice(&(data_len as u16).to_le_bytes());
        match self.signal {
            Signal::ConnectionRequest {
                psm,
                source_channel,
            } => {
                output[4..6].copy_from_slice(&psm.to_le_bytes());
                output[6..8].copy_from_slice(&source_channel.to_le_bytes());
            }
            Signal::ConnectionResponse {
                destination_channel,
                source_channel,
                result,
                status,
            } => {
                output[4..6].copy_from_slice(&destination_channel.to_le_bytes());
                output[6..8].copy_from_slice(&source_channel.to_le_bytes());
                output[8..10].copy_from_slice(&(result as u16).to_le_bytes());
                output[10..12].copy_from_slice(&(status as u16).to_le_bytes());
            }
            Signal::ConfigurationRequest {
                destination_channel,
                options,
            } => {
                output[4..6].copy_from_slice(&destination_channel.to_le_bytes());
                output[6..8].fill(0);
                options.encode(&mut output[8..total]);
            }
            Signal::ConfigurationResponse {
                source_channel,
                result,
                options,
            } => {
                output[4..6].copy_from_slice(&source_channel.to_le_bytes());
                output[6..8].fill(0);
                output[8..10].copy_from_slice(&(result as u16).to_le_bytes());
                options.encode(&mut output[10..total]);
            }
            Signal::DisconnectionRequest {
                destination_channel,
                source_channel,
            }
            | Signal::DisconnectionResponse {
                destination_channel,
                source_channel,
            } => {
                output[4..6].copy_from_slice(&destination_channel.to_le_bytes());
                output[6..8].copy_from_slice(&source_channel.to_le_bytes());
            }
        }
        Ok(total)
    }
}

const fn validate_psm(psm: u16) -> Result<(), SignalError> {
    if psm & 0x0101 == 0x0001 {
        Ok(())
    } else {
        Err(SignalError::InvalidPsm)
    }
}

const fn validate_dynamic_channel(channel: u16) -> Result<(), SignalError> {
    if channel >= FIRST_DYNAMIC_CHANNEL_ID {
        Ok(())
    } else {
        Err(SignalError::InvalidChannel)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelPhase {
    Closed,
    WaitConnection,
    Configuring,
    Open,
    WaitDisconnection,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelEvent {
    ConnectionStarted,
    ConfigurationStarted,
    PeerConfigured,
    Opened,
    DisconnectStarted,
    Closed,
    ConnectionRejected(ConnectionResult),
    ConfigurationRejected(ConfigurationResult),
    ConnectionTimedOut,
    ConfigurationTimedOut,
    DisconnectionTimedOut,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChannelProgress {
    pub event: ChannelEvent,
    /// Bytes of a single outbound signaling command in the caller's buffer.
    pub output_len: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelError {
    ForeignConnection,
    InvalidEpoch,
    InvalidTransition,
    StaleIdentifier,
    WrongChannel,
    Signal(SignalError),
}

impl From<SignalError> for ChannelError {
    fn from(value: SignalError) -> Self {
        Self::Signal(value)
    }
}

/// One initiator-side BR/EDR Basic-mode channel for a caller-owned connection.
///
/// The caller serializes signaling, supplies deadlines, and calls [`Self::timeout`]
/// explicitly. This machine never retries, advances time, allocates, or relabels
/// another handle/epoch. At most one locally initiated transaction is pending.
pub struct ClassicChannel {
    connection: Connection,
    psm: u16,
    local_channel: u16,
    remote_channel: Option<u16>,
    local_mtu: u16,
    peer_mtu: u16,
    phase: ChannelPhase,
    pending_identifier: Option<u8>,
    next_identifier: u8,
    local_configured: bool,
    peer_configured: bool,
}

impl ClassicChannel {
    pub fn new(
        connection: Connection,
        psm: u16,
        local_channel: u16,
        local_mtu: u16,
        first_identifier: u8,
    ) -> Result<Self, ChannelError> {
        validate_psm(psm)?;
        validate_dynamic_channel(local_channel)?;
        if local_mtu < MINIMUM_MTU {
            return Err(SignalError::InvalidMtu.into());
        }
        if first_identifier == 0 {
            return Err(SignalError::Identifier.into());
        }
        Ok(Self {
            connection,
            psm,
            local_channel,
            remote_channel: None,
            local_mtu,
            peer_mtu: DEFAULT_MTU,
            phase: ChannelPhase::Closed,
            pending_identifier: None,
            next_identifier: first_identifier,
            local_configured: false,
            peer_configured: false,
        })
    }

    pub const fn connection(&self) -> Connection {
        self.connection
    }

    pub const fn phase(&self) -> ChannelPhase {
        self.phase
    }

    pub const fn local_channel(&self) -> u16 {
        self.local_channel
    }

    pub const fn remote_channel(&self) -> Option<u16> {
        self.remote_channel
    }

    pub const fn local_mtu(&self) -> u16 {
        self.local_mtu
    }

    pub const fn peer_mtu(&self) -> u16 {
        self.peer_mtu
    }

    pub const fn pending_identifier(&self) -> Option<u8> {
        self.pending_identifier
    }

    pub fn connect(
        &mut self,
        connection: Connection,
        output: &mut [u8],
    ) -> Result<ChannelProgress, ChannelError> {
        self.check_connection(connection)?;
        if self.phase != ChannelPhase::Closed {
            return Err(ChannelError::InvalidTransition);
        }
        let identifier = self.next_identifier;
        let length = SignalCommand {
            identifier,
            signal: Signal::ConnectionRequest {
                psm: self.psm,
                source_channel: self.local_channel,
            },
        }
        .encode(output)?;
        self.allocate_identifier();
        self.phase = ChannelPhase::WaitConnection;
        self.pending_identifier = Some(identifier);
        Ok(ChannelProgress {
            event: ChannelEvent::ConnectionStarted,
            output_len: length,
        })
    }

    pub fn receive(
        &mut self,
        connection: Connection,
        command: SignalCommand,
        output: &mut [u8],
    ) -> Result<ChannelProgress, ChannelError> {
        self.check_connection(connection)?;
        if command.identifier == 0 {
            return Err(SignalError::Identifier.into());
        }
        command.signal.validate()?;
        match command.signal {
            Signal::ConnectionResponse {
                destination_channel,
                source_channel,
                result,
                status,
            } => self.receive_connection_response(
                command.identifier,
                destination_channel,
                source_channel,
                result,
                status,
                output,
            ),
            Signal::ConfigurationRequest {
                destination_channel,
                options,
            } => self.receive_configuration_request(
                command.identifier,
                destination_channel,
                options,
                output,
            ),
            Signal::ConfigurationResponse {
                source_channel,
                result,
                options,
            } => self.receive_configuration_response(
                command.identifier,
                source_channel,
                result,
                options,
            ),
            Signal::DisconnectionRequest {
                destination_channel,
                source_channel,
            } => self.receive_disconnection_request(
                command.identifier,
                destination_channel,
                source_channel,
                output,
            ),
            Signal::DisconnectionResponse {
                destination_channel,
                source_channel,
            } => self.receive_disconnection_response(
                command.identifier,
                destination_channel,
                source_channel,
            ),
            Signal::ConnectionRequest { .. } => Err(ChannelError::InvalidTransition),
        }
    }

    pub fn disconnect(
        &mut self,
        connection: Connection,
        output: &mut [u8],
    ) -> Result<ChannelProgress, ChannelError> {
        self.check_connection(connection)?;
        if !matches!(self.phase, ChannelPhase::Configuring | ChannelPhase::Open) {
            return Err(ChannelError::InvalidTransition);
        }
        let remote_channel = self.remote_channel.ok_or(ChannelError::InvalidTransition)?;
        let identifier = self.next_identifier;
        let length = SignalCommand {
            identifier,
            signal: Signal::DisconnectionRequest {
                destination_channel: remote_channel,
                source_channel: self.local_channel,
            },
        }
        .encode(output)?;
        self.allocate_identifier();
        self.phase = ChannelPhase::WaitDisconnection;
        self.pending_identifier = Some(identifier);
        Ok(ChannelProgress {
            event: ChannelEvent::DisconnectStarted,
            output_len: length,
        })
    }

    /// Cancel connection setup locally, or start exactly one peer-visible
    /// disconnection after a remote channel exists.
    pub fn cancel(
        &mut self,
        connection: Connection,
        output: &mut [u8],
    ) -> Result<ChannelProgress, ChannelError> {
        self.check_connection(connection)?;
        match self.phase {
            ChannelPhase::WaitConnection => {
                self.clear_channel();
                Ok(ChannelProgress {
                    event: ChannelEvent::Closed,
                    output_len: 0,
                })
            }
            ChannelPhase::Configuring | ChannelPhase::Open => self.disconnect(connection, output),
            ChannelPhase::Closed | ChannelPhase::WaitDisconnection => {
                Err(ChannelError::InvalidTransition)
            }
        }
    }

    /// Apply a caller-owned deadline to the exact locally pending transaction.
    /// Stale identifiers do not alter state. A configuration timeout retains the
    /// allocated remote channel so the caller can invoke [`Self::cancel`].
    pub fn timeout(
        &mut self,
        connection: Connection,
        identifier: u8,
    ) -> Result<ChannelProgress, ChannelError> {
        self.check_connection(connection)?;
        if self.pending_identifier != Some(identifier) {
            return Err(ChannelError::StaleIdentifier);
        }
        let event = match self.phase {
            ChannelPhase::WaitConnection => {
                self.clear_channel();
                ChannelEvent::ConnectionTimedOut
            }
            ChannelPhase::Configuring => {
                self.pending_identifier = None;
                ChannelEvent::ConfigurationTimedOut
            }
            ChannelPhase::WaitDisconnection => {
                self.clear_channel();
                ChannelEvent::DisconnectionTimedOut
            }
            ChannelPhase::Closed | ChannelPhase::Open => {
                return Err(ChannelError::InvalidTransition);
            }
        };
        Ok(ChannelProgress {
            event,
            output_len: 0,
        })
    }

    /// Rebind only after caller-managed teardown has established a newer epoch.
    pub fn reset(&mut self, connection: Connection) -> Result<(), ChannelError> {
        if connection.epoch <= self.connection.epoch {
            return Err(ChannelError::InvalidEpoch);
        }
        self.connection = connection;
        self.clear_channel();
        Ok(())
    }

    fn receive_connection_response(
        &mut self,
        identifier: u8,
        destination_channel: u16,
        source_channel: u16,
        result: ConnectionResult,
        status: ConnectionStatus,
        output: &mut [u8],
    ) -> Result<ChannelProgress, ChannelError> {
        if self.phase != ChannelPhase::WaitConnection {
            return Err(ChannelError::InvalidTransition);
        }
        self.check_identifier(identifier)?;
        if source_channel != self.local_channel {
            return Err(ChannelError::WrongChannel);
        }
        if result == ConnectionResult::Pending {
            return Err(ChannelError::InvalidTransition);
        }
        if result != ConnectionResult::Success {
            self.clear_channel();
            return Ok(ChannelProgress {
                event: ChannelEvent::ConnectionRejected(result),
                output_len: 0,
            });
        }
        if status != ConnectionStatus::NoFurtherInformation {
            return Err(ChannelError::Signal(SignalError::InvalidStatus));
        }
        validate_dynamic_channel(destination_channel)?;
        let next_identifier = self.next_identifier;
        let length = SignalCommand {
            identifier: next_identifier,
            signal: Signal::ConfigurationRequest {
                destination_channel,
                options: ConfigurationOptions {
                    mtu: Some(self.local_mtu),
                },
            },
        }
        .encode(output)?;
        self.allocate_identifier();
        self.remote_channel = Some(destination_channel);
        self.phase = ChannelPhase::Configuring;
        self.pending_identifier = Some(next_identifier);
        self.local_configured = false;
        self.peer_configured = false;
        Ok(ChannelProgress {
            event: ChannelEvent::ConfigurationStarted,
            output_len: length,
        })
    }

    fn receive_configuration_request(
        &mut self,
        identifier: u8,
        destination_channel: u16,
        options: ConfigurationOptions,
        output: &mut [u8],
    ) -> Result<ChannelProgress, ChannelError> {
        if self.phase != ChannelPhase::Configuring || destination_channel != self.local_channel {
            return Err(if destination_channel != self.local_channel {
                ChannelError::WrongChannel
            } else {
                ChannelError::InvalidTransition
            });
        }
        let length = SignalCommand {
            identifier,
            signal: Signal::ConfigurationResponse {
                source_channel: self.local_channel,
                result: ConfigurationResult::Success,
                options: ConfigurationOptions::default(),
            },
        }
        .encode(output)?;
        self.peer_mtu = options.mtu.unwrap_or(DEFAULT_MTU);
        self.peer_configured = true;
        let event = if self.local_configured {
            self.phase = ChannelPhase::Open;
            ChannelEvent::Opened
        } else {
            ChannelEvent::PeerConfigured
        };
        Ok(ChannelProgress {
            event,
            output_len: length,
        })
    }

    fn receive_configuration_response(
        &mut self,
        identifier: u8,
        source_channel: u16,
        result: ConfigurationResult,
        options: ConfigurationOptions,
    ) -> Result<ChannelProgress, ChannelError> {
        if self.phase != ChannelPhase::Configuring {
            return Err(ChannelError::InvalidTransition);
        }
        self.check_identifier(identifier)?;
        if Some(source_channel) != self.remote_channel {
            return Err(ChannelError::WrongChannel);
        }
        if result == ConfigurationResult::Pending {
            return Err(ChannelError::InvalidTransition);
        }
        if result != ConfigurationResult::Success {
            self.pending_identifier = None;
            return Ok(ChannelProgress {
                event: ChannelEvent::ConfigurationRejected(result),
                output_len: 0,
            });
        }
        if options != ConfigurationOptions::default() {
            return Err(ChannelError::Signal(SignalError::UnsupportedOption));
        }
        self.pending_identifier = None;
        self.local_configured = true;
        let event = if self.peer_configured {
            self.phase = ChannelPhase::Open;
            ChannelEvent::Opened
        } else {
            ChannelEvent::ConfigurationStarted
        };
        Ok(ChannelProgress {
            event,
            output_len: 0,
        })
    }

    fn receive_disconnection_request(
        &mut self,
        identifier: u8,
        destination_channel: u16,
        source_channel: u16,
        output: &mut [u8],
    ) -> Result<ChannelProgress, ChannelError> {
        if !matches!(
            self.phase,
            ChannelPhase::Configuring | ChannelPhase::Open | ChannelPhase::WaitDisconnection
        ) {
            return Err(ChannelError::InvalidTransition);
        }
        self.check_channel_pair(destination_channel, source_channel, false)?;
        let length = SignalCommand {
            identifier,
            signal: Signal::DisconnectionResponse {
                destination_channel,
                source_channel,
            },
        }
        .encode(output)?;
        self.clear_channel();
        Ok(ChannelProgress {
            event: ChannelEvent::Closed,
            output_len: length,
        })
    }

    fn receive_disconnection_response(
        &mut self,
        identifier: u8,
        destination_channel: u16,
        source_channel: u16,
    ) -> Result<ChannelProgress, ChannelError> {
        if self.phase != ChannelPhase::WaitDisconnection {
            return Err(ChannelError::InvalidTransition);
        }
        self.check_identifier(identifier)?;
        self.check_channel_pair(destination_channel, source_channel, true)?;
        self.clear_channel();
        Ok(ChannelProgress {
            event: ChannelEvent::Closed,
            output_len: 0,
        })
    }

    fn check_connection(&self, connection: Connection) -> Result<(), ChannelError> {
        if connection == self.connection {
            Ok(())
        } else {
            Err(ChannelError::ForeignConnection)
        }
    }

    fn check_identifier(&self, identifier: u8) -> Result<(), ChannelError> {
        if self.pending_identifier == Some(identifier) {
            Ok(())
        } else {
            Err(ChannelError::StaleIdentifier)
        }
    }

    fn check_channel_pair(
        &self,
        destination_channel: u16,
        source_channel: u16,
        response: bool,
    ) -> Result<(), ChannelError> {
        let remote = self.remote_channel.ok_or(ChannelError::InvalidTransition)?;
        let expected = if response {
            (remote, self.local_channel)
        } else {
            (self.local_channel, remote)
        };
        if (destination_channel, source_channel) == expected {
            Ok(())
        } else {
            Err(ChannelError::WrongChannel)
        }
    }

    fn allocate_identifier(&mut self) -> u8 {
        let identifier = self.next_identifier;
        self.next_identifier = if identifier == u8::MAX {
            1
        } else {
            identifier + 1
        };
        identifier
    }

    fn clear_channel(&mut self) {
        self.remote_channel = None;
        self.peer_mtu = DEFAULT_MTU;
        self.phase = ChannelPhase::Closed;
        self.pending_identifier = None;
        self.local_configured = false;
        self.peer_configured = false;
    }
}
