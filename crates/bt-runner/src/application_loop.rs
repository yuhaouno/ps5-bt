// SPDX-License-Identifier: GPL-3.0-or-later
//! Typed lane adapter for real application, HTTP, capture and ACL owners.
use crate::{
    AudioPipeline, Continuity, InputPump, Lane, OutputPump, PipelineCleanup, PipelineError,
    PipelineSnapshot, PumpError, PumpProgress, Quantum, Route, Runner, SendBuffers,
};
use bt_app::{Application, Phase};
use bt_audio::FinishEncoder;
use bt_capture::Backend;
use bt_control::{
    Control,
    http::Policy,
    management::Manager,
    pump::{Cause as HttpCause, ConnectionPump, Failure as HttpFailure, SocketIo},
};
use bt_core::BluetoothTransport;
use bt_device::BondStore;
use bt_hci::flow::{AclCreditOwner, CreditContext};
use bt_l2cap::Connection;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnedPacketKind {
    Hci { route: Route, epoch: u64 },
    Acl(Connection),
    Completed(CreditContext),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OwnedPacket {
    pub kind: OwnedPacketKind,
    pub bytes: usize,
}
/// Exactly one nonblocking read of already-demultiplexed owned traffic. The
/// dispatcher must preserve system/foreign traffic and supply genuine epochs;
/// returning queue loss/uncertain routing as an error is mandatory. This trait
/// never authorizes a competing raw HCI reader. None means no owned input ready.
pub trait OwnedInput {
    fn read_once(&mut self, output: &mut [u8]) -> Result<Option<OwnedPacket>, i32>;
}
pub trait HttpWork {
    fn step<const N: usize>(
        &mut self,
        now: u64,
        policy: Policy<'_>,
        legacy: &mut Control,
        manager: &mut Manager<N>,
    ) -> Result<Quantum, HttpFailure>;
    fn cancel(&mut self) -> Result<(), HttpFailure>;
}
impl<I: SocketIo> HttpWork for ConnectionPump<'_, I> {
    fn step<const N: usize>(
        &mut self,
        now: u64,
        policy: Policy<'_>,
        legacy: &mut Control,
        manager: &mut Manager<N>,
    ) -> Result<Quantum, HttpFailure> {
        self.step(now, policy, legacy, manager)?;
        // The connection primitive does not distinguish read/write WouldBlock
        // from partial progress, so permit exactly one syscall per owner turn.
        Ok(Quantum::default())
    }
    fn cancel(&mut self) -> Result<(), HttpFailure> {
        match self.cancel() {
            Err(failure) if failure.cause == HttpCause::Cancelled && failure.cleanup.is_none() => {
                Ok(())
            }
            result => result,
        }
    }
}
pub struct NoHttp;
impl HttpWork for NoHttp {
    fn step<const N: usize>(
        &mut self,
        _: u64,
        _: Policy<'_>,
        _: &mut Control,
        _: &mut Manager<N>,
    ) -> Result<Quantum, HttpFailure> {
        Ok(Quantum::default())
    }
    fn cancel(&mut self) -> Result<(), HttpFailure> {
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApplicationLoopError {
    InvalidConfiguration,
    MissingConnection,
    MissingAudio,
    InvalidInputCount,
    Input(i32),
    Http(HttpFailure),
    Runner(crate::Error),
    Pump(PumpError),
    Pipeline(PipelineError),
}
pub struct LoopBuffers<'a> {
    pub input: &'a mut [u8],
    pub offer: &'a mut [u8],
    pub sdp: &'a mut [u8],
    pub rtp: &'a mut [u8],
    pub basic: &'a mut [u8],
    pub acl: &'a mut [u8],
}
struct ConnectionWork<'a, 'storage> {
    input: &'a mut InputPump<'storage>,
    output: &'a mut OutputPump<'storage>,
    credits: &'a mut AclCreditOwner,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApplicationLoopSnapshot {
    pub stopping: bool,
    pub protocols_closed: bool,
    pub pending_input: Option<OwnedPacket>,
    pub audio_cleanup: Option<PipelineCleanup>,
    pub http_error: Option<HttpFailure>,
    pub pipeline: Option<PipelineSnapshot>,
    pub last_error: Option<ApplicationLoopError>,
}
/// Concrete assembly of existing Rust owners. Native resources and fixed buffers
/// are constructed by the enclosing MainLoop factory; this adapter borrows them
/// exclusively and binds every lane to the actual implementation. Native routing,
/// capture format evidence and credit reservation remain required prerequisites.
///
/// Attach pumps after secured connection ownership/credits exist, and attach a
/// freshly constructed AudioPipeline after exact SBC negotiation. These explicit
/// transitions also allow a bounded candidate to stop while discovering/pairing.
/// No native resource is opened or inferred by attaching a Rust owner.
pub struct ApplicationLoop<
    'a,
    'storage,
    S: BondStore,
    B: Backend,
    E: FinishEncoder,
    C: crate::CommandSink,
    T: BluetoothTransport,
    I: OwnedInput,
    H: HttpWork,
    const N: usize,
> {
    app: &'a mut Application<'storage, S>,
    runner: &'a mut Runner<'storage, N>,
    audio: Option<&'a mut AudioPipeline<'storage, B, E>>,
    connection: Option<ConnectionWork<'a, 'storage>>,
    commands: &'a mut C,
    transport: &'a mut T,
    input: &'a mut I,
    http: H,
    policy: Policy<'a>,
    buffers: LoopBuffers<'a>,
    pending: Option<OwnedPacket>,
    stopping: bool,
    closed: bool,
    aborted: bool,
    audio_cleanup: Option<PipelineCleanup>,
    http_error: Option<HttpFailure>,
    http_disabled: bool,
    last_error: Option<ApplicationLoopError>,
}
impl<
    'a,
    'storage,
    S: BondStore,
    B: Backend,
    E: FinishEncoder,
    C: crate::CommandSink,
    T: BluetoothTransport,
    I: OwnedInput,
    H: HttpWork,
    const N: usize,
> ApplicationLoop<'a, 'storage, S, B, E, C, T, I, H, N>
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        app: &'a mut Application<'storage, S>,
        runner: &'a mut Runner<'storage, N>,
        commands: &'a mut C,
        transport: &'a mut T,
        input: &'a mut I,
        http: H,
        policy: Policy<'a>,
        buffers: LoopBuffers<'a>,
    ) -> Result<Self, ApplicationLoopError> {
        if buffers.input.is_empty()
            || buffers.offer.is_empty()
            || buffers.sdp.is_empty()
            || buffers.rtp.is_empty()
            || buffers.basic.is_empty()
            || buffers.acl.is_empty()
        {
            return Err(ApplicationLoopError::InvalidConfiguration);
        }
        Ok(Self {
            app,
            runner,
            audio: None,
            connection: None,
            commands,
            transport,
            input,
            http,
            policy,
            buffers,
            pending: None,
            stopping: false,
            closed: false,
            aborted: false,
            audio_cleanup: None,
            http_error: None,
            http_disabled: false,
            last_error: None,
        })
    }
    pub fn attach_connection(
        &mut self,
        input: &'a mut InputPump<'storage>,
        output: &'a mut OutputPump<'storage>,
        credits: &'a mut AclCreditOwner,
    ) -> Result<(), ApplicationLoopError> {
        let connection = self
            .app
            .connection()
            .ok_or(ApplicationLoopError::MissingConnection)?;
        if self.connection.is_some()
            || self.stopping
            || input.connection() != connection
            || output.connection() != connection
            || credits.snapshot().context
                != (CreditContext {
                    handle: connection.handle,
                    epoch: connection.epoch,
                })
        {
            return Err(ApplicationLoopError::InvalidConfiguration);
        }
        self.connection = Some(ConnectionWork {
            input,
            output,
            credits,
        });
        Ok(())
    }
    pub fn attach_audio(
        &mut self,
        audio: &'a mut AudioPipeline<'storage, B, E>,
    ) -> Result<(), ApplicationLoopError> {
        if self
            .audio
            .as_ref()
            .is_some_and(|audio| !audio.snapshot().stopped)
            || self.stopping
            || self.app.negotiated_parameters() != Some(audio.parameters())
        {
            return Err(ApplicationLoopError::InvalidConfiguration);
        }
        self.audio = Some(audio);
        Ok(())
    }
    pub fn snapshot(&self) -> ApplicationLoopSnapshot {
        ApplicationLoopSnapshot {
            stopping: self.stopping,
            protocols_closed: self.closed,
            pending_input: self.pending,
            audio_cleanup: self.audio_cleanup,
            http_error: self.http_error,
            pipeline: self.audio.as_ref().map(|audio| audio.snapshot()),
            last_error: self.last_error,
        }
    }
    pub fn application_status(&self) -> bt_app::Status {
        self.app.status()
    }
    fn stop_audio(&mut self) {
        if let Some(audio) = self.audio.as_mut() {
            self.audio_cleanup = Some(audio.stop());
        }
    }
    fn network(&mut self, now: u64) -> Result<Quantum, ApplicationLoopError> {
        if self.stopping || self.http_disabled {
            return Ok(Quantum::default());
        }
        let (legacy, manager) = self.runner.controls();
        match self.http.step(now, self.policy, legacy, manager) {
            Ok(q) => Ok(q),
            Err(error) => {
                self.http_error = Some(error);
                self.http_disabled = true;
                if error.cleanup.is_some() {
                    Err(ApplicationLoopError::Http(error))
                } else {
                    Ok(Quantum::default())
                }
            }
        }
    }
    fn control(&mut self, now: u64) -> Result<Quantum, ApplicationLoopError> {
        self.runner
            .step(self.app, now)
            .map_err(ApplicationLoopError::Runner)?;
        if matches!(
            self.app.status().phase,
            Phase::Stopping | Phase::Stopped | Phase::Disconnecting | Phase::Failed | Phase::Closed
        ) {
            self.stop_audio();
        }
        if !self.runner.is_scanning()
            && matches!(self.app.status().phase, Phase::Idle | Phase::Closed)
        {
            return Ok(Quantum::default());
        }
        let progress = if self.runner.is_scanning() {
            self.runner
                .pump_discovery(self.app, now, self.commands, self.buffers.offer)
                .map_err(ApplicationLoopError::Runner)?
        } else if let Some(connection) = self.connection.as_mut() {
            let result = connection
                .output
                .poll_control(
                    self.app,
                    now,
                    self.commands,
                    self.transport,
                    connection.credits,
                    self.buffers.offer,
                    self.buffers.basic,
                    self.buffers.acl,
                )
                .map_err(ApplicationLoopError::Pump)?;
            let waiting = matches!(
                result,
                PumpProgress::WouldBlock
                    | PumpProgress::NoCredits
                    | PumpProgress::MediaDrain
                    | PumpProgress::ControlPending
            );
            return Ok(Quantum {
                progressed: !matches!(result, PumpProgress::Idle) && !waiting,
                continuity: Continuity {
                    send_waits: u64::from(waiting),
                    ..Continuity::default()
                },
                ..Quantum::default()
            });
        } else {
            self.runner
                .pump_application_commands(self.app, now, self.commands, self.buffers.offer)
                .map_err(ApplicationLoopError::Runner)?
        };
        Ok(Quantum {
            progressed: progress,
            ..Quantum::default()
        })
    }
    fn inbound(&mut self, now: u64) -> Result<Quantum, ApplicationLoopError> {
        if let Some(connection) = self.connection.as_mut() {
            connection
                .input
                .tick(self.app, now)
                .map_err(ApplicationLoopError::Pump)?;
            if connection.input.pending_basic().is_some() {
                connection
                    .input
                    .deliver(self.app, now, self.buffers.sdp)
                    .map_err(ApplicationLoopError::Pump)?;
                return Ok(Quantum {
                    progressed: true,
                    ..Quantum::default()
                });
            }
        }
        if self.pending.is_none() {
            self.pending = self
                .input
                .read_once(self.buffers.input)
                .map_err(ApplicationLoopError::Input)?;
        }
        let Some(packet) = self.pending else {
            return Ok(Quantum::default());
        };
        if packet.bytes == 0 || packet.bytes > self.buffers.input.len() {
            return Err(ApplicationLoopError::InvalidInputCount);
        }
        let bytes = &self.buffers.input[..packet.bytes];
        match packet.kind {
            OwnedPacketKind::Hci { route, epoch } => self
                .runner
                .receive_hci(self.app, route, epoch, now, bytes)
                .map_err(ApplicationLoopError::Runner)?,
            OwnedPacketKind::Acl(context) => {
                let connection = self
                    .connection
                    .as_mut()
                    .ok_or(ApplicationLoopError::MissingConnection)?;
                let result =
                    connection
                        .input
                        .receive(self.app, context, now, bytes, self.buffers.sdp);
                if connection.input.pending_basic().is_some() {
                    // Reassembly copied the packet to its retained inbox. It owns
                    // any delivery rejection; never replay the same ACL fragment.
                    self.pending = None;
                    self.buffers.input.fill(0);
                }
                result.map_err(ApplicationLoopError::Pump)?;
            }
            OwnedPacketKind::Completed(context) => {
                let connection = self
                    .connection
                    .as_mut()
                    .ok_or(ApplicationLoopError::MissingConnection)?;
                connection
                    .output
                    .complete(self.app, connection.credits, now, context, bytes)
                    .map_err(ApplicationLoopError::Pump)?;
            }
        }
        self.pending = None;
        self.buffers.input.fill(0);
        Ok(Quantum {
            progressed: true,
            ..Quantum::default()
        })
    }
    fn send(&mut self, now: u64) -> Result<Quantum, ApplicationLoopError> {
        let Some(audio) = self.audio.as_mut() else {
            return Ok(Quantum::default());
        };
        let Some(connection) = self.connection.as_mut() else {
            return Ok(Quantum::default());
        };
        audio
            .send_step(
                self.app,
                connection.output,
                connection.credits,
                self.transport,
                now,
                SendBuffers {
                    rtp: self.buffers.rtp,
                    basic: self.buffers.basic,
                    acl: self.buffers.acl,
                },
            )
            .map_err(ApplicationLoopError::Pipeline)
    }
    /// One concrete lane quantum; use MainLoop quotas to bound calls per turn.
    pub fn quantum(&mut self, lane: Lane, now: u64) -> Result<Quantum, ApplicationLoopError> {
        if self.closed {
            return Ok(Quantum::default());
        }
        let result = match lane {
            Lane::Network => self.network(now),
            Lane::Control => self.control(now),
            Lane::Input => self.inbound(now),
            Lane::Capture | Lane::Encode
                if !self.stopping && self.app.status().phase == Phase::Streaming =>
            {
                let audio = self
                    .audio
                    .as_mut()
                    .ok_or(ApplicationLoopError::MissingAudio)?;
                if lane == Lane::Capture {
                    audio.capture_step(now)
                } else {
                    audio.encode_step(now)
                }
                .map_err(ApplicationLoopError::Pipeline)
            }
            Lane::Capture | Lane::Encode => Ok(Quantum::default()),
            Lane::Send => self.send(now),
        };
        if let Err(error) = result {
            self.last_error = Some(error);
        }
        result
    }
    pub fn request_stop(&mut self, now: u64) -> Result<(), ApplicationLoopError> {
        if self.stopping {
            return Ok(());
        }
        self.stopping = true;
        if let Err(error) = self.http.cancel() {
            self.http_error = Some(error);
        }
        self.stop_audio();
        self.runner
            .request_shutdown(self.app, now)
            .map_err(ApplicationLoopError::Runner)
    }
    /// One receive, control output and healthy media drain at most. The
    /// enclosing owner supplies the aggregate deadline and terminal abort.
    pub fn poll_stop(&mut self, now: u64) -> Result<bool, ApplicationLoopError> {
        if !self.stopping {
            return Err(ApplicationLoopError::InvalidConfiguration);
        }
        if self.closed {
            return Ok(true);
        }
        if self
            .runner
            .poll_shutdown(self.app, now)
            .map_err(ApplicationLoopError::Runner)?
        {
            self.closed = true;
            return Ok(true);
        }
        self.inbound(now)?;
        self.control(now)?;
        self.send(now)?;
        self.closed = self
            .runner
            .poll_shutdown(self.app, now)
            .map_err(ApplicationLoopError::Runner)?;
        Ok(self.closed)
    }
    pub fn abort_stop(&mut self) {
        if self.aborted || self.closed {
            return;
        }
        self.aborted = true;
        self.stopping = true;
        if let Err(error) = self.http.cancel() {
            self.http_error = Some(error);
        }
        self.stop_audio();
        if let Some(connection) = self.connection.as_mut() {
            connection.output.cancel(self.app, connection.credits);
        }
        let _ = self.runner.abort_shutdown(self.app);
        // Retained input metadata remains observable; do not claim dispatch.
    }
}
impl<
    S: BondStore,
    B: Backend,
    E: FinishEncoder,
    C: crate::CommandSink,
    T: BluetoothTransport,
    I: OwnedInput,
    H: HttpWork,
    const N: usize,
> Drop for ApplicationLoop<'_, '_, S, B, E, C, T, I, H, N>
{
    fn drop(&mut self) {
        self.abort_stop();
        self.buffers.input.fill(0);
        self.buffers.offer.fill(0);
    }
}
