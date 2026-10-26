// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{
    Config, ConfigError, FactoryPhase, ResourceError, ResourceFactory, ResourceKind, ResourcePorts,
};
#[cfg(any(feature = "host-libsbc", feature = "target-libsbc"))]
use bt_audio::SbcEncoder;
use bt_capture::{Avcap2Backend, CaptureOwner, CaptureStorage, OwnerFailure, ResolverOps};
#[cfg(any(feature = "host-libsbc", feature = "target-libsbc"))]
use bt_codec::{Encoder, EncoderStorage};
use bt_listener::{Listener, ListenerStorage};
use bt_store::{FileStore, StoreStorage};
use ps5_platform::clock_wait::{ClockWait, ClockWaitError, ClockWaitStorage, Lookup, WaitOutcome};
pub type NativeStore<'a> = FileStore<'a, 1024>;
pub type NativeListener<'a> = Listener<'a, 256>;
pub type NativeClock<'a> = ClockWait<'a, 128>;
pub type NativeCapture<'a> = CaptureOwner<Avcap2Backend<'a, 9216>>;
#[cfg(any(feature = "host-libsbc", feature = "target-libsbc"))]
pub type NativeEncoder<'a> = SbcEncoder<'a, 'a, 8192>;
/// Fixed capacity is policy, checked against each backend's queried requirements.
/// Caller must place this outside the small loader stack and keep it stable while
/// borrowed. It contains no self references; resource handles borrow its fields.
pub struct NativeStorage {
    clock: ClockWaitStorage<128>,
    store: StoreStorage<1024>,
    listener: ListenerStorage<256>,
    capture: CaptureStorage<9216>,
    #[cfg(any(feature = "host-libsbc", feature = "target-libsbc"))]
    encoder: EncoderStorage<8192>,
    #[cfg(any(feature = "host-libsbc", feature = "target-libsbc"))]
    interleaved: [i16; 256],
}
impl NativeStorage {
    pub const fn new() -> Self {
        Self {
            clock: ClockWaitStorage::new(),
            store: StoreStorage::new(),
            listener: ListenerStorage::new(),
            capture: CaptureStorage::new(),
            #[cfg(any(feature = "host-libsbc", feature = "target-libsbc"))]
            encoder: EncoderStorage::new(),
            #[cfg(any(feature = "host-libsbc", feature = "target-libsbc"))]
            interleaved: [0; 256],
        }
    }
}
impl Default for NativeStorage {
    fn default() -> Self {
        Self::new()
    }
}
#[derive(Clone, Copy)]
pub struct Resolvers {
    pub current: Lookup,
    pub capture: ResolverOps,
}
pub struct NativePorts<'storage, 'config> {
    config: Config<'config>,
    resolvers: Resolvers,
    clock_storage: Option<&'storage mut ClockWaitStorage<128>>,
    store_storage: Option<&'storage mut StoreStorage<1024>>,
    listener_storage: Option<&'storage mut ListenerStorage<256>>,
    capture_storage: Option<&'storage mut CaptureStorage<9216>>,
    #[cfg(any(feature = "host-libsbc", feature = "target-libsbc"))]
    encoder_storage: Option<&'storage mut EncoderStorage<8192>>,
    #[cfg(any(feature = "host-libsbc", feature = "target-libsbc"))]
    interleaved: Option<&'storage mut [i16; 256]>,
    clock: Option<NativeClock<'storage>>,
    store: Option<NativeStore<'storage>>,
    listener: Option<NativeListener<'storage>>,
    capture: Option<NativeCapture<'storage>>,
    #[cfg(any(feature = "host-libsbc", feature = "target-libsbc"))]
    encoder: Option<NativeEncoder<'storage>>,
    store_lent: bool,
}
pub type NativeFactory<'storage, 'config> =
    ResourceFactory<'config, NativePorts<'storage, 'config>>;
