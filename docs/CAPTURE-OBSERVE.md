# Bounded capture observation candidate

Status: host-tested and target-compiled; Requires hardware. The separate
100 ms settled capture-once candidate returned 8,192 bytes on one Slim / 13.40
run with clean lifecycle cleanup. This candidate has not run on PS5 and does
not inherit that result.

## Purpose and bounds

`ps5_capture_probe_observe` distinguishes an immediately available buffer from
a short, explicitly bounded sequence of Empty or Overrun results. It reuses the
reviewed Avcap2 lookup, Initialize/Open/Start and exactly-once
Stop/Close/Terminate path. It does not change capture parameters, infer a PCM
format, retain samples or retry a terminal error.

After Start it obtains one caller-supplied monotonic microsecond value and makes
the first `READ_NOWAIT` call immediately. Empty or Overrun permits one external
100,000 us wait, followed by a new monotonic timestamp and at most one further
read. Observation stops on the first data or other error, at 20 reads, or at the
exclusive two-second deadline. There is no internal timer, sleep, thread or
unbounded loop. The result reports stop reason, read/Empty/Overrun counts, the
start-relative monotonic timestamp sampled immediately before the final actual
read, final raw read value and cleanup outcome. That timestamp excludes native
read duration and every later wait or cleanup; it is not a total-duration
measurement.

Data remains only in the existing 8,192-byte scratch and is wiped before
return. A positive value no greater than that capacity is recorded as bytes;
the observation path intentionally does not divide by an assumed frame size.
Overrun sets discontinuity. Empty/Overrun exhaustion is a bounded observation,
not PCM success. Zero and unknown native errors terminate as read errors.

## Required parent hooks

The observation candidate requires both callbacks in
`ps5_capture_probe_ops`:

```c
int wait_before_read(uint32_t microseconds);
int now_us(uint64_t *out_monotonic_us);
```

`wait_before_read` must synchronously wait exactly the supplied interval and
return zero only on success. `now_us` must read a monotonic clock, reject
invalid seconds/nanoseconds and conversion overflow, and return zero only after
writing microseconds. Failed output is ignored. The runner never resolves a
clock API itself and never falls back to a wall clock.

The intended parent adapter uses the already source-reviewed SDK
`clock_gettime(CLOCK_MONOTONIC, struct timespec *)` declaration. Its ABI and
conversion remain parent-owned: this module accepts only the checked integer
microseconds. Both callbacks are synchronous and non-reentrant.

The lower `capture_observe` state machine creates a cleanup obligation after a
successful begin. Every runner exit terminates that state and takes the token
once before entering the existing cleanup block. Failure to take it is an
internal failure; native Stop/Close/Terminate ownership rules remain unchanged.

## Host acceptance

Strict C11 and ASan/UBSan tests cover immediate data, Empty/Overrun/data,
20-read exhaustion, the two-second deadline, wait and clock failures, terminal
read error, cleanup failure and one-time cleanup ownership. The generic state
tests additionally cover cancellation, regressing clocks, invalid arguments,
oversized native lengths and atomic state on local rejection. Both the state
machine and runner compile with the reviewed freestanding PS5 target flags.

Hardware acceptance requires the exact artifact hash and source HEAD, durable
phase markers, data versus bounded-empty outcome, Stop/Close/Terminate status,
`cleanup_uncertain=0`, and normal wireless controller input before/during/after.
No result establishes the PCM sample format, clock rate or audible output.
