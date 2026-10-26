# Proposed platform ABI v1

The optional codec has a separate integer/pointer boundary in
`platform/codec/ps5_bt_sbc.h`, documented in SBC-IMPLEMENTATION.md. It queries
caller storage size/alignment and retains only pointers inside that borrowed
allocation until finish; it never retains PCM/output buffers. It changes neither this transport ABI nor the separately versioned
experimental capture ABI, and establishes no native capture format. The separate
generated-signal SBC self-test now links the target codec through that C
boundary; hardware execution remains separately recorded. The independent
`continuous-sbc` Rust runtime feature calls that same owner through
`ps5_rust_sbc_continuous(int32_t (*now_us)(uint64_t *out))`. Its synchronous
callback writes a host monotonic microsecond observation only on status zero,
retains no pointer and never supplies native capture timestamp units. Null,
error, regression and elapsed overflow fail closed; see SBC-CONTINUOUS.md.

The header `platform/ps5/include/ps5_bt.h` is the bootstrap source of truth.
These are our symbols, not existing PS5 SDK functions. ABI value `0x00010000`
is experimental major 1/minor 0; wrappers currently require an exact match.
Change the version for incompatible semantics. Never change one side silently.

## Transport contract

The independently selected native L2CAP availability diagnostic is C-only and
does not implement these functions. A proven native channel backend would need
a separately reviewed channel ABI so HCI/credits remain with the system owner;
successful socket allocation does not authorize pretending this ABI exists.

| Function | Meaning |
| --- | --- |
| `ps5_bt_abi_version()` | exact numeric ABI identifier |
| `ps5_bt_init(out_session)` | create one owned session; success returns a nonzero opaque u32 token |
| `ps5_bt_shutdown(session)` | revoke token, stop/reap owned transfers, release adapter state; return cleanup errors |
| `ps5_bt_send_hci_command(session, data, len)` | complete HCI command: LE opcode, u8 parameter length, parameters; no H4 byte |
| `ps5_bt_send_acl(session, data, len)` | complete ACL header + payload; no H4 byte |
| `ps5_bt_receive(session, data, capacity, out_len, out_kind, timeout_ms)` | one event (kind 1) or ACL (kind 2), including native HCI header; no H4 byte |
| `ps5_platform_log(data, len)` | synchronous byte output; caller storage is never retained |
| `ps5_platform_abort()` | non-returning panic termination in platform C |

All status returns are i32: `0` OK, `-1` unsupported, `-2` invalid,
`-3` busy, `-4` buffer too small, `-5` I/O, `-6` disconnected,
`-7` transport integrity lost, `-8` timeout. Unexpected values are preserved by
Rust as `Backend(code)`, never treated as success. This is not an errno ABI.

All non-success calls initialize output lengths/kinds/tokens to zero. Init
failure leaves no live token/resources. Tokens are adapter-managed, never raw
pointers, and must not be reused while stale sessions could exist. Current
session is neither thread-safe nor reentrant. The wrapper is not Send/Sync.
Only `ps5-platform` calls the unsafe declarations; portable code uses traits.

Caller buffers are valid only during a call. Send must copy all accepted bytes
before return and must return busy without accepting any part if the bounded
queue is full. Success is not a controller completion. Receive must validate
USB lengths before copying, must not write beyond capacity, and must not combine
packets or silently truncate them. A short destination leaves the queued frame
available and returns `-4` with output length zero. Queue overflow/framing loss
is a sticky transport integrity error, not a dropped-packet counter followed by
success. This refers to the raw HCI receive queue, not a PCM playout buffer:
bounded concealment/rebuffer and expired unsent media policy live above this ABI.
Those policies never acknowledge missing HCI completions or discard partially
sent L2CAP PDUs on a healthy connection. A higher-level typed terminal failure
or newer epoch may cancel the remainder and invalidate the old media session
with accepted-prefix accounting; uncertain controller credits are not refunded.
See AUDIO-RECOVERY.md; no C ABI change is introduced.
Busy is the only send error with a general no-acceptance guarantee here. The
portable credit owner treats other backend errors as terminal with an uncertain
current-send outcome; it neither refunds nor blindly retries that frame. Pure
Rust preflight errors occur before the native call and do not consume credit.
Timeout zero polls; positive waits are bounded in milliseconds. Timeout returns
`-8` with zero outputs and maps to `Ok(None)` in Rust receive.

Shutdown consumes the token even if it reports a failure. C must own all pending
transfer memory and quarantine it if cancellation/reaping cannot be proven;
it must never free storage still referenced by the platform. No Rust pointer is
retained. Explicit close returns the error; Drop can only attempt close and
log failure. No implicit reopen/recovery or controller reset.

The Rust wrapper checks frame lengths, packet kinds and received lengths. C's
capacity/write and lifetime guarantees remain necessary for memory safety;
post-call Rust checks cannot repair an out-of-bounds C write. Add sanitizer and
real C conformance tests when a real backend is selected.

## Experimental capture ABI v1

