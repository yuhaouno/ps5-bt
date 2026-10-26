// SPDX-License-Identifier: GPL-3.0-or-later

use crate::{
    CapabilityError, CompleteMessage, Header, Message, MessageType, PacketType, ParseError,
    ReassemblyError, SbcConfiguration, ServiceCapabilities, SignalId, SignalingContext,
    SignalingReassembler, StreamEndpointId, StreamEndpoints,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamState {
    Idle,
    Configured,
    TransportOpening,
    Open,
    Streaming,
    TransportClosing,
    Aborting,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RejectDetail {
    None,
    ServiceCategory(u8),
    Endpoint(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RemoteReject {
    pub signal: u8,
    pub error_code: Option<u8>,
    pub detail: RejectDetail,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    BufferTooSmall,
    Busy,
    InvalidState,
    InvalidEpoch,
    EpochNotAdvanced,
    NoPendingTransaction,
    StaleTransaction,
    TransactionExhausted,
    Collision,
    UnexpectedCommand,
    FragmentationUnsupported,
    Malformed(ParseError),
    Capability(CapabilityError),
}

impl From<ParseError> for Error {
    fn from(value: ParseError) -> Self {
        Self::Malformed(value)
    }
}

impl From<CapabilityError> for Error {
    fn from(value: CapabilityError) -> Self {
        Self::Capability(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReceiveError {
    Session(Error),
    Reassembly(ReassemblyError),
}

impl From<Error> for ReceiveError {
    fn from(value: Error) -> Self {
        Self::Session(value)
    }
}

impl From<ReassemblyError> for ReceiveError {
    fn from(value: ReassemblyError) -> Self {
        Self::Reassembly(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Command {
    pub epoch: u64,
    pub transaction_label: u8,
    pub signal: SignalId,
    pub bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PendingCommand {
    pub epoch: u64,
    pub transaction_label: u8,
    pub signal: SignalId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Effect {
    None,
    Configure {
        remote: StreamEndpointId,
        local: StreamEndpointId,
        configuration: SbcConfiguration,
    },
    Open,
    Start,
    Suspend,
    Close,
    Abort,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Pending {
    command: PendingCommand,
    generation: u64,
    effect: Effect,
    previous_state: StreamState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Accepted<'a> {
    Discovery(StreamEndpoints<'a>),
    Capabilities(ServiceCapabilities<'a>),
    Acknowledged(SignalId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome<'a> {
    Accepted(Accepted<'a>),
    Rejected(RemoteReject),
}

/// One initiator-side signaling transaction at a time. The caller must bind
/// `epoch` to its signaling connection and supply a strictly newer value after
/// timeout or disconnect. No method retries, sleeps or owns transport buffers.
pub struct Initiator {
    epoch: u64,
    state: StreamState,
    next_label: u8,
    next_generation: u64,
    pending: Option<Pending>,
    remote: Option<StreamEndpointId>,
    local: Option<StreamEndpointId>,
    configuration: Option<SbcConfiguration>,
}

impl Initiator {
    pub const fn new(epoch: u64) -> Self {
        Self {
            epoch,
            state: StreamState::Idle,
            next_label: 0,
            next_generation: 0,
            pending: None,
            remote: None,
            local: None,
            configuration: None,
        }
    }

    pub const fn epoch(&self) -> u64 {
        self.epoch
    }

    pub const fn state(&self) -> StreamState {
        self.state
    }

    pub const fn pending(&self) -> Option<PendingCommand> {
        match self.pending {
            Some(pending) => Some(pending.command),
            None => None,
        }
    }

    pub const fn remote_endpoint(&self) -> Option<StreamEndpointId> {
        self.remote
    }

    pub const fn local_endpoint(&self) -> Option<StreamEndpointId> {
        self.local
    }

    pub const fn configuration(&self) -> Option<SbcConfiguration> {
        self.configuration
    }

    pub fn discover(&mut self, output: &mut [u8]) -> Result<Command, Error> {
        self.emit(SignalId::Discover, &[], Effect::None, output)
    }

    pub fn get_capabilities(
        &mut self,
        remote: StreamEndpointId,
        output: &mut [u8],
    ) -> Result<Command, Error> {
        self.emit(
            SignalId::GetCapabilities,
            &[remote.wire()],
            Effect::None,
            output,
        )
    }

    pub fn set_configuration(
        &mut self,
        remote: StreamEndpointId,
        local: StreamEndpointId,
        configuration: SbcConfiguration,
        output: &mut [u8],
    ) -> Result<Command, Error> {
        if self.state != StreamState::Idle {
            return Err(Error::InvalidState);
        }
        let mut payload = [0u8; 12];
        payload[0] = remote.wire();
        payload[1] = local.wire();
        payload[2] = 0x01; // Media Transport.
        payload[3] = 0;
        let mut codec = [0u8; 8];
        configuration.encode_media_codec(&mut codec);
        payload[4..].copy_from_slice(&codec);
        self.emit(
            SignalId::SetConfiguration,
            &payload,
            Effect::Configure {
                remote,
                local,
                configuration,
            },
            output,
        )
    }

    pub fn open(&mut self, output: &mut [u8]) -> Result<Command, Error> {
        if self.state != StreamState::Configured {
            return Err(Error::InvalidState);
        }
        let remote = self.remote.ok_or(Error::InvalidState)?;
        self.emit(SignalId::Open, &[remote.wire()], Effect::Open, output)
    }

    /// Called only after the caller has established the media transport channel
    /// for the same epoch following an accepted Open response.
    pub fn transport_opened(&mut self, epoch: u64) -> Result<(), Error> {
        if epoch != self.epoch {
            return Err(Error::InvalidEpoch);
        }
        if self.pending.is_some() || self.state != StreamState::TransportOpening {
            return Err(Error::InvalidState);
        }
        self.state = StreamState::Open;
        Ok(())
    }

    pub fn start(&mut self, output: &mut [u8]) -> Result<Command, Error> {
        if self.state != StreamState::Open {
            return Err(Error::InvalidState);
        }
        let remote = self.remote.ok_or(Error::InvalidState)?;
        self.emit(SignalId::Start, &[remote.wire()], Effect::Start, output)
    }

    pub fn suspend(&mut self, output: &mut [u8]) -> Result<Command, Error> {
        if self.state != StreamState::Streaming {
            return Err(Error::InvalidState);
        }
        let remote = self.remote.ok_or(Error::InvalidState)?;
        self.emit(SignalId::Suspend, &[remote.wire()], Effect::Suspend, output)
    }

    pub fn close(&mut self, output: &mut [u8]) -> Result<Command, Error> {
        if !matches!(self.state, StreamState::Open | StreamState::Streaming) {
            return Err(Error::InvalidState);
        }
        let remote = self.remote.ok_or(Error::InvalidState)?;
        self.emit(SignalId::Close, &[remote.wire()], Effect::Close, output)
    }

    pub fn abort(&mut self, output: &mut [u8]) -> Result<Command, Error> {
        let remote = self.remote.ok_or(Error::InvalidState)?;
        self.emit(SignalId::Abort, &[remote.wire()], Effect::Abort, output)
    }

    /// Completes bounded lower-transport cleanup after accepted Close or Abort.
    pub fn transport_closed(&mut self, epoch: u64) -> Result<(), Error> {
        if epoch != self.epoch {
            return Err(Error::InvalidEpoch);
        }
        if self.pending.is_some()
            || !matches!(
                self.state,
                StreamState::TransportClosing | StreamState::Aborting
            )
        {
            return Err(Error::InvalidState);
        }
        self.clear_stream();
        Ok(())
    }

    /// A timeout is transport ambiguity, not permission to retry. The exact
    /// pending label must match and the caller must allocate a newer epoch.
    pub fn timeout(
        &mut self,
        epoch: u64,
        transaction_label: u8,
        new_epoch: u64,
    ) -> Result<PendingCommand, Error> {
        if epoch != self.epoch {
            return Err(Error::InvalidEpoch);
        }
        let pending = self.pending.ok_or(Error::NoPendingTransaction)?;
        if pending.command.transaction_label != transaction_label {
            return Err(Error::StaleTransaction);
        }
        self.reset_epoch(new_epoch)?;
        Ok(pending.command)
    }

    /// Invalidates all stream and transaction state on signaling disconnect.
    pub fn disconnected(&mut self, epoch: u64, new_epoch: u64) -> Result<(), Error> {
        if epoch != self.epoch {
            return Err(Error::InvalidEpoch);
        }
        self.reset_epoch(new_epoch)
    }

    pub fn receive<'a>(&mut self, epoch: u64, bytes: &'a [u8]) -> Result<Outcome<'a>, Error> {
        if epoch != self.epoch {
            return Err(Error::InvalidEpoch);
        }
        let message = Message::parse(bytes)?;
        let header = message.header();
        if header.packet_type() != PacketType::Single {
            return Err(Error::FragmentationUnsupported);
        }
        self.receive_message(CompleteMessage::single(message))
    }

    /// Consumes one ordered signaling L2CAP SDU. A partial response returns
    /// `None`; completion uses exactly the same semantic validation as receive.
    /// Start/Single correlation is checked before any reassembly storage write.
    /// A timeout does not clear the pending command: explicitly invalidate the
    /// Initiator and reset the reassembler to the same newer epoch.
    pub fn receive_packet<'a>(
        &mut self,
        reassembler: &'a mut SignalingReassembler<'_>,
        context: SignalingContext,
        now_us: u64,
        packet: &[u8],
    ) -> Result<Option<Outcome<'a>>, ReceiveError> {
        if context.epoch != self.epoch {
            return Err(Error::InvalidEpoch.into());
        }
        let parsed = reassembler.preflight(context, now_us, packet)?;
        let header = parsed.header();
        let pending = if matches!(header.packet_type(), PacketType::Single | PacketType::Start) {
            self.validate_response(
                header.transaction_label(),
                header.message_type(),
                header.raw_signal_id(),
            )?
        } else if let Some(partial) = reassembler.partial() {
            self.validate_response(
                partial.transaction_label,
                partial.message_type,
                Some(partial.signal_id),
            )?
        } else {
            return Err(ReassemblyError::NoStart.into());
        };
        match reassembler.push_parsed(now_us, packet.len(), parsed, Some(pending.generation))? {
            Some(message) => Ok(Some(self.receive_message(message)?)),
            None => Ok(None),
        }
    }

    fn validate_response(
        &self,
        transaction_label: u8,
        message_type: MessageType,
        raw_signal_id: Option<u8>,
    ) -> Result<Pending, Error> {
        let pending = self.pending.ok_or(Error::NoPendingTransaction)?;
        if message_type == MessageType::Command {
            return if raw_signal_id == Some(pending.command.signal.raw()) {
                Err(Error::Collision)
            } else {
                Err(Error::UnexpectedCommand)
            };
        }
        if transaction_label != pending.command.transaction_label
            || (message_type != MessageType::GeneralReject
                && raw_signal_id != Some(pending.command.signal.raw()))
        {
            return Err(Error::StaleTransaction);
        }
        Ok(pending)
    }

    fn receive_message<'a>(&mut self, message: CompleteMessage<'a>) -> Result<Outcome<'a>, Error> {
        let pending = self.validate_response(
            message.transaction_label(),
            message.message_type(),
            message.raw_signal_id(),
        )?;
        match message.message_type() {
            MessageType::Accept => {
                let accepted = Self::parse_accept(pending.command.signal, message.payload())?;
                self.apply_accept(pending.effect);
                self.pending = None;
                Ok(Outcome::Accepted(accepted))
            }
            MessageType::Reject => {
                let reject = self.parse_reject(pending.command.signal, message.payload())?;
                self.apply_reject(pending);
                self.pending = None;
                Ok(Outcome::Rejected(reject))
            }
            MessageType::GeneralReject => {
                let reject = RemoteReject {
                    signal: pending.command.signal.raw(),
                    error_code: None,
                    detail: RejectDetail::None,
                };
                self.apply_reject(pending);
                self.pending = None;
                Ok(Outcome::Rejected(reject))
            }
            MessageType::Command => unreachable!(),
        }
    }

    fn emit(
        &mut self,
        signal: SignalId,
        payload: &[u8],
        effect: Effect,
        output: &mut [u8],
    ) -> Result<Command, Error> {
        if self.pending.is_some() {
            return Err(Error::Busy);
        }
        let total = 2usize
            .checked_add(payload.len())
            .ok_or(Error::BufferTooSmall)?;
        if output.len() < total {
            return Err(Error::BufferTooSmall);
        }
        let next_generation = self
            .next_generation
            .checked_add(1)
            .ok_or(Error::TransactionExhausted)?;
        let label = self.next_label;
        let header = Header::single(label, MessageType::Command, signal)?;
        header.encode(&mut output[..2])?;
        output[2..total].copy_from_slice(payload);

        let previous_state = self.state;
        if matches!(effect, Effect::Close) {
            self.state = StreamState::TransportClosing;
        } else if matches!(effect, Effect::Abort) {
            self.state = StreamState::Aborting;
        }
        let command = Command {
            epoch: self.epoch,
            transaction_label: label,
            signal,
            bytes: total,
        };
        self.pending = Some(Pending {
            generation: self.next_generation,
            command: PendingCommand {
                epoch: self.epoch,
                transaction_label: label,
                signal,
            },
            effect,
            previous_state,
        });
        self.next_label = (label + 1) & 0x0f;
        self.next_generation = next_generation;
        Ok(command)
    }

    fn parse_accept<'a>(signal: SignalId, payload: &'a [u8]) -> Result<Accepted<'a>, Error> {
        match signal {
            SignalId::Discover => Ok(Accepted::Discovery(StreamEndpoints::parse(payload)?)),
            SignalId::GetCapabilities | SignalId::GetAllCapabilities => {
                Ok(Accepted::Capabilities(ServiceCapabilities::parse(payload)?))
            }
            SignalId::SetConfiguration
            | SignalId::Open
            | SignalId::Start
            | SignalId::Close
            | SignalId::Suspend
            | SignalId::Abort => {
                if payload.is_empty() {
                    Ok(Accepted::Acknowledged(signal))
                } else {
                    Err(Error::Malformed(ParseError::InvalidHeader))
                }
            }
            _ => Err(Error::Malformed(ParseError::InvalidHeader)),
        }
    }

    fn parse_reject(&self, signal: SignalId, payload: &[u8]) -> Result<RemoteReject, Error> {
        let (error_code, detail) = match signal {
            SignalId::SetConfiguration | SignalId::Reconfigure => match payload {
                [category, code] => (*code, RejectDetail::ServiceCategory(*category)),
                _ => return Err(Error::Malformed(ParseError::InvalidHeader)),
            },
            SignalId::Start | SignalId::Suspend => match payload {
                [seid, code] => {
                    let endpoint = StreamEndpointId::parse_wire(*seid)?;
                    if self.remote != Some(endpoint) {
                        return Err(Error::Malformed(ParseError::InvalidEndpoint));
                    }
                    (*code, RejectDetail::Endpoint(endpoint.value()))
                }
                _ => return Err(Error::Malformed(ParseError::InvalidHeader)),
            },
            SignalId::Abort => return Err(Error::Malformed(ParseError::InvalidHeader)),
            _ => match payload {
                [code] => (*code, RejectDetail::None),
                _ => return Err(Error::Malformed(ParseError::InvalidHeader)),
            },
        };
        Ok(RemoteReject {
            signal: signal.raw(),
            error_code: Some(error_code),
            detail,
        })
    }

    fn apply_accept(&mut self, effect: Effect) {
        match effect {
            Effect::None => {}
            Effect::Configure {
                remote,
                local,
                configuration,
            } => {
                self.remote = Some(remote);
                self.local = Some(local);
                self.configuration = Some(configuration);
                self.state = StreamState::Configured;
            }
            Effect::Open => self.state = StreamState::TransportOpening,
            Effect::Start => self.state = StreamState::Streaming,
            Effect::Suspend => self.state = StreamState::Open,
            Effect::Close => self.state = StreamState::TransportClosing,
            Effect::Abort => self.state = StreamState::Aborting,
        }
    }

    fn apply_reject(&mut self, pending: Pending) {
        if matches!(pending.effect, Effect::Close | Effect::Abort) {
            self.state = pending.previous_state;
        }
    }

    fn reset_epoch(&mut self, new_epoch: u64) -> Result<(), Error> {
        if new_epoch <= self.epoch {
            return Err(Error::EpochNotAdvanced);
        }
        self.epoch = new_epoch;
        self.pending = None;
        self.next_label = 0;
        self.next_generation = 0;
        self.clear_stream();
        Ok(())
    }

    fn clear_stream(&mut self) {
        self.state = StreamState::Idle;
        self.remote = None;
        self.local = None;
        self.configuration = None;
    }
}
