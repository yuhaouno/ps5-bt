// SPDX-License-Identifier: GPL-3.0-or-later
//! Single-owner portable application driver. No platform calls or implicit reader.
#![no_std]
#![forbid(unsafe_code)]

use bt_audio::{AudioScheduler, BlockEncoder};
use bt_avdtp::{ReassemblyLimits, SignalingContext, SignalingReassembler, StreamEndpointId};
use bt_channel::{ChannelBinding, ChannelConfig, ChannelCoordinator, ChannelRole, PreparedSdu};
use bt_core::recovery::{Failure as RecoveryFailure, Recovery};
use bt_core::{BluetoothTransport, Error as TransportError};
use bt_device::{ApprovalToken, BondStore, DeviceManager, DeviceNotice};
use bt_hci::flow::AclCreditOwner;
use bt_l2cap::{Connection, TerminalFailure};
use bt_link::{CommandTicket, Peer};
use bt_media_session::{DroppedPacket, QueuedPacket, TransmitOutcome};
use bt_session::{Channel, SignalSubmission, SourceSession};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Connecting,
    Discovering,
    Connected,
    Negotiating,
    Streaming,
    Stopping,
    Stopped,
    Disconnecting,
    Closed,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidState,
    InvalidConfig,
    OutputTooSmall,
    ForeignContext,
    SecurityChanged,
    ClockRegressed,
    Deadline,
    Overflow,
    Device(bt_device::Error),
    Channel(bt_channel::Error),
    Source(bt_session::Error),
    Reassembly(bt_avdtp::ReassemblyError),
    Audio(bt_audio::Error),
    EncoderMismatch,
    RecoveryBlocked,
    Recovery(bt_core::recovery::RecoveryError),
    Transport(TransportError),
    Transmit(bt_session::SessionTransmitError),
}

pub struct Config {
    pub link: bt_link::Config,
    pub channels: ChannelConfig,
    pub source: bt_session::Config,
    pub local_endpoint: StreamEndpointId,
    pub reassembly: ReassemblyLimits,
    /// Externally established controller ACL data limit, not a buffer reservation.
    pub acl_data_max: usize,
    pub disconnect_timeout_us: u64,
    /// Total queue-to-last-fragment budget; Busy/no-credit cannot extend it.
    pub media_pdu_timeout_us: u64,
}

pub struct Storage<'a> {
    pub channel_tx: &'a mut [u8],
    pub sdp: &'a mut [u8],
    pub media_pdu: &'a mut [u8],
    pub avdtp: &'a mut [u8],
}

/// Instance-scoped exact retained offer. Bytes may contain a local bond key;
/// command output must be protected and scrubbed after transport copying.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Submission {
    Command { ticket: CommandTicket, len: usize },
    Channel(PreparedSdu),
    Signal(SignalSubmission),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Status {
    pub phase: Phase,
    pub device: bt_device::DeviceStatus,
    pub source: Option<bt_session::Snapshot>,
    pub failure: Option<Error>,
    pub cleanup_required: bool,
    pub recovery: bt_core::recovery::Snapshot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioProgress {
    Waiting(bt_audio::Action),
    Queued(QueuedPacket),
    Sending(TransmitOutcome),
    Stopped,
}

/// One selected connection/playback lifecycle. The caller provides already
/// authorized command/ACL submission and routes foreign traffic elsewhere.
/// `prepare` offers at most one whole command/Basic PDU/AVDTP SDU per call;
/// only `confirm_accepted` commits transport acceptance. This is not a scheduler
/// for the system's Bluetooth owner and does not discover controller credits.
/// A caller-owned Recovery survives reconstruction between connection attempts.
pub struct Application<'a, S: BondStore> {
    device: DeviceManager<S>,
    recovery: &'a mut Recovery,
    config: Config,
    storage: Storage<'a>,
    channels: Option<ChannelCoordinator<'a>>,
    source: Option<SourceSession<'a>>,
    reassembly: Option<SignalingReassembler<'a>>,
    connection: Option<Connection>,
    phase: Phase,
    failure: Option<Error>,
    last_now: u64,
    disconnect_deadline: Option<u64>,
    disconnect_requested: bool,
    channel_cancel_started: bool,
    device_cancel_started: bool,
    media_open_started: bool,
    media_close_started: bool,
    cleanup_required: bool,
    recovery_attempt: Option<bt_core::recovery::AttemptTicket>,
    completed_media: bool,
    media_deadline: Option<u64>,
}

