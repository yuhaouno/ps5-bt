// SPDX-License-Identifier: GPL-3.0-or-later
//! Caller-driven A2DP source binding; no radio, capture, encoder or platform calls.
#![no_std]
#![forbid(unsafe_code)]

use bt_a2dp::{PacketError, SbcRtpPacketizer};
use bt_avdtp::{
    Accepted, Command, Initiator, Outcome, RemoteReject, SbcConfiguration, SignalId,
    StreamEndpointId,
};
use bt_core::{BluetoothTransport, Error as TransportError};
use bt_hci::flow::AclCreditOwner;
use bt_l2cap::{Connection, TerminalFailure};
use bt_media_session::{
    DroppedPacket, MediaSession, QueuedPacket, SendProgress, TransmitError, TransmitOutcome,
};
use bt_sdp::SinkService;

/// Externally established Basic-mode channel. Identifiers/MTU are negotiated,
/// not guessed. The dispatcher must retain the original connection epoch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Channel {
    pub connection: Connection,
    pub local_cid: u16,
    pub remote_cid: u16,
    pub peer_mtu: u16,
}

impl Channel {
    fn validate(self) -> Result<(), Error> {
        if self.local_cid < 0x0040 || self.remote_cid < 0x0040 || self.peer_mtu < 48 {
            return Err(Error::InvalidChannel);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    pub signal_timeout_us: u64,
    pub media_open_timeout_us: u64,
    pub stop_timeout_us: u64,
    pub bitpool: u8,
    pub payload_type: u8,
    pub ssrc: u32,
    pub first_sequence: u16,
    pub first_timestamp: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Discovering,
    Capabilities,
    Configuring,
    Opening,
    AwaitMedia,
    Starting,
    Streaming,
    Draining,
    Suspending,
    Closing,
    AwaitMediaClose,
    Stopped,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    Deadline,
    ClockRegressed,
    Overflow,
    Cancelled,
    Disconnected,
    Transport(TransportError),
    PeerRejected(RemoteReject),
    NoAudioSink,
    UnsupportedCapabilities,
    MediaSubmission,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidConfig,
    InvalidChannel,
    ForeignContext,
    InvalidState,
    LinkNotReady,
    StaleSubmission,
    ResponseBeforeAcceptance,
    OutputTooSmall,
    PduStorageTooSmall,
    InvalidAclLimit,
    InvalidSamplePosition,
    Avdtp(bt_avdtp::Error),
    Receive(bt_avdtp::ReceiveError),
    Packet(PacketError),
    Media(bt_media_session::Error),
    Failed(Failure),
}

impl From<bt_avdtp::Error> for Error {
    fn from(value: bt_avdtp::Error) -> Self {
        Self::Avdtp(value)
    }
}

/// An immutable retry identity for a complete signaling SDU, not an HCI credit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SignalSubmission {
    id: u64,
    pub connection: Connection,
    pub remote_cid: u16,
    pub command: Command,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Pending {
    submission: SignalSubmission,
    accepted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub phase: Phase,
    pub failure: Option<Failure>,
    pub terminal_failure: Option<TerminalFailure>,
    pub deadline_us: Option<u64>,
    pub pending: Option<SignalSubmission>,
    pub signal_accepted: bool,
    pub media_queued: bool,
}

impl Snapshot {
    /// Ending a local session never proves ownership/credit reconciliation.
    pub const fn recovery_failure(self) -> Option<bt_core::recovery::Failure> {
        match self.terminal_failure {
            None => None,
            Some(TerminalFailure::Disconnected) => Some(bt_core::recovery::Failure::Retryable),
            Some(_) => Some(bt_core::recovery::Failure::IntegrityLost),
        }
    }
}

/// Connects the existing SBC initiator to negotiated channels and media sender.
///
/// Construction requires an externally authenticated/encrypted owned link and
/// SDP SinkService established on that same context. This type cannot verify
/// ownership from numeric handles. It never establishes PS5 transport access.
/// Commands are retained until whole-SDU acceptance, and responses are rejected
/// before that acceptance. Signaling transport uncertainty invalidates this
/// session; it is never retried as Busy or used to manufacture a fresh epoch.
pub struct SourceSession<'storage> {
    config: Config,
    signaling: Channel,
    initiator: Initiator,
    local_endpoint: StreamEndpointId,
    remote_endpoint: Option<StreamEndpointId>,
    phase: Phase,
    failure: Option<Failure>,
    terminal_failure: Option<TerminalFailure>,
    pending: Option<Pending>,
    next_submission: u64,
    command_bytes: [u8; 64],
    last_now: u64,
    deadline: Option<u64>,
    stop_deadline: Option<u64>,
    pdu_storage: Option<&'storage mut [u8]>,
    media: Option<MediaSession<'storage>>,
    media_channel: Option<Channel>,
    cancellation: Option<DroppedPacket>,
    next_media_sample: u64,
}

impl<'storage> SourceSession<'storage> {
    /// Bind the portable Classic link only after authentication, encryption and
    /// minimum-key-size checks reached Ready. Before each source operation, the
    /// same owner must gate traffic if `ready_connection()` no longer matches,
    /// including security revalidation without a terminal link failure, and
    /// forward later failures. This borrow does not own the physical link.
    pub fn from_link(
        link: &bt_link::LinkSession,
        signaling: Channel,
        service: SinkService,
        local_endpoint: StreamEndpointId,
        config: Config,
        pdu_storage: &'storage mut [u8],
        now_us: u64,
    ) -> Result<Self, Error> {
        let (handle, epoch) = link.ready_connection().ok_or(Error::LinkNotReady)?;
        if signaling.connection != (Connection { handle, epoch }) {
            return Err(Error::ForeignContext);
        }
        Self::new(
            signaling,
            service,
            local_endpoint,
            config,
            pdu_storage,
            now_us,
        )
    }

