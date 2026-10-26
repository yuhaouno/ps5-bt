# Persistent bounded HTTP listener service

Status: implemented and tested offline, including sequential real loopback
connections. PS5 socket imports, listener permissions, wake behavior and LAN
operation remain **Requires hardware**. No Bluetooth capability or playback
support is inferred from the service.

## Concrete ownership and lending

`bt_listener::ServiceState` now drives the existing authenticated HTTP service
through one concrete native `Listener`. Each step either accepts one client or
performs one existing connection read/write step. When that client finishes or
fails with known cleanup, its socket is consumed and the next step may accept
another. The service never holds a borrowed `Client` inside its own listener.

`bt_control::pump::ConnectionState` contains only request/response buffers,
parser/write cursors, fixed deadline and terminal outcome. It accepts a borrowed
`SocketIo` for one synchronous step. The original `ConnectionPump` still owns
one socket and wraps that same state, preserving its public API and exactly-once
close behavior. A live ConnectionState cannot be restarted to extend its
deadline or replace a retained partial response. Its explicit begin/reset is
allowed only after terminal cleanup; dropping it scrubs buffers and performs
no guessed I/O.

The native accepted token remains in a private managed slot inside `Listener`.
That slot owns the client across calls. Each service quantum temporarily lends
a regular borrowed Client adapter, uses the existing native read/write/close
checks, then restores the slot if the client is still active. Normal client
close consumes the slot once; listener close/drop consumes both native
resources when needed. No pointer to a movable Rust owner, unsafe self-reference,
unbounded allocation or detached native token is introduced.

Two stable, exclusively borrowed HTTP buffer addresses identify the service's
managed slot internally. A different live service cannot borrow those same
buffers through safe Rust. The identity is compared before every managed
operation and is never logged or exposed over HTTP. A mismatched service cannot
read, close or take over another service's client. The original `Listener::accept`
API also refuses to extract a managed client. Closing/replacing a Listener while
a service is active is a lifecycle error: stop/cancel the service before the
factory consumes or reconstructs its listener.

The enclosing factory retains Listener ownership. Always call
`service.cancel(&mut listener)` before abandoning ServiceState while retaining
the listener. Because the state retains no listener borrow, its Drop cannot
perform native cleanup or return errors. If it is dropped early, Listener still
owns the bounded managed slot and explicit listener close/drop cleans it. That
fallback is not an observable service cleanup success; explicit results remain
necessary.

## API and ApplicationLoop integration

The service is available with `bt-listener/external-posix` or `native-posix`:

```text
ServiceState::new(ServiceConfig, request_buffer, response_buffer)
ServiceState::step(&mut Listener, now_us, Policy, &mut Control, &mut Manager)
    -> Result<ServiceProgress, bt_control::pump::Failure>
ServiceState::request_stop()
ServiceState::cancel(&mut Listener) -> Result<(), Failure>
ServiceState::snapshot() -> ServiceSnapshot
```

`ServiceProgress` exposes `progressed`, `wake_at_us` and `stopped`. An
ApplicationLoop HttpWork adapter lends the factory listener and ServiceState
for one network quantum, maps those progress/wake fields to Quantum, and
forwards cancel. No new dependency from bt-listener to bt-runner is required.
Application, manager, HTTP buffers and native resources survive between these
short borrows; a fresh ConnectionPump is not reconstructed on each tick.

The configuration requires a nonzero fixed `request_timeout_us` and a nonzero
`max_connections`, plus an optional absolute total `deadline_us`. The connection
budget counts actual accepts and is never reset by per-client failures. The
last admitted client may complete before exhaustion stops new accepts. A total
service deadline cancels an active client and stops the service. Both request
and service deadlines use the owner's host monotonic microseconds, independently
of native audio timestamp units. Each native call must still return: a Rust
budget cannot preempt a hung foreign call.

A partial header, WouldBlock or partial write does not extend the request's
original accept-time deadline. The suggested wake is the earlier active request
or service deadline. The enclosing MainLoop still bounds waits and work quotas;
there is no internal native retry/sleep loop, background thread or readiness
API guessed by this crate. Accept consumes a separate quantum from request I/O,
so Stop/Cancel can run between connection admission and parsing buffered bytes.

## Errors, Stop and command ownership

Ordinary per-client EOF, malformed-count/I/O error, request timeout or response
failure closes that client using the existing ConnectionState contract. If
cleanup is known, the service records the failure and returns normal progress
so the application's HTTP lane can accept another client. It never leaves a
recoverable request timeout as a permanently disabled HTTP listener. The
application's Bluetooth owner still controls a command already admitted by
HTTP: a failed reply, timeout or HTTP cancellation cannot undo its ticket.

Native listener/accept failure, clock regression, mismatched listener ownership
and uncertain client cleanup latch service stop. Cleanup errors remain in the
returned Failure and the sticky snapshot; no replacement accept or close retry
can hide them. The factory separately closes its remaining listener resource
and preserves that distinct cleanup outcome. Raw native Diagnostic is retained
for listener failures; no request body, bearer secret or native descriptor is
included.

`request_stop()` latches without a clock or native call. The next step checks it
before accepting or reading a ready mutating request. `cancel()` latches first,
consumes the active client once and forbids later accepts/reads/writes. A clean
explicit cancellation is normal service shutdown; its original Cancelled client
outcome is still visible. Repeated cancel and later steps retain cleanup failure
rather than issuing another close. HTTP Stop and the application's audio/ACL
stop are distinct owners: this service never discards a partially sent media
PDU or changes HCI credits.

ServiceSnapshot exposes accepted/completed/failed client counts, active/stopped
state, stop reason/result, last client outcome, last failure and listener error.
Completed means an HTTP connection finished, including an authentication error
response; it does not mean a requested operation succeeded. Last failure remains
visible after a later successful connection. Buffers are scrubbed after terminal
connection cleanup and when the state drops.

## Scope, source and verification

This is original safe Rust scheduling around the existing reviewed native
listener and HTTP parser. No native API, layout, socket constant or upstream
component is added. Existing v1 C ABI, single-client limit, native lifecycle
checks, source/license review and host/target feature gates remain as documented
in [CONTROL-LISTENER.md](CONTROL-LISTENER.md). Authentication, Host/Origin checks,
request/response bounds, command tickets, redaction and static page policy
remain those in [LAN-MANAGEMENT.md](LAN-MANAGEMENT.md).

```sh
cargo test --locked -p bt-control --test pump
cargo test --locked -p bt-listener --features native-posix
cargo test --locked -p bt-listener --features external-posix
cargo clippy --locked -p bt-listener -p bt-control --all-targets --features bt-listener/native-posix -- -D warnings
cargo clippy --locked -p bt-listener --all-targets --features external-posix -- -D warnings
cargo check --locked -p bt-listener --target x86_64-unknown-none --features external-posix
```

Real loopback tests cover successive clients after an authentication rejection,
fixed partial-header timeout followed by reaccept, command ownership after reply
timeout, Stop before buffered mutation, total deadline/clock cancellation and
factory cleanup of an abandoned managed client. Synthetic ABI faults cover dirty
accept, failed/dirty client close, begin-time deadline overflow, refusal of a
second service, Stop-before-accept and sticky consume-once cleanup. Existing
ConnectionPump partial-write, malformed-input and lifecycle checks remain, with
an added reusable external-state/deadline regression.

Tests make finite host-only loopback attempts with separate test deadlines.
They establish no PS5 socket link/permission result, console listener longevity,
network exposure or wireless-controller/audio acceptance. Native PS5 activation
still requires reviewed imports, explicit configuration and the authorized
bounded hardware window.
