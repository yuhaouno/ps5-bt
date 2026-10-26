// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(feature = "host-candidate")]
use bt_capture::{Budget, Format, ResolverOps};
use bt_control::pump::SocketIo;
use bt_core::recovery::{Action, Failure, Policy, Recovery, Requirement};
use bt_device::{BOND_RECORD_BYTES, BondStore};
use bt_listener::BindConfig;
use core::ffi::{c_char, c_void};
use ps5_app::{rebuild::*, *};
use std::{
    cell::Cell,
    fs,
    net::{Ipv4Addr, TcpListener, TcpStream},
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
unsafe extern "C" {
    fn ps5_scheduler_test_reset();
    fn ps5_scheduler_test_clock(raw: i32, seconds: i64, nanoseconds: i64);
    fn ps5_scheduler_test_count(which: u32) -> u32;
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
fn clock(now: u64) {
    // SAFETY: this single integration test owns the serialized synthetic clock
    // fixture. It changes only known test exports, never native PS5 state.
    unsafe {
        ps5_scheduler_test_clock(
            0,
            (now / 1_000_000) as i64,
            ((now % 1_000_000) * 1000) as i64,
        )
    };
}
fn reset() {
    // SAFETY: one test owns both synthetic C export fixtures. Reset represents
    // a fresh host test process; no production uncertainty reset is introduced.
    unsafe {
        ps5_scheduler_test_reset();
        ps5_capture_test_reset();
    }
}
fn directory() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(1);
    let path = std::env::temp_dir().join(format!(
        "ps5-native-rebuild-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&path).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    fs::canonicalize(path).unwrap()
}
fn config(path: &[u8]) -> Config<'_> {
    Config::control_only(
        path,
        BindConfig {
            address: [127, 0, 0, 1],
            port: 0,
        },
    )
}
fn recovery() -> Recovery {
    Recovery::new(Policy {
        max_attempts: 2,
        episode_timeout_us: 100,
        initial_backoff_us: 5,
        max_backoff_us: 10,
        stable_success_us: 20,
    })
    .unwrap()
}
fn budget() -> NativeAttemptBudget {
    NativeAttemptBudget {
        deadline_us: 1000,
        max_operations: 8,
    }
}
fn initial<'s, 'c>(storage: &'s mut NativeStorage, config: Config<'c>) -> NativeFactory<'s, 'c> {
    // SAFETY: caller keeps stable backing outside this factory; only reviewed
    // static synthetic clock/capture exports plus actual host POSIX are selected.
    let mut factory = unsafe { NativeFactory::create(storage, config, resolvers()) }.unwrap();
    for resource in [
        ResourceKind::Clock,
        ResourceKind::Store,
        ResourceKind::Listener,
        ResourceKind::Capture,
        ResourceKind::Encoder,
    ] {
        factory.acquire(resource, 0).unwrap();
    }
    factory
}
fn use_local_resources(factory: &mut NativeFactory<'_, '_>) -> Result<u16, ResourceError> {
    assert_eq!(
        factory
            .store_mut()
            .unwrap()
            .load(bt_core_peer(), &mut [0; BOND_RECORD_BYTES]),
        Ok(None)
    );
    let listener = factory.listener_mut().unwrap();
    let port = listener.local_port();
    assert_ne!(port, 0);
    let client = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
    let mut confirmed = false;
    for _ in 0..1000 {
        if let Some(mut accepted) = listener.accept().unwrap() {
            assert_eq!(accepted.close(), Ok(()));
            confirmed = true;
            break;
        }
        std::thread::yield_now();
    }
    assert!(confirmed, "bounded host accept probe exhausted");
    drop(client);
    Ok(port)
}
fn bt_core_peer() -> bt_link::Peer {
    bt_link::Peer::from_hci_bytes([1, 2, 3, 4, 5, 6])
}
#[test]
fn concrete_native_rebuild_reuses_one_storage_and_preserves_fault_episode() {
    reset();
    let path = directory();
    let bytes = path.to_str().unwrap().as_bytes();
    let mut storage = NativeStorage::new();
    let mut policy = recovery();
    let old = initial(&mut storage, config(bytes));
    let mut rebuild = NativeRebuild::new(&mut policy, 1).unwrap();
    assert!(
        !rebuild
            .retire(old, 0, Failure::Retryable, None)
            .unwrap()
            .cleanup_uncertain()
    );
    assert!(matches!(rebuild.poll(0).unwrap(), Action::Attempt(_)));
    // SAFETY: same reviewed callbacks/host native contracts as initial; the old
    // factory was consumed. HRTB scope prevents any native borrow escaping.
    let report = unsafe {
        rebuild.with_attempt(
            &mut storage,
            config(bytes),
            resolvers(),
            0,
            budget(),
            || false,
            |live| {
                let port = live.operate(use_local_resources).unwrap();
                live.confirm_working().unwrap();
                clock(10);
                assert!(matches!(live.poll().unwrap(), Action::StabilizingUntil(20)));
                clock(20);
                assert_eq!(live.poll().unwrap(), Action::Healthy);
                NativeSessionExit::Fault {
                    value: port,
                    failure: Failure::Retryable,
                }
            },
        )
    }
    .unwrap();
    assert!(report.ready);
    assert_eq!(report.local_epoch, 2);
    assert!(report.value.unwrap() > 0);
    assert_eq!(report.before_cleanup.action, Action::Healthy);
    assert_eq!(report.before_cleanup.attempts, 0);
    assert_eq!(report.after_cleanup.attempts, 0);
    assert_eq!(report.after_cleanup.episode_deadline_us, Some(120));
    assert!(!report.cleanup_uncertain);
    assert!(report.cleanup.acquired.iter().all(|active| !*active));
    assert!(matches!(rebuild.poll(20).unwrap(), Action::Attempt(_)));
    clock(20);
    // SAFETY: the prior scope has fully closed and released the same storage.
    let report = unsafe {
        rebuild.with_attempt(
            &mut storage,
            config(bytes),
            resolvers(),
            20,
            budget(),
            || false,
            |live| {
                live.operate(use_local_resources).unwrap();
                NativeSessionExit::Fault {
                    value: (),
                    failure: Failure::Retryable,
                }
            },
        )
    }
    .unwrap();
    assert_eq!(report.local_epoch, 3);
    assert_eq!(report.after_cleanup.attempts, 1);
    assert_eq!(report.after_cleanup.episode_deadline_us, Some(120));
    assert!(matches!(rebuild.poll(20).unwrap(), Action::WaitUntil(25)));
    // Stop during backoff never constructs/opens another native factory.
    // SAFETY: read-only synthetic counter query; test owns fixture.
    let lookups = unsafe { ps5_scheduler_test_count(0) };
    // SAFETY: exports remain valid; stop rejects before any storage/native access.
    let stopped = unsafe {
        rebuild.with_attempt(
            &mut storage,
            config(bytes),
            resolvers(),
            21,
            budget(),
            || true,
            |_| NativeSessionExit::Stop(()),
        )
    };
    assert!(matches!(stopped, Err(NativeRebuildError::Stopped)));
    assert_eq!(rebuild.snapshot().recovery.action, Action::Stopped);
    // SAFETY: read-only fixture counter.
    assert_eq!(unsafe { ps5_scheduler_test_count(0) }, lookups);
    // Partially failed actual native bind unwinds the actual store lock; retry
    // uses the SAME storage and original attempt/deadline, never a fresh policy.
    reset();
    let mut policy = recovery();
    let old = initial(&mut storage, config(bytes));
    let mut rebuild = NativeRebuild::new(&mut policy, 10).unwrap();
    rebuild.retire(old, 0, Failure::Retryable, None).unwrap();
    let occupied = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let mut unavailable = config(bytes);
    unavailable.bind.port = occupied.local_addr().unwrap().port();
    // SAFETY: scope owns all resource borrows and uses actual host POSIX errors.
    let failed: NativeAttemptReport<()> = unsafe {
        rebuild.with_attempt(
            &mut storage,
            unavailable,
            resolvers(),
            0,
            budget(),
            || false,
            |_| panic!("failed native bind must not run body"),
        )
    }
    .unwrap();
    assert!(!failed.ready);
    assert!(matches!(failed.error, Some(NativeRebuildError::Native(_))));
    assert_eq!(failed.cleanup.primary.unwrap().0, ResourceKind::Listener);
    assert!(
        failed.cleanup.cleanup[ResourceKind::Store as usize]
            .unwrap()
            .is_ok()
    );
    assert_eq!(failed.after_cleanup.attempts, 1);
    assert_eq!(failed.after_cleanup.episode_deadline_us, Some(100));
    assert!(matches!(rebuild.poll(0).unwrap(), Action::WaitUntil(5)));
    drop(occupied);
    clock(5);
    // SAFETY: prior failed acquisition fully unwound and released backing.
    let retried = unsafe {
        rebuild.with_attempt(
            &mut storage,
            config(bytes),
            resolvers(),
            5,
            budget(),
            || false,
            |live| {
                live.operate(use_local_resources).unwrap();
                NativeSessionExit::Fault {
                    value: (),
                    failure: Failure::Retryable,
                }
            },
        )
    }
    .unwrap();
    assert_eq!(retried.local_epoch, 12);
    assert_eq!(retried.after_cleanup.attempts, 2);
    assert_eq!(retried.after_cleanup.episode_deadline_us, Some(100));
    assert!(matches!(rebuild.poll(5).unwrap(), Action::Exhausted(_)));
    // Stop immediately after actual Store acquisition closes Store/Clock before
    // listener bind and never executes a user operation.
    reset();
    let mut policy = recovery();
    let old = initial(&mut storage, config(bytes));
    let mut rebuild = NativeRebuild::new(&mut policy, 20).unwrap();
    rebuild.retire(old, 0, Failure::Retryable, None).unwrap();
    let calls = Cell::new(0);
    // SAFETY: host callbacks and stable storage as above; callback is finite.
    let stopped: NativeAttemptReport<()> = unsafe {
        rebuild.with_attempt(
            &mut storage,
            config(bytes),
            resolvers(),
            0,
            budget(),
            || {
                calls.set(calls.get() + 1);
                calls.get() == 5
            },
            |_| panic!("stop before ready"),
        )
    }
    .unwrap();
    assert!(!stopped.ready);
    assert_eq!(stopped.after_cleanup.action, Action::Stopped);
    assert!(
        stopped.cleanup.cleanup[ResourceKind::Store as usize]
            .unwrap()
            .is_ok()
    );
    assert!(stopped.cleanup.cleanup[ResourceKind::Listener as usize].is_none());
    assert!(!stopped.cleanup_uncertain);
    // A successful reopen alone cannot reset the policy. Both the local work
    // budget and the persistent episode deadline terminate live native owners.
    for limit in 0..2 {
        reset();
        let mut policy = recovery();
        let old = initial(&mut storage, config(bytes));
        let mut rebuild = NativeRebuild::new(&mut policy, 25).unwrap();
        rebuild.retire(old, 0, Failure::Retryable, None).unwrap();
        let limits = NativeAttemptBudget {
            deadline_us: 1000,
            max_operations: if limit == 0 { 1 } else { 8 },
        };
        // SAFETY: actual scoped native owners and the same reviewed host exports.
        let report = unsafe {
            rebuild.with_attempt(
                &mut storage,
                config(bytes),
                resolvers(),
                0,
                limits,
                || false,
                |live| {
                    live.operate(use_local_resources).unwrap();
                    if limit == 0 {
                        assert_eq!(live.poll(), Err(NativeRebuildError::OperationBudget));
                    } else {
                        clock(100);
                        assert!(matches!(
                            live.poll(),
                            Err(NativeRebuildError::NotReady(Action::Exhausted(_)))
                        ));
                    }
                    NativeSessionExit::Fault {
                        value: (),
                        failure: Failure::Retryable,
                    }
                },
            )
        }
        .unwrap();
        assert!(report.error.is_some());
        assert_eq!(report.after_cleanup.attempts, 1);
        assert!(!report.cleanup_uncertain);
        assert_eq!(report.cleanup.acquired, [false; 5]);
    }
    // Host unwinding cannot leave a closed resource stabilizing in the policy.
    // An incomplete scope poisons further use and stops BEFORE a later poll can
    // grant elapsed stable time to resources already dropped during unwind.
    reset();
    let mut policy = recovery();
    let old = initial(&mut storage, config(bytes));
    let mut rebuild = NativeRebuild::new(&mut policy, 27).unwrap();
    rebuild.retire(old, 0, Failure::Retryable, None).unwrap();
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: host-only unwind test owns reviewed exports and stable storage;
        // actual NativeFactory Drop consumes all resources on this synthetic panic.
        let _: Result<NativeAttemptReport<()>, _> = unsafe {
            rebuild.with_attempt(
                &mut storage,
                config(bytes),
                resolvers(),
                0,
                budget(),
                || false,
                |live| {
                    live.operate(use_local_resources).unwrap();
                    live.confirm_working().unwrap();
                    panic!("bounded host unwind fixture")
                },
            )
        };
    }));
    assert!(panicked.is_err());
    assert!(rebuild.snapshot().scope_incomplete);
    assert_eq!(rebuild.poll(20), Err(NativeRebuildError::CleanupUnknown));
    assert_eq!(rebuild.snapshot().recovery.action, Action::Stopped);
    assert_eq!(rebuild.snapshot().recovery.attempts, 1);
    // Earlier client-close uncertainty survives a fully clean final factory
    // report and permanently blocks automatic native reopen in this coordinator.
    reset();
    let mut policy = recovery();
    let old = initial(&mut storage, config(bytes));
    let mut rebuild = NativeRebuild::new(&mut policy, 28).unwrap();
    let prior = ResourceError {
        flags: 1,
        cleanup: [5, 0, 0],
        ..ResourceError::local(-5)
    };
    let report = rebuild
        .retire(old, 0, Failure::Retryable, Some(prior))
        .unwrap();
    assert!(!report.cleanup_uncertain());
    assert!(rebuild.snapshot().cleanup_uncertain);
    assert_eq!(rebuild.snapshot().prior_cleanup, Some(prior));
    assert_eq!(rebuild.poll(1), Err(NativeRebuildError::CleanupUnknown));
    rebuild.stop();
    // A real C capture cleanup failure plus successful later store/listener
    // cleanup is still uncertain; no elapsed time or new local epoch repairs it.
    reset();
    let mut observed = config(bytes);
    observed.mode = Mode::CaptureObserve;
    observed.capture = Some(CaptureConfig {
        budget: Budget {
            max_reads: 2,
            deadline_us: 1000,
        },
        format: Format::UNKNOWN,
    });
    let old = initial(&mut storage, observed);
    // SAFETY: synthetic C fixture injects an exact Stop failure only.
    unsafe { ps5_capture_test_fail(5, -66) };
    let mut policy = recovery();
    let mut rebuild = NativeRebuild::new(&mut policy, 30).unwrap();
    let retired = rebuild.retire(old, 0, Failure::Retryable, None).unwrap();
    assert!(retired.cleanup_uncertain());
    assert!(matches!(
        rebuild.snapshot().recovery.action,
        Action::AwaitSafety {
            requirement: Requirement::Reconciliation,
            ..
        }
    ));
    assert_eq!(rebuild.poll(0), Err(NativeRebuildError::CleanupUnknown));
    // SAFETY: fixture counter query, no native operation.
    let lookups = unsafe { ps5_scheduler_test_count(0) };
    // SAFETY: coordinator rejects uncertainty before creating any new factory.
    let blocked = unsafe {
        rebuild.with_attempt(
            &mut storage,
            config(bytes),
            resolvers(),
            1,
            budget(),
            || false,
            |_| NativeSessionExit::Stop(()),
        )
    };
    assert!(matches!(blocked, Err(NativeRebuildError::CleanupUnknown)));
    // SAFETY: read-only fixture counters establish consume-once cleanup/no reopen.
    unsafe {
        assert_eq!(ps5_scheduler_test_count(0), lookups);
        for phase in [5, 6, 7] {
            assert_eq!(ps5_capture_test_calls(phase), 1);
        }
    }
    rebuild.stop();
    fs::remove_dir_all(path).unwrap();
}