pub struct ResourceParts<'a, 'storage> {
    pub clock: Option<&'a mut NativeClock<'storage>>,
    pub store: Option<&'a mut NativeStore<'storage>>,
    pub listener: Option<&'a mut NativeListener<'storage>>,
    pub capture: Option<&'a mut NativeCapture<'storage>>,
    #[cfg(any(feature = "host-libsbc", feature = "target-libsbc"))]
    pub encoder: Option<&'a mut NativeEncoder<'storage>>,
}
fn clock_error(error: ClockWaitError) -> ResourceError {
    use ClockWaitError::*;
    let diagnostic = match error {
        Abi(d) | Native(d) | InvalidTime(d) | TimeOverflow(d) | ClockRegressed(d) => Some(d),
        _ => None,
    };
    diagnostic.map_or(
        ResourceError::local(match error {
            InvalidStorage => -1100,
            Closed => -1102,
            DeadlineOverflow => -1103,
            _ => -1101,
        }),
        |d| ResourceError {
            code: if matches!(error, Abi(_)) {
                -1101
            } else {
                d.status
            },
            stage: d.stage,
            raw_status: d.raw_status,
            raw_lookup: d.raw_lookup,
            flags: 0,
            cleanup: [0; 3],
            raw_time: [d.raw_seconds, d.raw_nanoseconds],
        },
    )
}
fn store_error(d: bt_store::Diagnostic) -> ResourceError {
    ResourceError {
        code: if d.status == 0 { -1300 } else { d.status },
        stage: d.stage,
        raw_status: d.raw_errno,
        flags: d.flags,
        cleanup: [d.cleanup_errno, 0, 0],
        ..ResourceError::default()
    }
}
fn listener_error(d: bt_listener::Diagnostic) -> ResourceError {
    ResourceError {
        code: if d.status == 0 { -1300 } else { d.status },
        stage: d.stage,
        raw_status: d.raw_errno,
        flags: d.flags,
        cleanup: [d.cleanup_errno, 0, 0],
        ..ResourceError::default()
    }
}
fn capture_error(f: OwnerFailure) -> ResourceError {
    let mut result = match f.error {
        bt_capture::CaptureError::Backend(d) => ResourceError {
            code: d.status,
            stage: d.stage,
            raw_status: d.raw_status,
            raw_lookup: d.raw_lookup,
            flags: d.cleanup_flags,
            cleanup: [d.raw_stop, d.raw_close, d.raw_terminate],
            ..ResourceError::default()
        },
        error => ResourceError::local(match error {
            bt_capture::CaptureError::InvalidBudget => -1201,
            bt_capture::CaptureError::InvalidState => -1202,
            bt_capture::CaptureError::InvalidBuffer => -1203,
            bt_capture::CaptureError::InvalidOutcome => -1204,
            bt_capture::CaptureError::ClockRegressed => -1205,
            bt_capture::CaptureError::ReadBudget => -1206,
            bt_capture::CaptureError::Deadline => -1207,
            bt_capture::CaptureError::Backend(_) => unreachable!(),
        }),
    };
    let preserve_primary = matches!(f.error, bt_capture::CaptureError::Backend(_));
    if let Some(Err(d)) = f.cleanup.stop {
        if !preserve_primary {
            result.code = d.status;
            result.stage = d.stage;
            result.raw_status = d.raw_status;
        }
        result.flags |= 1;
        result.cleanup[0] = d.raw_stop;
    }
    if let Some(Err(d)) = f.cleanup.close {
        if !preserve_primary && !matches!(f.cleanup.stop, Some(Err(_))) {
            result.code = d.status;
            result.stage = d.stage;
            result.raw_status = d.raw_status;
        }
        result.flags |= d.cleanup_flags | 2;
        result.cleanup[1] = d.raw_close;
        result.cleanup[2] = d.raw_terminate;
    }
    result
}
#[cfg(any(feature = "host-libsbc", feature = "target-libsbc"))]
fn codec_error(error: bt_core::Error) -> ResourceError {
    ResourceError::local(match error {
        bt_core::Error::Backend(raw) => raw,
        bt_core::Error::BufferTooSmall => -4,
        bt_core::Error::Unsupported => -1,
        bt_core::Error::Invalid => -2,
        bt_core::Error::Busy => -3,
        bt_core::Error::Io => -5,
        bt_core::Error::Disconnected => -6,
        bt_core::Error::IntegrityLost => -7,
        bt_core::Error::Timeout => -8,
        bt_core::Error::AbiMismatch => -9,
    })
}
impl<'storage, 'config> NativeFactory<'storage, 'config> {
    /// # Safety
    /// The existing loader resolver must be initialized and both callback sets
    /// must satisfy the reviewed callable-export signature/lifetime contracts in
    /// PLATFORM-CLOCK.md and CAPTURE-STREAM.md for this factory's full lifetime.
    /// The matching source/layout-reviewed target store/listener implementation
    /// must be linked and its import table initialized before native acquisition.
    /// No callback may reenter. This does not authorize execution or radio use.
    pub unsafe fn create(
        storage: &'storage mut NativeStorage,
        config: Config<'config>,
        resolvers: Resolvers,
    ) -> Result<Self, ConfigError> {
        config.validate()?;
        #[cfg(not(any(feature = "host-libsbc", feature = "target-libsbc")))]
        if config.encoder.is_some() {
            return Err(ConfigError::EncoderUnavailable);
        }
        let NativeStorage {
            clock,
            store,
            listener,
            capture,
            #[cfg(any(feature = "host-libsbc", feature = "target-libsbc"))]
            encoder,
            #[cfg(any(feature = "host-libsbc", feature = "target-libsbc"))]
            interleaved,
        } = storage;
        ResourceFactory::new(
            config,
            NativePorts {
                config,
                resolvers,
                clock_storage: Some(clock),
                store_storage: Some(store),
                listener_storage: Some(listener),
                capture_storage: Some(capture),
                #[cfg(any(feature = "host-libsbc", feature = "target-libsbc"))]
                encoder_storage: Some(encoder),
                #[cfg(any(feature = "host-libsbc", feature = "target-libsbc"))]
                interleaved: Some(interleaved),
                clock: None,
                store: None,
                listener: None,
                capture: None,
                #[cfg(any(feature = "host-libsbc", feature = "target-libsbc"))]
                encoder: None,
                store_lent: false,
            },
        )
    }
    /// Bind a pending capture budget after the owned clock has been observed.
    /// No capture resource may have been acquired or handed off yet. This changes
    /// neither format evidence nor the optional resource set, and never restarts.
    pub fn set_capture_budget_before_start(
        &mut self,
        budget: bt_capture::Budget,
        now_us: u64,
    ) -> Result<(), ConfigError> {
        if self.report().phase != FactoryPhase::Starting
            || !self.report().acquired[ResourceKind::Clock as usize]
            || self.report().acquired[ResourceKind::Capture as usize]
            || self.report().cleanup[ResourceKind::Capture as usize].is_some()
            || budget.max_reads == 0
            || budget.deadline_us <= now_us
        {
            return Err(ConfigError::CaptureBudget);
        }
        let mut next = self.config;
        let Some(capture) = next.capture.as_mut() else {
            return Err(ConfigError::ModeResources);
        };
        capture.budget = budget;
        next.validate()?;
        self.config = next;
        self.ports.config = next;
        Ok(())
    }
    pub fn clock_mut(&mut self) -> Option<&mut NativeClock<'storage>> {
        self.ports.clock.as_mut()
    }
    pub fn store_mut(&mut self) -> Option<&mut NativeStore<'storage>> {
        self.ports.store.as_mut()
    }
    pub fn listener_mut(&mut self) -> Option<&mut NativeListener<'storage>> {
        self.ports.listener.as_mut()
    }
    pub fn parts_mut(&mut self) -> ResourceParts<'_, 'storage> {
        ResourceParts {
            clock: self.ports.clock.as_mut(),
            store: self.ports.store.as_mut(),
            listener: self.ports.listener.as_mut(),
            capture: self.ports.capture.as_mut(),
            #[cfg(any(feature = "host-libsbc", feature = "target-libsbc"))]
            encoder: self.ports.encoder.as_mut(),
        }
    }
    pub fn now_us(&mut self) -> Result<u64, ResourceError> {
        self.ports
            .clock
            .as_mut()
            .ok_or(ResourceError::local(-1102))?
            .now_us()
            .map_err(clock_error)
    }
    pub fn wait_slice(
        &mut self,
        deadline: u64,
        cancelled: bool,
    ) -> Result<WaitOutcome, ResourceError> {
        self.ports
            .clock
            .as_mut()
            .ok_or(ResourceError::local(-1102))?
            .wait_slice(deadline, cancelled)
            .map_err(clock_error)
    }
    /// Transfer to an external StoreSlot. The factory's cleanup obligation stays
    /// pending; destroy the application's lease and restore before Store cleanup.
    pub fn take_store(&mut self) -> Option<NativeStore<'storage>> {
        let store = self.ports.store.take()?;
        self.ports.store_lent = true;
        Some(store)
    }
    pub fn restore_store(
        &mut self,
        store: NativeStore<'storage>,
    ) -> Result<(), NativeStore<'storage>> {
        if !self.ports.store_lent
            || self.ports.store.is_some()
            || !self.report().acquired[ResourceKind::Store as usize]
            || matches!(self.report().phase, FactoryPhase::Closed)
        {
            return Err(store);
        }
        self.ports.store = Some(store);
        self.ports.store_lent = false;
        Ok(())
    }
}
impl ResourcePorts for NativePorts<'_, '_> {
    fn cleanup_deferred(&self, resource: ResourceKind) -> bool {
        resource == ResourceKind::Store && self.store_lent
    }
    fn acquire(&mut self, resource: ResourceKind, now: u64) -> Result<(), ResourceError> {
        let invalid = || ResourceError::local(-1001);
        match resource {
            ResourceKind::Clock => {
                let storage = self.clock_storage.take().ok_or_else(invalid)?;
                // SAFETY: NativeFactory::create binds the reviewed resolver and
                // export lifetimes; storage is exclusively borrowed and stable.
                self.clock = Some(
                    unsafe { ClockWait::new(storage, self.resolvers.current) }
                        .map_err(clock_error)?,
                );
            }
            ResourceKind::Store => {
                self.store = Some(
                    FileStore::open(
                        self.store_storage.take().ok_or_else(invalid)?,
                        self.config.store_directory,
                    )
                    .map_err(store_error)?,
                );
            }
            ResourceKind::Listener => {
                self.listener = Some(
                    Listener::bind(
                        self.listener_storage.take().ok_or_else(invalid)?,
                        self.config.bind,
                    )
                    .map_err(listener_error)?,
                );
            }
            ResourceKind::Capture => {
                let config = self.config.capture.ok_or_else(invalid)?;
                // SAFETY: explicit factory constructor contract covers the
                // reviewed native export signatures and the stable storage borrow.
                let backend = unsafe {
                    Avcap2Backend::new(
                        self.capture_storage.take().ok_or_else(invalid)?,
                        self.resolvers.capture,
                    )
                }
                .map_err(|d| {
                    capture_error(OwnerFailure {
                        error: bt_capture::CaptureError::Backend(d),
                        cleanup: bt_capture::Cleanup::default(),
                    })
                })?;
                let mut capture =
                    CaptureOwner::open(backend, config.budget, now).map_err(capture_error)?;
                capture.start().map_err(capture_error)?;
                self.capture = Some(capture);
            }
            ResourceKind::Encoder => {
                #[cfg(any(feature = "host-libsbc", feature = "target-libsbc"))]
                {
                    let config = self.config.encoder.ok_or_else(invalid)?;
                    let scratch = self.interleaved.take().ok_or_else(invalid)?;
                    if scratch.len() < config.parameters.pcm_samples() {
                        return Err(ResourceError::local(-4));
                    }
                    let encoder = Encoder::new(
                        self.encoder_storage.take().ok_or_else(invalid)?,
                        config.parameters,
                        config.negotiated_bitpool,
                    )
                    .map_err(codec_error)?;
                    self.encoder = Some(SbcEncoder::new(encoder, scratch).map_err(codec_error)?);
                }
                #[cfg(not(any(feature = "host-libsbc", feature = "target-libsbc")))]
                return Err(ResourceError::local(-1));
            }
        }
        Ok(())
    }
    fn cleanup(&mut self, resource: ResourceKind) -> Result<(), ResourceError> {
        match resource {
            ResourceKind::Clock => {
                if let Some(mut clock) = self.clock.take() {
                    clock.close().map_err(clock_error)?;
                }
            }
            ResourceKind::Store => {
                if self.store_lent {
                    return Err(ResourceError {
                        flags: 1,
                        ..ResourceError::local(-1010)
                    });
                }
                if let Some(store) = self.store.take() {
                    store.close().map_err(|f| {
                        let mut error = store_error(f.diagnostic);
                        if f.returned_status != 0 {
                            error.code = f.returned_status;
                        }
                        error
                    })?;
                }
            }
            ResourceKind::Listener => {
                if let Some(listener) = self.listener.take() {
                    listener.close().map_err(listener_error)?;
                }
            }
            ResourceKind::Capture => {
                if let Some(mut capture) = self.capture.take() {
                    let cleanup = capture.close();
                    if cleanup.uncertain() {
                        return Err(capture_error(OwnerFailure {
                            error: bt_capture::CaptureError::InvalidState,
                            cleanup,
                        }));
                    }
                }
            }
            ResourceKind::Encoder =>
            {
                #[cfg(any(feature = "host-libsbc", feature = "target-libsbc"))]
                if let Some(encoder) = self.encoder.take() {
                    encoder.finish().map_err(codec_error)?;
                }
            }
        }
        Ok(())
    }
}