impl<'a, S: BondStore> Application<'a, S> {
    pub fn new(
        device: DeviceManager<S>,
        recovery: &'a mut Recovery,
        config: Config,
        storage: Storage<'a>,
        now_us: u64,
    ) -> Result<Self, Error> {
        if config.media_pdu_timeout_us == 0
            || config.disconnect_timeout_us == 0
            || config.acl_data_max == 0
            || config.acl_data_max > u16::MAX as usize
            || storage.channel_tx.is_empty()
            || storage.sdp.is_empty()
            || storage.media_pdu.is_empty()
            || storage.avdtp.is_empty()
            || config.source.signal_timeout_us == 0
            || config.source.media_open_timeout_us == 0
            || config.source.stop_timeout_us == 0
            || !(2..=53).contains(&config.source.bitpool)
            || !(96..=127).contains(&config.source.payload_type)
            || config.source.ssrc == 0
            || config.reassembly.max_packets < 2
            || config.reassembly.timeout_us == 0
            || !(3..=u16::MAX as usize).contains(&config.reassembly.max_packet_bytes)
        {
            return Err(Error::InvalidConfig);
        }
        for budget in [
            config.source.signal_timeout_us,
            config.source.media_open_timeout_us,
            config.source.stop_timeout_us,
            config.reassembly.timeout_us,
            config.disconnect_timeout_us,
            config.media_pdu_timeout_us,
        ] {
            now_us.checked_add(budget).ok_or(Error::Overflow)?;
        }
        Ok(Self {
            device,
            recovery,
            config,
            storage,
            channels: None,
            source: None,
            reassembly: None,
            connection: None,
            phase: Phase::Idle,
            failure: None,
            last_now: now_us,
            disconnect_deadline: None,
            disconnect_requested: false,
            channel_cancel_started: false,
            device_cancel_started: false,
            media_open_started: false,
            media_close_started: false,
            cleanup_required: false,
            recovery_attempt: None,
            completed_media: false,
            media_deadline: None,
        })
    }

    pub fn status(&self) -> Status {
        Status {
            phase: self.phase,
            device: self.device.status(),
            source: self.source.as_ref().map(SourceSession::snapshot),
            failure: self.failure,
            cleanup_required: self.cleanup_required,
            recovery: self.recovery.snapshot(),
        }
    }

    pub fn negotiated_parameters(&self) -> Option<bt_core::SbcParameters> {
        self.source
            .as_ref()?
            .configuration()
            .map(|c| c.parameters())
    }

    /// Known routed connection identity, not an independent permission/credit
    /// proof. May remain available for diagnostics after terminal failure.
    pub const fn connection(&self) -> Option<Connection> {
        self.connection
    }

    pub const fn acl_data_max(&self) -> usize {
        self.config.acl_data_max
    }

    pub fn has_bond(&mut self, peer: Peer) -> Result<bool, Error> {
        self.device.has_bond(peer).map_err(Error::Device)
    }

    /// Retry only an already revoked deletion; never reauthorize the old link.
    pub fn retry_forget(&mut self) -> Result<(), Error> {
        self.device.retry_forget().map_err(Error::Device)
    }

    /// Delete a protected selected peer's bond even before connecting. A local
    /// idle deletion has no radio cleanup to manufacture. Active deletion still
    /// revokes the current attempt and requires external transport cleanup.
    pub fn forget_device(
        &mut self,
        peer: Peer,
        now_us: u64,
    ) -> Result<bt_device::ForgetOutcome, Error> {
        if matches!(self.phase, Phase::Idle | Phase::Closed) {
            self.time(now_us)?;
            self.device.select(peer).map_err(Error::Device)?;
            return self.device.forget(now_us).map_err(Error::Device);
        }
        if self.device.selected_peer() != Some(peer) {
            return Err(Error::ForeignContext);
        }
        self.forget(now_us)
    }