    /// For an externally managed authenticated link (for example a future
    /// native channel backend), the caller attests link security and routing.
    /// Prefer `from_link` when using the portable raw-HCI connection manager.
    pub fn new(
        signaling: Channel,
        service: SinkService,
        local_endpoint: StreamEndpointId,
        config: Config,
        pdu_storage: &'storage mut [u8],
        now_us: u64,
    ) -> Result<Self, Error> {
        signaling.validate()?;
        if config.signal_timeout_us == 0
            || config.media_open_timeout_us == 0
            || config.stop_timeout_us == 0
            || !(2..=53).contains(&config.bitpool)
            || !(96..=127).contains(&config.payload_type)
            || config.ssrc == 0
            || service.avdtp_version < 0x0100
            || service.profile_version < 0x0100
        {
            return Err(Error::InvalidConfig);
        }
        if pdu_storage.len() < 4 {
            return Err(Error::PduStorageTooSmall);
        }
        // Validate time arithmetic before borrowing storage into the session.
        now_us
            .checked_add(config.signal_timeout_us)
            .ok_or(Error::Failed(Failure::Overflow))?;
        let mut session = Self {
            config,
            signaling,
            initiator: Initiator::new(signaling.connection.epoch),
            local_endpoint,
            remote_endpoint: None,
            phase: Phase::Discovering,
            failure: None,
            terminal_failure: None,
            pending: None,
            next_submission: 0,
            command_bytes: [0; 64],
            last_now: now_us,
            deadline: None,
            stop_deadline: None,
            pdu_storage: Some(pdu_storage),
            media: None,
            media_channel: None,
            cancellation: None,
            next_media_sample: 0,
        };
        session.emit(SignalId::Discover, now_us)?;
        Ok(session)
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            phase: self.phase,
            failure: self.failure,
            terminal_failure: self.terminal_failure,
            deadline_us: self.deadline,
            pending: self.pending.map(|p| p.submission),
            signal_accepted: self.pending.is_some_and(|p| p.accepted),
            media_queued: self.media.as_ref().is_some_and(|m| m.queued().is_some()),
        }
    }

    pub const fn configuration(&self) -> Option<SbcConfiguration> {
        self.initiator.configuration()
    }
    pub const fn connection(&self) -> Connection {
        self.signaling.connection
    }
    pub const fn media_channel(&self) -> Option<Channel> {
        self.media_channel
    }

