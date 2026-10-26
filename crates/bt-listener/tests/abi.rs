// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(all(feature = "external-posix", not(feature = "native-posix")))]
use bt_control::pump::{IoError, SocketIo};
use bt_listener::{BindConfig, Diagnostic, Listener, ListenerStorage};
use std::sync::Mutex;
static GUARD: Mutex<()> = Mutex::new(());
#[derive(Default)]
struct State {
    mode: u8,
    closed: usize,
    client_closed: usize,
    read: usize,
    write: usize,
}
static STATE: Mutex<State> = Mutex::new(State {
    mode: 0,
    closed: 0,
    client_closed: 0,
    read: 0,
    write: 0,
});
fn reset(mode: u8) {
    *STATE.lock().unwrap() = State {
        mode,
        ..State::default()
    };
}
fn config() -> BindConfig {
    BindConfig {
        address: [127, 0, 0, 1],
        port: 0,
    }
}
unsafe fn set_report(pointer: *mut Diagnostic, status: i32, stage: u32) {
    // SAFETY: all mock calls originate in the wrapper with a valid report pointer.
    unsafe {
        pointer.write(Diagnostic {
            abi_version: 1,
            status,
            stage,
            raw_errno: if status != 0 { 35 } else { 0 },
            ..Diagnostic::default()
        });
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn ps5_bt_listener_abi_version() -> u32 {
    1
}
#[unsafe(no_mangle)]
pub extern "C" fn ps5_bt_listener_storage_size() -> u32 {
    32
}
#[unsafe(no_mangle)]
pub extern "C" fn ps5_bt_listener_storage_alignment() -> u32 {
    8
}
#[unsafe(no_mangle)]
unsafe extern "C" fn ps5_bt_listener_bind(
    _: *mut core::ffi::c_void,
    _: u32,
    _: *const u8,
    _: u16,
    port: *mut u16,
    r: *mut Diagnostic,
) -> i32 {
    let mode = STATE.lock().unwrap().mode;
    // SAFETY: caller provides writable primitive outputs/report for this call.
    unsafe {
        set_report(r, 0, 6);
        *port = 12345;
        if mode == 1 || mode == 2 {
            (*r).abi_version = 99;
        }
    }
    0
}
#[unsafe(no_mangle)]
unsafe extern "C" fn ps5_bt_listener_accept(
    _: *mut core::ffi::c_void,
    token: *mut u64,
    r: *mut Diagnostic,
) -> i32 {
    let mode = STATE.lock().unwrap().mode;
    // SAFETY: caller provides writable token/report for this call.
    unsafe {
        set_report(r, if mode == 4 { 1 } else { 0 }, 9);
        *token = if mode == 3 { 0 } else { 1 };
    }
    if mode == 4 { 1 } else { 0 }
}
#[unsafe(no_mangle)]
unsafe extern "C" fn ps5_bt_listener_read(
    _: *mut core::ffi::c_void,
    _: u64,
    output: *mut u8,
    capacity: u32,
    count: *mut u32,
    r: *mut Diagnostic,
) -> i32 {
    let mut state = STATE.lock().unwrap();
    state.read += 1;
    let mode = state.mode;
    // SAFETY: caller supplies capacity bytes and primitive count/report outputs.
    unsafe {
        set_report(r, if mode == 6 { 1 } else { 0 }, 10);
        core::ptr::write_bytes(output, 0xa5, capacity as usize);
        *count = if mode == 5 {
            capacity + 1
        } else if mode == 6 {
            1
        } else {
            0
        };
    }
    if mode == 6 { 1 } else { 0 }
}
#[unsafe(no_mangle)]
unsafe extern "C" fn ps5_bt_listener_write(
    _: *mut core::ffi::c_void,
    _: u64,
    _: *const u8,
    length: u32,
    count: *mut u32,
    r: *mut Diagnostic,
) -> i32 {
    STATE.lock().unwrap().write += 1;
    // SAFETY: caller supplies primitive count/report outputs; no data is retained.
    unsafe {
        set_report(r, 0, 11);
        *count = length.min(2);
    }
    0
}
#[unsafe(no_mangle)]
unsafe extern "C" fn ps5_bt_listener_client_close(
    _: *mut core::ffi::c_void,
    _: u64,
    r: *mut Diagnostic,
) -> i32 {
    let mut state = STATE.lock().unwrap();
    state.client_closed += 1;
    let mode = state.mode;
    // SAFETY: caller supplies a writable report and this mock retains no pointer.
    unsafe {
        set_report(r, if mode == 8 { -2 } else { 0 }, 12);
        if mode == 8 {
            (*r).cleanup_errno = 35;
            (*r).flags = 1;
        }
        if mode == 9 {
            (*r).abi_version = 99;
        }
    }
    if mode == 8 { -2 } else { 0 }
}
#[unsafe(no_mangle)]
unsafe extern "C" fn ps5_bt_listener_close(_: *mut core::ffi::c_void, r: *mut Diagnostic) -> i32 {
    let mut state = STATE.lock().unwrap();
    state.closed += 1;
    let mode = state.mode;
    // SAFETY: caller supplies a writable report and this consumes the mock context.
    unsafe {
        set_report(r, if mode == 2 { -2 } else { 0 }, 13);
        if mode == 2 {
            (*r).cleanup_errno = 35;
            (*r).flags = 1;
        }
    }
    if mode == 2 || mode == 7 { -2 } else { 0 }
}
#[test]
fn successful_bind_with_dirty_report_closes_context_once_before_releasing_storage() {
    let _guard = GUARD.lock().unwrap();
    for mode in [1, 2] {
        reset(mode);
        let mut storage = ListenerStorage::<128>::new();
        let r = match Listener::bind(&mut storage, config()) {
            Err(r) => r,
            Ok(_) => panic!("dirty bind"),
        };
        assert_eq!(r.status, -1000);
        assert_eq!(STATE.lock().unwrap().closed, 1);
        if mode == 2 {
            assert_eq!(r.flags & 1, 1);
            assert_eq!(r.cleanup_errno, 35);
        }
    }
}
#[test]
fn dirty_accept_closes_whole_listener_including_unknown_client_token() {
    let _guard = GUARD.lock().unwrap();
    for mode in [3, 4] {
        reset(mode);
        let mut storage = ListenerStorage::<128>::new();
        {
            let mut listener = Listener::bind(&mut storage, config()).unwrap();
            assert!(listener.accept().is_err());
            assert!(listener.accept().is_err());
        }
        assert_eq!(STATE.lock().unwrap().closed, 1);
        assert_eq!(STATE.lock().unwrap().client_closed, 0);
    }
}
#[test]
fn dirty_io_counts_clear_output_and_invalidate_client_before_any_retry() {
    let _guard = GUARD.lock().unwrap();
    for mode in [5, 6] {
        reset(mode);
        let mut storage = ListenerStorage::<128>::new();
        {
            let mut listener = Listener::bind(&mut storage, config()).unwrap();
            {
                let mut client = listener.accept().unwrap().unwrap();
                let mut output = [0xff; 8];
                assert_eq!(client.read(&mut output), Err(IoError::Failed(-1000)));
                assert_eq!(output, [0; 8]);
                assert!(matches!(client.read(&mut output), Err(IoError::Failed(_))));
                assert!(matches!(client.write(b"abc"), Err(IoError::Failed(_))));
                client.close().unwrap();
                client.close().unwrap();
            }
        }
        let s = STATE.lock().unwrap();
        assert_eq!((s.closed, s.client_closed, s.read, s.write), (1, 1, 1, 0));
    }
}
#[test]
fn close_return_cannot_be_masked_by_dirty_success_report() {
    let _guard = GUARD.lock().unwrap();
    reset(7);
    let mut storage = ListenerStorage::<128>::new();
    let listener = Listener::bind(&mut storage, config()).unwrap();
    let error = listener.close().unwrap_err();
    assert_eq!(error.status, -1000);
    assert_eq!(error.flags & 1, 1);
    assert_eq!(STATE.lock().unwrap().closed, 1);
}
#[test]
fn failed_or_dirty_client_close_is_sticky_and_prevents_accepting_replacement() {
    let _guard = GUARD.lock().unwrap();
    for mode in [8, 9] {
        reset(mode);
        let mut storage = ListenerStorage::<128>::new();
        {
            let mut listener = Listener::bind(&mut storage, config()).unwrap();
            {
                let mut client = listener.accept().unwrap().unwrap();
                let failure = client.close();
                assert!(failure.is_err());
                assert_eq!(client.close(), failure);
            }
            assert!(listener.accept().is_err());
        }
        let s = STATE.lock().unwrap();
        assert_eq!((s.closed, s.client_closed), (1, 1));
    }
}
