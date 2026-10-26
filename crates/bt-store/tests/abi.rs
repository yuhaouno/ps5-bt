// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(all(feature = "external-posix", not(feature = "native-posix")))]
use bt_device::{BondStore, StoreError};
use bt_link::Peer;
use bt_store::{Diagnostic, FileStore, StoreStorage};
use core::ffi::c_void;
use std::sync::Mutex;

#[derive(Clone, Copy, Default)]
enum Behavior {
    #[default]
    Clean,
    Unwritten,
    Dirty,
    Contradict,
}
#[derive(Default)]
struct Fake {
    open: Behavior,
    operation: Behavior,
    close: Behavior,
    opened: usize,
    closed: usize,
    operations: usize,
    live: usize,
}
static FAKE: Mutex<Fake> = Mutex::new(Fake {
    open: Behavior::Clean,
    operation: Behavior::Clean,
    close: Behavior::Clean,
    opened: 0,
    closed: 0,
    operations: 0,
    live: 0,
});
fn reset(open: Behavior, operation: Behavior, close: Behavior) {
    *FAKE.lock().unwrap() = Fake {
        open,
        operation,
        close,
        ..Fake::default()
    };
}
fn write_report(report: *mut Diagnostic, behavior: Behavior, stage: u32) -> i32 {
    if !matches!(behavior, Behavior::Unwritten) {
        let value = Diagnostic {
            abi_version: if matches!(behavior, Behavior::Dirty) {
                99
            } else {
                1
            },
            stage,
            status: 0,
            raw_errno: 0,
            flags: 0,
            cleanup_errno: 0,
        };
        // SAFETY: the tested adapter supplies one valid uniquely writable report.
        unsafe {
            report.write(value);
        }
    }
    if matches!(behavior, Behavior::Contradict) {
        -2
    } else {
        0
    }
}
#[unsafe(no_mangle)]
extern "C" fn ps5_bt_store_abi_version() -> u32 {
    1
}
#[unsafe(no_mangle)]
extern "C" fn ps5_bt_store_storage_size() -> u32 {
    16
}
#[unsafe(no_mangle)]
extern "C" fn ps5_bt_store_storage_alignment() -> u32 {
    8
}
#[unsafe(no_mangle)]
extern "C" fn ps5_bt_store_open(
    _: *mut c_void,
    _: u32,
    _: *const u8,
    _: u32,
    r: *mut Diagnostic,
) -> i32 {
    let mut state = FAKE.lock().unwrap();
    state.opened += 1;
    state.live += 1;
    write_report(r, state.open, 2)
}
#[unsafe(no_mangle)]
extern "C" fn ps5_bt_store_save(
    _: *mut c_void,
    _: *const u8,
    _: *const u8,
    r: *mut Diagnostic,
) -> i32 {
    let mut state = FAKE.lock().unwrap();
    state.operations += 1;
    write_report(r, state.operation, 11)
}
#[unsafe(no_mangle)]
extern "C" fn ps5_bt_store_delete(_: *mut c_void, _: *const u8, r: *mut Diagnostic) -> i32 {
    let mut state = FAKE.lock().unwrap();
    state.operations += 1;
    write_report(r, state.operation, 11)
}
#[unsafe(no_mangle)]
extern "C" fn ps5_bt_store_load(
    _: *mut c_void,
    _: *const u8,
    out: *mut u8,
    length: *mut u32,
    r: *mut Diagnostic,
) -> i32 {
    let mut state = FAKE.lock().unwrap();
    state.operations += 1;
    // SAFETY: the tested owner supplies an exact32-byte output and u32 count.
    unsafe {
        core::ptr::write_bytes(out, 0x44, 32);
        length.write(32);
    }
    write_report(r, state.operation, 5)
}
#[unsafe(no_mangle)]
extern "C" fn ps5_bt_store_close(_: *mut c_void, r: *mut Diagnostic) -> i32 {
    let mut state = FAKE.lock().unwrap();
    state.closed += 1;
    state.live -= 1;
    write_report(r, state.close, 13)
}

#[test]
fn malformed_abi_reports_never_leak_acquired_owner_reuse_old_success_or_double_close() {
    for open in [Behavior::Unwritten, Behavior::Dirty] {
        reset(open, Behavior::Clean, Behavior::Clean);
        let mut storage = StoreStorage::<64>::new();
        assert!(FileStore::open(&mut storage, b"/synthetic").is_err());
        let state = FAKE.lock().unwrap();
        assert_eq!((state.opened, state.closed, state.live), (1, 1, 0));
    }
    reset(Behavior::Dirty, Behavior::Clean, Behavior::Unwritten);
    let mut storage = StoreStorage::<64>::new();
    let report = match FileStore::open(&mut storage, b"/synthetic") {
        Ok(_) => panic!("dirty open"),
        Err(report) => report,
    };
    assert_eq!(report.abi_version, 99);
    assert_ne!(report.flags & 4, 0);
    assert_eq!(FAKE.lock().unwrap().closed, 1);

    for close in [Behavior::Unwritten, Behavior::Dirty, Behavior::Contradict] {
        reset(Behavior::Clean, Behavior::Clean, close);
        let mut storage = StoreStorage::<64>::new();
        let store = FileStore::open(&mut storage, b"/synthetic").unwrap();
        let error = store.close().unwrap_err();
        assert_ne!(error.diagnostic.flags & 4, 0);
        assert_eq!(
            error.returned_status,
            if matches!(close, Behavior::Contradict) {
                -2
            } else {
                0
            }
        );
        let state = FAKE.lock().unwrap();
        assert_eq!((state.closed, state.live), (1, 0));
    }
    for operation in [0, 1, 2] {
        reset(Behavior::Clean, Behavior::Clean, Behavior::Clean);
        let mut storage = StoreStorage::<64>::new();
        let mut store = FileStore::open(&mut storage, b"/synthetic").unwrap();
        let peer = Peer::from_hci_bytes([1, 2, 3, 4, 5, 6]);
        store.save_atomic(peer, &[0x44; 32]).unwrap(); // prior successful report
        FAKE.lock().unwrap().operation = Behavior::Unwritten;
        if operation == 0 {
            let mut output = [0xa5; 32];
            assert_eq!(store.load(peer, &mut output), Err(StoreError::Unavailable));
            assert_eq!(output, [0; 32]);
        } else if operation == 1 {
            assert_eq!(
                store.save_atomic(peer, &[0x44; 32]),
                Err(StoreError::DurabilityUnknown)
            );
        } else {
            assert_eq!(
                store.delete_atomic(peer),
                Err(StoreError::DurabilityUnknown)
            );
        }
        let calls = FAKE.lock().unwrap().operations;
        assert_eq!(store.delete_atomic(peer), Err(StoreError::Unavailable));
        assert_eq!(FAKE.lock().unwrap().operations, calls); // poisoned ABI never called again.
        store.close().unwrap();
        assert_eq!(FAKE.lock().unwrap().closed, 1);
    }
}