    /// The selected discovery result supplies the real page-scan repetition
    /// mode. The caller must require a quiescent, non-stale scan result first.
    pub fn connect_discovered(
        &mut self,
        peer: Peer,
        page_scan_repetition: u8,
        epoch: u64,
        now_us: u64,
    ) -> Result<(), Error> {
        if self.phase != Phase::Idle || page_scan_repetition > 2 {
            return Err(Error::InvalidState);
        }
        self.config.link.page_scan_repetition = page_scan_repetition;
        self.connect(peer, epoch, now_us)
    }

    /// Protected management/UI accessor; numeric approval values and tokens
    /// belong to an authorized local session, not public status broadcasts.
    pub fn pending_confirmation(&self) -> Option<(ApprovalToken, u32)> {
        self.device.pending_confirmation()
    }

    /// Explicit external attestation of working recovered playback, not merely
    /// socket open/AVDTP Start. The actual observer/platform must establish it.
    /// A complete accepted media PDU is also required, but that necessary
    /// condition alone is not headset/audibility evidence.
    pub fn confirm_recovered_working(&mut self, now_us: u64) -> Result<(), Error> {
        self.tick(now_us)?;
        if self.phase != Phase::Streaming || !self.completed_media {
            return Err(Error::InvalidState);
        }
        let ticket = self.recovery_attempt.ok_or(Error::InvalidState)?;
        self.recovery
            .finish_attempt(now_us, ticket, bt_core::recovery::AttemptOutcome::Success)
            .map_err(Error::Recovery)?;
        self.recovery_attempt = None;
        Ok(())
    }

    /// Revoke the selected bond before attempting deletion or old-link cleanup.
    /// Failure remains observable and cannot resurrect a provisional save.
    pub fn forget(&mut self, now_us: u64) -> Result<bt_device::ForgetOutcome, Error> {
        self.time(now_us)?;
        let result = self.device.forget(now_us).map_err(Error::Device);
        self.fail(
            Error::Transport(TransportError::Disconnected),
            now_us,
            RecoveryFailure::IntegrityLost,
        );
        result
    }

    pub fn connect(&mut self, peer: Peer, epoch: u64, now_us: u64) -> Result<(), Error> {
        self.time(now_us)?;
        if self.phase != Phase::Idle {
            return Err(Error::InvalidState);
        }
        match self.recovery.poll(now_us).map_err(Error::Recovery)? {
            bt_core::recovery::Action::Healthy => {}
            bt_core::recovery::Action::Attempt(ticket) => self
                .recovery
                .accept_attempt(now_us, ticket)
                .map(|_| self.recovery_attempt = Some(ticket))
                .map_err(Error::Recovery)?,
            _ => return Err(Error::RecoveryBlocked),
        }
        let result = self
            .device
            .select(peer)
            .and_then(|()| self.device.start(epoch, now_us, self.config.link))
            .map_err(Error::Device);
        if let Err(error) = result {
            self.fail(error, now_us, RecoveryFailure::IntegrityLost);
            return Err(error);
        }
        self.phase = Phase::Connecting;
        self.cleanup_required = true;
        Ok(())
    }

    pub fn approve(
        &mut self,
        token: ApprovalToken,
        now_us: u64,
        approved: bool,
    ) -> Result<(), Error> {
        self.time(now_us)?;
        let result = self
            .device
            .approve(token, now_us, approved)
            .map_err(Error::Device);
        self.check_security(now_us)?;
        result
    }

    /// Borrowed controller frames that return foreign/stale/Busy errors remain
    /// the dispatcher's responsibility. Never consume-and-discard them.
    pub fn receive_hci(
        &mut self,
        epoch: u64,
        now_us: u64,
        bytes: &[u8],
    ) -> Result<DeviceNotice, Error> {
        self.time(now_us)?;
        let result = self
            .device
            .receive(epoch, now_us, bytes)
            .map_err(Error::Device);
        self.check_security(now_us)?;
        if !self.device.status().ready
            && matches!(
                &result,
                Err(Error::Device(
                    bt_device::Error::Storage(_) | bt_device::Error::Record(_)
                ))
            )
        {
            let error = result
                .as_ref()
                .err()
                .copied()
                .unwrap_or(Error::InvalidState);
            self.fail(error, now_us, RecoveryFailure::IntegrityLost);
        }
        if let Some(link) = self.device.link()
            && (link.failure().is_some() || link.state() == bt_link::State::Failed)
            && !self.disconnect_requested
        {
            let error = result
                .as_ref()
                .err()
                .copied()
                .unwrap_or(Error::SecurityChanged);
            self.fail(error, now_us, RecoveryFailure::IntegrityLost);
        }
        result
    }

