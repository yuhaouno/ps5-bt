# Bounded raw capture workload for application integration

Status: allocation-free Rust workload implemented and host-tested, including the
existing actual Rust/C capture adapter with synthetic reviewed exports. Native
PS5 continuous reads, scheduling, permissions, format/timing and controller
coexistence remain **Requires hardware**. No console was contacted by this work.
This is raw-byte observation, not PCM, normalization, SBC or playback acceptance.

## Public integration contract

`ps5_app::observe::ObserveWorkload` borrows an already started
`CaptureOwner<B>` and caller-owned raw byte buffer for one step at a time:

```rust
let mut observation = ObserveWorkload::new(ObserveConfig::BOUNDED, started_us)?;
let progress = observation.step(capture, raw_bytes, now_us, stop_requested)?;
// progress: progressed, wake_at_us, finished
let report = observation.snapshot();
```

The module uses existing `bt-capture` contracts only. Its default path has no
native symbol call or feature dependency; a selected concrete backend determines
whether the one owner read reaches native capture. It does not initialize,
replace, restart or close the capture owner, obtain a new permission, change the
format or reset any existing read/recovery budget. Factory setup and final
consume-once cleanup remain with the enclosing resource owner. Terminal native
Ended/Error and backend failures retain `CaptureOwner`'s existing automatic
cleanup behavior; this workload observes those results rather than repeating it.

Create the workload after actual Capture acquisition, using a successful host
monotonic observation from the same clock domain used by `CaptureOwner`. The
existing owner's absolute deadline may include a separately bounded startup
allowance; the workload's independent duration begins at its own creation.
It cannot extend the owner's earlier deadline or replenish its consumed reads.
A pre-consumed/stricter owner still fails according to that original budget.
Changing the owner's read count between workload steps is an explicit ownership
error, not permission to refresh the workload or continue invisibly.

## Finite reads and pacing

`ObserveConfig` admits at most 20 read invocations and at most 2,000,000 host
microseconds, with an exclusive deadline. First settle is either zero or
100,000 us; the fixed interval is 100,000 us. The bounded default selects the
100 ms settle. With exact regular scheduling that gives 19 possible reads before
the exclusive two-second deadline; 20 is a maximum, not a promised count.
Choosing immediate first read can consume 20 reads at 0..1,900,000 us. Smaller
configured read/time bounds stop earlier. Invalid/overflowing configuration fails
before a read. These are independent candidate operation bounds, not audio-rate
capture scheduling or a measured latency policy.

The 100 ms scheduling request is grounded in the separately reviewed settle and
observation diagnostics: CAPTURE-PROBE.md/CAPTURE-OBSERVE.md record the explicit
wait interval and observed immediate Empty followed by a later nonempty read.
No waiting implementation is copied here and no native API is added. This module
returns only an absolute suggested wake. The application's reviewed clock/wait
adapter may approach it using finite <=1 ms wait requests, checking Stop between
caller steps; there is no sleep, retry loop, thread or hidden catch-up inside this
workload. A delayed caller reads once at the current instant and schedules its
next interval from that instant; it never issues a burst to catch up old deadlines.
The existing C-only first-data observation and one-read Rust diagnostic retain
their original separate bounds.

A step first clears caller raw storage, checks existing terminal state, then
honors Stop before clock/buffer validation or any new owner call. Stop finishes
normally without a read. Time regression fails; reaching the exclusive deadline
finishes without a read. Before due time it returns a wake suggestion with no
native operation. At due time it calls `CaptureOwner::read_once` at most once,
copies only diagnostic summary fields, clears the raw buffer and returns. Raw
storage is empty on every normal/error/wait/Stop return. Valid buffer sizes are
1..=8192 bytes, delegated consistently to the existing owner. The workload retains
no read buffer reference or audio byte. No native call can be preempted by this
Rust deadline; actual NOWAIT duration/stop latency still need hardware validation.

Unlike the old first-data diagnostic, nonempty Data continues under the same
finite observation budget. Empty, Zero and Overrun also continue at the fixed
cadence; terminal errors never retry. Stop, read/time exhaustion and Ended finish
observably. A failed workload remains terminal and returns its original failure
on later steps without further native calls. The outer application must convert
finish/failure to its observable stop/cleanup path, keeping control/network work
bounded alongside the capture lane.

## Diagnostics and unknown units

`ObserveSnapshot` records configuration, host start/exclusive deadline/last
observation/next due time, finish reason, complete failure, last read summary,
initial/final existing owner read counts, owner cleanup already attempted and
bounded counters:

- Workload owner invocations versus actual advances of the owner's read budget.
- Data, Empty, Zero, Overrun, Ended, unknown native-error and owner-failure counts.
- Total and maximum returned byte counts, with no division into sample frames.
- Discontinuity and unknown-gap event counts; no lost-sample count is invented.

The last read summary retains kind, full signed native status, byte count,
discontinuity and caller host observation immediately before the native call.
That observation is not sample time, completion time, native duration or a wall
clock. A separately supplied typed raw `NativeTime` remains in its original domain
without conversion, arithmetic or pacing; the actual Avcap2 adapter supplies None.
Format and native timestamp units remain Unknown. The sixteen opaque native
metadata bytes are deliberately excluded from public snapshot/Debug and are not
logged or stored; no field is interpreted as a timestamp or address.

Read/owner failures retain the complete existing `OwnerFailure`, including full
resolver/native status and separate cleanup. Unknown native read errors retain
raw status plus cleanup already attempted. Ended with uncertain cleanup is a
failure, never a clean success. Normal workload completion does not certify
cleanup: snapshot cleanup None means the owner has not attempted it yet. The
factory must explicitly close and retain its final cleanup report. Counters fit
the configured hard read bound; no per-frame allocation or counter reset occurs.
No radio command, allowance, credit owner or transport routing is created.

## Checks and next native integration

```sh
cargo test -p ps5-app --test observe --locked
cargo test -p ps5-app --test observe --features host-candidate --locked
cargo clippy -p ps5-app --test observe --features host-candidate --locked -- -D warnings
cargo check -p ps5-app --target x86_64-unknown-none --locked
```

Portable fixtures test exact Data/Empty/Zero/Overrun/Ended sequencing, byte counts
without frame assumptions, distinct host/native time domains, first-settle/wake
boundaries, read/time exhaustion, no late catch-up, Stop priority, pre-consumed
owner budgets, interleaved-owner rejection, malformed configuration/buffers,
regressing/near-overflow clocks, unknown errors and consume-once terminal cleanup.
A host-feature test calls the actual C capture adapter with reviewed synthetic
exports, observes a seven-byte Data read (with no PCM interpretation), an Overrun
and Empty, checks cleared raw storage and verifies only caller cleanup consumes
Stop/Close/Terminate. These fixtures perform no native PS5 capture or OS wait.

A runtime integration must select its own artifact/feature, start the workload
only after source-reviewed acquisition, preserve the baseline diagnostics,
service Stop/control between quanta and record bounded numeric outcomes plus
actual factory cleanup. Current hardware authorization and exact build identity
are required for any later execution, with controller checks before/during/after.
No deployment or console execution is authorized or performed by this module.
