# Experimental Rust capture stream boundary

Status: one immediate read through this Rust/native boundary completed on
Slim/13.40 on 2026-10-26: Empty (`0x81950002`), zero bytes, clean stop/close and
matching lifecycle. Controller input was reported normal before/during/after.
See [HW4-EVIDENCE.md](HW4-EVIDENCE.md). Native capture format, metadata units,
continuous-read behavior and scheduling remain **Requires hardware**. The
established C-only one-read/observation diagnostics remain independently selected
and unchanged. Nonempty data in those earlier diagnostics does not validate this
adapter or its normalized PCM contract.

## Ownership and independent ABI

`crates/bt-capture` is `no_std` and allocates nothing. Its default build has no
native symbol dependencies. `external-avcap2` selects the safe Rust adapter and
requires the final linker to provide
`platform/ps5/capture/capture_stream.c`. The independent ABI identifier is
`0x00010000`, exposed by `ps5_capture_stream_abi_version`; it does not change the
raw Bluetooth ABI. The C header is the layout source of truth.

The C context size/alignment queries allow caller-owned, address-stable storage.
`CaptureStorage<9216>` provides sufficient storage for the current implementation
with 16-byte alignment, but the Rust constructor always checks the queried size
and alignment. C retains only its own context/native function pointers and opaque
native handle inside that exclusive borrow. It never retains Rust read buffers,
resolver table pointers or report pointers. A private aligned 8,192-byte scratch
and 16-byte metadata block isolate native calls from the Rust destination;
metadata and scratch are wiped after every read, and context is wiped at close.
The scratch capacity is independently bounded even for smaller read requests.

`ResolverOps` contains an exact version/size and the already-reviewed synchronous
current-process/module resolver callbacks from CAPTURE-PROBE.md. The unsafe Rust
constructor requires those callbacks to return callable exports with the reviewed
signatures, the existing loader kernel resolver to remain initialized and no
concurrent native owner or callback reentry. No new export resolver, kernel
structure, offset, credential mutation, system-driver operation or initial kernel
access mechanism is introduced.

C Open resolves `sceKernelLoadStartModule`, loads Ipmi before Avcap2, resolves all
seven capture exports, initializes and opens the same source-2/mode-0 request
as the existing diagnostic. The private `0x38` parameter's unknown fields retain
the reviewed values (`+0x08=0x18`, `+0x1c=0xffffffff`, other unknowns zero). Those
values describe the request, not a verified PCM format. Start, one NOWAIT read,
Stop and Close are separate calls. Close terminates the definitely initialized
capture lifecycle as well. Process modules are left loaded because their safe
unload ownership has not been established.

`CaptureOwner<B>` implements Open -> Start -> bounded one-read steps -> Stop ->
Close. Each definitely acquired phase is consumed once, including Start failure,
read failure, Ended, budget/deadline exhaustion, explicit close and Drop. Failed
Open must consume its definitely acquired resources; dirty failed-open handles
are not treated as owned. Primary errors and both owner cleanup results remain
separate. Native reports additionally preserve raw Stop/Close/Terminate statuses
and cleanup failure flags. Any native cleanup uncertainty blocks further opens
in that payload process; a failed Stop is not retried by Close. There is no
implicit reopen, wait, background task or retry loop. `Avcap2Backend` is neither
Send nor Sync. A single native active-context guard rejects competing owners;
this is not evidence about a system capture owner or Bluetooth coexistence.

The Rust owner takes a nonzero maximum read count and an exclusive absolute
host-monotonic deadline. Each read step makes at most one native call. Clock
regression and exhausted budgets fail and clean up. Stop remains callable without
another read. NOWAIT's actual latency is still a native/hardware uncertainty:
checking a deadline cannot preempt a native call that unexpectedly blocks.
Drop attempts cleanup; callers must explicitly close and retain its report when
an observable result is required.

## Read and metadata semantics

The 36-byte C read record holds version, kind, full signed native return, byte
length, discontinuity bit and all sixteen opaque metadata bytes. The 40-byte
report contains version/stage/project status, raw native status, independent
cleanup fields and a full signed 64-bit resolver return. Reports contain no
pointer, native handle or PCM bytes. Opaque metadata is available only to the
local adapter and must not be logged, persisted or exposed over LAN.