    /// Copy the same complete SDU until confirmed. The caller must supply its
    /// real ordered L2CAP transport and enforce signaling credits/fragmentation.
    pub fn prepare_signal(
        &mut self,
        now_us: u64,
        output: &mut [u8],
    ) -> Result<Option<SignalSubmission>, Error> {
        self.check_time(now_us)?;
        let Some(pending) = self.pending.filter(|p| !p.accepted) else {
            return Ok(None);
        };
        let n = pending.submission.command.bytes;
        if output.len() < n {
            return Err(Error::OutputTooSmall);
        }
        output[..n].copy_from_slice(&self.command_bytes[..n]);
        Ok(Some(pending.submission))
    }

    pub fn confirm_signal_accepted(
        &mut self,
        submission: SignalSubmission,
        now_us: u64,
    ) -> Result<(), Error> {
        self.check_time(now_us)?;
        match self.pending.as_mut() {
            Some(pending) if !pending.accepted && pending.submission == submission => {
                pending.accepted = true;
                Ok(())
            }
            _ => Err(Error::StaleSubmission),
        }
    }

    /// Busy is whole-SDU rejection; every other native error is terminal and
    /// its acceptance/credit effects need external reconciliation.
    pub fn signal_send_failed(
        &mut self,
        submission: SignalSubmission,
        error: TransportError,
    ) -> Result<(), Error> {
        self.active()?;
        if !self
            .pending
            .is_some_and(|p| !p.accepted && p.submission == submission)
        {
            return Err(Error::StaleSubmission);
        }
        if error == TransportError::Busy {
            return Ok(());
        }
        self.fail(Failure::Transport(error), TerminalFailure::TransportLost);
        Err(Error::Failed(Failure::Transport(error)))
    }

    pub fn receive_signal(
        &mut self,
        channel: Channel,
        now_us: u64,
        bytes: &[u8],
    ) -> Result<(), Error> {
        self.check_channel(channel, self.signaling)?;
        self.check_time(now_us)?;
        if !self.pending.is_some_and(|p| p.accepted) {
            return Err(Error::ResponseBeforeAcceptance);
        }
        let outcome = self.initiator.receive(channel.connection.epoch, bytes)?;
        self.consume(outcome, now_us)
    }

    /// Receive one complete L2CAP SDU, including a bounded fragmented response.
    /// The reassembler belongs exclusively to this signaling channel/session.
    /// Its timeout/clock failure invalidates the whole application session;
    /// malformed or foreign packets cannot advance a negotiation.
    pub fn receive_signal_packet(
        &mut self,
        reassembler: &mut bt_avdtp::SignalingReassembler<'_>,
        channel: Channel,
        now_us: u64,
        bytes: &[u8],
    ) -> Result<bool, Error> {
        self.check_channel(channel, self.signaling)?;
        self.check_time(now_us)?;
        if !self.pending.is_some_and(|p| p.accepted) {
            return Err(Error::ResponseBeforeAcceptance);
        }
        let context = bt_avdtp::SignalingContext {
            epoch: channel.connection.epoch,
            channel_id: channel.local_cid,
        };
        match self
            .initiator
            .receive_packet(reassembler, context, now_us, bytes)
        {
            Ok(Some(outcome)) => {
                self.consume(outcome, now_us)?;
                Ok(true)
            }
            Ok(None) => Ok(false),
            Err(error) => {
                match error {
                    bt_avdtp::ReceiveError::Reassembly(bt_avdtp::ReassemblyError::Timeout) => {
                        self.fail(Failure::Deadline, TerminalFailure::TransportLost)
                    }
                    bt_avdtp::ReceiveError::Reassembly(
                        bt_avdtp::ReassemblyError::ClockRegressed,
                    ) => self.fail(Failure::ClockRegressed, TerminalFailure::IntegrityLost),
                    bt_avdtp::ReceiveError::Reassembly(
                        bt_avdtp::ReassemblyError::DeadlineOverflow,
                    ) => self.fail(Failure::Overflow, TerminalFailure::IntegrityLost),
                    _ => {}
                }
                Err(Error::Receive(error))
            }
        }
    }

