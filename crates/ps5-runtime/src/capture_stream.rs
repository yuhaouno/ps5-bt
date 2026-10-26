// SPDX-License-Identifier: GPL-3.0-or-later
//! Independently selected one-read experimental owner exercise; no PCM claims.
use bt_capture::{
    ABI_VERSION, Avcap2Backend, Budget, CaptureError, CaptureOwner, CaptureStorage, Cleanup,
    Diagnostic, MAX_READ_BYTES, ReadKind, ResolverOps,
};
use core::ffi::{c_char, c_void};
use ps5_platform::log;

unsafe extern "C" {
    fn ps5_bootstrap_lookup(name: *const c_char, out: *mut *mut c_void) -> i64;
    fn ps5_bootstrap_module_lookup(module: i32, name: *const c_char, out: *mut *mut c_void) -> i64;
}
fn diagnostic(record: Diagnostic) {
    super::field(b"capture_stream_stage=0x", record.stage);
    super::field(b"capture_stream_status_bits=0x", record.status as u32);
    super::field(
        b"capture_stream_raw_status_bits=0x",
        record.raw_status as u32,
    );
    super::field(
        b"capture_stream_lookup_hi=0x",
        (record.raw_lookup as u64 >> 32) as u32,
    );
    super::field(b"capture_stream_lookup_lo=0x", record.raw_lookup as u32);
    super::field(b"capture_stream_cleanup_flags=0x", record.cleanup_flags);
    super::field(b"capture_stream_raw_stop_bits=0x", record.raw_stop as u32);
    super::field(b"capture_stream_raw_close_bits=0x", record.raw_close as u32);
    super::field(
        b"capture_stream_raw_terminate_bits=0x",
        record.raw_terminate as u32,
    );
}
fn error(error: CaptureError) {
    match error {
        CaptureError::Backend(record) => diagnostic(record),
        error => super::field(
            b"capture_stream_owner_error=0x",
            match error {
                CaptureError::InvalidBudget => 1,
                CaptureError::InvalidState => 2,
                CaptureError::InvalidBuffer => 3,
                CaptureError::InvalidOutcome => 4,
                CaptureError::ClockRegressed => 5,
                CaptureError::ReadBudget => 6,
                CaptureError::Deadline => 7,
                CaptureError::Backend(_) => unreachable!(),
            },
        ),
    }
}
fn cleanup(report: Cleanup) -> bool {
    let mut failed = false;
    for (name, result) in [
        (b"capture_stream_stop_bits=0x".as_slice(), report.stop),
        (b"capture_stream_close_bits=0x".as_slice(), report.close),
    ] {
        if let Some(result) = result {
            super::field(
                name,
                match result {
                    Ok(()) => 0,
                    Err(record) => record.status as u32,
                },
            );
            if let Err(record) = result {
                diagnostic(record);
                failed = true;
            }
        }
    }
    log(b"event=capture_stream_cleanup_complete\n");
    failed
}
/// One NOWAIT read, at most two seconds of observed elapsed time. Clock calls
/// and synchronous native calls must themselves return; an external run ceiling
/// is still required. A successful Empty/Zero read proves no PCM format.
// SAFETY: unique primitive C entry; the supplied callback writes an exclusive
// u64 synchronously, retains no pointer, and returns zero only for valid time.
#[unsafe(no_mangle)]
pub extern "C" fn ps5_rust_capture_stream(
    now_us: Option<unsafe extern "C" fn(*mut u64) -> i32>,
) -> i32 {
    log(b"event=capture_stream_begin schema=1 reads_max=1 format=Unknown native_time=Unknown\n");
    let result = exercise(now_us);
    log(if result == 0 {
        b"event=capture_stream_complete\n"
    } else {
        b"event=capture_stream_failed\n"
    });
    result
}
fn exercise(now_us: Option<unsafe extern "C" fn(*mut u64) -> i32>) -> i32 {
    let Some(now_us) = now_us else {
        return 1;
    };
    let now = || -> Result<u64, i32> {
        let mut value = 0;
        // SAFETY: synchronous reviewed callback receives a valid exclusive u64.
        let status = unsafe { now_us(&mut value) };
        if status == 0 { Ok(value) } else { Err(status) }
    };
    let start = match now() {
        Ok(value) => value,
        Err(raw) => {
            super::field(b"capture_stream_clock_bits=0x", raw as u32);
            return 1;
        }
    };
    let Some(deadline_us) = start.checked_add(2_000_000) else {
        return 1;
    };
    let mut storage = CaptureStorage::<9216>::new();
    let ops = ResolverOps {
        abi_version: ABI_VERSION,
        struct_bytes: core::mem::size_of::<ResolverOps>() as u32,
        lookup_current: ps5_bootstrap_lookup,
        lookup_module: ps5_bootstrap_module_lookup,
    };
    // SAFETY: the explicitly selected loader resolver is initialized before main.
    // Pinned export signatures/layout and single-owner lifetime are reviewed in
    // CAPTURE-STREAM.md. No other capture diagnostic is linked into this variant.
    let backend = match unsafe { Avcap2Backend::new(&mut storage, ops) } {
        Ok(backend) => backend,
        Err(record) => {
            diagnostic(record);
            return 1;
        }
    };
    let mut owner = match CaptureOwner::open(
        backend,
        Budget {
            max_reads: 1,
            deadline_us,
        },
        start,
    ) {
        Ok(owner) => owner,
        Err(failure) => {
            error(failure.error);
            cleanup(failure.cleanup);
            return 1;
        }
    };
    if let Err(failure) = owner.start() {
        error(failure.error);
        cleanup(failure.cleanup);
        return 1;
    }
    let mut bytes = [0; MAX_READ_BYTES];
    let observed = now();
    let mut failed;
    match observed {
        Ok(observed) => match owner.read_once(observed, &mut bytes) {
            Ok(read) => {
                super::field(
                    b"capture_stream_kind=0x",
                    match read.kind {
                        ReadKind::Data => 1,
                        ReadKind::Zero => 2,
                        ReadKind::Empty => 3,
                        ReadKind::Overrun => 4,
                        ReadKind::Ended => 5,
                        ReadKind::Error => 6,
                    },
                );
                super::field(
                    b"capture_stream_read_status_bits=0x",
                    read.raw_status as u32,
                );
                super::field(b"capture_stream_bytes=0x", read.bytes as u32);
                super::field(
                    b"capture_stream_discontinuity=0x",
                    u32::from(read.discontinuity),
                );
                failed = read.kind == ReadKind::Error;
            }
            Err(failure) => {
                error(failure.error);
                failed = true;
            }
        },
        Err(raw) => {
            super::field(b"capture_stream_clock_bits=0x", raw as u32);
            failed = true;
        }
    }
    bytes.fill(0);
    match now() {
        Ok(end) if end >= observed.unwrap_or(start) && end < deadline_us => {}
        _ => {
            log(b"event=capture_stream_clock_or_deadline_failed\n");
            failed = true;
        }
    }
    failed |= cleanup(owner.close());
    i32::from(failed)
}
