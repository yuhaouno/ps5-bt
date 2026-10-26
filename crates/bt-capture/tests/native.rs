// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(feature = "host-avcap2-tests")]
use bt_capture::*;
use core::ffi::{c_char, c_void};
unsafe extern "C" {
    fn ps5_capture_test_reset();
    fn ps5_capture_test_fail(stage: u32, raw: i32);
    fn ps5_capture_test_read(raw: i32);
    fn ps5_capture_test_calls(phase: u32) -> u32;
    fn ps5_capture_test_current(name: *const c_char, out: *mut *mut c_void) -> i64;
    fn ps5_capture_test_module(module: i32, name: *const c_char, out: *mut *mut c_void) -> i64;
}
fn ops() -> ResolverOps {
    ResolverOps {
        abi_version: ABI_VERSION,
        struct_bytes: core::mem::size_of::<ResolverOps>() as u32,
        lookup_current: ps5_capture_test_current,
        lookup_module: ps5_capture_test_module,
    }
}
#[test]
fn real_c_boundary_owner_retains_raw_status_and_cleans_every_phase_once() {
    // SAFETY: this single test owns the process-global synthetic C fixture and
    // non-reentrant C adapter. Callbacks provide reviewed signatures and no OS I/O.
    unsafe {
        assert_eq!(core::mem::size_of::<Diagnostic>(), 40);
        for phase in 1..=11u32 {
            if phase == 4 {
                continue;
            }
            ps5_capture_test_reset();
            ps5_capture_test_fail(phase, -44);
            let mut storage = CaptureStorage::<9216>::new();
            let backend = Avcap2Backend::new(&mut storage, ops()).unwrap();
            let result = CaptureOwner::open(
                backend,
                Budget {
                    max_reads: 2,
                    deadline_us: 100,
                },
                0,
            );
            match result {
                Err(failure) => {
                    assert!(phase == 1 || phase == 2 || phase >= 8);
                    let CaptureError::Backend(diagnostic) = failure.error else {
                        panic!("wrong error");
                    };
                    if phase == 10 {
                        assert_eq!(diagnostic.raw_lookup, 0x100000000 - 44);
                    } else if phase == 11 {
                        assert_eq!(diagnostic.raw_lookup, -44);
                    } else {
                        assert_eq!(diagnostic.raw_status, -44);
                    }
                    assert_eq!(ps5_capture_test_calls(6), 0);
                }
                Ok(mut owner) => {
                    let started = owner.start();
                    assert_eq!(started.is_err(), phase == 3);
                    let cleanup = owner.close();
                    assert_eq!(cleanup.uncertain(), (5..=7).contains(&phase));
                    owner.close();
                    drop(owner);
                    assert_eq!(ps5_capture_test_calls(5), if phase == 3 { 0 } else { 1 });
                    assert_eq!(ps5_capture_test_calls(6), 1);
                    assert_eq!(ps5_capture_test_calls(7), 1);
                }
            }
        }
        for (raw, kind) in [
            (8, ReadKind::Data),
            (0, ReadKind::Zero),
            (0x81950002u32 as i32, ReadKind::Empty),
            (0x81950004u32 as i32, ReadKind::Overrun),
            (0x81950008u32 as i32, ReadKind::Ended),
            (-99, ReadKind::Error),
        ] {
            ps5_capture_test_reset();
            ps5_capture_test_read(raw);
            let mut storage = CaptureStorage::<9216>::new();
            let backend = Avcap2Backend::new(&mut storage, ops()).unwrap();
            let mut owner = CaptureOwner::open(
                backend,
                Budget {
                    max_reads: 2,
                    deadline_us: 100,
                },
                0,
            )
            .unwrap();
            owner.start().unwrap();
            let mut bytes = [0; 16];
            let read = owner.read_once(1, &mut bytes).unwrap();
            assert_eq!(read.kind, kind);
            assert_eq!(read.raw_status, raw);
            assert_eq!(read.metadata, [0xa5; 16]);
            assert_eq!(read.native_time, None);
            if kind == ReadKind::Data {
                assert_eq!(read.bytes, 8);
                assert_eq!(bytes[..8], [0x3c; 8]);
            } else {
                assert_eq!(bytes, [0; 16]);
            }
            owner.close();
            drop(owner);
            for phase in [5, 6, 7] {
                assert_eq!(ps5_capture_test_calls(phase), 1);
            }
        }
        ps5_capture_test_reset();
        ps5_capture_test_read(33);
        let mut storage = CaptureStorage::<9216>::new();
        let backend = Avcap2Backend::new(&mut storage, ops()).unwrap();
        let mut owner = CaptureOwner::open(
            backend,
            Budget {
                max_reads: 2,
                deadline_us: 100,
            },
            0,
        )
        .unwrap();
        owner.start().unwrap();
        let mut bytes = [7; 32];
        assert!(owner.read_once(1, &mut bytes).is_err());
        assert_eq!(bytes, [0; 32]);
        drop(owner);
        for phase in [5, 6, 7] {
            assert_eq!(ps5_capture_test_calls(phase), 1);
        }
    }
}