    pub fn start(&mut self, now_us: u64) -> Result<(), Error> {
        self.tick(now_us)?;
        if self.phase == Phase::Stopped {
            self.completed_media = false;
            let source = self.source.as_mut().ok_or(Error::InvalidState)?;
            source.restart(now_us).map_err(Error::Source)?;
            self.media_open_started = false;
            self.media_close_started = false;
            self.phase = Phase::Negotiating;
            return Ok(());
        }
        if self.phase != Phase::Connected || self.source.is_some() {
            return Err(Error::InvalidState);
        }
        let channels = self.channels.as_ref().ok_or(Error::InvalidState)?;
        let channel = as_source(
            channels
                .binding(ChannelRole::Signaling)
                .ok_or(Error::InvalidState)?,
        );
        let result = SourceSession::from_link(
            self.device.link().ok_or(Error::InvalidState)?,
            channel,
            channels.service().ok_or(Error::InvalidState)?,
            self.config.local_endpoint,
            self.config.source,
            core::mem::take(&mut self.storage.media_pdu),
            now_us,
        )
        .map_err(Error::Source);
        let source = match result {
            Ok(source) => source,
            Err(error) => {
                self.fail(error, now_us, RecoveryFailure::IntegrityLost);
                return Err(error);
            }
        };
        let result = SignalingReassembler::new(
            SignalingContext {
                epoch: channel.connection.epoch,
                channel_id: channel.local_cid,
            },
            core::mem::take(&mut self.storage.avdtp),
            self.config.reassembly,
        )
        .map_err(Error::Reassembly);
        let assembler = match result {
            Ok(assembler) => assembler,
            Err(error) => {
                self.fail(error, now_us, RecoveryFailure::IntegrityLost);
                return Err(error);
            }
        };
        self.source = Some(source);
        self.reassembly = Some(assembler);
        self.phase = Phase::Negotiating;
        Ok(())
    }

    pub fn stop(&mut self, now_us: u64) -> Result<(), Error> {
        self.tick(now_us)?;
        let source = self.source.as_mut().ok_or(Error::InvalidState)?;
        source.stop(now_us).map_err(Error::Source)?;
        if let Some(failure) = source.snapshot().failure {
            let error = Error::Source(bt_session::Error::Failed(failure));
            self.fail(error, now_us, RecoveryFailure::IntegrityLost);
            return Err(error);
        }
        self.phase = Phase::Stopping;
        Ok(())
    }

    pub fn disconnect(&mut self, now_us: u64) -> Result<(), Error> {
        self.time(now_us)?;
        if matches!(self.phase, Phase::Idle | Phase::Closed | Phase::Failed) {
            return Err(Error::InvalidState);
        }
        if !self.disconnect_requested {
            self.disconnect_deadline = Some(
                now_us
                    .checked_add(self.config.disconnect_timeout_us)
                    .ok_or(Error::Overflow)?,
            );
            self.disconnect_requested = true;
            if let Some(source) = self.source.as_mut()
                && !matches!(
                    source.snapshot().phase,
                    bt_session::Phase::Stopped | bt_session::Phase::Failed
                )
            {
                source.stop(now_us).map_err(Error::Source)?;
            }
        }
        self.phase = Phase::Disconnecting;
        Ok(())
    }

    /// Cancel without a clock. Actual native work must still be cancelled/reaped
    /// externally; polling this local failure never proves cleanup.
    pub fn cancel(&mut self) {
        self.fail(
            Error::Transport(TransportError::Disconnected),
            self.last_now,
            RecoveryFailure::IntegrityLost,
        );
    }