Positive native returns are byte counts and are accepted only within the exact
requested destination capacity. No divisibility-by-eight or PCM interpretation
is imposed at this boundary. Zero, Empty (`0x81950002`), Overrun (`0x81950004`),
Ended (`0x81950008`) and other negative statuses stay distinct; all raw bits are
preserved. Overrun sets discontinuity with unknown missing-sample count. Ended
and other native errors terminate the Rust owner's run. Empty and Zero consume
one read budget entry and do not create samples or retry internally. Unknown
errors are not mapped to successful PCM. ABI/bounds rejection zeroes the Rust
output before propagating a failure. No metadata field is currently interpreted
as a timestamp, so native adapter `native_time` is always `None`.

## Explicit PCM contract and timestamps

`Format::UNKNOWN` is the only warranted native format today. `Evidence::Unknown`
and `Evidence::Requested` cannot produce `ValidatedFormat`; neither may reach
normalization/encoding. Offline callers may declare a concrete input contract
with `Evidence::Declared`; that is not hardware verification. Native production
activation needs its own evidence and platform gating. `HardwareVerified` is an
explicit evidence label supplied by the integration owner, not autodetection.

The portable normalizer accepts declared/verified interleaved mono or stereo
signed i16 little-endian and IEEE-754 binary32 little-endian. It retains the
explicit sample rate (8,000..192,000 Hz), duplicates mono into both channels,
and performs no resampling, channel-layout inference or format heuristics.
The scheduler must additionally require the actual encoder's sample rate and
complete PCM block size. Unknown sample types/layouts and unsupported channel
counts fail. Short frames, insufficient destination capacity and any NaN/Inf
reject the entire input without changing normalized output. Finite float samples
are clipped to [-32768,32767] and otherwise truncated toward zero after scaling
by 32768. Conversion uses integer IEEE-754 bit arithmetic; it needs no native
float-valued ABI, floating-point/SIMD state or soft-float runtime.

`Timeline` separately stores raw `NativeTime {value,domain}`, host observation
microseconds and nominal sample position. It never derives native units from
metadata or substitutes host observation time for capture time. Optional jump
thresholds are explicitly in the supplied native domain's raw units. Raw native
regression, excessive forward steps and domain changes are observable flags;
missing native values retain the last comparable value. Host regressions fail.
Sample/gap arithmetic is checked before committing advancement. A known sample
gap advances sample position; unknown loss requires an explicit new stream epoch.
`restart` preserves the host clock's monotonicity while resetting native-domain
comparison and nominal sample origin.

## Capture-to-scheduler bridge

`CaptureBridge` borrows a normalization staging buffer and one encoding-block
buffer. Ingest accepts one read outcome and explicit host observation time;
normalization happens only after format validation. `prepare_block` copies at
most one block's remaining capacity, joins short reads, preserves staged leftovers
and exposes `{first_sample,samples}` only for a complete block. The caller submits
that block to the existing audio scheduler's `push_pcm`, then calls
`confirm_block` only on acceptance. Busy/capacity failure retains the identical
block and all unused staging samples. No per-frame allocation is introduced.

Empty creates no sample advance. Exact caller-established sample gaps drop a
partial block with accounting, then preserve the explicit next sample position;
the scheduler decides bounded concealment/rebuffer. An Overrun with unknown loss
raises `needs_epoch`, drops the partial block with accounting and prohibits further
data ingestion until the caller coordinates `restart_epoch` with a fresh downstream
stream. No missing sample count is guessed from elapsed time or bytes. Ended/error
discards a partial final block with accounting; it never pads a final block
silently. The bridge records data/Empty/Overrun/known and unknown gaps, dropped
partial input and downstream-accepted block counts with saturating telemetry.
Existing partial-PDU/ACL ownership remains wholly downstream and unchanged.

## Source/layout review and licensing

