// SPDX-License-Identifier: GPL-3.0-or-later
use bt_app::{Application, AudioProgress, Submission};
use bt_audio::{AudioScheduler, BlockEncoder};
use bt_core::{BluetoothTransport, Error};
use bt_device::BondStore;
use bt_hci::flow::{AclCreditOwner, CreditContext, CreditError};
use bt_l2cap::{Acceptance, BasicFrame, Cancellation, Connection, Fragmenter, TerminalFailure};

/// The platform dispatcher must prove command routing/allowance before calling
/// its native send. Busy guarantees zero bytes accepted; no other error does.
pub trait CommandSink {
    fn send_owned_command(&mut self, epoch: u64, packet: &[u8]) -> Result<(), Error>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PumpError {
    Invalid,
    Busy,
    Deadline,
    ClockRegressed,
    ForeignContext,
    OutputTooSmall,
    Application(bt_app::Error),
    Fragment(bt_l2cap::FragmentError),
    Reassembly(bt_l2cap::ReassemblyError),
    Credits(CreditError),
    Transport(Error),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PumpProgress {
    Idle,
    CommandAccepted,
    ControlQueued,
    ControlAccepted { complete: bool },
    WouldBlock,
    NoCredits,
    MediaDrain,
    ControlPending,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioStep {
    Progress(AudioProgress),
    ControlPending,
}

/// One connection's noninterleaving ACL owner. The external actual reservation
/// is shared by control and media; this type discovers no controller capacity.
pub struct OutputPump<'a> {
    connection: Connection,
    fragmenter: Fragmenter<'a>,
    timeout_us: u64,
    deadline: Option<u64>,
    last_us: u64,
    media_partial: bool,
    failure: Option<PumpError>,
    cancelled: Option<Cancellation>,
}
impl<'a> OutputPump<'a> {
    pub fn new(
        connection: Connection,
        acl_data_max: usize,
        storage: &'a mut [u8],
        timeout_us: u64,
        now: u64,
    ) -> Result<Self, PumpError> {
        if timeout_us == 0 || now.checked_add(timeout_us).is_none() {
            return Err(PumpError::Invalid);
        }
        Ok(Self {
            connection,
            fragmenter: Fragmenter::new(connection, acl_data_max, storage)
                .map_err(PumpError::Fragment)?,
            timeout_us,
            deadline: None,
            last_us: now,
            media_partial: false,
            failure: None,
            cancelled: None,
        })
    }
    pub fn pending_control(&self) -> bool {
        self.fragmenter.is_busy()
    }
    pub fn take_cancellation(&mut self) -> Option<Cancellation> {
        self.cancelled.take()
    }
    fn preflight<S: BondStore>(
        &mut self,
        app: &mut Application<'_, S>,
        credits: &mut AclCreditOwner,
        now: u64,
    ) -> Result<(), PumpError> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        let error = if now < self.last_us {
            Some(PumpError::ClockRegressed)
        } else if self.deadline.is_some_and(|end| now >= end) {
            Some(PumpError::Deadline)
        } else if credits.snapshot().context
            != (CreditContext {
                handle: self.connection.handle,
                epoch: self.connection.epoch,
            })
            || credits.acl_data_max() != self.fragmenter.acl_data_max()
            || app.acl_data_max() != usize::from(self.fragmenter.acl_data_max())
        {
            Some(PumpError::ForeignContext)
        } else {
            None
        };
        if let Some(error) = error {
            self.fail(app, credits, error);
            return Err(error);
        }
        self.last_us = now;
        if let Err(error) = app.tick(now) {
            let error = PumpError::Application(error);
            self.fail(app, credits, error);
            return Err(error);
        }
        if app.connection() != Some(self.connection) {
            self.fail(app, credits, PumpError::ForeignContext);
            return Err(PumpError::ForeignContext);
        }
        Ok(())
    }
    /// At most one new whole command/PDU handoff OR one ACL fragment send.
    /// `offer` is transient and scrubbed after commands; `basic` is Basic-frame
    /// scratch. A queued control PDU cannot be interleaved with Source media.
    #[allow(clippy::too_many_arguments)]
    pub fn poll_control<S: BondStore, C: CommandSink + ?Sized, T: BluetoothTransport + ?Sized>(
        &mut self,
        app: &mut Application<'_, S>,
        now: u64,
        commands: &mut C,
        transport: &mut T,
        credits: &mut AclCreditOwner,
        offer: &mut [u8],
        basic: &mut [u8],
        acl: &mut [u8],
    ) -> Result<PumpProgress, PumpError> {
        self.preflight(app, credits, now)?;
        if self.media_partial {
            return Ok(PumpProgress::MediaDrain);
        }
        if self.fragmenter.is_busy() {
            let prepared = self
                .fragmenter
                .prepare(self.connection, acl)
                .map_err(PumpError::Fragment)?;
            let context = CreditContext {
                handle: self.connection.handle,
                epoch: self.connection.epoch,
            };
            match credits.send_acl(context, &acl[..prepared.packet_len], transport) {
                Ok(()) => {
                    let progress = match self.fragmenter.confirm_accepted(self.connection) {
                        Ok(value) => value,
                        Err(error) => {
                            let error = PumpError::Fragment(error);
                            self.fail(app, credits, error);
                            return Err(error);
                        }
                    };
                    let complete = matches!(progress, Acceptance::Complete { .. });
                    if complete {
                        self.deadline = None;
                    }
                    return Ok(PumpProgress::ControlAccepted { complete });
                }
                Err(CreditError::NoCredits) => return Ok(PumpProgress::NoCredits),
                Err(CreditError::Rejected(Error::Busy)) => return Ok(PumpProgress::WouldBlock),
                Err(error) => {
                    let error = PumpError::Credits(error);
                    self.fail(app, credits, error);
                    return Err(error);
                }
            }
        }
        let Some(submission) = app.prepare(now, offer).map_err(PumpError::Application)? else {
            return Ok(PumpProgress::Idle);
        };
        match submission {
            Submission::Command { len, .. } => {
                let result = commands.send_owned_command(self.connection.epoch, &offer[..len]);
                offer.fill(0);
                match result {
                    Ok(()) => {
                        if let Err(error) = app.confirm_accepted(now, submission) {
                            let error = PumpError::Application(error);
                            self.fail(app, credits, error);
                            return Err(error);
                        }
                        Ok(PumpProgress::CommandAccepted)
                    }
                    Err(Error::Busy) => Ok(PumpProgress::WouldBlock),
                    Err(error) => {
                        let _ = app.submission_failed(submission, error);
                        let error = PumpError::Transport(error);
                        self.fail(app, credits, error);
                        Err(error)
                    }
                }
            }
            Submission::Channel(prepared) => {
                if prepared.connection != self.connection {
                    return Err(PumpError::ForeignContext);
                }
                let end = now.checked_add(self.timeout_us).ok_or(PumpError::Invalid)?;
                self.fragmenter
                    .start(self.connection, &offer[..prepared.len])
                    .map_err(PumpError::Fragment)?;
                self.deadline = Some(end);
                if let Err(error) = app.confirm_accepted(now, submission) {
                    let error = PumpError::Application(error);
                    self.fail(app, credits, error);
                    return Err(error);
                }
                Ok(PumpProgress::ControlQueued)
            }
            Submission::Signal(signal) => {
                if signal.connection != self.connection {
                    return Err(PumpError::ForeignContext);
                }
                let end = now.checked_add(self.timeout_us).ok_or(PumpError::Invalid)?;
                let frame = BasicFrame::new(signal.remote_cid, &offer[..signal.command.bytes])
                    .map_err(|_| PumpError::Invalid)?;
                let len = frame.encode(basic).map_err(|_| PumpError::OutputTooSmall)?;
                self.fragmenter
                    .start(self.connection, &basic[..len])
                    .map_err(PumpError::Fragment)?;
                self.deadline = Some(end);
                if let Err(error) = app.confirm_accepted(now, submission) {
                    let error = PumpError::Application(error);
                    self.fail(app, credits, error);
                    return Err(error);
                }
                Ok(PumpProgress::ControlQueued)
            }
        }
    }
    /// Never invokes the media sender while a control PDU is retained. Before
    /// ordinary control work, finish a healthy Source partial PDU in this path.
    #[allow(clippy::too_many_arguments)]
    pub fn poll_audio<S: BondStore, E: BlockEncoder, T: BluetoothTransport + ?Sized>(
        &mut self,
        app: &mut Application<'_, S>,
        now: u64,
        audio: &mut AudioScheduler<'_>,
        encoder: &mut E,
        credits: &mut AclCreditOwner,
        transport: &mut T,
        rtp: &mut [u8],
        basic: &mut [u8],
        acl: &mut [u8],
    ) -> Result<AudioStep, PumpError> {
        if let Err(error) = self.preflight(app, credits, now) {
            audio.stop();
            return Err(error);
        }
        if self.fragmenter.is_busy() {
            return Ok(AudioStep::ControlPending);
        }
        let progress =
            match app.pump_audio(now, audio, encoder, credits, transport, rtp, basic, acl) {
                Ok(value) => value,
                Err(error) => {
                    let error = PumpError::Application(error);
                    self.fail(app, credits, error);
                    return Err(error);
                }
            };
        if let AudioProgress::Sending(bt_media_session::TransmitOutcome::Accepted(progress)) =
            progress
        {
            self.media_partial = matches!(progress, bt_media_session::SendProgress::More { .. });
        }
        Ok(AudioStep::Progress(progress))
    }
    pub fn complete<S: BondStore>(
        &mut self,
        app: &mut Application<'_, S>,
        credits: &mut AclCreditOwner,
        now: u64,
        context: CreditContext,
        event: &[u8],
    ) -> Result<u16, PumpError> {
        self.preflight(app, credits, now)?;
        match credits.complete(context, event) {
            Ok(count) => Ok(count),
            Err(error) => {
                let error = PumpError::Credits(error);
                self.fail(app, credits, error);
                Err(error)
            }
        }
    }
    pub fn cancel<S: BondStore>(
        &mut self,
        app: &mut Application<'_, S>,
        credits: &mut AclCreditOwner,
    ) {
        self.fail(app, credits, PumpError::Transport(Error::Disconnected));
    }
    fn fail<S: BondStore>(
        &mut self,
        app: &mut Application<'_, S>,
        credits: &mut AclCreditOwner,
        error: PumpError,
    ) {
        if self.failure.is_none() {
            self.failure = Some(error);
            credits.mark_integrity_lost();
            app.transport_lost(Error::IntegrityLost);
            if self.fragmenter.is_busy() {
                let _ = self
                    .fragmenter
                    .mark_terminal_failure(self.connection, TerminalFailure::IntegrityLost);
                self.cancelled = self.fragmenter.cancel(self.connection).ok();
            }
        }
    }
}

/// Owned-connection ACL reassembly with one retained complete Basic-PDU inbox.
/// Delivery rejection never permits overwriting that inbox. Foreign/native raw
/// packets must already have been separated by the actual platform dispatcher.
pub struct InputPump<'a> {
    connection: Connection,
    reassembler: bt_l2cap::Reassembler<'a>,
    inbox: &'a mut [u8],
    pending: Option<usize>,
    deadline: Option<u64>,
    timeout: u64,
    last_us: u64,
    failed: bool,
}
impl<'a> InputPump<'a> {
    pub fn new(
        connection: Connection,
        reassembly: &'a mut [u8],
        inbox: &'a mut [u8],
        timeout_us: u64,
        now: u64,
    ) -> Result<Self, PumpError> {
        if timeout_us == 0
            || inbox.len() < reassembly.len()
            || now.checked_add(timeout_us).is_none()
        {
            return Err(PumpError::Invalid);
        }
        Ok(Self {
            connection,
            reassembler: bt_l2cap::Reassembler::new(connection, reassembly)
                .map_err(PumpError::Transport)?,
            inbox,
            pending: None,
            deadline: None,
            timeout: timeout_us,
            last_us: now,
            failed: false,
        })
    }
    pub fn pending_basic(&self) -> Option<&[u8]> {
        self.pending.map(|len| &self.inbox[..len])
    }
    /// Call only after another established recipient copied the entire retained
    /// packet. This is never an automatic drop-on-error policy.
    pub fn confirm_forwarded(&mut self) {
        self.pending = None;
        self.deadline = None;
    }
    pub fn tick<S: BondStore>(
        &mut self,
        app: &mut Application<'_, S>,
        now: u64,
    ) -> Result<(), PumpError> {
        if self.failed {
            return Err(PumpError::Transport(Error::IntegrityLost));
        }
        if now < self.last_us || self.deadline.is_some_and(|end| now >= end) {
            self.failed = true;
            app.transport_lost(Error::IntegrityLost);
            return Err(if now < self.last_us {
                PumpError::ClockRegressed
            } else {
                PumpError::Deadline
            });
        }
        self.last_us = now;
        Ok(())
    }
    pub fn receive<S: BondStore>(
        &mut self,
        app: &mut Application<'_, S>,
        connection: Connection,
        now: u64,
        acl: &[u8],
        sdp_scratch: &mut [u8],
    ) -> Result<bool, PumpError> {
        if connection != self.connection {
            return Err(PumpError::ForeignContext);
        }
        self.tick(app, now)?;
        if self.pending.is_some() {
            return Err(PumpError::Busy);
        }
        let packet = match bt_hci::AclPacket::parse(acl) {
            Ok(packet) => packet,
            Err(_) => {
                self.failed = true;
                app.transport_lost(Error::IntegrityLost);
                return Err(PumpError::Invalid);
            }
        };
        if packet.handle() != self.connection.handle {
            return Err(PumpError::ForeignContext);
        }
        if !self.reassembler.in_progress() {
            self.deadline = Some(now.checked_add(self.timeout).ok_or(PumpError::Invalid)?);
        }
        let result = self.reassembler.push(connection, packet);
        match result {
            Ok(Some(frame)) => {
                let len = frame
                    .encode(self.inbox)
                    .map_err(|_| PumpError::OutputTooSmall)?;
                self.pending = Some(len); // retain the original finite deadline through Busy delivery
                self.deliver(app, now, sdp_scratch)?;
                Ok(true)
            }
            Ok(None) => Ok(false),
            Err(error) => {
                self.failed = true;
                app.transport_lost(Error::IntegrityLost);
                Err(PumpError::Reassembly(error))
            }
        }
    }
    pub fn deliver<S: BondStore>(
        &mut self,
        app: &mut Application<'_, S>,
        now: u64,
        sdp_scratch: &mut [u8],
    ) -> Result<(), PumpError> {
        self.tick(app, now)?;
        let len = self.pending.ok_or(PumpError::Invalid)?;
        app.receive_l2cap(self.connection, now, &self.inbox[..len], sdp_scratch)
            .map_err(PumpError::Application)?;
        self.pending = None;
        self.deadline = None;
        Ok(())
    }
}