    pub fn transport_lost(&mut self, error: TransportError) {
        if error != TransportError::Busy {
            self.fail(
                Error::Transport(error),
                self.last_now,
                RecoveryFailure::IntegrityLost,
            );
        }
    }

    pub fn tick(&mut self, now_us: u64) -> Result<(), Error> {
        self.time(now_us)?;
        let action = match self.recovery.poll(now_us) {
            Ok(action) => action,
            Err(error) => {
                let error = Error::Recovery(error);
                self.fail(error, now_us, RecoveryFailure::IntegrityLost);
                return Err(error);
            }
        };
        if self.phase == Phase::Idle || self.phase == Phase::Closed {
            return Ok(());
        }
        if !matches!(
            action,
            bt_core::recovery::Action::Healthy
                | bt_core::recovery::Action::InFlight(_)
                | bt_core::recovery::Action::StabilizingUntil(_)
        ) {
            self.fail(
                Error::RecoveryBlocked,
                now_us,
                RecoveryFailure::IntegrityLost,
            );
            return Err(Error::RecoveryBlocked);
        }
        if self
            .source
            .as_ref()
            .is_some_and(|source| !source.snapshot().media_queued)
        {
            self.media_deadline = None;
        }
        if self
            .media_deadline
            .is_some_and(|deadline| now_us >= deadline)
        {
            self.fail(Error::Deadline, now_us, RecoveryFailure::IntegrityLost);
            return Err(Error::Deadline);
        }
        if let Err(error) = self.device.tick(now_us) {
            let error = Error::Device(error);
            self.fail(error, now_us, RecoveryFailure::IntegrityLost);
            return Err(error);
        }
        self.check_security(now_us)?;
        if self
            .disconnect_deadline
            .is_some_and(|deadline| now_us >= deadline)
        {
            self.fail(Error::Deadline, now_us, RecoveryFailure::IntegrityLost);
            return Err(Error::Deadline);
        }
        if let Some(channels) = self.channels.as_mut()
            && let Err(error) = channels.tick(now_us)
        {
            let error = Error::Channel(error);
            self.fail(error, now_us, RecoveryFailure::IntegrityLost);
            return Err(error);
        }
        if let Some(source) = self.source.as_mut()
            && let Err(error) = source.tick(now_us)
        {
            let error = Error::Source(error);
            self.fail(error, now_us, RecoveryFailure::IntegrityLost);
            return Err(error);
        }
        let result = self.advance(now_us);
        if let Err(error) = result {
            self.fail(error, now_us, RecoveryFailure::IntegrityLost);
        }
        result
    }

    /// Output kind defines framing: commands have no H4 byte, channel offers
    /// include a Basic header, source signal offers are SDUs for their remote CID.
    /// Repeated Busy offers retain byte identity. Use a fresh time before send.
    pub fn prepare(&mut self, now_us: u64, output: &mut [u8]) -> Result<Option<Submission>, Error> {
        self.tick(now_us)?;
        if self.device.command_len().is_some() {
            let ticket = self
                .device
                .prepare_command(now_us, output)
                .map_err(Error::Device)?;
            if let Some(ticket) = ticket {
                return Ok(Some(Submission::Command {
                    ticket,
                    len: self.device.command_len().ok_or(Error::InvalidState)?,
                }));
            }
        }
        if let Some(channels) = self.channels.as_mut()
            && let Some(prepared) = channels
                .prepare_sdu(now_us, output)
                .map_err(Error::Channel)?
        {
            return Ok(Some(Submission::Channel(prepared)));
        }
        if let Some(source) = self.source.as_mut()
            && let Some(prepared) = source
                .prepare_signal(now_us, output)
                .map_err(Error::Source)?
        {
            return Ok(Some(Submission::Signal(prepared)));
        }
        Ok(None)
    }