This adapter reuses only the narrow mechanisms already inspected for
[CAPTURE-PROBE.md](CAPTURE-PROBE.md), with local changes for separate calls,
caller-owned stable storage, raw metadata, bounded byte copying, explicit ABI
records, consume-once cleanup and a sticky cleanup-uncertainty gate:

- FGGstore/FGG-PlayPods `d726e1c32568048aae66f3218c0540adc357e408`,
  `src/capture.c`, root GNU GPL version 3 license: module dependency/order,
  signatures, private request layout, known raw statuses and cleanup. The new C
  implementation/header and C fixtures conservatively retain `GPL-3.0-only`.
- X-F1REBALL-X/HearBridge-PS5 `cd5b7fce1b8226d8850cdd306437550098c6a62b`,
  `src/avcap2.c`, GPL-3.0-or-later: matching signatures/layout and 16-byte
  metadata-size initialization. The metadata contents/units remain unknown.
- ps5-payload-dev/sdk `a7685ce8e97ce1f8a4f38253a41d54241c9124f1`,
  reviewed `kernel.c/kernel.h/nid.c` selection: existing current-process export
  resolver; none is copied into this adapter.

See [UPSTREAM.md](UPSTREAM.md). No whole capture application, retry loop, PCM
heuristic, credential fallback, native-float conversion or proprietary module
is imported. The Rust protocol/normalization/clock/queue owner is project code;
C remains the narrow native ABI adaptation.

## Offline checks

```sh
cargo test -p bt-capture --locked
cargo test -p bt-capture --features host-avcap2-tests --locked
cargo clippy -p bt-capture --all-targets --features host-avcap2-tests --locked -- -D warnings
cargo check -p bt-capture --target x86_64-unknown-none --features external-avcap2 --locked
cc -std=c11 -DPS5_CAPTURE_STREAM_HOST_TEST -Wall -Wextra -Wpedantic -Werror \
  -fsanitize=address,undefined -fno-omit-frame-pointer \
  -Iplatform/ps5/capture -Itests/platform \
  platform/ps5/capture/capture_stream.c \
  tests/platform/capture_stream_fixture.c tests/platform/capture_stream_test.c \
  -o /tmp/ps5-capture-stream-sanitize
/tmp/ps5-capture-stream-sanitize
```

The host fixture feature compiles the actual C adapter with synthetic reviewed
exports; it rejects HOST != TARGET. It is never suitable for a payload. The
`PS5_CAPTURE_STREAM_HOST_TEST` macro exposes only a test process-reset hook so
independent fault cases can simulate a fresh process after the sticky uncertainty
latch. Production builds omit that hook. Portable tests cover malformed formats,
atomic normalization and 10,000 float bit patterns, raw/native/host/sample clock
separation, exact and unknown gaps, overflow, bounded resource ownership,
short-chunk stitching and retained block acceptance. The actual Rust/C fixture
checks raw resolver/native results and lifecycle failures/cleanup; the standalone
C fault harness also checks layout, request bytes, alignment, dirty handles,
oversized native byte claims, rejected aliasing and the cleanup uncertainty gate.
None is evidence of native format, continuous capture, audio quality or timing.

## Bounded independent native candidate admission

The integration owner may select a separate capture-stream artifact that calls
Rust `CaptureOwner` through this C boundary for exactly one NOWAIT read, with a
checked native host clock before/after, finite run/read budgets and an explicit
close result. It must retain `Format::UNKNOWN`, report byte counts/raw statuses
only, perform no normalization, SBC encoding or Bluetooth traffic, and keep the
existing C-only diagnostics independently selectable. The hypothesis is that the
new ownership/FFI boundary reproduces the reviewed lifecycle and bounded byte
return; it does not test an assumed PCM format. Required prerequisites are the
existing loader kernel resolver/export lifetimes and a current authorized PS5
window. Unresolved facts include native read duration, metadata meaning, format,
permissions and long-running capture behavior. Stop on any call/clock/bounds
failure; consume owned Stop/Close/Terminate once and never retry uncertainty.
The executable's build identity, entered/final outcome, cleanup results and
abnormal-interruption marker must be durable. Check wireless controller behavior
before/during/after the eventual run; no past hardware window authorizes it.
