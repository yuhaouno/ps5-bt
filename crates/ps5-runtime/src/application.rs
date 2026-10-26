// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit finite control-only startup; Bluetooth/capture/codec stay unavailable.
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
    Config, FactoryReport, NativeFactory, NativeListener, NativeStorage, NativeStore, Resolvers,
    ResourceError, ResourceKind,
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
}
impl Buffers {
    const fn new() -> Self {
        Self {
            native: NativeStorage::new(),
            rust: ControlStorage::new(),
            request: [0; 1024],
            response: [0; HTTP_RESPONSE],
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
        if resource == Resource::Encoder {
            self.initialize_rust(now)?;
        }
        Ok(())
    }
    fn take_stop(&mut self) -> bool {
        self.stop.load(Ordering::Acquire)
    }
    fn quantum(&mut self, lane: Lane, now: u64) -> Result<Quantum, i32> {
        // No transport/owned routing exists. Never call even the unavailable
        // input adapter or bind pumps/credits merely to populate a loop slot.
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
    duration_us: u64,
    stop_timeout_us: u64,
    budget: DriveBudget,
    max_connections: u32,
}
impl RunConfig {
    const DEFAULT: Self = Self {
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
}
fn uncertain_http(error: HttpFailure) -> bool {
    error.cleanup.is_some() || matches!(error.cause, bt_control::pump::Cause::Close(_))
}
impl Outcome {
    /// Retain prior cleanup uncertainty even if the final listener/factory close
    /// subsequently succeeds. No later cleanup attests an earlier client close.
    fn cleanup_uncertain(&self) -> bool {
        self.factory
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
        }
    }
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
            Config::control_only(
                directory,
                BindConfig {
                    address: [127, 0, 0, 1],
                    port: 0,
                },
            ),
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
    ps5_platform::log(b"event=application_begin schema=1 mode=control-only transport=Unavailable capture=Unavailable native_format=Unknown codec=Unavailable\n");
    let config = RunConfig::DEFAULT;
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
