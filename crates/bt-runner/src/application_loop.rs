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
    /// A persistent listener service uses Err for owner-wide failure; a single
    /// ConnectionPump may instead report an isolated, cleanly closed client.
    fn terminal_on_error(&self) -> bool {
        false
    }
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
/// Persistent, pointer-free adapter state for per-turn lending. It must remain
/// paired with the SAME Application/Runner, scratch input and native owners until
/// closed. Replacing this state is not native cleanup or a new connection epoch.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ApplicationLoopState {
    pending: Option<OwnedPacket>,
    stopping: bool,
    closed: bool,
    aborted: bool,
    audio_cleanup: Option<PipelineCleanup>,
    http_error: Option<HttpFailure>,
    http_disabled: bool,
    last_error: Option<ApplicationLoopError>,
    pipeline: Option<PipelineSnapshot>,
    bound_connection: Option<Connection>,
}
impl ApplicationLoopState {
    pub fn snapshot(&self) -> ApplicationLoopSnapshot {
        ApplicationLoopSnapshot {
            stopping: self.stopping,
            protocols_closed: self.closed,
            pending_input: self.pending,
            audio_cleanup: self.audio_cleanup,
            http_error: self.http_error,
            pipeline: self.pipeline,
            last_error: self.last_error,
        }
    }
}
// Fixed bounded ownership avoids heap allocation in the portable owner; lending
// retains only a reference logically even though the enum reserves inline space.
#[allow(clippy::large_enum_variant)]
enum StateBorrow<'a> {
    Owned(ApplicationLoopState),
    Lent(&'a mut ApplicationLoopState),
}
impl core::ops::Deref for StateBorrow<'_> {
    type Target = ApplicationLoopState;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Owned(state) => state,
            Self::Lent(state) => state,
        }
    }
}
impl core::ops::DerefMut for StateBorrow<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            Self::Owned(state) => state,
            Self::Lent(state) => state,
        }
    }
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
    state: StateBorrow<'a>,
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
            state: StateBorrow::Owned(ApplicationLoopState::default()),
        })
    }
    /// Lend existing native/application owners for a single quantum without a
    /// self-reference. Drop releases these borrows without aborting/scrubbing
    /// persistent state; the enclosing MainLoop MUST stop/cleanup on every exit.
    #[allow(clippy::too_many_arguments)]
    pub fn lend(
        app: &'a mut Application<'storage, S>,
        runner: &'a mut Runner<'storage, N>,
        commands: &'a mut C,
        transport: &'a mut T,
        input: &'a mut I,
        http: H,
        policy: Policy<'a>,
        buffers: LoopBuffers<'a>,
        state: &'a mut ApplicationLoopState,
    ) -> Result<Self, ApplicationLoopError> {
        let mut owner = Self::new(
            app, runner, commands, transport, input, http, policy, buffers,
        )?;
        owner.state = StateBorrow::Lent(state);
        Ok(owner)
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
            || (self.state.stopping && self.state.bound_connection != Some(connection))
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
        self.state.bound_connection = Some(connection);
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
            || (self.state.stopping && !audio.snapshot().stopped)
            || self.app.negotiated_parameters() != Some(audio.parameters())
        {
            return Err(ApplicationLoopError::InvalidConfiguration);
        }
        self.audio = Some(audio);
        Ok(())
    }
    pub fn snapshot(&self) -> ApplicationLoopSnapshot {
        ApplicationLoopSnapshot {
            stopping: self.state.stopping,
            protocols_closed: self.state.closed,
            pending_input: self.state.pending,
            audio_cleanup: self.state.audio_cleanup,
            http_error: self.state.http_error,
            pipeline: self
                .audio
                .as_ref()
                .map(|audio| audio.snapshot())
                .or(self.state.pipeline),
            last_error: self.state.last_error,
        }
    }
    pub fn application_status(&self) -> bt_app::Status {
        self.app.status()
    }
    fn stop_audio(&mut self) {
        if let Some(audio) = self.audio.as_mut() {
            self.state.audio_cleanup = Some(audio.stop());
        }
    }
    fn network(&mut self, now: u64) -> Result<Quantum, ApplicationLoopError> {
        if self.state.stopping || self.state.http_disabled {
            return Ok(Quantum::default());
        }
        let (legacy, manager) = self.runner.controls();
        match self.http.step(now, self.policy, legacy, manager) {
            Ok(q) => Ok(q),
            Err(error) => {
                self.state.http_error = Some(error);
                self.state.http_disabled = true;
                if error.cleanup.is_some() || self.http.terminal_on_error() {
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
        if self.state.pending.is_none() {
            self.state.pending = self
                .input
                .read_once(self.buffers.input)
                .map_err(ApplicationLoopError::Input)?;
        }
        let Some(packet) = self.state.pending else {
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
                    self.state.pending = None;
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
        self.state.pending = None;
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
        if self.state.closed {
            return Ok(Quantum::default());
        }
        let result = match lane {
            Lane::Network => self.network(now),
            Lane::Control => self.control(now),
            Lane::Input => self.inbound(now),
            Lane::Capture | Lane::Encode
                if !self.state.stopping && self.app.status().phase == Phase::Streaming =>
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
            self.state.last_error = Some(error);
        }
        result
    }
    pub fn request_stop(&mut self, now: u64) -> Result<(), ApplicationLoopError> {
        if self.state.stopping {
            return Ok(());
        }
        self.state.stopping = true;
        if let Err(error) = self.http.cancel() {
            self.state.http_error = Some(error);
        }
        self.stop_audio();
        self.runner
            .request_shutdown(self.app, now)
            .map_err(ApplicationLoopError::Runner)
    }
    /// One receive, control output and healthy media drain at most. The
    /// enclosing owner supplies the aggregate deadline and terminal abort.
    pub fn poll_stop(&mut self, now: u64) -> Result<bool, ApplicationLoopError> {
        if !self.state.stopping {
            return Err(ApplicationLoopError::InvalidConfiguration);
        }
        if self.state.closed {
            return Ok(true);
        }
        if self
            .runner
            .poll_shutdown(self.app, now)
            .map_err(ApplicationLoopError::Runner)?
        {
            self.state.closed = true;
            return Ok(true);
        }
        self.inbound(now)?;
        self.control(now)?;
        self.send(now)?;
        self.state.closed = self
            .runner
            .poll_shutdown(self.app, now)
            .map_err(ApplicationLoopError::Runner)?;
        Ok(self.state.closed)
    }
    pub fn abort_stop(&mut self) {
        if self.state.aborted || self.state.closed {
            return;
        }
        self.state.aborted = true;
        self.state.stopping = true;
        if let Err(error) = self.http.cancel() {
            self.state.http_error = Some(error);
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
        if let Some(audio) = self.audio.as_ref() {
            self.state.pipeline = Some(audio.snapshot());
        }
        if matches!(self.state, StateBorrow::Owned(_)) {
            self.abort_stop();
            self.buffers.input.fill(0);
            self.buffers.offer.fill(0);
        }
    }
}
