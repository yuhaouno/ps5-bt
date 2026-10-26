// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(feature = "host-candidate")]
use bt_audio::BlockEncoder;
use bt_capture::{Budget, Format, ReadKind, ResolverOps};
use bt_core::{
    SbcAllocation, SbcBitpoolRange, SbcBlocks, SbcChannelMode, SbcParameters, SbcSampleRate,
    SbcSubbands,
};
use bt_listener::BindConfig;
use core::ffi::{c_char, c_void};
use ps5_app::*;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
unsafe extern "C" {
    fn ps5_scheduler_test_reset();
    fn ps5_scheduler_test_lookup_failure(which: u32, raw: i64, null_symbol: u32);
    fn ps5_scheduler_test_lookup(name: *const c_char, out: *mut *mut c_void) -> i64;
    fn ps5_capture_test_reset();
    fn ps5_capture_test_fail(stage: u32, raw: i32);
    fn ps5_capture_test_calls(stage: u32) -> u32;
    fn ps5_capture_test_current(name: *const c_char, out: *mut *mut c_void) -> i64;
    fn ps5_capture_test_module(module: i32, name: *const c_char, out: *mut *mut c_void) -> i64;
}
fn resolvers() -> Resolvers {
    Resolvers {
        current: ps5_scheduler_test_lookup,
        capture: ResolverOps {
            abi_version: bt_capture::ABI_VERSION,
            struct_bytes: core::mem::size_of::<ResolverOps>() as u32,
            lookup_current: ps5_capture_test_current,
            lookup_module: ps5_capture_test_module,
        },
    }
}
fn directory() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(1);
    let path = std::env::temp_dir().join(format!(
        "ps5-app-factory-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&path).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    fs::canonicalize(path).unwrap()
}
fn bind() -> BindConfig {
    BindConfig {
        address: [127, 0, 0, 1],
        port: 0,
    }
}
fn acquire(factory: &mut NativeFactory<'_, '_>) {
    factory.acquire(ResourceKind::Clock, 0).unwrap();
    let now = factory.now_us().unwrap();
    for resource in [
        ResourceKind::Store,
        ResourceKind::Listener,
        ResourceKind::Capture,
        ResourceKind::Encoder,
    ] {
        factory.acquire(resource, now).unwrap();
    }
}
#[test]
fn actual_native_factory_control_store_handoff_generated_sbc_and_capture_observation() {
    // SAFETY: this single test owns both non-reentrant synthetic C export
    // fixtures; the actual store/listener use only a temporary private directory
    // and loopback. Resolver signatures match reviewed APIs and never touch PS5.
    unsafe {
        ps5_scheduler_test_reset();
        ps5_capture_test_reset();
        let path = directory();
        let bytes = path.to_str().unwrap().as_bytes();
        let mut storage = NativeStorage::new();
        let mut factory = NativeFactory::create(
            &mut storage,
            Config::control_only(bytes, bind()),
            resolvers(),
        )
        .unwrap();
        acquire(&mut factory);
        assert_eq!(factory.report().phase, FactoryPhase::Ready);
        assert_eq!(factory.report().acquired, [true, true, true, false, false]);
        let port = factory.listener_mut().unwrap().local_port();
        assert_ne!(port, 0);
        assert!(factory.listener_mut().unwrap().accept().unwrap().is_none());
        let store = factory.take_store().unwrap();
        assert!(factory.store_mut().is_none());
        assert!(factory.cleanup(ResourceKind::Listener).is_ok());
        assert!(factory.cleanup(ResourceKind::Store).is_err());
        assert!(factory.report().acquired[1]);
        assert!(factory.report().cleanup[1].is_none());
        assert!(factory.restore_store(store).is_ok());
        assert!(!factory.shutdown().cleanup_uncertain());
        factory.shutdown();
        drop(factory);
        // Native import failure and real store/listener setup failures preserve
        // the primary error and release every earlier concrete resource.
        ps5_scheduler_test_reset();
        ps5_scheduler_test_lookup_failure(2, -64, 1);
        let mut storage = NativeStorage::new();
        let mut failed = NativeFactory::create(
            &mut storage,
            Config::control_only(bytes, bind()),
            resolvers(),
        )
        .unwrap();
        let failure = failed.acquire(ResourceKind::Clock, 0).unwrap_err();
        assert_eq!(failure.error.raw_lookup, -64);
        assert_eq!(failure.report.acquired, [false; 5]);
        drop(failed);
        ps5_scheduler_test_reset();
        let absent = path.join("absent");
        let mut storage = NativeStorage::new();
        let mut failed = NativeFactory::create(
            &mut storage,
            Config::control_only(absent.to_str().unwrap().as_bytes(), bind()),
            resolvers(),
        )
        .unwrap();
        failed.acquire(ResourceKind::Clock, 0).unwrap();
        let failure = failed.acquire(ResourceKind::Store, 0).unwrap_err();
        assert_ne!(failure.error.raw_status, 0);
        assert_eq!(failure.report.acquired, [false; 5]);
        assert_eq!(failure.report.cleanup[0], Some(Ok(())));
        drop(failed);
        ps5_scheduler_test_reset();
        let occupied = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let mut storage = NativeStorage::new();
        let mut failed = NativeFactory::create(
            &mut storage,
            Config::control_only(
                bytes,
                BindConfig {
                    address: [127, 0, 0, 1],
                    port: occupied.local_addr().unwrap().port(),
                },
            ),
            resolvers(),
        )
        .unwrap();
        failed.acquire(ResourceKind::Clock, 0).unwrap();
        failed.acquire(ResourceKind::Store, 0).unwrap();
        let failure = failed.acquire(ResourceKind::Listener, 0).unwrap_err();
        assert_ne!(failure.error.raw_status, 0);
        assert_eq!(failure.report.acquired, [false; 5]);
        assert!(!failure.report.cleanup_uncertain());
        drop(failed);
        drop(occupied);
        // A fully closed factory releases the actual protected store lock.
        ps5_scheduler_test_reset();
        let mut storage = NativeStorage::new();
        let mut config = Config::control_only(bytes, bind());
        config.mode = Mode::GeneratedSbc;
        config.encoder = Some(EncoderConfig {
            parameters: SbcParameters::new(
                SbcSampleRate::Hz48000,
                SbcChannelMode::JointStereo,
                SbcBlocks::Sixteen,
                SbcSubbands::Eight,
                SbcAllocation::Loudness,
                32,
            )
            .unwrap(),
            negotiated_bitpool: SbcBitpoolRange::new(2, 53).unwrap(),
        });
        let mut factory = NativeFactory::create(&mut storage, config, resolvers()).unwrap();
        acquire(&mut factory);
        let mut parts = factory.parts_mut();
        assert!(parts.capture.is_none());
        let encoder = parts.encoder.as_mut().unwrap();
        let mut out = [0; 128];
        assert_eq!(encoder.encode_stereo(&[[0; 2]; 128], &mut out), Ok(77));
        assert!(!factory.shutdown().cleanup_uncertain());
        drop(factory);
        ps5_scheduler_test_reset();
        ps5_capture_test_reset();
        let mut storage = NativeStorage::new();
        let mut config = Config::control_only(bytes, bind());
        config.mode = Mode::CaptureObserve;
        config.capture = Some(CaptureConfig {
            budget: Budget {
                max_reads: 2,
                deadline_us: 100,
            },
            format: Format::UNKNOWN,
        });
        let mut factory = NativeFactory::create(&mut storage, config, resolvers()).unwrap();
        let budget = Budget {
            max_reads: 2,
            deadline_us: 100,
        };
        assert_eq!(
            factory.set_capture_budget_before_start(budget, 0),
            Err(ConfigError::CaptureBudget)
        );
        factory.acquire(ResourceKind::Clock, 0).unwrap();
        assert_eq!(
            factory.set_capture_budget_before_start(
                Budget {
                    max_reads: 0,
                    ..budget
                },
                0
            ),
            Err(ConfigError::CaptureBudget)
        );
        assert_eq!(
            factory.set_capture_budget_before_start(budget, 100),
            Err(ConfigError::CaptureBudget)
        );
        factory.set_capture_budget_before_start(budget, 0).unwrap();
        assert_eq!(factory.config().capture.unwrap().budget, budget);
        acquire(&mut factory);
        assert_eq!(
            factory.set_capture_budget_before_start(
                Budget {
                    max_reads: 20,
                    deadline_us: 200
                },
                1
            ),
            Err(ConfigError::CaptureBudget)
        );
        let parts = factory.parts_mut();
        assert!(parts.encoder.is_none());
        let read = parts.capture.unwrap().read_once(1, &mut [0; 16]).unwrap();
        assert_eq!(read.kind, ReadKind::Data);
        assert_eq!(read.bytes, 8);
        assert_eq!(read.native_time, None);
        assert!(!factory.shutdown().cleanup_uncertain());
        drop(factory);
        for phase in [5, 6, 7] {
            assert_eq!(ps5_capture_test_calls(phase), 1);
        }
        // A real C capture Start failure must close/terminate once, then unwind
        // the already opened actual listener/store and cached clock.
        ps5_scheduler_test_reset();
        ps5_capture_test_reset();
        ps5_capture_test_fail(3, -44);
        let mut storage = NativeStorage::new();
        let mut factory = NativeFactory::create(&mut storage, config, resolvers()).unwrap();
        for resource in [
            ResourceKind::Clock,
            ResourceKind::Store,
            ResourceKind::Listener,
        ] {
            factory.acquire(resource, 0).unwrap();
        }
        let failure = factory.acquire(ResourceKind::Capture, 0).unwrap_err();
        assert_eq!(failure.error.raw_status, -44);
        assert_eq!(failure.report.acquired, [false; 5]);
        assert!(!failure.report.cleanup_uncertain());
        drop(factory);
        assert_eq!(ps5_capture_test_calls(5), 0);
        assert_eq!(ps5_capture_test_calls(6), 1);
        assert_eq!(ps5_capture_test_calls(7), 1);
        // Cleanup uncertainty preserves the actual Stop error while still
        // closing all remaining concrete resources once.
        ps5_scheduler_test_reset();
        ps5_capture_test_reset();
        let mut storage = NativeStorage::new();
        let mut factory = NativeFactory::create(&mut storage, config, resolvers()).unwrap();
        acquire(&mut factory);
        ps5_capture_test_fail(5, -66);
        let report = factory.shutdown();
        let error = report.cleanup[ResourceKind::Capture as usize]
            .unwrap()
            .unwrap_err();
        assert_eq!(error.code, -5);
        assert_eq!(error.raw_status, -66);
        assert_eq!(error.cleanup[0], -66);
        assert!(report.cleanup_uncertain());
        assert_eq!(report.acquired, [false; 5]);
        drop(factory);
        for phase in [5, 6, 7] {
            assert_eq!(ps5_capture_test_calls(phase), 1);
        }
        fs::remove_dir_all(path).unwrap();
    }
}