    pub fn confirm_accepted(&mut self, now_us: u64, submission: Submission) -> Result<(), Error> {
        // Record irreversible acceptance before observing a new clock/deadline
        // failure. A late acknowledgement must never cause duplicate sending.
        let result = match submission {
            Submission::Command { ticket, .. } => {
                self.device.confirm_accepted(ticket).map_err(Error::Device)
            }
            Submission::Channel(prepared) => self
                .channels
                .as_mut()
                .ok_or(Error::InvalidState)?
                .confirm_accepted(prepared.ticket, now_us)
                .map(|_| ())
                .map_err(Error::Channel),
            Submission::Signal(prepared) => self
                .source
                .as_mut()
                .ok_or(Error::InvalidState)?
                .confirm_signal_accepted(prepared, now_us)
                .map_err(Error::Source),
        };
        self.time(now_us)?;
        self.synchronize_failure(now_us, result)
    }

    pub fn submission_failed(
        &mut self,
        submission: Submission,
        error: TransportError,
    ) -> Result<(), Error> {
        if error == TransportError::Busy {
            return Ok(());
        }
        // No other send error guarantees zero accepted bytes. Never retry.
        if let Submission::Command { .. } = submission {
            let _ = self.device.fail_transport(error);
        }
        self.transport_lost(error);
        Err(Error::Transport(error))
    }

    pub fn receive_l2cap(
        &mut self,
        connection: Connection,
        now_us: u64,
        packet: &[u8],
        scratch: &mut [u8],
    ) -> Result<(), Error> {
        self.tick(now_us)?;
        if self.connection != Some(connection) {
            return Err(Error::ForeignContext);
        }
        let result = self
            .channels
            .as_mut()
            .ok_or(Error::InvalidState)?
            .receive(connection, now_us, packet, scratch)
            .map_err(Error::Channel);
        let notice = self.synchronize_failure(now_us, result)?;
        if let bt_channel::Notice::Data {
            role,
            binding,
            payload,
        } = notice
        {
            if role != ChannelRole::Signaling {
                return Err(Error::ForeignContext);
            }
            let source = self.source.as_mut().ok_or(Error::InvalidState)?;
            let result = source
                .receive_signal_packet(
                    self.reassembly.as_mut().ok_or(Error::InvalidState)?,
                    as_source(binding),
                    now_us,
                    payload,
                )
                .map_err(Error::Source);
            self.synchronize_failure(now_us, result)?;
        }
        let result = self.advance(now_us);
        if let Err(error) = result {
            self.fail(error, now_us, RecoveryFailure::IntegrityLost);
        }
        result
    }

    /// Queue or submit at most one unit of media work. The scheduler's retained
    /// SBC block transfers only after Source copied the complete PDU. Once a PDU
    /// is queued, it belongs to Source and drains across Busy/credit starvation.
    #[allow(clippy::too_many_arguments)]
    pub fn pump_audio<E: BlockEncoder, T: BluetoothTransport + ?Sized>(
        &mut self,
        now_us: u64,
        audio: &mut AudioScheduler<'_>,
        encoder: &mut E,
        credits: &mut AclCreditOwner,
        transport: &mut T,
        rtp: &mut [u8],
        l2cap: &mut [u8],
        acl: &mut [u8],
    ) -> Result<AudioProgress, Error> {
        if let Err(error) = self.tick(now_us) {
            audio.stop();
            return Err(error);
        }
        let Some(source) = self.source.as_mut() else {
            return Err(Error::InvalidState);
        };
        let phase = source.snapshot().phase;
        if matches!(
            phase,
            bt_session::Phase::Streaming | bt_session::Phase::Draining
        ) && source.snapshot().media_queued
        {
            if phase == bt_session::Phase::Draining {
                audio.stop();
            }
            return match source.transmit_next(now_us, credits, transport, acl) {
                Ok(progress) => {
                    if matches!(
                        progress,
                        TransmitOutcome::Accepted(bt_media_session::SendProgress::Complete { .. })
                    ) {
                        self.completed_media = true;
                        self.media_deadline = None;
                    }
                    Ok(AudioProgress::Sending(progress))
                }
                Err(error) => {
                    audio.stop();
                    let error = Error::Transmit(error);
                    self.fail(error, now_us, RecoveryFailure::IntegrityLost);
                    Err(error)
                }
            };
        }
        if phase != bt_session::Phase::Streaming {
            audio.stop();
            return Ok(AudioProgress::Stopped);
        }
        if source
            .configuration()
            .ok_or(Error::InvalidState)?
            .parameters()
            != encoder.parameters()
            || audio.parameters() != encoder.parameters()
        {
            audio.stop();
            self.fail(
                Error::EncoderMismatch,
                now_us,
                RecoveryFailure::IntegrityLost,
            );
            return Err(Error::EncoderMismatch);
        }
        let action = match audio.pull(now_us, encoder) {
            Ok(action) => action,
            Err(error) => {
                let error = Error::Audio(error);
                self.fail(error, now_us, RecoveryFailure::IntegrityLost);
                return Err(error);
            }
        };
        if let bt_audio::Action::Ready(block) = action {
            let deadline = now_us
                .checked_add(self.config.media_pdu_timeout_us)
                .ok_or(Error::Overflow)?;
            let frame = audio.encoded_frame(block.ticket).map_err(Error::Audio)?;
            let queued = source
                .queue_sbc_frames_at(block.first_sample, &[frame], rtp, l2cap)
                .map_err(Error::Source)?;
            self.media_deadline = Some(deadline);
            audio
                .confirm_accepted(now_us, block.ticket)
                .map_err(Error::Audio)?;
            Ok(AudioProgress::Queued(queued))
        } else {
            Ok(AudioProgress::Waiting(action))
        }
    }