`platform/ps5/capture/capture_stream.h` defines an independently versioned
`0x00010000` candidate. `bt-capture` checks the queried storage size/alignment,
40-byte report and 36-byte read result, borrows address-stable storage and
consumes lifecycle phases once. Raw resolver status is i64; native lifecycle
statuses are i32. Native metadata stays opaque, native time absent and format
Unknown. Cleanup uncertainty blocks native reopen in that process. See
[CAPTURE-STREAM.md](CAPTURE-STREAM.md) for source/layout/lifetime review.
The separate `ps5-runtime/capture-stream` entry performs only one NOWAIT read,
never decodes PCM or sends Bluetooth, and retains the older diagnostic.

## Proposed production audio ABI (not implemented or frozen)

The independent C-only capture diagnostic in CAPTURE-PROBE.md and experimental
Rust/C candidate in CAPTURE-STREAM.md do not implement
this future Rust audio ABI or expose native capture handles to Rust. It has its
own injected metadata interface; no transport ABI version change follows.

Proposed `ps5_audio_open`, `ps5_audio_read`, `ps5_audio_close` use a separate opaque
token and caller-owned interleaved PCM buffer. Return format, actual whole frame
count, monotonic timestamp with defined origin, and discontinuity flags through
primitive output arguments or a separately versioned project-owned record.
Do not expose Avcap2 handles, private parameter blocks, NIDs or SDK structs.
48 kHz stereo float is an initial request based on upstream, not an established
capture contract. Determine bytes vs frames, time origin, finite sample handling,
permissions, empty/overrun/ended statuses, and cleanup through hardware tests
before freezing this ABI or claiming captured-audio support. Source-reviewed,
explicitly experimental versioned adapters and host conformance tests may be
implemented beforehand, with unknown format/timing fields remaining unknown.
Do not infer format from audio content or enable a production path on guessed
units. The established one-read diagnostic remains independently selectable.

## Independent clock/wait ABI v1

`platform/ps5/scheduler/scheduler_clock.h` defines a separate `0x00010000`
clock/wait contract with caller-owned checked storage and a 48-byte diagnostic.
`ps5-platform/external-clock-wait` preserves errors through `Result`, including
raw i64 resolver and i32 native results. Cached `clock_gettime` reads and one
bounded `usleep` request per caller step retain no output pointer. Cancellation
skips native calls; close consumes cached state once. See PLATFORM-CLOCK.md for
exact signatures, lifetime, conversion, regression, request bounds and source
review. An at-most-1ms request is not a measured bound on native elapsed time.


## Native application imports and control-only entry

The separate `application-control` feature uses the unchanged store/listener
boundaries and independent clock ABI through `ps5-app` fixed-storage owners.
`platform/ps5/application/posix_bridge.h` defines the independent `0x00010000`
selected-export bridge and its 48-byte numeric report. Native sockaddr/stat,
flags and errno types stay in C compiled against the exact pinned header closure.
All required exports resolve before any descriptor acquisition; no native layout,
raw syscall number or libc import is invented. See PS5-POSIX-IMPORTS.md.

The Rust application entry and Stop hooks are primitive experimental functions:

```c
void ps5_rust_application_request_stop(void);
int32_t ps5_rust_application_stop_requested(void);
int32_t ps5_rust_application_probe(
    int64_t (*lookup)(const char *, void **),
    const uint8_t *secret, uint32_t secret_len);
```

The caller supplies exactly 32 readable lowercase ASCII hex bytes from fresh
128-bit entropy, keeps the reviewed resolver/export lifetimes valid, initializes
matching native POSIX imports first and scrubs its original token after return.
Rust validates/copies the secret into an owned scrub guard and retains no pointer
after the call. No secret, path or native pointer appears in reports. Missing
imports/entropy prevent entry; no predictable fallback is permitted. The concurrent
Stop latch remains available throughout finite startup/drive/cleanup and is not
reset at entry. Non-reentrant backing storage is protected by an atomic guard;
cleanup uncertainty poisons reuse. Return 0 means clean control-only completion
or cancellation, never audio acceptance; return 1 means failure. Actual operation
and cleanup masks/diagnostics distinguish unacquired from closed resources.

This entry binds only an ephemeral loopback listener and uses the existing private
`/data/ps5-bt/control` directory, requiring mode 0700. It issues no radio command,
creates no credit owner and performs no capture or codec call. It changes no
transport ABI version or existing diagnostic's bounds. Native imports, entropy,
protection and timing remain Requires hardware; see APPLICATION-STARTUP.md.

## Fixed namespace preparation boundary

The separately selected application-setup candidate has its own 64-byte
`PS5_APP_SETUP_ABI=0x00010000` report, declared in
`platform/ps5/application/setup_namespace.h`. It uses the already initialized
POSIX import bridge and a separately typed `mkdirat` lookup; no transport, capture
or codec ABI changes. Stages, raw errors, owned/closed masks, cancellation and
sticky unknown cleanup are specified in [APPLICATION-SETUP.md](APPLICATION-SETUP.md).
Native owner/permission/sync behavior remains Requires hardware.