    fn consume(&mut self, outcome: Outcome<'_>, now_us: u64) -> Result<(), Error> {
        self.pending = None;
        self.deadline = None;
        match outcome {
            Outcome::Rejected(reject) => {
                self.fail(
                    Failure::PeerRejected(reject),
                    TerminalFailure::TransportLost,
                );
                Err(Error::Failed(Failure::PeerRejected(reject)))
            }
            Outcome::Accepted(accepted) => match (self.phase, accepted) {
                (Phase::Discovering, Accepted::Discovery(endpoints)) => {
                    let Some(endpoint) = endpoints.first_available_audio_sink() else {
                        self.fail(Failure::NoAudioSink, TerminalFailure::TransportLost);
                        return Err(Error::Failed(Failure::NoAudioSink));
                    };
                    self.remote_endpoint = Some(endpoint.id);
                    self.phase = Phase::Capabilities;
                    self.emit(SignalId::GetCapabilities, now_us)
                }
                (Phase::Capabilities, Accepted::Capabilities(caps)) => {
                    let configuration = caps
                        .conservative_sbc()
                        .and_then(|c| c.select(self.config.bitpool));
                    let configuration = match configuration {
                        Ok(c) => c,
                        Err(_) => {
                            self.fail(
                                Failure::UnsupportedCapabilities,
                                TerminalFailure::TransportLost,
                            );
                            return Err(Error::Failed(Failure::UnsupportedCapabilities));
                        }
                    };
                    self.phase = Phase::Configuring;
                    let command = self.initiator.set_configuration(
                        self.remote_endpoint.ok_or(Error::InvalidState)?,
                        self.local_endpoint,
                        configuration,
                        &mut self.command_bytes,
                    )?;
                    self.retain(command, now_us)
                }
                (Phase::Configuring, Accepted::Acknowledged(SignalId::SetConfiguration)) => {
                    self.phase = Phase::Opening;
                    self.emit(SignalId::Open, now_us)
                }
                (Phase::Opening, Accepted::Acknowledged(SignalId::Open)) => {
                    self.phase = Phase::AwaitMedia;
                    self.deadline =
                        Some(self.add_deadline(now_us, self.config.media_open_timeout_us)?);
                    Ok(())
                }
                (Phase::Starting, Accepted::Acknowledged(SignalId::Start)) => {
                    self.phase = Phase::Streaming;
                    Ok(())
                }
                (Phase::Suspending, Accepted::Acknowledged(SignalId::Suspend)) => {
                    self.phase = Phase::Closing;
                    self.emit(SignalId::Close, now_us)
                }
                (Phase::Closing, Accepted::Acknowledged(SignalId::Close)) => {
                    self.phase = Phase::AwaitMediaClose;
                    self.deadline = self.stop_deadline;
                    Ok(())
                }
                _ => Err(Error::InvalidState),
            },
        }
    }

    /// Bind the separate media channel only after accepted Open. ACL maximum
    /// and later credit allowance must come from the established transport owner.
    pub fn bind_media(
        &mut self,
        channel: Channel,
        acl_data_max: usize,
        now_us: u64,
    ) -> Result<(), Error> {
        self.check_time(now_us)?;
        if self.phase != Phase::AwaitMedia {
            return Err(Error::InvalidState);
        }
        channel.validate()?;
        if channel.connection != self.connection()
            || channel.local_cid == self.signaling.local_cid
            || channel.remote_cid == self.signaling.remote_cid
        {
            return Err(Error::ForeignContext);
        }
        if acl_data_max == 0 || acl_data_max > usize::from(u16::MAX) {
            return Err(Error::InvalidAclLimit);
        }
        let configuration = self.configuration().ok_or(Error::InvalidState)?;
        let packetizer = SbcRtpPacketizer::new(
            configuration.parameters(),
            channel.peer_mtu,
            self.config.payload_type,
            self.config.ssrc,
            self.config.first_sequence,
            self.config.first_timestamp,
        )
        .map_err(Error::Packet)?;
        let minimum =
            configuration.parameters().frame_length_bytes() + bt_a2dp::PACKET_OVERHEAD_BYTES + 4;
        if let Some(media) = self.media.as_mut() {
            if self
                .media_channel
                .is_some_and(|old| old.local_cid == channel.local_cid)
                || media.accepted_fragments() > 0 && media.queued().is_some()
            {
                return Err(Error::ForeignContext);
            }
            if usize::from(media.acl_data_max()) != acl_data_max {
                return Err(Error::InvalidAclLimit);
            }
            media
                .rebind_closed(packetizer, channel.connection, channel.remote_cid)
                .map_err(Error::Media)?;
        } else {
            if self.pdu_storage.as_ref().is_none_or(|s| s.len() < minimum) {
                return Err(Error::PduStorageTooSmall);
            }
            let storage = self.pdu_storage.take().ok_or(Error::InvalidState)?;
            self.media = Some(
                MediaSession::new(
                    packetizer,
                    self.connection(),
                    channel.remote_cid,
                    acl_data_max,
                    storage,
                )
                .map_err(Error::Media)?,
            );
        }
        self.media_channel = Some(channel);
        self.initiator.transport_opened(self.connection().epoch)?;
        self.phase = Phase::Starting;
        self.emit(SignalId::Start, now_us)
    }

