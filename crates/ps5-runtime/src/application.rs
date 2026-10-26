// SPDX-License-Identifier: GPL-3.0-or-later
//! Finite factory/HTTP startup with separately selected local diagnostic workloads.
#[cfg(all(feature = "application-generated", feature = "application-observe"))]
compile_error!("select one application diagnostic mode");
use bt_app::Application;
use bt_audio::{BlockEncoder, FinishEncoder};
use bt_capture::{Avcap2Backend, ResolverOps};
use bt_control::{Control, http::Policy, management::Manager, pump::Failure as HttpFailure};
use bt_device::{BondPolicy, DeviceManager};
use bt_discovery::ResultSlot;
use bt_listener::{BindConfig, ServiceConfig, ServiceState};
use bt_runner::{
    ApplicationLoop, ApplicationLoopSnapshot, ApplicationLoopState, Capabilities, CommandSink,
    ControlStorage, DriveBudget, DriveReport, DriveResources, HttpWork, Lane, LoopBuffers,
    LoopConfig, LoopPhase, LoopResources, MainLoop, OwnedInput, OwnedPacket, Quantum, Resource,
    StoreLease, StoreSlot, control_application_config, control_recovery_policy,
    control_runner_config, drive_bounded,
};
use core::{
    cell::UnsafeCell,
    ffi::{c_char, c_void},
    sync::atomic::{AtomicBool, Ordering},
};
use ps5_app::{
    CaptureConfig, Config, EncoderConfig, FactoryReport, Mode, NativeFactory, NativeListener,
    NativeStorage, NativeStore, Resolvers, ResourceError, ResourceKind,
    generated::{GeneratedConfig, GeneratedSnapshot, GeneratedStorage, GeneratedWorkload},
    observe::{ObserveConfig, ObserveSnapshot, ObserveWorkload},
};

const DEVICES: usize = 4;
const HTTP_RESPONSE: usize = 6144;
static STOP: AtomicBool = AtomicBool::new(false);
static RUNNING: AtomicBool = AtomicBool::new(false);
static POISONED: AtomicBool = AtomicBool::new(false);

/// An external loader/native callback may latch Stop before or during startup.
/// It is deliberately not reset at entry; no active stage can lose a pending Stop.
#[unsafe(no_mangle)]
pub extern "C" fn ps5_rust_application_request_stop() {
    STOP.store(true, Ordering::Release);
}

/// Observe the priority latch before native import/entropy initialization.
#[unsafe(no_mangle)]
pub extern "C" fn ps5_rust_application_stop_requested() -> i32 {
    i32::from(STOP.load(Ordering::Acquire))
}

struct Secret([u8; 32]);
impl Secret {
    fn copy(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 32
            || !bytes
                .iter()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
        {
            return None;
        }
        let mut value = Self([0; 32]);
        value.0.copy_from_slice(bytes);
        Some(value)
    }
}
impl Drop for Secret {
    fn drop(&mut self) {
        for byte in &mut self.0 {
            // SAFETY: each byte is owned, live and uniquely borrowed; volatile writes
            // keep cleanup observable to the compiler. No pointer escapes this guard.
            unsafe { core::ptr::write_volatile(byte, 0) };
        }
    }
}
struct Buffers {
    native: NativeStorage,
    rust: ControlStorage<DEVICES>,
    request: [u8; 1024],
    response: [u8; HTTP_RESPONSE],
    generated: GeneratedStorage,
    capture_raw: [u8; bt_capture::MAX_READ_BYTES],
}
impl Buffers {
    const fn new() -> Self {
        Self {
            native: NativeStorage::new(),
            rust: ControlStorage::new(),
            request: [0; 1024],
            response: [0; HTTP_RESPONSE],
            generated: GeneratedStorage::new(),
            capture_raw: [0; bt_capture::MAX_READ_BYTES],
        }
    }
}
struct GlobalBuffers(UnsafeCell<Buffers>);
// SAFETY: RUNNING grants one non-reentrant caller exclusive access; every native
// owner borrows fields only inside that guard. POISONED forbids reuse after
// uncertain cleanup. STOP is the only concurrently accessible mutable state.
unsafe impl Sync for GlobalBuffers {}
static BUFFERS: GlobalBuffers = GlobalBuffers(UnsafeCell::new(Buffers::new()));
struct EntryGuard<'a> {
    running: &'a AtomicBool,
}
impl<'a> EntryGuard<'a> {
    /// Caller has just acquired running by compare_exchange. Recheck poison
    /// AFTER that acquisition synchronizes with the previous owner's release;
    /// an earlier optimistic read cannot authorize backing-storage reuse.
    fn checked_after_acquire(running: &'a AtomicBool, poisoned: &AtomicBool) -> Option<Self> {
        let guard = Self { running };
        if poisoned.load(Ordering::Acquire) {
            None
        } else {
            Some(guard)
        }
    }
}
impl Drop for EntryGuard<'_> {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
    }
}

unsafe extern "C" {
    fn ps5_bootstrap_module_lookup(module: i32, name: *const c_char, out: *mut *mut c_void) -> i64;
}

struct DisabledEncoder(bt_core::SbcParameters);
impl BlockEncoder for DisabledEncoder {
    fn parameters(&self) -> bt_core::SbcParameters {
        self.0
    }
    fn encode_stereo(&mut self, _: &[[i16; 2]], _: &mut [u8]) -> Result<usize, bt_core::Error> {
        Err(bt_core::Error::Unsupported)
    }
}
impl FinishEncoder for DisabledEncoder {
    fn finish(self) -> Result<(), bt_core::Error> {
        Err(bt_core::Error::Unsupported)
    }
}
struct UnavailableCommands;
impl CommandSink for UnavailableCommands {
    fn send_owned_command(&mut self, _: u64, _: &[u8]) -> Result<(), bt_core::Error> {
        Err(bt_core::Error::Unsupported)
    }
}
struct UnavailableTransport;
impl bt_core::BluetoothTransport for UnavailableTransport {
    fn send_command(&mut self, _: &[u8]) -> Result<(), bt_core::Error> {
        Err(bt_core::Error::Unsupported)
    }
    fn send_acl(&mut self, _: &[u8]) -> Result<(), bt_core::Error> {
        Err(bt_core::Error::Unsupported)
    }
    fn receive<'a>(
        &mut self,
        _: &'a mut [u8],
        _: u32,
    ) -> Result<Option<bt_core::Received<'a>>, bt_core::Error> {
        Err(bt_core::Error::Unsupported)
    }
}
struct UnavailableInput;
impl OwnedInput for UnavailableInput {
    fn read_once(&mut self, _: &mut [u8]) -> Result<Option<OwnedPacket>, i32> {
        Err(-1)
    }
}
struct EndpointPolicy {
    host: [u8; 32],
    host_len: usize,
    origin: [u8; 40],
    origin_len: usize,
}
impl EndpointPolicy {
    fn loopback(port: u16) -> Self {
        let mut out = Self {
            host: [0; 32],
            host_len: 10,
            origin: [0; 40],
            origin_len: 0,
        };
        out.host[..10].copy_from_slice(b"127.0.0.1:");
        let mut digits = [0; 5];
        let mut cursor = digits.len();
        let mut value = port;
        loop {
            cursor -= 1;
            digits[cursor] = b'0' + (value % 10) as u8;
            value /= 10;
            if value == 0 {
                break;
            }
        }
        out.host[out.host_len..out.host_len + digits.len() - cursor]
            .copy_from_slice(&digits[cursor..]);
        out.host_len += digits.len() - cursor;
        out.origin[..7].copy_from_slice(b"http://");
        out.origin[7..7 + out.host_len].copy_from_slice(&out.host[..out.host_len]);
        out.origin_len = 7 + out.host_len;
        out
    }
}
struct ListenerHttp<'a, 'buffers, 'native> {
    listener: &'a mut NativeListener<'native>,
    service: &'a mut ServiceState<'buffers>,
}
impl HttpWork for ListenerHttp<'_, '_, '_> {
    fn step<const N: usize>(
        &mut self,
        now: u64,
        policy: Policy<'_>,
        legacy: &mut Control,
        manager: &mut Manager<N>,
    ) -> Result<Quantum, HttpFailure> {
        self.service
            .step(self.listener, now, policy, legacy, manager)
            .map(|progress| Quantum {
                progressed: progress.progressed,
                wake_at_us: progress.wake_at_us,
                ..Quantum::default()
            })
    }
    fn cancel(&mut self) -> Result<(), HttpFailure> {
        self.service.cancel(self.listener)
    }
    fn terminal_on_error(&self) -> bool {
        true
    }
}
type Lease<'a, 'native> = StoreLease<'a, NativeStore<'native>>;
type Lent<'owner, 'a, 'native> = ApplicationLoop<
    'owner,
    'a,
    Lease<'a, 'native>,
    Avcap2Backend<'a, 9216>,
    DisabledEncoder,
    UnavailableCommands,
    UnavailableTransport,
    UnavailableInput,
    ListenerHttp<'owner, 'a, 'native>,
    DEVICES,