    pub fn take_cancellation(&mut self) -> Option<DroppedPacket> {
        self.source
            .as_mut()
            .and_then(SourceSession::take_cancellation)
    }

    /// Reconstruct only after actual cleanup and a newer externally established
    /// epoch. Dropping this driver is not native cleanup or credit reconciliation.
    pub fn into_device(self) -> DeviceManager<S> {
        self.device
    }

    fn synchronize_failure<T>(
        &mut self,
        now_us: u64,
        result: Result<T, Error>,
    ) -> Result<T, Error> {
        if let Err(error) = result.as_ref() {
            let terminal = self
                .source
                .as_ref()
                .is_some_and(|source| source.snapshot().failure.is_some())
                || self.channels.as_ref().is_some_and(|channels| {
                    channels.phase() == bt_channel::CoordinatorPhase::Failed
                })
                || self
                    .device
                    .link()
                    .is_some_and(|link| link.state() == bt_link::State::Failed);
            if terminal {
                self.fail(*error, now_us, RecoveryFailure::IntegrityLost);
            }
        }
        result
    }

    fn check_security(&mut self, now_us: u64) -> Result<(), Error> {
        if let Some(connection) = self.connection {
            let expected = Some((connection.handle, connection.epoch));
            if self.device.ready_connection() != expected && !self.device_cancel_started {
                self.fail(
                    Error::SecurityChanged,
                    now_us,
                    RecoveryFailure::IntegrityLost,
                );
                return Err(Error::SecurityChanged);
            }
        }
        Ok(())
    }