    pub fn queue_sbc_frames(
        &mut self,
        frames: &[&[u8]],
        rtp_scratch: &mut [u8],
        l2cap_scratch: &mut [u8],
    ) -> Result<QueuedPacket, Error> {
        self.queue_sbc_frames_at(self.next_media_sample, frames, rtp_scratch, l2cap_scratch)
    }

    /// Start a new negotiation after accepted Close AND confirmed real media
    /// cleanup. Submission identities and AVDTP generations continue; the old
    /// media CIDs must not be reused. The signaling channel remains established.
    /// Create a fresh audio scheduler for sample zero of this new stream.
    pub fn restart(&mut self, now_us: u64) -> Result<(), Error> {
        self.check_time(now_us)?;
        if self.phase != Phase::Stopped
            || self.media_channel.is_none()
            || self
                .media
                .as_ref()
                .is_none_or(|m| m.state() != bt_media_session::SessionState::Stopped)
        {
            return Err(Error::InvalidState);
        }
        self.stop_deadline = None;
        self.next_media_sample = 0;
        self.remote_endpoint = None;
        self.phase = Phase::Discovering;
        self.emit(SignalId::Discover, now_us)
    }

    /// Queue complete frames at a full-width PCM sample position relative to
    /// this stream's origin. Gaps represent explicitly expired, wholly unsent
    /// audio; overlapping/overflowing positions fail without advancing state.
    /// RTP wraps independently from this monotonic position. Busy never replaces
    /// the retained PDU, including one with no accepted fragments yet.
    pub fn queue_sbc_frames_at(
        &mut self,
        first_sample: u64,
        frames: &[&[u8]],
        rtp_scratch: &mut [u8],
        l2cap_scratch: &mut [u8],
    ) -> Result<QueuedPacket, Error> {
        self.active()?;
        if self.phase != Phase::Streaming {
            return Err(Error::InvalidState);
        }
        if first_sample < self.next_media_sample {
            return Err(Error::InvalidSamplePosition);
        }
        let samples = self
            .configuration()
            .ok_or(Error::InvalidState)?
            .parameters()
            .pcm_frames();
        let count = u64::try_from(frames.len())
            .ok()
            .and_then(|count| count.checked_mul(samples as u64))
            .and_then(|count| first_sample.checked_add(count))
            .ok_or(Error::InvalidSamplePosition)?;
        let connection = self.connection();
        let queued = self
            .media
            .as_mut()
            .ok_or(Error::InvalidState)?
            .queue_sbc_frames_at(
                connection,
                self.config
                    .first_timestamp
                    .wrapping_add(first_sample as u32),
                frames,
                rtp_scratch,
                l2cap_scratch,
            )
            .map_err(Error::Media)?;
        self.next_media_sample = count;
        Ok(queued)
    }