>;

struct ControlResources<'a, 'native, 'config> {
    native: NativeFactory<'native, 'config>,
    store: &'a StoreSlot<NativeStore<'native>>,
    app: Option<Application<'a, Lease<'a, 'native>>>,
    runner: Option<bt_runner::Runner<'a, DEVICES>>,
    app_storage: Option<bt_app::Storage<'a>>,
    slots: Option<&'a mut [ResultSlot]>,
    recovery: Option<&'a mut bt_core::recovery::Recovery>,
    service: ServiceState<'a>,
    buffers: LoopBuffers<'a>,
    state: ApplicationLoopState,
    commands: UnavailableCommands,
    transport: UnavailableTransport,
    input: UnavailableInput,
    endpoint: Option<EndpointPolicy>,
    token: &'a [u8; 32],
    stop: &'a AtomicBool,
    mode: Mode,
    generated_storage: Option<&'a mut GeneratedStorage>,
    generated: Option<GeneratedWorkload<'a>>,
    observe: Option<ObserveWorkload>,
    capture_raw: &'a mut [u8; bt_capture::MAX_READ_BYTES],
    workload_finished: bool,
    last_work_us: Option<u64>,
    last_port_error: Option<ResourceError>,
    app_error: Option<bt_runner::ApplicationLoopError>,
}
impl<'a, 'native> ControlResources<'a, 'native, '_> {
    fn lend(&mut self) -> Result<Lent<'_, 'a, 'native>, i32> {
        let listener = self.native.listener_mut().ok_or(-401)?;
        let app = self.app.as_mut().ok_or(-402)?;
        let runner = self.runner.as_mut().ok_or(-403)?;
        let buffers = LoopBuffers {
            input: self.buffers.input,
            offer: self.buffers.offer,
            sdp: self.buffers.sdp,
            rtp: self.buffers.rtp,
            basic: self.buffers.basic,
            acl: self.buffers.acl,
        };
        let endpoint = self.endpoint.as_ref().ok_or(-418)?;
        ApplicationLoop::lend(
            app,
            runner,
            &mut self.commands,
            &mut self.transport,
            &mut self.input,
            ListenerHttp {
                listener,
                service: &mut self.service,
            },
            Policy {
                host: &endpoint.host[..endpoint.host_len],
                origin: &endpoint.origin[..endpoint.origin_len],
                bearer_token: self.token,
            },
            buffers,
            &mut self.state,
        )
        .map_err(|_| -404)
    }
    fn initialize_rust(&mut self, now: u64) -> Result<(), i32> {
        if self.mode != Mode::ControlOnly {
            let work_now = self.native.now_us().map_err(|error| {
                self.last_port_error = Some(error);
                error.code
            })?;
            if self.mode == Mode::GeneratedSbc {
                self.generated = Some(
                    GeneratedWorkload::new(
                        GeneratedConfig {
                            parameters: diagnostic_parameters(),
                            epoch_us: work_now,
                            duration_us: ps5_app::generated::DEFAULT_DURATION_US,
                            max_blocks: ps5_app::generated::DEFAULT_BLOCKS,
                            max_operations: ps5_app::generated::DEFAULT_OPERATIONS,
                            max_lateness_us: ps5_app::generated::DEFAULT_LATENESS_US,
                        },
                        self.generated_storage.take().ok_or(-430)?,
                    )
                    .map_err(|_| -431)?,
                );
            } else if self.mode == Mode::CaptureObserve {
                self.observe = Some(
                    ObserveWorkload::new(ObserveConfig::CONTINUOUS, work_now).map_err(|_| -432)?,
                );
            }
        }
        let store = self.native.take_store().ok_or(-405)?;
        if let Err(store) = self.store.install(store) {
            let _ = self.native.restore_store(store);
            return Err(-406);
        }
        let device = DeviceManager::new(
            self.store.lease(),
            BondPolicy {
                minimum_key_size: 16,
                require_mitm: false,
                require_secure_connections: false,
            },
        )
        .map_err(|_| -407)?;
        let app = Application::new(
            device,
            self.recovery.take().ok_or(-408)?,
            control_application_config(),
            self.app_storage.take().ok_or(-409)?,
            now,
        )
        .map_err(|_| -410)?;
        self.app = Some(app);
        self.runner = Some(
            bt_runner::Runner::new(
                control_runner_config(),
                Capabilities::default(),
                self.slots.take().ok_or(-411)?,
                now,
            )
            .map_err(|_| -412)?,
        );
        Ok(())
    }
    fn remember<T>(
        &mut self,
        result: Result<T, bt_runner::ApplicationLoopError>,
    ) -> Result<T, i32> {
        result.map_err(|error| {
            self.app_error = Some(error);
            -413
        })
    }
    fn cancel_service(&mut self) -> Result<(), i32> {
        self.service.request_stop();
        match self.native.listener_mut() {
            Some(listener) => self.service.cancel(listener).map_err(|_| -414),
            None => Ok(()),
        }
    }
}
fn kind(resource: Resource) -> Option<ResourceKind> {
    match resource {
        Resource::Store => Some(ResourceKind::Store),
        Resource::Transport => None,
        Resource::Listener => Some(ResourceKind::Listener),
        Resource::Capture => Some(ResourceKind::Capture),
        Resource::Encoder => Some(ResourceKind::Encoder),
    }
}
impl LoopResources for ControlResources<'_, '_, '_> {
    fn acquire(&mut self, resource: Resource, now: u64) -> Result<(), i32> {
        if self.stop.load(Ordering::Acquire) {
            return Err(-415);
        }
        if let Some(kind) = kind(resource) {
            self.native
                .acquire(kind, now)
                .map_err(|failure| failure.error.code)?;
        }
        if resource == Resource::Listener {
            let port = self.native.listener_mut().ok_or(-419)?.local_port();
            self.endpoint = Some(EndpointPolicy::loopback(port));
            #[cfg(all(test, feature = "application-control-host"))]
            tests::listener_bound(port);
        }
        if resource == Resource::Encoder
            && let Err(primary) = self.initialize_rust(now)
        {
            // The outer loop records this slot only after acquire returns Ok.
            // Release its successful native acquisition before earlier slots.
            if let Some(work) = self.generated.as_mut() {
                work.stop();
            }
            if let Err(cleanup) = self.native.cleanup(ResourceKind::Encoder) {
                self.last_port_error.get_or_insert(cleanup);
            }
            return Err(primary);
        }
        Ok(())
    }
    fn latest_observed_us(&self) -> Option<u64> {
        self.last_work_us
    }
    fn take_stop(&mut self) -> bool {
        self.stop.load(Ordering::Acquire) || self.workload_finished
    }
    fn quantum(&mut self, lane: Lane, now: u64) -> Result<Quantum, i32> {
        // Diagnostic lanes have no connection, packetization or radio sink.
        // Network/control continue through the actual Rust application owner.
        if self.mode == Mode::GeneratedSbc
            && matches!(lane, Lane::Capture | Lane::Encode | Lane::Send)
        {
            if self.stop.load(Ordering::Acquire) {
                return Ok(Quantum::default());
            }
            let observed = self.native.now_us().map_err(|error| {
                self.last_port_error = Some(error);
                error.code
            })?;
            self.last_work_us = Some(observed);
            if self.stop.load(Ordering::Acquire) {
                return Ok(Quantum::default());
            }
            let work = self.generated.as_mut().ok_or(-433)?;
            let progress = match lane {
                Lane::Capture => work.generate(observed),
                Lane::Encode => {
                    #[cfg(any(
                        feature = "application-generated-target",
                        feature = "application-control-host"
                    ))]
                    {
                        work.encode(observed, self.native.parts_mut().encoder.ok_or(-434)?)
                    }
                    #[cfg(not(any(
                        feature = "application-generated-target",
                        feature = "application-control-host"
                    )))]
                    {
                        return Err(-435);
                    }
                }
                Lane::Send => work.discard(observed),
                _ => unreachable!(),
            }
            .map_err(|_| -436)?;
            if lane == Lane::Send && progress.finished && !self.stop.load(Ordering::Acquire) {
                // Observe after the real codec and discard/hash work. A stale
                // turn timestamp cannot certify elapsed completion.
                let completed = self.native.now_us().map_err(|error| {
                    self.last_port_error = Some(error);
                    error.code
                })?;
                self.last_work_us = Some(completed);
                if !self.stop.load(Ordering::Acquire) {
                    work.confirm_completion(completed).map_err(|_| -441)?;
                }
            }
            self.workload_finished |= progress.finished;
            return Ok(Quantum {
                progressed: progress.progressed,
                wake_at_us: progress.wake_at_us,
                ..Quantum::default()
            });
        }
        if self.mode == Mode::CaptureObserve && lane == Lane::Capture {
            if self.stop.load(Ordering::Acquire) {
                return Ok(Quantum::default());
            }
            let observed = self.native.now_us().map_err(|error| {
                self.last_port_error = Some(error);
                error.code
            })?;
            self.last_work_us = Some(observed);
            if self.stop.load(Ordering::Acquire) {
                return Ok(Quantum::default());
            }
            let progress = self
                .observe
                .as_mut()
                .ok_or(-437)?
                .step(
                    self.native.parts_mut().capture.ok_or(-438)?,
                    self.capture_raw,
                    observed,
                    self.stop.load(Ordering::Acquire),
                )
                .map_err(|_| -439)?;
            self.workload_finished |= progress.finished;
            return Ok(Quantum {
                progressed: progress.progressed,
                wake_at_us: progress.wake_at_us,
                ..Quantum::default()
            });
        }
        if matches!(
            lane,
            Lane::Input | Lane::Capture | Lane::Encode | Lane::Send
        ) {
            return Ok(Quantum::default());
        }
        let result = {
            let mut work = self.lend()?;
            work.quantum(lane, now)
        };
        self.remember(result)
    }
    fn request_stop(&mut self, now: u64) -> Result<(), i32> {
        if let Some(work) = self.generated.as_mut() {
            work.stop();
        }
        if let Some(work) = self.observe.as_mut()
            && let Some(capture) = self.native.parts_mut().capture
        {
            work.step(capture, self.capture_raw, now, true)
                .map_err(|_| -440)?;
        }
        self.cancel_service()?;
        if self.app.is_some() && self.runner.is_some() {
            let result = {
                let mut work = self.lend()?;
                work.request_stop(now)
            };
            self.remember(result)?;
        }
        Ok(())
    }
    fn poll_stop(&mut self, now: u64) -> Result<bool, i32> {
        if self.app.is_none() || self.runner.is_none() {
            return Ok(true);
        }
        let result = {
            let mut work = self.lend()?;
            work.poll_stop(now)
        };
        self.remember(result)
    }
    fn abort_stop(&mut self) {
        if let Some(work) = self.generated.as_mut() {
            work.stop();
        }
        self.capture_raw.fill(0);
        let _ = self.cancel_service();
        if self.app.is_some()
            && self.runner.is_some()
            && let Ok(mut work) = self.lend()
        {
            work.abort_stop();
        }
    }
    fn cleanup(&mut self, resource: Resource) -> Result<(), i32> {
        if resource == Resource::Listener {
            self.cancel_service()?;
        }
        if resource == Resource::Store {
            self.runner.take();
            self.app.take();
            if let Some(store) = self.store.take().map_err(|_| -416)?
                && let Err(store) = self.native.restore_store(store)
            {
                let _ = store.close();
                return Err(-417);
            }
        }
        match kind(resource) {
            Some(kind) => self.native.cleanup(kind).map_err(|error| error.code),
            None => Ok(()),
        }
    }
}
impl DriveResources for ControlResources<'_, '_, '_> {
    fn now_us(&mut self) -> Result<u64, i32> {
        self.native.now_us().map_err(|error| {
            self.last_port_error = Some(error);
            error.code
        })
    }
    fn wait_slice(&mut self, deadline: u64, cancelled: bool) -> Result<(), i32> {
        self.native
            .wait_slice(deadline, cancelled)
            .map(|_| ())
            .map_err(|error| {
                self.last_port_error = Some(error);
                error.code
            })
    }
}
#[derive(Clone, Copy)]
struct RunConfig {
    mode: Mode,
    duration_us: u64,
    stop_timeout_us: u64,
    budget: DriveBudget,
    max_connections: u32,
}
impl RunConfig {
    const DEFAULT: Self = Self {
        mode: Mode::ControlOnly,
        duration_us: 5_000_000,
        stop_timeout_us: 100_000,
        budget: DriveBudget {
            max_turns: 20_000,
            max_waits: 10_000,
        },
        max_connections: 64,
    };
}
struct Outcome {
    code: i32,
    drive: Option<DriveReport>,
    factory: Option<FactoryReport>,
    application: Option<ApplicationLoopSnapshot>,
    service: Option<bt_listener::ServiceSnapshot>,
    port_error: Option<ResourceError>,
    generated: Option<GeneratedSnapshot>,
    observe: Option<ObserveSnapshot>,
}
fn uncertain_http(error: HttpFailure) -> bool {
    error.cleanup.is_some() || matches!(error.cause, bt_control::pump::Cause::Close(_))
}
impl Outcome {
    /// Retain prior cleanup uncertainty even if the final listener/factory close
    /// subsequently succeeds. No later cleanup attests an earlier client close.
    fn cleanup_uncertain(&self) -> bool {
        self.observe
            .is_some_and(|work| work.owner_cleanup.uncertain())
            || self
                .factory
                .is_some_and(|factory| factory.cleanup_uncertain())
            || self.service.is_some_and(|service| {
                service.last_failure.is_some_and(uncertain_http)
                    || service
                        .last_client
                        .is_some_and(|result| result.is_err_and(uncertain_http))
                    || service
                        .stop_outcome
                        .is_some_and(|result| result.is_err_and(uncertain_http))
                    || service
                        .listener_error
                        .is_some_and(|error| error.flags != 0 || error.cleanup_errno != 0)
            })
            || self
                .application
                .is_some_and(|application| application.http_error.is_some_and(uncertain_http))
            || self.drive.is_some_and(|drive| {
                drive
                    .outcome
                    .cleanup
                    .iter()
                    .any(|result| matches!(result, Some(Err(_))))
                    || drive.outcome.acquired.iter().any(|owned| *owned)
            })
            || self
                .port_error
                .is_some_and(|error| error.flags != 0 || error.cleanup.iter().any(|raw| *raw != 0))
    }
    fn settled(mut self) -> Self {
        if self.cleanup_uncertain() {
            self.code = 1;
        }
        self
    }
    fn early(code: i32) -> Self {
        Self {
            code,
            drive: None,
            factory: None,
            application: None,
            service: None,
            port_error: None,
            generated: None,
            observe: None,
        }
    }
}