    fn time(&mut self, now_us: u64) -> Result<(), Error> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        if now_us < self.last_now {
            self.fail(
                Error::ClockRegressed,
                self.last_now,
                RecoveryFailure::IntegrityLost,
            );
            return Err(Error::ClockRegressed);
        }
        self.last_now = now_us;
        Ok(())
    }

    fn fail(&mut self, error: Error, now_us: u64, reason: RecoveryFailure) {
        if let Some(source) = self.source.as_mut() {
            source.transport_lost(TerminalFailure::IntegrityLost);
        }
        if let Some(channels) = self.channels.as_mut() {
            let _ = channels.fail_transport(TransportError::IntegrityLost);
        }
        if self.failure.is_none() {
            self.failure = Some(error);
            self.phase = Phase::Failed;
            self.cleanup_required = true;
            let _ = self.device.cancel_at_last_observation();
            // Recovery reports its own clock/counter failures in status. No
            // caller operation here can confirm the outstanding safety ticket.
            let _ = self.recovery.trigger(now_us, reason);
        }
    }

    fn advance(&mut self, now_us: u64) -> Result<(), Error> {
        if self.failure.is_some() {
            return Err(self.failure.unwrap_or(Error::InvalidState));
        }
        if self.channels.is_none()
            && let Some((handle, epoch)) = self.device.ready_connection()
        {
            let connection = Connection { handle, epoch };
            let mut channels = ChannelCoordinator::new(
                connection,
                now_us,
                self.config.channels,
                core::mem::take(&mut self.storage.channel_tx),
                core::mem::take(&mut self.storage.sdp),
            )
            .map_err(Error::Channel)?;
            channels.begin_discovery(now_us).map_err(Error::Channel)?;
            self.connection = Some(connection);
            self.channels = Some(channels);
            self.phase = Phase::Discovering;
        }
        if self.disconnect_requested {
            let source_done = self
                .source
                .as_ref()
                .is_none_or(|source| matches!(source.snapshot().phase, bt_session::Phase::Stopped));
            if source_done && !self.channel_cancel_started {
                if let Some(channels) = self.channels.as_mut() {
                    channels.cancel(now_us).map_err(Error::Channel)?;
                }
                self.channel_cancel_started = true;
            }
            let channels_done = self
                .channels
                .as_ref()
                .is_none_or(ChannelCoordinator::is_closed);
            if source_done && channels_done && !self.device_cancel_started {
                self.device.cancel(now_us).map_err(Error::Device)?;
                self.device_cancel_started = true;
            }
            if self.device_cancel_started
                && self
                    .device
                    .link()
                    .is_some_and(|link| link.state() == bt_link::State::Closed)
            {
                self.phase = Phase::Closed;
                self.cleanup_required = false;
                self.disconnect_deadline = None;
            }
        } else if self.source.is_none()
            && let Some(channels) = self.channels.as_mut()
            && channels.sdp_closed()
        {
            if channels.binding(ChannelRole::Signaling).is_some() {
                self.phase = Phase::Connected;
            } else if channels.channel_phase(ChannelRole::Signaling).is_none() {
                channels.open_signaling(now_us).map_err(Error::Channel)?;
            }
        }
        if let Some(channels) = self.channels.as_ref() {
            if !self.disconnect_requested
                && matches!(
                    self.phase,
                    Phase::Connected
                        | Phase::Negotiating
                        | Phase::Streaming
                        | Phase::Stopping
                        | Phase::Stopped
                )
                && channels.binding(ChannelRole::Signaling).is_none()
            {
                return Err(Error::ForeignContext);
            }
            if self.source.as_ref().is_some_and(|source| {
                matches!(
                    source.snapshot().phase,
                    bt_session::Phase::Streaming | bt_session::Phase::Draining
                )
            }) && channels.binding(ChannelRole::Media).is_none()
            {
                return Err(Error::ForeignContext);
            }
        }
        if let (Some(source), Some(channels)) = (self.source.as_mut(), self.channels.as_mut()) {
            if source.snapshot().phase == bt_session::Phase::AwaitMedia && !self.media_open_started
            {
                channels.open_media(now_us).map_err(Error::Channel)?;
                self.media_open_started = true;
            }
            if source.snapshot().phase == bt_session::Phase::AwaitMedia
                && let Some(binding) = channels.binding(ChannelRole::Media)
            {
                source
                    .bind_media(as_source(binding), self.config.acl_data_max, now_us)
                    .map_err(Error::Source)?;
            }
            if source.snapshot().phase == bt_session::Phase::Streaming && !self.disconnect_requested
            {
                self.phase = Phase::Streaming;
            }
            if source.snapshot().phase == bt_session::Phase::Stopped && !self.disconnect_requested {
                self.phase = Phase::Stopped;
            }
            if source.snapshot().phase == bt_session::Phase::AwaitMediaClose {
                if !self.media_close_started {
                    channels.close_media(now_us).map_err(Error::Channel)?;
                    self.media_close_started = true;
                }
                if channels.media_closed() {
                    source
                        .media_closed(source.media_channel().ok_or(Error::InvalidState)?, now_us)
                        .map_err(Error::Source)?;
                }
            }
        }
        Ok(())
    }
}

fn as_source(binding: ChannelBinding) -> Channel {
    Channel {
        connection: binding.connection,
        local_cid: binding.local_cid,
        remote_cid: binding.remote_cid,
        peer_mtu: binding.peer_mtu,
    }
}
