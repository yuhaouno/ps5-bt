# Experimental fallible platform clock and finite wait

Status: source-reviewed, host-tested and target-object-compiled candidate. Export
availability, actual wait latency, native clock behavior and PS5 scheduling remain
**Requires hardware**. This adapter is independently selected and is not a native
application-loop factory or readiness backend. The existing diagnostic clock
helpers and payload variants remain independently selectable and unchanged.

## Boundary and source review

The optional `ps5-platform/external-clock-wait` feature exposes
`ps5_platform::clock_wait`. The final platform linker must explicitly supply
`platform/ps5/scheduler/scheduler_clock.c`. Default builds have no new native
symbols. The project-owned integer/pointer ABI is independently versioned
`0x00010000`; it does not change the transport, capture or codec ABIs.

The reviewed API/layout comes from ps5-payload-dev/sdk release v0.45, exact
revision `a7685ce8e97ce1f8a4f38253a41d54241c9124f1`:

- [`include/freebsd/time.h`](https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/include/freebsd/time.h)
  declares `clock_gettime(clockid_t, struct timespec *)` returning int and defines
  `CLOCK_MONOTONIC=4`. Its Regents/USL notice uses Berkeley redistribution terms.
- [`include/freebsd/sys/_types.h`](https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/include/freebsd/sys/_types.h)
  defines the clock ID as signed 32-bit and useconds as unsigned int; its Mike
  Barcroft notice has two-clause redistribution terms.
- [`include/freebsd/sys/_timespec.h`](https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/include/freebsd/sys/_timespec.h)
  and [`include/freebsd/x86/_types.h`](https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/include/freebsd/x86/_types.h)
  establish signed 64-bit seconds and LP64 long nanoseconds. Their individual
  Regents/Barcroft BSD notices differ; the latter retains an advertising clause.
- [`include/freebsd/unistd.h`](https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/include/freebsd/unistd.h)
  declares `usleep(useconds_t)` returning int. Its Regents notice has three-clause
  redistribution terms.

These are the signatures/layout already used in the source-reviewed diagnostic
helpers in `platform/ps5/main.c`; see CAPTURE-OBSERVE.md and UPSTREAM.md. This new
adapter is original project code under GPL-3.0-or-later. No SDK header or function
implementation is copied, selected for compilation or modified, so no new SDK
source-distribution input is introduced. Root SDK licensing does not replace
file-specific notices. There is no new poll struct, readiness constant, syscall
number, offset, privilege change, kernel operation or library loader.

## Ownership and ABI invariants

`ClockWaitStorage<128>` provides zeroed caller-owned 16-byte-aligned storage. The
Rust owner queries and validates actual context size/alignment/version before
initialization. C storage must be fresh zeroed or previously closed; init rejects
an already initialized/busy context before changing cached functions. It resolves
`clock_gettime` and `usleep` exactly once each through an injected synchronous
current-process resolver and caches their callable pointers. Initialization makes
no clock/wait call, opens no descriptor and allocates no external resource.
Failure clears its context. The context stays address-stable and exclusively
borrowed until close; it retains no caller callback-table, report or output pointer.
The resolver's returned function pointers must remain valid for that entire
borrow. Function/data pointer size and target time-layout assertions are explicit.

`ClockWait::new` is unsafe because the integration owner must establish those
resolver/export signature and lifetime guarantees and preserve the existing loader
resolver's lifetime. All subsequent methods are safe, synchronous and single-owner;
`ClockWait` is neither Send nor Sync. C rejects callback reentry while busy.
Close consumes cached state once, clears storage and acquires/releases no native
resource. Rust Drop calls close; explicitly call close when the result needs to
be observed. Initialization/clock/wait errors are returned with their diagnostics,
never logged as pointers or silently converted to zero time. Rust native/ABI
failures are sticky until close, so later calls do not hide them by retrying.

The 48-byte `ClockDiagnostic` matches the C header: ABI version, stage, project
status, complete native int return, full signed 64-bit resolver return, requested
microseconds, a zero reserved field, and raw seconds/nanoseconds. It contains no
pointer. Project statuses distinguish unavailable resolution, invalid parameters,
busy/reentry, native failure, invalid time, conversion overflow and regression.
Native errno detail is not inferred from the raw return; no unreviewed errno slot
or status mapping is introduced. Rust also checks stage/version/status, reserved
fields, returned outputs and exact time conversion before accepting a result.