    /// At most one whole ACL submission. Backpressure preserves the PDU, and
    /// a healthy Stop drains its accepted prefix before sending Suspend/Close.
    pub fn transmit_next<T: BluetoothTransport + ?Sized>(
        &mut self,
        now_us: u64,
        credits: &mut AclCreditOwner,
        transport: &mut T,
        output: &mut [u8],
    ) -> Result<TransmitOutcome, SessionTransmitError> {
        self.check_time(now_us)
            .map_err(SessionTransmitError::Session)?;
        if !matches!(self.phase, Phase::Streaming | Phase::Draining) {
            return Err(SessionTransmitError::Session(Error::InvalidState));
        }
        let connection = self.connection();
        let result = self
            .media
            .as_mut()
            .ok_or(SessionTransmitError::Session(Error::InvalidState))?
            .transmit_next(connection, credits, transport, output);
        match result {
            Ok(outcome) => {
                if matches!(
                    outcome,
                    TransmitOutcome::Accepted(SendProgress::Complete { .. })
                ) && self.phase == Phase::Draining
                {
                    self.begin_suspend(now_us)
                        .map_err(SessionTransmitError::Session)?;
                }
                Ok(outcome)
            }
            Err(error) => {
                if matches!(
                    error,
                    TransmitError::Terminal { .. } | TransmitError::Confirmation { .. }
                ) {
                    // The media sender already consumed cancellation evidence;
                    // retain it at the application boundary before invalidation.
                    self.cancellation = match error {
                        TransmitError::Terminal { dropped, .. }
                        | TransmitError::Confirmation { dropped, .. } => dropped,
                        _ => None,
                    };
                    self.fail(
                        Failure::MediaSubmission,
                        TerminalFailure::CreditsUnreconciled,
                    );
                }
                Err(SessionTransmitError::Transmit(error))
            }
        }
    }

    pub fn stop(&mut self, now_us: u64) -> Result<(), Error> {
        self.check_time(now_us)?;
        if matches!(
            self.phase,
            Phase::Draining
                | Phase::Suspending
                | Phase::Closing
                | Phase::AwaitMediaClose
                | Phase::Stopped
        ) {
            return Ok(());
        }
        if self.phase != Phase::Streaming {
            // Negotiation cancellation invalidates this local session. The
            // owner must still clean up the actual pending native operation.
            self.fail(Failure::Cancelled, TerminalFailure::TransportLost);
            return Ok(());
        }
        self.stop_deadline = Some(self.add_deadline(now_us, self.config.stop_timeout_us)?);
        let media = self.media.as_mut().ok_or(Error::InvalidState)?;
        if media.queued().is_some() && media.accepted_fragments() > 0 {
            self.phase = Phase::Draining;
            self.deadline = self.stop_deadline;
            return Ok(());
        }
        self.begin_suspend(now_us)
    }

    fn begin_suspend(&mut self, now_us: u64) -> Result<(), Error> {
        let connection = self.connection();
        let cancelled = self
            .media
            .as_mut()
            .ok_or(Error::InvalidState)?
            .stop(connection)
            .map_err(Error::Media)?;
        if cancelled.is_some() {
            self.cancellation = cancelled;
        }
        self.phase = Phase::Suspending;
        self.emit(SignalId::Suspend, now_us)
    }

    /// Call only after confirmed cleanup of the exact media channel. Local
    /// state completion is not a refund or controller-buffer reconciliation.
    pub fn media_closed(&mut self, channel: Channel, now_us: u64) -> Result<(), Error> {
        self.check_time(now_us)?;
        self.check_channel(channel, self.media_channel.ok_or(Error::InvalidState)?)?;
        if self.phase != Phase::AwaitMediaClose {
            return Err(Error::InvalidState);
        }
        self.initiator.transport_closed(self.connection().epoch)?;
        self.phase = Phase::Stopped;
        self.deadline = None;
        Ok(())
    }

    pub fn tick(&mut self, now_us: u64) -> Result<(), Error> {
        self.check_time(now_us)
    }

    /// Terminal caller-established failure. Always available even if the clock
    /// failed; it never manufactures a new connection or restores credits.
    pub fn transport_lost(&mut self, failure: TerminalFailure) {
        self.fail(Failure::Disconnected, failure);
    }

    /// Clock-independent operator cancellation. The owner must still cancel and
    /// reap native work, tear down the old channel/link and quarantine its traffic
    /// before reusing any context. This does not restore controller credits.
    pub fn cancel(&mut self) {
        self.fail(Failure::Cancelled, TerminalFailure::TransportLost);
    }