fn diagnostic_parameters() -> bt_core::SbcParameters {
    bt_core::SbcParameters::new(
        bt_core::SbcSampleRate::Hz48000,
        bt_core::SbcChannelMode::JointStereo,
        bt_core::SbcBlocks::Sixteen,
        bt_core::SbcSubbands::Eight,
        bt_core::SbcAllocation::Loudness,
        32,
    )
    .expect("fixed conservative diagnostic SBC")
}
fn native_config(directory: &[u8], mode: Mode) -> Config<'_> {
    let mut config = Config::control_only(
        directory,
        BindConfig {
            address: [127, 0, 0, 1],
            port: 0,
        },
    );
    config.mode = mode;
    if mode == Mode::GeneratedSbc {
        config.encoder = Some(EncoderConfig {
            parameters: diagnostic_parameters(),
            negotiated_bitpool: bt_core::SbcBitpoolRange::new(2, 53).expect("fixed SBC range"),
        });
    }
    if mode == Mode::CaptureObserve {
        config.capture = Some(CaptureConfig {
            // Pending only: rebound to the first actual owned-clock observation
            // before any capture acquisition. No read uses this placeholder.
            budget: bt_capture::Budget {
                max_reads: 256,
                deadline_us: u64::MAX,
            },
            format: bt_capture::Format::UNKNOWN,
        });
    }
    config
}
#[allow(clippy::too_many_arguments)]
fn exercise(
    buffers: &mut Buffers,
    directory: &[u8],
    resolvers: Resolvers,
    secret: &Secret,
    stop: &AtomicBool,
    config: RunConfig,
) -> Outcome {
    if stop.load(Ordering::Acquire) {
        return Outcome::early(0);
    }
    if config.duration_us == 0 || config.stop_timeout_us == 0 || config.max_connections == 0 {
        return Outcome::early(1);
    }
    // SAFETY: the entry's caller establishes reviewed export signatures/lifetimes;
    // the resources borrow split address-stable backing fields until shutdown.
    let mut native = match unsafe {
        NativeFactory::create(
            &mut buffers.native,
            native_config(directory, config.mode),
            resolvers,
        )
    } {
        Ok(native) => native,
        Err(_) => return Outcome::early(1),
    };
    if let Err(failure) = native.acquire(ResourceKind::Clock, 0) {
        let mut out = Outcome::early(1);
        out.port_error = Some(failure.error);
        out.factory = Some(native.shutdown());
        return out.settled();
    }
    if stop.load(Ordering::Acquire) {
        let mut out = Outcome::early(0);
        out.factory = Some(native.shutdown());
        return out.settled();
    }
    let now = match native.now_us() {
        Ok(now) => now,
        Err(error) => {
            let mut out = Outcome::early(1);
            out.port_error = Some(error);
            out.factory = Some(native.shutdown());
            return out.settled();
        }
    };
    if now.checked_add(config.stop_timeout_us).is_none()
        || now.checked_add(1000).is_none()
        || now.checked_add(1_000_000).is_none()
    {
        let mut out = Outcome::early(1);
        out.factory = Some(native.shutdown());
        return out.settled();
    }
    let Some(deadline) = now.checked_add(config.duration_us) else {
        let mut out = Outcome::early(1);
        out.factory = Some(native.shutdown());
        return out.settled();
    };
    if config.mode == Mode::CaptureObserve
        && native
            .set_capture_budget_before_start(
                bt_capture::Budget {
                    max_reads: 256,
                    deadline_us: deadline,
                },
                now,
            )
            .is_err()
    {
        let mut out = Outcome::early(1);
        out.factory = Some(native.shutdown());
        return out.settled();
    }
    let store = StoreSlot::new();
    let mut recovery = match bt_core::recovery::Recovery::new(control_recovery_policy()) {
        Ok(recovery) => recovery,
        Err(_) => {
            let mut out = Outcome::early(1);
            out.factory = Some(native.shutdown());
            return out.settled();
        }
    };
    let service = match ServiceState::new(
        ServiceConfig {
            request_timeout_us: 100_000,
            max_connections: config.max_connections,
            deadline_us: None,
        },
        &mut buffers.request,
        &mut buffers.response,
    ) {
        Ok(service) => service,
        Err(_) => {
            let mut out = Outcome::early(1);
            out.factory = Some(native.shutdown());
            return out.settled();
        }
    };
    let parts = buffers.rust.parts();
    let resources = ControlResources {
        native,
        store: &store,
        app: None,
        runner: None,
        app_storage: Some(parts.application),
        slots: Some(parts.discovery),
        recovery: Some(&mut recovery),
        service,
        buffers: parts.work,
        state: ApplicationLoopState::default(),
        commands: UnavailableCommands,
        transport: UnavailableTransport,
        input: UnavailableInput,
        endpoint: None,
        token: &secret.0,
        stop,
        mode: config.mode,
        generated_storage: Some(&mut buffers.generated),
        generated: None,
        observe: None,
        capture_raw: &mut buffers.capture_raw,
        workload_finished: false,
        last_work_us: None,
        last_port_error: None,
        app_error: None,
    };
    let mut owner = match MainLoop::new_retaining(
        resources,
        LoopConfig {
            quotas: [1, 1, 1, 1, 1, 1],
            max_wait_us: 1000,
            stop_timeout_us: config.stop_timeout_us,
            deadline_us: Some(deadline),
        },
        now,
    ) {
        Ok(owner) => owner,
        Err((_, mut resources)) => {
            let mut out = Outcome::early(1);
            out.factory = Some(resources.native.shutdown());
            return out.settled();
        }
    };
    let drive = drive_bounded(&mut owner, config.budget, now);
    let resources = owner.resources_mut();
    let application = resources.state.snapshot();
    let service = resources.service.snapshot();
    let port_error = resources.last_port_error;
    let generated = resources.generated.as_ref().map(|work| work.snapshot());
    let observe = resources.observe.as_ref().map(|work| work.snapshot());
    let factory = resources.native.shutdown();
    let okay = drive.failure.is_none()
        && drive.outcome.phase == LoopPhase::Finished
        && matches!(
            drive.outcome.reason,
            Some(bt_runner::StopReason::Requested | bt_runner::StopReason::Deadline)
        )
        && !drive.outcome.forced
        && drive.outcome.stop_error.is_none()
        && drive
            .outcome
            .cleanup
            .iter()
            .all(|result| !matches!(result, Some(Err(_))))
        && !factory.cleanup_uncertain()
        && factory.primary.is_none();
    Outcome {
        code: if okay { 0 } else { 1 },
        drive: Some(drive),
        factory: Some(factory),
        application: Some(application),
        service: Some(service),
        port_error,
        generated,
        observe,
    }
    .settled()
}
fn report_native(error: ResourceError) {
    super::field(b"application_error_stage=0x", error.stage);
    super::field(b"application_error_bits=0x", error.code as u32);
    super::field(
        b"application_native_status_bits=0x",
        error.raw_status as u32,
    );
    super::field(b"application_error_flags=0x", error.flags);
    super::field(
        b"application_lookup_hi=0x",
        (error.raw_lookup as u64 >> 32) as u32,
    );
    super::field(b"application_lookup_lo=0x", error.raw_lookup as u32);
    for raw in error.cleanup {
        super::field(b"application_native_cleanup_bits=0x", raw as u32);
    }
}
fn http_cause(error: HttpFailure) -> (u32, i32) {
    use bt_control::pump::Cause;
    match error.cause {
        Cause::InvalidConfig => (1, 0),
        Cause::DeadlineOverflow => (2, 0),
        Cause::ClockRegression => (3, 0),
        Cause::Timeout => (4, 0),
        Cause::Read(raw) => (5, raw),
        Cause::Accept(raw) => (6, raw),
        Cause::ListenerChanged => (7, 0),
        Cause::Write(raw) => (8, raw),
        Cause::Close(raw) => (9, raw),
        Cause::Eof => (10, 0),
        Cause::WriteZero => (11, 0),
        Cause::InvalidCount => (12, 0),
        Cause::ResponseCapacity => (13, 0),
        Cause::Cancelled => (14, 0),
    }
}
fn report_http(error: HttpFailure, source: u32) {
    let (cause, raw) = http_cause(error);
    super::field(b"application_http_failure_source=0x", source);
    super::field(b"application_http_failure_cause=0x", cause);
    super::field(b"application_http_failure_raw_bits=0x", raw as u32);
    super::field(
        b"application_http_cleanup_present=0x",
        u32::from(error.cleanup.is_some()),
    );
    super::field(
        b"application_http_cleanup_bits=0x",
        error.cleanup.unwrap_or(0) as u32,
    );
}
fn report(out: &Outcome) {
    super::field(b"application_result=0x", out.code as u32);
    super::field(
        b"application_cleanup_uncertain=0x",
        u32::from(out.cleanup_uncertain()),
    );
    super::field(
        b"application_drive_entered=0x",
        u32::from(out.drive.is_some()),
    );
    super::field(
        b"application_factory_entered=0x",
        u32::from(out.factory.is_some()),
    );
    if let Some(drive) = out.drive {
        let reason = match drive.outcome.reason {
            None => 0,
            Some(bt_runner::StopReason::Requested) => 1,
            Some(bt_runner::StopReason::Deadline) => 2,
            Some(bt_runner::StopReason::Startup { resource, error }) => {
                super::field(b"application_fault_resource=0x", resource as u32);
                super::field(b"application_fault_bits=0x", error as u32);
                3
            }
            Some(bt_runner::StopReason::Work { lane, error }) => {
                super::field(b"application_fault_lane=0x", lane as u32);
                super::field(b"application_fault_bits=0x", error as u32);
                4
            }
            Some(bt_runner::StopReason::ClockRegressed) => 5,
            Some(bt_runner::StopReason::ClockOverflow) => 6,
            Some(bt_runner::StopReason::StopFailed(error)) => {
                super::field(b"application_fault_bits=0x", error as u32);
                7
            }
            Some(bt_runner::StopReason::StopTimeout) => 8,
            Some(bt_runner::StopReason::Dropped) => 9,
        };
        super::field(b"application_stop_reason=0x", reason);
        super::field(
            b"application_drive_failure=0x",
            match drive.failure {
                None => 0,
                Some(bt_runner::DriveFailure::InvalidBudget) => 1,
                Some(bt_runner::DriveFailure::Clock(_)) => 2,
                Some(bt_runner::DriveFailure::Wait(_)) => 3,
                Some(bt_runner::DriveFailure::TurnBudget) => 4,
                Some(bt_runner::DriveFailure::WaitBudget) => 5,
            },
        );
        super::field(b"application_clock_calls=0x", drive.clock_calls);
        super::field(b"application_turns=0x", drive.turns);
        super::field(
            b"application_cleanup_turns=0x",
            u32::from(drive.cleanup_turns),
        );
        super::field(b"application_waits=0x", drive.waits);
        super::field(b"application_forced=0x", u32::from(drive.outcome.forced));
        super::field(
            b"application_finished=0x",
            u32::from(drive.outcome.phase == LoopPhase::Finished),
        );
        super::field(
            b"application_stop_error_bits=0x",
            drive.outcome.stop_error.unwrap_or(0) as u32,
        );
    }
    if let Some(factory) = out.factory {
        if let Some((resource, error)) = factory.primary {
            super::field(b"application_failed_resource=0x", resource as u32);
            report_native(error);
        }
        let mask = |flags: [bool; 5]| {
            flags.iter().enumerate().fold(0u32, |mask, (index, flag)| {
                mask | (u32::from(*flag) << index)
            })
        };
        super::field(b"application_required_mask=0x", mask(factory.required));
        super::field(b"application_active_mask=0x", mask(factory.acquired));
        super::field(
            b"application_cleanup_mask=0x",
            mask(factory.cleanup.map(|result| result.is_some())),
        );
        super::field(
            b"application_cleanup_failed_mask=0x",
            mask(factory.cleanup.map(|result| matches!(result, Some(Err(_))))),
        );
        for result in factory.cleanup {
            if let Some(Err(error)) = result {
                super::field(b"application_cleanup_error_bits=0x", error.code as u32);
            }
        }
    }
    if let Some(service) = out.service {
        if let Some(error) = service.last_failure {
            report_http(error, 1);
        }
        if let Some(Err(error)) = service.stop_outcome {
            report_http(error, 2);
        }
        if let Some(Err(error)) = service.last_client {
            report_http(error, 3);
        }
        super::field(b"application_clients=0x", service.accepted);
        super::field(b"application_client_failures=0x", service.failed);
        if let Some(error) = service.listener_error {
            super::field(b"application_listener_status_bits=0x", error.status as u32);
            super::field(
                b"application_listener_errno_bits=0x",
                error.raw_errno as u32,
            );
            super::field(
                b"application_listener_cleanup_bits=0x",
                error.cleanup_errno as u32,
            );
            super::field(b"application_listener_flags=0x", error.flags);
        }
    }
    if let Some(application) = out.application {
        if let Some(error) = application.http_error {
            report_http(error, 4);
        }
        super::field(
            b"application_protocols_closed=0x",
            u32::from(application.protocols_closed),
        );
    }
    if let Some(work) = out.generated {
        use ps5_app::generated::{GeneratedFailure, Phase};
        super::field(
            b"application_generated_phase=0x",
            match work.phase {
                Phase::Running => 0,
                Phase::Complete => 1,
                Phase::Stopped => 2,
                Phase::Failed => 3,
            },
        );
        super::field(
            b"application_generated_completion_observed=0x",
            u32::from(work.completion_observed),
        );
        super::field(
            b"application_generated_error=0x",
            match work.error {
                None => 0,
                Some(GeneratedFailure::ClockRegression) => 1,
                Some(GeneratedFailure::Deadline) => 2,
                Some(GeneratedFailure::OperationBudget) => 3,
                Some(GeneratedFailure::Overflow) => 4,
                Some(GeneratedFailure::Continuity) => 5,
                Some(GeneratedFailure::ExpiredOffer) => 6,
                Some(GeneratedFailure::SummaryMismatch) => 7,
                Some(GeneratedFailure::Audio(_)) => 8,
            },
        );
        for (name, value) in [
            (
                b"application_generated_blocks=0x".as_slice(),
                work.generated_blocks,
            ),
            (
                b"application_generated_discarded_blocks=0x",
                work.discarded_blocks,
            ),
            (b"application_generated_bytes=0x", work.discarded_bytes),
            (b"application_generated_operations=0x", work.operations),
            (b"application_generated_prefix=0x", work.prefix),
            (b"application_generated_digest=0x", work.digest),
            (
                b"application_generated_summary_matches=0x",
                match work.summary_matches {
                    None => 0,
                    Some(false) => 1,
                    Some(true) => 2,
                },
            ),
            (
                b"application_generated_elapsed_hi=0x",
                (work.elapsed_us >> 32) as u32,
            ),
            (
                b"application_generated_elapsed_lo=0x",
                work.elapsed_us as u32,
            ),
            (
                b"application_generated_concealed=0x",
                work.scheduler.statistics.concealed_frames as u32,
            ),
            (
                b"application_generated_rebuffer=0x",
                work.scheduler.statistics.rebuffer_episodes as u32,
            ),
            (
                b"application_generated_expired=0x",
                work.scheduler.statistics.expired_encoded_blocks as u32,
            ),
            (
                b"application_generated_stopped_frames=0x",
                work.scheduler.statistics.stopped_pcm_frames as u32 + work.stopped_staged_frames,
            ),
            (
                b"application_generated_stopped_blocks=0x",
                work.scheduler.statistics.stopped_encoded_blocks as u32,
            ),
        ] {
            super::field(name, value);
        }
        let raw = match work.scheduler.failure {
            Some(bt_audio::Failure::Encoder(error) | bt_audio::Failure::Consumer(error)) => {
                match error {
                    bt_core::Error::Backend(raw) => raw,
                    bt_core::Error::Unsupported => -1,
                    bt_core::Error::Invalid => -2,
                    bt_core::Error::Busy => -3,
                    bt_core::Error::BufferTooSmall => -4,
                    bt_core::Error::Io => -5,
                    bt_core::Error::Disconnected => -6,
                    bt_core::Error::IntegrityLost => -7,
                    bt_core::Error::Timeout => -8,
                    bt_core::Error::AbiMismatch => -9,
                }
            }
            _ => 0,
        };
        super::field(b"application_generated_codec_bits=0x", raw as u32);
    }
    if let Some(work) = out.observe {
        use ps5_app::observe::{ObserveError, ObserveStop};
        let stats = work.statistics;
        for (name, value) in [
            (
                b"application_observe_started_hi=0x".as_slice(),
                (work.started_us >> 32) as u32,
            ),
            (b"application_observe_started_lo=0x", work.started_us as u32),
            (
                b"application_observe_elapsed_hi=0x",
                (work.last_observed_us.saturating_sub(work.started_us) >> 32) as u32,
            ),
            (
                b"application_observe_elapsed_lo=0x",
                work.last_observed_us.saturating_sub(work.started_us) as u32,
            ),
            (b"application_observe_max_reads=0x", work.config.max_reads),
            (
                b"application_observe_duration_us=0x",
                work.config.duration_us as u32,
            ),
            (
                b"application_observe_first_settle_us=0x",
                work.config.first_settle_us as u32,
            ),
            (
                b"application_observe_interval_us=0x",
                work.config.interval_us as u32,
            ),
        ] {
            super::field(name, value);
        }

        super::field(
            b"application_observe_reason=0x",
            match work.stop_reason {
                None => 0,
                Some(ObserveStop::Cancelled) => 1,
                Some(ObserveStop::Deadline) => 2,
                Some(ObserveStop::ReadBudget) => 3,
                Some(ObserveStop::Ended) => 4,
                Some(ObserveStop::Failed) => 5,
            },
        );
        super::field(
            b"application_observe_error=0x",
            match work.error {
                None => 0,
                Some(ObserveError::InvalidConfig) => 1,
                Some(ObserveError::InvalidBuffer) => 2,
                Some(ObserveError::ClockRegressed) => 3,
                Some(ObserveError::ClockOverflow) => 4,
                Some(ObserveError::OwnerProgressChanged) => 5,
                Some(ObserveError::Owner(_)) => 6,
                Some(ObserveError::NativeRead { .. }) => 7,
                Some(ObserveError::Cleanup(_)) => 8,
            },
        );
        for (name, value) in [
            (
                b"application_observe_reads=0x".as_slice(),
                stats.native_reads,
            ),
            (b"application_observe_data=0x", stats.data),
            (b"application_observe_empty=0x", stats.empty),
            (b"application_observe_zero=0x", stats.zero),
            (b"application_observe_overruns=0x", stats.overruns),
            (b"application_observe_ended=0x", stats.ended),
            (b"application_observe_native_errors=0x", stats.native_errors),
            (
                b"application_observe_owner_failures=0x",
                stats.owner_failures,
            ),
            (b"application_observe_gaps=0x", stats.unknown_gap_events),
            (
                b"application_observe_bytes_hi=0x",
                (stats.total_bytes >> 32) as u32,
            ),
            (b"application_observe_bytes_lo=0x", stats.total_bytes as u32),
            (b"application_observe_max_bytes=0x", stats.max_read_bytes),
        ] {
            super::field(name, value);
        }
        let last = work.last_read;
        super::field(
            b"application_observe_last_raw_bits=0x",
            last.map_or(0, |r| r.raw_status as u32),
        );
        super::field(
            b"application_observe_last_kind=0x",
            last.map_or(0, |r| match r.kind {
                bt_capture::ReadKind::Data => 1,
                bt_capture::ReadKind::Zero => 2,
                bt_capture::ReadKind::Empty => 3,
                bt_capture::ReadKind::Overrun => 4,
                bt_capture::ReadKind::Ended => 5,
                bt_capture::ReadKind::Error => 6,
            }),
        );
        super::field(
            b"application_observe_last_discontinuity=0x",
            u32::from(last.is_some_and(|r| r.discontinuity)),
        );
        let time = last.and_then(|r| r.native_time);
        super::field(
            b"application_observe_native_time_present=0x",
            u32::from(time.is_some()),
        );
        super::field(
            b"application_observe_native_value_hi=0x",
            time.map_or(0, |t| (t.value >> 32) as u32),
        );
        super::field(
            b"application_observe_native_value_lo=0x",
            time.map_or(0, |t| t.value as u32),
        );
        super::field(
            b"application_observe_native_domain=0x",
            time.map_or(0, |t| t.domain),
        );
        super::field(
            b"application_observe_host_hi=0x",
            last.map_or(0, |r| (r.host_observed_us >> 32) as u32),
        );
        super::field(
            b"application_observe_host_lo=0x",
            last.map_or(0, |r| r.host_observed_us as u32),
        );
        if let Some(ObserveError::Owner(bt_capture::OwnerFailure {
            error: bt_capture::CaptureError::Backend(d),
            ..
        })) = work.error
        {
            report_native(ResourceError {
                code: d.status,
                stage: d.stage,
                raw_status: d.raw_status,
                raw_lookup: d.raw_lookup,
                flags: d.cleanup_flags,
                cleanup: [d.raw_stop, d.raw_close, d.raw_terminate],
                ..ResourceError::default()
            });
        }
    }
    if let Some(error) = out.port_error {
        report_native(error);
    }
}
/// # Safety
/// secret must reference exactly32 readable ASCII lowercase hex bytes for this
/// call. lookup and the existing bootstrap module resolver must remain callable
/// with PLATFORM-CLOCK/CAPTURE-STREAM signatures for the entire owner lifetime.
/// The platform must initialize the reviewed POSIX store/listener imports. No
/// address/token is retained after return. This does not authorize a console run.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ps5_rust_application_probe(
    lookup: ps5_platform::clock_wait::Lookup,
    secret: *const u8,
    secret_len: u32,
) -> i32 {
    if secret.is_null() || secret_len != 32 || POISONED.load(Ordering::Acquire) {
        return 1;
    }
    if RUNNING
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        return 1;
    }
    let Some(_entry) = EntryGuard::checked_after_acquire(&RUNNING, &POISONED) else {
        return 1;
    };
    // SAFETY: exact readable input contract is imposed on the FFI caller above;
    // only32 bytes are copied, and this borrowed slice is never retained/logged.
    let Some(secret) = Secret::copy(unsafe { core::slice::from_raw_parts(secret, 32) }) else {
        return 1;
    };
    let mode = if cfg!(feature = "application-generated") {
        Mode::GeneratedSbc
    } else if cfg!(feature = "application-observe") {
        Mode::CaptureObserve
    } else {
        Mode::ControlOnly
    };
    ps5_platform::log(match mode {
        Mode::GeneratedSbc => b"event=application_begin schema=1 mode=generated-sbc transport=Unavailable capture=Unavailable native_format=Unknown codec=diagnostic\n",
        Mode::CaptureObserve => b"event=application_begin schema=1 mode=capture-observe transport=Unavailable capture=raw-observe native_format=Unknown codec=Unavailable\n",
        _ => b"event=application_begin schema=1 mode=control-only transport=Unavailable capture=Unavailable native_format=Unknown codec=Unavailable\n",
    });
    let config = RunConfig {
        mode,
        ..RunConfig::DEFAULT
    };
    super::field(b"application_duration_us=0x", config.duration_us as u32);
    super::field(
        b"application_stop_timeout_us=0x",
        config.stop_timeout_us as u32,
    );
    super::field(b"application_max_turns=0x", config.budget.max_turns);
    super::field(b"application_max_waits=0x", config.budget.max_waits);
    super::field(b"application_max_clients=0x", config.max_connections);
    let resolvers = Resolvers {
        current: lookup,
        capture: ResolverOps {
            abi_version: bt_capture::ABI_VERSION,
            struct_bytes: core::mem::size_of::<ResolverOps>() as u32,
            lookup_current: lookup,
            lookup_module: ps5_bootstrap_module_lookup,
        },
    };
    // SAFETY: successful RUNNING acquisition excludes every other user of this
    // static backing storage until all resources close and _entry releases it.
    let outcome = exercise(
        unsafe { &mut *BUFFERS.0.get() },
        b"/data/ps5-bt/control",
        resolvers,
        &secret,
        &STOP,
        config,
    );
    if outcome.cleanup_uncertain() {
        POISONED.store(true, Ordering::Release);
    }
    report(&outcome);
    ps5_platform::log(if outcome.code == 0 {
        b"event=application_complete\n"
    } else {
        b"event=application_failed\n"
    });
    outcome.code
}