## Fallible clock and bounded wait requests

`now_us(&mut self) -> Result<u64, ClockWaitError>` makes one cached native
`clock_gettime(4, &time)` call. It preserves native errors, rejects negative
seconds/nanoseconds or nanoseconds >=1,000,000,000, and checks microsecond overflow.
The successful origin is the native monotonic domain, not wall time or audio
sample time. Sub-microsecond values are truncated. C compares the raw time pair
against its last successful read, catching regression even within a single
microsecond; Rust independently rejects a regressed microsecond output. Failed
native reads leave the numeric output zero and never replace the last successful
clock. No fallback clock is used. This API intentionally does not implement the
infallible `bt_core::Clock` trait, which cannot preserve native failure.

`deadline_after(now_us, delay_us)` uses checked addition. The outer loop combines
its actual deadlines and passes the earliest applicable one to
`wait_slice(deadline_us, cancelled)`:

1. An active cancelled step returns `Cancelled` without a clock or wait call.
2. Otherwise read the native clock exactly once; failure is returned immediately.
3. If now is at/past the exclusive deadline, return `DeadlineReached` without wait.
4. Otherwise request one `usleep(min(deadline-now, 1000))` and return its result.
   Zero and >1000 us requests are rejected at the C boundary. There is no retry
   after interruption, internal wait loop, background worker or inferred readiness.

`WaitReturned` records only successful return, the before-wait observation and
requested duration. Native scheduling may oversleep or return early; actual
elapsed duration is unknown until a later successful clock read. A <=1 ms request
is not a hard execution-time bound. The caller re-enters the owner loop and handles
Stop/Cancel between slices, supplying its own finite operation/time budget.
Native ready notification and Bluetooth ownership/coexistence remain separate
missing prerequisites; waiting does not create HCI credit or signal readiness.

## Offline verification

```sh
cargo test -p ps5-platform --features host-clock-wait-tests --locked
cargo clippy -p ps5-platform --all-targets --features host-clock-wait-tests --locked -- -D warnings
cargo check -p ps5-platform --target x86_64-unknown-none --features external-clock-wait --locked
cc -std=c11 -Wall -Wextra -Wpedantic -Werror \
  -fsanitize=address,undefined -fno-omit-frame-pointer \
  -Iplatform/ps5/scheduler -Itests/platform \
  platform/ps5/scheduler/scheduler_clock.c \
  tests/platform/scheduler_clock_fixture.c tests/platform/scheduler_clock_test.c \
  -o /tmp/ps5-scheduler-clock-test
/tmp/ps5-scheduler-clock-test
```

The host-only feature compiles the actual C adapter plus synthetic reviewed exports
and rejects HOST != TARGET. The Rust integration test covers cached resolution,
full-width resolver/native errors, cancellation with no native call, exact short
and maximum slices, reached deadlines, sticky errors, checked addition, invalid
time/overflow/regression and consume-once close. A compile-fail check rejects Send.
The C harness additionally checks exact layout, misaligned/short/aliased storage,
dirty/null lookup outputs, reentry, raw nanosecond regression, the exact u64 time
boundary, invalid wait bounds and absence of retries. Neither invokes OS timing.

LLVM 18.1.8 compiles the C adapter with `-target x86_64-sie-ps5 -O1
-ffreestanding -fno-builtin -fPIC -fno-stack-protector -fno-plt -mno-red-zone
-mgeneral-regs-only -ffunction-sections -fdata-sections`. The inspected object is
little-endian ELF64 x86-64 relocatable, has no undefined imports and uses no
XMM/YMM/ZMM registers. This object check and the Rust freestanding check do not
establish a runnable native application loop or PS5 scheduling behavior.

## Remaining integration and hardware gates

The application owner must initialize this adapter with its already-reviewed
current-process resolver, keep exports live, explicitly link the C object, pass
only the shared native monotonic domain to its timers and retain primary errors
before close. Default runtime behavior remains unchanged. No seventh diagnostic
payload is added by this module; a later explicit bounded run needs its own
artifact identity, finite calls/deadline, durable entered/final markers, current
PS5 authorization and wireless controller checks before/during/after. Native
resolution, clock validity and wait duration remain Requires hardware until that
candidate is executed. Do not infer production readiness, audio pacing quality
or controller coexistence from host fixtures or prior diagnostic windows.
