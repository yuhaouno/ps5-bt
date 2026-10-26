# Concrete control-only application startup candidate

Status: implemented offline, with real Rust owners and native C adapter host
checks. The separately selected `ps5-runtime/application-control` candidate
assembles protected store, persistent listener, fallible clock/wait, application,
management runner and bounded owner loop. It performs **no Bluetooth traffic,
capture operation or codec execution**. Transport/capture/codec/playback remain
Unavailable; native capture format and ownership/credits remain Unknown.
PS5 native imports, filesystem permissions, scheduling and coexistence remain
**Requires hardware**. This is an executable control-only integration candidate,
not headset playback or production activation.

## Native and Rust resource ownership

The runtime supplies stable caller storage to `ps5-app::NativeFactory`, then
acquires its cached clock first. One successful clock observation establishes
the shared microsecond origin. MainLoop startup acquires the actual protected
FileStore and loopback Listener, with explicit disabled capture/encoder slots.
There is no constructor for a native Bluetooth transport or credit allowance.
The transport loop slot records only its unavailable policy state, never a native
handle. The factory retains Clock cleanup separately from MainLoop's five slots.

Only after the required native stages succeed does startup construct the
DeviceManager/Application, Runner and their fixed storage. A caller-owned
`StoreSlot<FileStore>` holds the real store. The application owns a `StoreLease`
which references that external slot; per-operation fallible borrowing rejects
reentry and absent storage without panicking. No application reference points
into its own resource aggregate. App-construction failure drops only a lease,
leaving the actual store available for observable cleanup. Cleanup drops the
application, takes the actual store, restores it to the native factory and invokes
its consume-once close. The factory records both primary and cleanup failures.

The native default store configuration is `/data/ps5-bt/control`. The existing
FileStore opens and verifies a supplied private directory; this candidate does
not create that directory, assume permissions or report it as durable merely
because a path was configured. A missing/unprotected directory is an observable
startup failure, followed by cleanup. Directory preparation is a separate local
configuration/deployment operation. Paths never enter runtime logs or HTTP.

`ControlStorage<4>` supplies fixed protocol, discovery and work buffers. Its
protocol configuration is inert while capabilities are Unavailable: the selected
ACL scratch envelope is a Rust storage check, not controller capacity or owned
credits. No queued scan/pair/start action can promote capabilities. Unavailable
radio adapters return Unsupported if accidentally invoked. The concrete runtime
skips uninstalled Input/Capture/Encode/Send lanes entirely instead of pretending
a native Empty read occurred.

## Per-turn lending and persistent HTTP

`ApplicationLoopState` retains pointer-free protocol-adapter state across calls.
`ApplicationLoop::lend` temporarily borrows existing Application, Runner, native
listener service, state and the same scratch buffers for one bounded quantum.
Dropping a lending view releases references without aborting native owners or
scrubbing a retained input packet. The enclosing MainLoop must explicitly stop
and clean up on every exit. The original owned ApplicationLoop keeps its
consume-on-Drop behavior. Replacing lending state is neither native cleanup nor
permission to reset a connection/recovery epoch.

The persistent `bt-listener::ServiceState` owns HTTP cursors and fixed buffers;
Listener retains accepted-client ownership. A runtime-local HttpWork view lends
both for at most one accept or one request/response operation. Closing one client
permits another accept without reconstructing Application or losing queued
management commands. Per-client errors remain observable; uncertainty escalates.
The actual bound ephemeral loopback port is included in the authorization Host
and Origin policy. No port/address or bearer token is logged.

The service has a 100,000 us per-request policy and at most64 total accepts.
These are explicit candidate budgets, not measured PS5 recommendations. The
6144-byte response buffer fits the existing static assets. Protected operations
use the existing bearer/Host/Origin/custom-header/generation checks. The page
reports actual unavailable operations. This candidate does not bind a LAN
address, accept arbitrary configuration through HTTP, or expose files, command
execution, memory, keys or raw capture data.

## Finite drive and Stop

The default independent candidate runs at most5,000,000 observed microseconds,
20,000 owner turns and10,000 wait-port calls. Each lane quota is1. Suggested waits
are at most1000 us and the native adapter performs at most one reviewed wait
slice. Native returns may oversleep; the request bound is not a hard execution
latency guarantee. The clock origin is separate from capture/sample timestamps.

`drive_bounded` checks Stop before a clock observation, after that observation
and before a wait. A pending Stop skips the next native clock/wait operation and
starts cancellation at the last accepted clock instant. Once the owner enters
Stopping, the sticky latch permits only bounded teardown clock/wait advancement,
so a delayed partial drain still reaches its original aggregate timeout. It never
runs another normal lane or refreshes that deadline. Stop between initialization
stages prevents later native acquisitions. The runtime exposes these primitive
C functions:

```c
void ps5_rust_application_request_stop(void);
int32_t ps5_rust_application_stop_requested(void);
int32_t ps5_rust_application_probe(
    int64_t (*lookup)(const char *, void **),
    const uint8_t *secret, uint32_t secret_len);
```

The priority latch is not reset at entry, so pre-entry cancellation is retained.
The C integration checks it before native import/entropy work as well. A normal
Stop gets one aggregate100,000 us protocol-stop deadline. Clock/wait errors and
turn/wait-budget exhaustion cause one terminal abort followed by at most six
cleanup steps using the last accepted instant, with no further native time or
wait calls. Per-resource closes are consume-once. Native elapsed time never
restores credits or proves transfer cleanup.

`MainLoop::new_retaining` returns a preinitialized resource owner on rejected
loop configuration, allowing the caller to close Clock and retain its outcome.
Native acquisition failure unwinds prior actual resources. A partial failure,
Stop during startup, normal expiry and native clock error all have observable
resource outcomes. Required/active/attempted-cleanup/failed-cleanup masks distinguish
unacquired resources from successful close results. Clean protocol closure is separate from native ownership
reconciliation; this candidate creates neither a live connection nor credits.

## Secret and storage invariants

The C entry must supply exactly32 lowercase hexadecimal ASCII bytes derived
from fresh native entropy (128 bits before encoding), through a readable pointer
valid for that call. The Rust entry validates/copies them and never logs them.
A small owned guard uses volatile writes on every normal exit; the caller wipes
its original buffers too. There is no committed or predictable fallback token.
The C entropy acquisition's finite operations, device validation and failure
handling are independently reviewed; its native availability remains Requires
hardware. A missing entropy/import prerequisite prevents entry.

Large fixed backing storage lives outside the small loader stack behind a
non-reentrant Atomic guard. The only concurrently mutable public state is the
Stop latch. Unsafe invariants are: the successful guard grants exclusive access
to the complete static backing, all native owners borrow stable disjoint fields
only within it, the injected exports retain their reviewed signatures/lifetimes,
and no pointer/borrow survives return. Poison is checked again after acquiring
the non-reentrant guard, before copying a secret or touching backing storage;
a stale optimistic precheck cannot authorize reuse. A failed concurrent entry
returns an error.
Cleanup uncertainty poisons subsequent entries in that process; there is no
retry/reset of an unknown native outcome. Certainty aggregates prior HTTP client
close/stop failures, listener cleanup flags, application HTTP errors and MainLoop
cleanup errors together with the final factory report. A later successful
Listener.close cannot erase an earlier uncertain client close. No heap or per-frame allocation,
privilege change, system-driver modification or new exploit is introduced.

## Outcomes and verification

The runtime emits numeric-only counts, stop/failure classifications, factory
primary/cleanup errors and complete native status/lookup bit patterns. It logs
no token, path, address, pointer or private device data. Return0 means a clean
finite control-only diagnostic or clean requested cancellation; return1 means
configuration, startup, drive or cleanup failure. It never means audio support.
The surrounding native experiment markers supply build/run identity and durable
result correlation; missing final markers remain abnormal/interrupted outcomes.

Offline checks:

```sh
cargo test -p bt-runner --locked
cargo test -p ps5-runtime --features application-control-host --locked
cargo clippy -p ps5-runtime --all-targets --features application-control-host --locked -- -D warnings
cargo check -p ps5-runtime --features application-control --target x86_64-unknown-none --locked
```

Portable driver tests cover cancellation before clock/startup, failed clock/wait,
finite turn/wait exhaustion, reverse partial-startup cleanup and the actual store
lease. Runtime host checks use reviewed synthetic clock callbacks with the actual
C clock context, private temporary FileStore, real POSIX loopback Listener,
persistent ServiceState and Rust Application/Runner. Two separately authenticated
TCP clients complete across lending turns and report unavailable capture/transport.
Other scenarios reach the planned duration, stop before clock or between each
startup stage, preserve injected native clock/wait errors, reject an unprotected
store directory and unwind earlier resources. These checks do not run on PS5.

Building/linking this separately selected ELF is necessary but remains candidate
evidence. A fresh authorized console window, exact artifact/run identity, bounded
external stop ceiling, durable log collection and wireless DualSense checks
before/during/after are still required. The baseline diagnostic, capture modes
and SBC modes remain independently selectable. Audio format, owned Bluetooth
routing, real allowances, native audio factory activation and headset acceptance
remain separate gates.
