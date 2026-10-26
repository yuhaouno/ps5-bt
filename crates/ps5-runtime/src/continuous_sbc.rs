// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit generated-only target candidate; the baseline probe stays separate.
use bt_audio::diagnostic::{MAX_BLOCKS, MAX_ELAPSED_US, Outcome, run};
use bt_core::Error;
use ps5_platform::log;

/// The callback writes a monotonic microsecond observation synchronously and
/// returns zero on success. Native timestamp units are unrelated to this clock.
/// A null callback fails without opening codec state.
// SAFETY: unique C ABI symbol; the callback must obey the documented synchronous
// writable-u64 contract and must not retain its pointer past the invocation.
#[unsafe(no_mangle)]
pub extern "C" fn ps5_rust_sbc_continuous(
    now_us: Option<unsafe extern "C" fn(*mut u64) -> i32>,
) -> i32 {
    let Some(now_us) = now_us else {
        log(b"event=sbc_continuous_failed reason=null_clock\n");
        return 1;
    };
    log(b"event=sbc_continuous_begin schema=1 generated=1 nominal_clock=synthetic sink=discard\n");
    log(b"codec=BlueZ_libsbc_2.2 license=LGPL-2.1-or-later generic=1\n");
    let report = run(
        || {
            let mut value = 0;
            // SAFETY: value is an initialized exclusive writable u64, valid only for
            // this call. The source-reviewed C clock adapter retains no pointer.
            let status = unsafe { now_us(&mut value) };
            if status == 0 {
                Ok(value)
            } else {
                Err(Error::Backend(status))
            }
        },
        MAX_BLOCKS,
        MAX_ELAPSED_US,
    );
    super::field(b"sbc_continuous_outcome=0x", report.outcome as u32);
    super::field(b"sbc_continuous_blocks=0x", report.blocks);
    super::field(b"sbc_continuous_pcm_frames=0x", report.pcm_frames as u32);
    super::field(b"sbc_continuous_bytes=0x", report.encoded_bytes as u32);
    super::field(
        b"sbc_continuous_elapsed_us_hi=0x",
        (report.elapsed_us >> 32) as u32,
    );
    super::field(b"sbc_continuous_elapsed_us_lo=0x", report.elapsed_us as u32);
    super::field(b"sbc_continuous_prefix=0x", report.prefix);
    super::field(b"sbc_continuous_digest=0x", report.digest);
    super::field(
        b"sbc_continuous_cleanup_attempted=0x",
        u32::from(report.cleanup_attempted),
    );
    if let Some(error) = report.backend_error {
        super::report_error(error);
    }
    if let Some(error) = report.cleanup_error {
        log(b"event=sbc_continuous_cleanup_failed\n");
        super::report_error(error);
    }
    if report.outcome == Outcome::Complete {
        log(b"event=sbc_continuous_complete\n");
        0
    } else {
        log(b"event=sbc_continuous_failed\n");
        1
    }
}