    pub fn take_cancellation(&mut self) -> Option<DroppedPacket> {
        self.cancellation.take()
    }

    fn emit(&mut self, signal: SignalId, now_us: u64) -> Result<(), Error> {
        let command = match signal {
            SignalId::Discover => self.initiator.discover(&mut self.command_bytes),
            SignalId::GetCapabilities => self.initiator.get_capabilities(
                self.remote_endpoint.ok_or(Error::InvalidState)?,
                &mut self.command_bytes,
            ),
            SignalId::Open => self.initiator.open(&mut self.command_bytes),
            SignalId::Start => self.initiator.start(&mut self.command_bytes),
            SignalId::Suspend => self.initiator.suspend(&mut self.command_bytes),
            SignalId::Close => self.initiator.close(&mut self.command_bytes),
            _ => return Err(Error::InvalidState),
        }?;
        self.retain(command, now_us)
    }

    fn retain(&mut self, command: Command, now_us: u64) -> Result<(), Error> {
        if command.bytes > usize::from(self.signaling.peer_mtu) {
            return Err(Error::InvalidChannel);
        }
        let Some(next) = self.next_submission.checked_add(1) else {
            self.fail(Failure::Overflow, TerminalFailure::IntegrityLost);
            return Err(Error::Failed(Failure::Overflow));
        };
        self.next_submission = next;
        let deadline = self.add_deadline(now_us, self.config.signal_timeout_us)?;
        self.deadline = Some(
            self.stop_deadline
                .map_or(deadline, |stop| stop.min(deadline)),
        );
        self.pending = Some(Pending {
            submission: SignalSubmission {
                id: next,
                connection: self.connection(),
                remote_cid: self.signaling.remote_cid,
                command,
            },
            accepted: false,
        });
        Ok(())
    }

    fn check_channel(&self, actual: Channel, expected: Channel) -> Result<(), Error> {
        if actual != expected {
            Err(Error::ForeignContext)
        } else {
            Ok(())
        }
    }

    fn active(&self) -> Result<(), Error> {
        self.failure.map_or(Ok(()), |f| Err(Error::Failed(f)))
    }

    fn check_time(&mut self, now_us: u64) -> Result<(), Error> {
        self.active()?;
        if now_us < self.last_now {
            self.fail(Failure::ClockRegressed, TerminalFailure::IntegrityLost);
            return Err(Error::Failed(Failure::ClockRegressed));
        }
        self.last_now = now_us;
        if self.deadline.is_some_and(|deadline| now_us >= deadline) {
            self.fail(Failure::Deadline, TerminalFailure::TransportLost);
            return Err(Error::Failed(Failure::Deadline));
        }
        Ok(())
    }

    fn add_deadline(&mut self, now_us: u64, budget: u64) -> Result<u64, Error> {
        match now_us.checked_add(budget) {
            Some(deadline) => Ok(deadline),
            None => {
                self.fail(Failure::Overflow, TerminalFailure::IntegrityLost);
                Err(Error::Failed(Failure::Overflow))
            }
        }
    }

    fn fail(&mut self, failure: Failure, terminal: TerminalFailure) {
        // Keep the first user-facing cause and escalate later integrity evidence,
        // including when no media PDU existed to carry cancellation metadata.
        let severity = |value| match value {
            TerminalFailure::Disconnected => 0,
            TerminalFailure::TransportLost => 1,
            TerminalFailure::IntegrityLost => 2,
            TerminalFailure::CreditsUnreconciled => 3,
        };
        if self
            .terminal_failure
            .is_none_or(|old| severity(terminal) > severity(old))
        {
            self.terminal_failure = Some(terminal);
        }
        if self.failure.is_some() {
            return;
        }
        self.failure = Some(failure);
        self.phase = Phase::Failed;
        self.pending = None;
        self.deadline = None;
        if let Some(media) = self.media.as_mut()
            && media.state() == bt_media_session::SessionState::Active
            && let Ok(Some(dropped)) = media.fail_transport(self.signaling.connection, terminal)
        {
            self.cancellation = Some(dropped);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionTransmitError {
    Session(Error),
    Transmit(TransmitError),
}