#[cfg(all(test, feature = "application-control-host"))]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        path::PathBuf,
        sync::{
            Mutex,
            atomic::{AtomicI32, AtomicU32, AtomicU64, AtomicUsize},
        },
    };
    struct Clients {
        enabled: bool,
        port: u16,
        sent: usize,
        stream: Option<std::net::TcpStream>,
        current: Vec<u8>,
        responses: Vec<Vec<u8>>,
    }
    static CLIENTS: Mutex<Clients> = Mutex::new(Clients {
        enabled: false,
        port: 0,
        sent: 0,
        stream: None,
        current: Vec::new(),
        responses: Vec::new(),
    });
    pub(super) fn listener_bound(port: u16) {
        CLIENTS.lock().unwrap().port = port;
    }
    fn client_step() {
        let mut clients = CLIENTS.lock().unwrap();
        if !clients.enabled || clients.port == 0 || clients.responses.len() == 2 {
            return;
        }
        if clients.stream.is_none() {
            let mut stream =
                std::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, clients.port))
                    .unwrap();
            let request = format!(
                "GET /v2/status HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer 00112233445566778899aabbccddeeff\r\n\r\n",
                clients.port
            );
            stream.write_all(request.as_bytes()).unwrap();
            stream.set_nonblocking(true).unwrap();
            clients.stream = Some(stream);
            clients.sent += 1;
            return;
        }
        let mut bytes = [0; 2048];
        match clients.stream.as_mut().unwrap().read(&mut bytes) {
            Ok(0) => {
                clients.stream.take();
                let response = core::mem::take(&mut clients.current);
                clients.responses.push(response);
            }
            Ok(count) => clients.current.extend_from_slice(&bytes[..count]),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => panic!("client read: {error}"),
        }
    }
    static SERIAL: Mutex<()> = Mutex::new(());
    static TEST_STOP: AtomicBool = AtomicBool::new(false);
    static CLOCKS: AtomicU32 = AtomicU32::new(0);
    static WAITS: AtomicU32 = AtomicU32::new(0);
    static TICKS: AtomicU64 = AtomicU64::new(0);
    static STOP_CLOCK: AtomicU32 = AtomicU32::new(0);
    static FAIL_CLOCK: AtomicU32 = AtomicU32::new(0);
    static JUMP_CLOCK: AtomicU32 = AtomicU32::new(0);
    static FAIL_WAIT: AtomicI32 = AtomicI32::new(0);
    static STOP_LOOKUP: AtomicBool = AtomicBool::new(false);
    #[repr(C)]
    struct Time {
        seconds: i64,
        nanoseconds: i64,
    }
    unsafe extern "C" fn now(id: i32, out: *mut Time) -> i32 {
        assert_eq!(id, 4);
        let call = CLOCKS.fetch_add(1, Ordering::Relaxed) + 1;
        if call == STOP_CLOCK.load(Ordering::Relaxed) {
            TEST_STOP.store(true, Ordering::Release);
        }
        if call == FAIL_CLOCK.load(Ordering::Relaxed) {
            return -99;
        }
        if call == JUMP_CLOCK.load(Ordering::Relaxed) {
            TICKS.fetch_add(2_000_000, Ordering::Relaxed);
        }
        let us = TICKS.fetch_add(100, Ordering::Relaxed) + 100;
        // SAFETY: the actual C clock owner supplies one live reviewed timespec
        // output exclusively for this synchronous test callback.
        unsafe {
            out.write(Time {
                seconds: (us / 1_000_000) as i64,
                nanoseconds: ((us % 1_000_000) * 1000) as i64,
            })
        };
        0
    }
    unsafe extern "C" fn wait(us: u32) -> i32 {
        assert!(us > 0 && us <= 1000);
        WAITS.fetch_add(1, Ordering::Relaxed);
        if FAIL_WAIT.load(Ordering::Relaxed) == 0 {
            TICKS.fetch_add(u64::from(us), Ordering::Relaxed);
            client_step();
        }
        FAIL_WAIT.load(Ordering::Relaxed)
    }
    unsafe extern "C" fn lookup(name: *const c_char, out: *mut *mut c_void) -> i64 {
        if STOP_LOOKUP.load(Ordering::Relaxed) {
            TEST_STOP.store(true, Ordering::Release);
        }
        // SAFETY: C passes a reviewed nul-terminated export name and live single
        // pointer output. The callable synthetic exports remain static forever.
        unsafe {
            let name = core::ffi::CStr::from_ptr(name).to_bytes();
            let pointer = if name == b"clock_gettime" {
                now as *const () as *mut c_void
            } else {
                assert_eq!(name, b"usleep");
                wait as *const () as *mut c_void
            };
            out.write(pointer);
        }
        0
    }
    unsafe extern "C" fn no_module(_: i32, _: *const c_char, _: *mut *mut c_void) -> i64 {
        panic!("capture disabled in control-only candidate")
    }
    fn resolvers() -> Resolvers {
        Resolvers {
            current: lookup,
            capture: ResolverOps {
                abi_version: bt_capture::ABI_VERSION,
                struct_bytes: core::mem::size_of::<ResolverOps>() as u32,
                lookup_current: lookup,
                lookup_module: no_module,
            },
        }
    }
    fn reset() {
        TEST_STOP.store(false, Ordering::Relaxed);
        CLOCKS.store(0, Ordering::Relaxed);
        WAITS.store(0, Ordering::Relaxed);
        TICKS.store(0, Ordering::Relaxed);
        STOP_CLOCK.store(0, Ordering::Relaxed);
        FAIL_CLOCK.store(0, Ordering::Relaxed);
        JUMP_CLOCK.store(0, Ordering::Relaxed);
        FAIL_WAIT.store(0, Ordering::Relaxed);
        STOP_LOOKUP.store(false, Ordering::Relaxed);
        let mut clients = CLIENTS.lock().unwrap();
        *clients = Clients {
            enabled: false,
            port: 0,
            sent: 0,
            stream: None,
            current: Vec::new(),
            responses: Vec::new(),
        };
    }
    fn directory() -> PathBuf {
        static NEXT: AtomicUsize = AtomicUsize::new(1);
        let path = std::env::temp_dir().join(format!(
            "ps5-runtime-control-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        fs::canonicalize(path).unwrap()
    }
    fn run(path: &std::path::Path) -> Outcome {
        let mut buffers = Buffers::new();
        let secret = Secret::copy(b"00112233445566778899aabbccddeeff").unwrap();
        exercise(
            &mut buffers,
            path.to_str().unwrap().as_bytes(),
            resolvers(),
            &secret,
            &TEST_STOP,
            RunConfig {
                mode: Mode::ControlOnly,
                duration_us: if CLIENTS.lock().unwrap().enabled {
                    30_000
                } else {
                    3000
                },
                stop_timeout_us: 1000,
                budget: DriveBudget {
                    max_turns: 1000,
                    max_waits: 1000,
                },
                max_connections: 8,
            },
        )
    }
    #[test]
    fn actual_native_control_startup_duration_stop_and_faults_are_bounded() {
        let _serial = SERIAL.lock().unwrap();
        let path = directory();
        reset();
        let out = run(&path);
        assert_eq!(out.code, 0);
        let drive = out.drive.unwrap();
        assert!(drive.turns > 5);
        assert!(drive.waits > 0);
        assert!(!drive.outcome.forced);
        assert!(matches!(
            drive.outcome.reason,
            Some(bt_runner::StopReason::Deadline)
        ));
        assert!(out.application.unwrap().protocols_closed);
        assert_eq!(out.service.unwrap().accepted, 0);
        assert!(out.factory.unwrap().acquired.iter().all(|active| !*active));
        assert!(
            out.factory.unwrap().cleanup[ResourceKind::Clock as usize]
                .unwrap()
                .is_ok()
        );
        reset();
        CLIENTS.lock().unwrap().enabled = true;
        let out = run(&path);
        assert_eq!(out.code, 0);
        assert_eq!(out.service.unwrap().accepted, 2);
        assert_eq!(out.service.unwrap().completed, 2);
        {
            let clients = CLIENTS.lock().unwrap();
            assert_eq!(clients.responses.len(), 2);
            for response in &clients.responses {
                let response = std::str::from_utf8(response).unwrap();
                assert!(response.starts_with("HTTP/1.1 200 OK"));
                assert!(response.contains("\"transport\":\"unavailable\""));
                assert!(response.contains("\"capture\":\"unavailable\""));
            }
        }
        reset();
        TEST_STOP.store(true, Ordering::Relaxed);
        let out = run(&path);
        assert_eq!(out.code, 0);
        assert_eq!(CLOCKS.load(Ordering::Relaxed), 0);
        assert!(out.factory.is_none());
        reset();
        STOP_LOOKUP.store(true, Ordering::Relaxed);
        let out = run(&path);
        assert_eq!(out.code, 0);
        assert_eq!(CLOCKS.load(Ordering::Relaxed), 0);
        assert!(out.factory.unwrap().cleanup[ResourceKind::Store as usize].is_none());
        for stop_at in 2..=7 {
            reset();
            STOP_CLOCK.store(stop_at, Ordering::Relaxed);
            let out = run(&path);
            assert_eq!(out.code, 0, "stop_at={stop_at}");
            assert_eq!(CLOCKS.load(Ordering::Relaxed), stop_at);
            assert!(!out.factory.unwrap().cleanup_uncertain());
            assert_eq!(WAITS.load(Ordering::Relaxed), 0);
        }
        reset();
        FAIL_CLOCK.store(3, Ordering::Relaxed);
        let out = run(&path);
        assert_eq!(out.code, 1);
        assert_eq!(out.port_error.unwrap().raw_status, -99);
        assert!(out.drive.unwrap().outcome.forced);
        assert!(out.factory.unwrap().acquired.iter().all(|active| !*active));
        reset();
        FAIL_WAIT.store(-88, Ordering::Relaxed);
        let out = run(&path);
        assert_eq!(out.code, 1);
        assert_eq!(out.port_error.unwrap().raw_status, -88);
        assert_eq!(WAITS.load(Ordering::Relaxed), 1);
        assert!(out.factory.unwrap().acquired.iter().all(|active| !*active));
        reset();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        let out = run(&path);
        assert_eq!(out.code, 1);
        assert!(matches!(
            out.factory.unwrap().primary,
            Some((ResourceKind::Store, _))
        ));
        assert!(out.factory.unwrap().cleanup[ResourceKind::Listener as usize].is_none());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        fs::remove_dir_all(path).unwrap();
    }
    unsafe extern "C" {
        fn ps5_capture_test_reset();
        fn ps5_capture_test_read(raw: i32);
        fn ps5_capture_test_calls(phase: u32) -> u32;
        fn ps5_capture_test_current(name: *const c_char, out: *mut *mut c_void) -> i64;
        fn ps5_capture_test_module(module: i32, name: *const c_char, out: *mut *mut c_void) -> i64;
    }
    #[test]
    fn actual_generated_and_raw_observe_share_finite_native_startup() {
        let _serial = SERIAL.lock().unwrap();
        let path = directory();
        for mode in [Mode::GeneratedSbc, Mode::CaptureObserve] {
            reset();
            // SAFETY: SERIAL exclusively owns the process-global synthetic C
            // capture fixture, with exact reviewed export signatures and no PS5.
            unsafe {
                ps5_capture_test_reset();
                ps5_capture_test_read(8);
            }
            let mut buffers = Buffers::new();
            let secret = Secret::copy(b"00112233445566778899aabbccddeeff").unwrap();
            let mut ports = resolvers();
            ports.capture.lookup_current = ps5_capture_test_current;
            ports.capture.lookup_module = ps5_capture_test_module;
            let out = exercise(
                &mut buffers,
                path.to_str().unwrap().as_bytes(),
                ports,
                &secret,
                &TEST_STOP,
                RunConfig {
                    mode,
                    ..RunConfig::DEFAULT
                },
            );
            assert_eq!(out.code, 0, "mode={mode:?}");
            assert!(!out.cleanup_uncertain());
            assert!(!out.drive.unwrap().outcome.forced);
            let factory = out.factory.unwrap();
            assert_eq!(factory.acquired, [false; 5]);
            if mode == Mode::GeneratedSbc {
                let work = out.generated.unwrap();
                assert_eq!(work.phase, ps5_app::generated::Phase::Complete);
                assert_eq!(work.discarded_blocks, 750);
                assert_eq!(work.discarded_bytes, 57750);
                assert_eq!(work.summary_matches, Some(true));
                assert!(work.completion_observed);
                assert!(work.elapsed_us < 2_000_000);
                assert!(out.observe.is_none());
                assert!(!factory.required[ResourceKind::Capture as usize]);
                // SAFETY: fixture counter read under the same SERIAL guard.
                assert_eq!(unsafe { ps5_capture_test_calls(1) }, 0);
            } else {
                let work = out.observe.unwrap();
                assert_eq!(
                    work.stop_reason,
                    Some(ps5_app::observe::ObserveStop::Deadline)
                );
                assert!((20..=190).contains(&work.statistics.data));
                assert_eq!(
                    work.statistics.total_bytes,
                    u64::from(work.statistics.data) * 8
                );
                assert_eq!(work.statistics.unknown_gap_events, 0);
                assert!(work.error.is_none());
                assert!(out.generated.is_none());
                assert!(!factory.required[ResourceKind::Encoder as usize]);
                for stage in [5, 6, 7] {
                    // SAFETY: same exclusively held fixture, no native handles.
                    assert_eq!(unsafe { ps5_capture_test_calls(stage) }, 1);
                }
                assert!(buffers.capture_raw.iter().all(|byte| *byte == 0));
            }
        }
        // Priority Stop while each workload is active must retire local audio
        // before native factory cleanup, without running to its normal deadline.
        for mode in [Mode::GeneratedSbc, Mode::CaptureObserve] {
            reset();
            STOP_CLOCK.store(500, Ordering::Relaxed);
            // SAFETY: same SERIAL fixture owner as the full-profile runs above.
            unsafe {
                ps5_capture_test_reset();
                ps5_capture_test_read(8);
            }
            let mut buffers = Buffers::new();
            let secret = Secret::copy(b"00112233445566778899aabbccddeeff").unwrap();
            let mut ports = resolvers();
            ports.capture.lookup_current = ps5_capture_test_current;
            ports.capture.lookup_module = ps5_capture_test_module;
            let out = exercise(
                &mut buffers,
                path.to_str().unwrap().as_bytes(),
                ports,
                &secret,
                &TEST_STOP,
                RunConfig {
                    mode,
                    ..RunConfig::DEFAULT
                },
            );
            assert_eq!(out.code, 0);
            assert!(!out.cleanup_uncertain());
            // One bounded stopping clock may follow the priority observation.
            assert!((500..=501).contains(&CLOCKS.load(Ordering::Relaxed)));
            if mode == Mode::GeneratedSbc {
                let work = out.generated.unwrap();
                assert_eq!(work.phase, ps5_app::generated::Phase::Stopped);
                assert!(work.discarded_blocks > 0 && work.discarded_blocks < 750);
            } else {
                let work = out.observe.unwrap();
                assert_eq!(
                    work.stop_reason,
                    Some(ps5_app::observe::ObserveStop::Cancelled)
                );
                assert!(work.statistics.data > 0 && work.statistics.data < 190);
            }
        }
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn failed_final_acquisition_and_fresh_lane_deadlines_preserve_evidence() {
        let _serial = SERIAL.lock().unwrap();
        let path = directory();
        for scenario in 0..3 {
            reset();
            // SAFETY: one SERIAL guard owns the host capture/export fixture.
            unsafe {
                ps5_capture_test_reset();
                ps5_capture_test_read(8);
            }
            let mode = if scenario == 2 {
                Mode::CaptureObserve
            } else {
                Mode::GeneratedSbc
            };
            if scenario == 0 {
                FAIL_CLOCK.store(7, Ordering::Relaxed);
            } else {
                JUMP_CLOCK.store(if scenario == 1 { 11 } else { 9 }, Ordering::Relaxed);
            }
            let mut buffers = Buffers::new();
            let secret = Secret::copy(b"00112233445566778899aabbccddeeff").unwrap();
            let mut ports = resolvers();
            ports.capture.lookup_current = ps5_capture_test_current;
            ports.capture.lookup_module = ps5_capture_test_module;
            let out = exercise(
                &mut buffers,
                path.to_str().unwrap().as_bytes(),
                ports,
                &secret,
                &TEST_STOP,
                RunConfig {
                    mode,
                    ..RunConfig::DEFAULT
                },
            );
            assert!(!out.cleanup_uncertain(), "scenario={scenario}");
            if scenario == 0 {
                assert_eq!(out.code, 1);
                assert_eq!(out.port_error.unwrap().raw_status, -99);
                assert!(
                    out.factory.unwrap().cleanup[ResourceKind::Encoder as usize]
                        .unwrap()
                        .is_ok()
                );
                assert!(
                    out.drive
                        .unwrap()
                        .outcome
                        .cleanup
                        .iter()
                        .all(|result| !matches!(result, Some(Err(_))))
                );
            } else if scenario == 1 {
                assert_eq!(out.code, 1);
                let work = out.generated.unwrap();
                assert_eq!(work.scheduler.statistics.encoded_blocks, 1);
                assert_eq!(work.discarded_blocks, 0);
                assert_eq!(
                    work.error,
                    Some(ps5_app::generated::GeneratedFailure::Deadline)
                );
                assert!(!work.completion_observed);
            } else {
                assert_eq!(out.code, 0);
                let work = out.observe.unwrap();
                assert_eq!(
                    work.stop_reason,
                    Some(ps5_app::observe::ObserveStop::Deadline)
                );
                assert_eq!(work.statistics.native_reads, 0);
                // SAFETY: counter read under the same exclusive fixture guard.
                assert_eq!(unsafe { ps5_capture_test_calls(4) }, 0);
            }
        }
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn secret_validation_and_endpoint_policy_are_bounded() {
        assert!(Secret::copy(b"invalid").is_none());
        assert!(Secret::copy(&[0; 32]).is_none());
        let policy = EndpointPolicy::loopback(65535);
        assert_eq!(&policy.host[..policy.host_len], b"127.0.0.1:65535");
        assert_eq!(
            &policy.origin[..policy.origin_len],
            b"http://127.0.0.1:65535"
        );
    }
    #[test]
    fn exported_stop_latch_is_clock_independent_and_not_reset() {
        assert_eq!(ps5_rust_application_stop_requested(), 0);
        ps5_rust_application_request_stop();
        assert_eq!(ps5_rust_application_stop_requested(), 1);
        ps5_rust_application_request_stop();
        assert_eq!(ps5_rust_application_stop_requested(), 1);
    }
    #[test]
    fn earlier_client_cleanup_failure_survives_clean_factory_close() {
        let clean_factory = FactoryReport {
            phase: ps5_app::FactoryPhase::Closed,
            acquired: [false; 5],
            required: [true, true, true, false, false],
            cleanup: [Some(Ok(())), Some(Ok(())), Some(Ok(())), None, None],
            primary: None,
        };
        let failure = HttpFailure {
            cause: bt_control::pump::Cause::Close(5),
            cleanup: Some(5),
        };
        assert_eq!(http_cause(failure), (9, 5));
        for source in 0..5 {
            let mut out = Outcome::early(0);
            out.factory = Some(clean_factory);
            assert!(!out.cleanup_uncertain());
            let mut service = bt_listener::ServiceSnapshot::default();
            match source {
                0 => service.last_failure = Some(failure),
                1 => service.stop_outcome = Some(Err(failure)),
                2 => {
                    service.listener_error = Some(bt_listener::Diagnostic {
                        flags: 1,
                        cleanup_errno: 5,
                        ..bt_listener::Diagnostic::default()
                    })
                }
                3 => {
                    let mut app = ApplicationLoopState::default().snapshot();
                    app.http_error = Some(failure);
                    out.application = Some(app);
                }
                _ => {
                    let mut cleanup = [None; 5];
                    cleanup[2] = Some(Err(-5));
                    out.drive = Some(DriveReport {
                        outcome: bt_runner::LoopReport {
                            phase: LoopPhase::Finished,
                            reason: Some(bt_runner::StopReason::Work {
                                lane: Lane::Network,
                                error: -413,
                            }),
                            forced: true,
                            stop_error: Some(-5),
                            cleanup,
                            acquired: [false; 5],
                            statistics: bt_runner::LoopStatistics::default(),
                        },
                        failure: None,
                        clock_calls: 0,
                        waits: 0,
                        turns: 0,
                        cleanup_turns: 0,
                        last_observed_us: 0,
                    });
                }
            }
            out.service = Some(service);
            assert!(!out.factory.unwrap().cleanup_uncertain());
            assert!(out.cleanup_uncertain());
            let out = out.settled();
            assert_eq!(out.code, 1);
            // The entry uses this same aggregate predicate for its process-wide
            // poison latch; later successful factory close cannot reopen it.
            let poisoned = AtomicBool::new(false);
            if out.cleanup_uncertain() {
                poisoned.store(true, Ordering::Release);
            }
            assert!(poisoned.load(Ordering::Acquire));
            if source == 0 {
                assert_eq!(out.service.unwrap().last_failure, Some(failure));
            }
            if source == 4 {
                assert!(matches!(
                    out.drive.unwrap().outcome.reason,
                    Some(bt_runner::StopReason::Work { error: -413, .. })
                ));
            }
        }
    }
    #[test]
    fn poison_is_rechecked_after_acquiring_entry_and_releases_guard() {
        let running = AtomicBool::new(false);
        let poisoned = AtomicBool::new(false);
        // Simulate an optimistic precheck before the preceding owner poisons
        // storage and releases running. The successful CAS alone is insufficient.
        assert!(!poisoned.load(Ordering::Acquire));
        poisoned.store(true, Ordering::Release);
        assert!(
            running
                .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
        );
        assert!(EntryGuard::checked_after_acquire(&running, &poisoned).is_none());
        assert!(!running.load(Ordering::Acquire));
        let clean = AtomicBool::new(false);
        assert!(
            running
                .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
        );
        let guard = EntryGuard::checked_after_acquire(&running, &clean).unwrap();
        assert!(running.load(Ordering::Acquire));
        drop(guard);
        assert!(!running.load(Ordering::Acquire));
    }
}
