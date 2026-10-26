# Bounded TCP control listener candidate

`bt-listener` supplies an original, allocation-free native POSIX TCP listener
and accepted-client adapter for `bt_control::pump::SocketIo`. The actual host
loopback listener, authenticated HTTP service and connection pump are exercised
together. **PS5 listener activation and native compatibility remain Requires
hardware**. This does not establish Bluetooth ownership, capture, headset
playback or wireless-controller coexistence.

## Features and ownership

The default `no_std` crate contains only project-defined records/constants and
performs no native call. `external-posix` enables the safe Rust owner and declares
a narrow C ABI, without running a host C compiler. A target integration must
separately compile/link and review its matching C implementation. `native-posix`
builds the concrete C candidate against **host system headers**, and refuses a
host/target mismatch. It does not link an SDK runtime, guess syscall numbers or
reuse a host struct layout on PS5. No feature is added to the PS5 diagnostic.

`ListenerStorage<N>::new()` supplies zeroed, 64-byte-aligned caller storage.
`Listener::bind(&mut storage, BindConfig { address, port })` queries C's size and
alignment before borrowing this stable storage. Native sockaddr, socklen_t,
flags and descriptor representations stay entirely in C. The Rust owner is
neither Send nor Sync and cannot outlive its exclusive storage borrow.

The caller explicitly supplies four IPv4 octets and a port. Wildcard, first-octet
zero, multicast/reserved first octets at or above 224, and a zero port outside
`127.0.0.1` are rejected. Port zero is reserved for loopback development tests.
There is no automatic LAN selection, DNS, IPv6, broadcast, socket reuse or
background daemon. `local_port()` returns the bound port verified through
getsockname, allowing the owner to configure exact HTTP Host/Origin policy.
A production listener needs an explicitly chosen LAN address/port and secret;
none is activated by constructing a configuration record.

The C context owns one listener descriptor, backlog argument one, and at most
one accepted client descriptor. The OS may apply its own backlog accounting;
this is not a claim to control every kernel pending connection. The owner must
bound overall listener lifetime and stop it explicitly. Each `accept()` makes
one nonblocking attempt: no client returns `Ok(None)` and a real error retains
its exact errno. There is no hidden sleep, accept loop, thread or retry.

`Client` holds a mutable borrow of `Listener`. While a connection pump owns it,
safe Rust cannot accept a second client, close/move the listener or perform
concurrent native I/O. C also enforces its single-client slot. Native client
identifiers increase monotonically, never wrap, and reject stale/closed tokens;
they are not descriptors and are never exposed on the LAN page.

## Native behavior and cleanup

C creates an IPv4 TCP socket, sets O_NONBLOCK and FD_CLOEXEC while preserving
existing flags, binds, listens and validates the bound address/port. Accepted
sockets are explicitly configured again; portable code does not assume inherited
flags. Configuration failure closes the newly owned descriptor before returning.
This candidate requires a single owner with no concurrent fork/exec between
creation and flag setup; it does not claim atomic accept4/socket flag setup.

SIGPIPE suppression is scoped to the socket: SO_NOSIGPIPE when available, otherwise
MSG_NOSIGNAL on each send. If neither reviewed mechanism is available, setup
fails Unsupported. A failed setsockopt fails setup; it does not silently fall
back to a process-global signal disposition. No implementation code calls
signal or sigaction. recv/send each issue exactly one native I/O operation and
return the actual accepted prefix. EAGAIN/EWOULDBLOCK map to WouldBlock with zero
accepted bytes; EINTR remains an explicit raw error without an internal retry.
Each I/O slice is limited to 65,536 bytes, and short transfers are retained by
the caller's existing connection pump.

`Diagnostic` is a project-defined v1 integer record containing ABI version,
stage, result, immediate raw errno, cleanup errno and a cleanup-unknown flag.
It contains no request, token, secret, address or file data. Native outputs are
initialized before operations; failed bind/accept/I/O returns a zero bound port,
client token or byte count as applicable. Invalid counts and report disagreement
are rejected by Rust; failed reads clear the supplied output slice. These checks
cannot undo a native out-of-bounds write, so C's pointer/capacity contract and
sanitizer checks remain necessary.

Both close paths consume descriptor ownership **before** calling close, and
never retry even on EINTR. Raw close failure and cleanup uncertainty remain
observable. Client close failure poisons the listener against replacement
accepts, while explicit listener close can still consume its remaining resource.
Repeated Rust client close returns the cached result and does not call C again.
Explicit listener close consumes the owner; Drop only provides the final
one-attempt cleanup fallback and cannot report its failure.

A successful native bind with an incoherent report is still evidence that a
context was created: Rust closes it once before returning failure. An incoherent
accept closes the entire listener context, including any accepted client with an
unknown token. Client I/O/report violations make that client terminal until
close. A negative close return cannot be hidden by a dirty success report.
An external implementation violating failed-open ownership cleanup is itself a
broken ABI; post-call checks cannot discover arbitrary unreported resources.

## Connection-pump integration

The owner accepts one client, moves it into a `ConnectionPump` with caller-owned
request/response buffers and a finite monotonic timeout, and services it through
bounded `step()` calls alongside Bluetooth work. The pump's fixed deadline
covers request and response; it retains partial writes and stops on EOF, clock
failure, write failure or timeout. Drop the pump after its terminal outcome to
release the listener borrow, then accept the next client or explicitly close
the listener. HTTP authentication, generation tickets, CSP and protected/public
redaction are described in [LAN-MANAGEMENT.md](LAN-MANAGEMENT.md).

The independently usable [persistent service](LISTENER-SERVICE.md) now retains
HTTP state while this Listener owns the accepted client across short borrows.
It accepts successive clients, recovers ordinary per-client faults, bounds total
accepts/lifetime and processes Stop before accepting or reading. Native PS5
resource/runtime installation remains separate. The original host test binds
only `127.0.0.1:0`, sends an authenticated typed Scan request, runs the real
Client -> ConnectionPump -> service path, verifies a queued ticket and 202
response, and observes socket EOF. Another test expires an idle client's pump,
observes EOF and accepts the next client. A queued command still belongs to the
Bluetooth owner even if the HTTP connection fails after admission.

## Source inspection and licenses

Source/layout review completed 2026-10-26 before the C implementation. No upstream
implementation or header is copied or adapted into these files. They are
original GPL-3.0-or-later code, built with the host's own headers. The source
references below are behavioral/declaration evidence, not PS5 compatibility
results; a future target C build must preserve any selected header notices and
record its actual dependency closure separately.

The fixed SDK reference is `ps5-payload-dev/sdk` revision
`a7685ce8e97ce1f8a4f38253a41d54241c9124f1` (v0.45). Inspected declarations:

- [sys/socket.h](https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/include/freebsd/sys/socket.h): socket/bind/listen/accept/getsockname/recv/send/setsockopt prototypes, SO_NOSIGPIPE and MSG_NOSIGNAL. Its notice is Berkeley three-clause, numbered 1/2/4.
- [netinet/in.h](https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/include/freebsd/netinet/in.h): IPv4 sockaddr fields, TCP protocol and byte-order declarations; Berkeley three-clause, numbered 1/2/4.
- [sys/fcntl.h](https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/include/freebsd/sys/fcntl.h): nonblocking and descriptor flag operations; Berkeley three-clause notice with USL attribution.
- [unistd.h](https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/include/freebsd/unistd.h): close declaration; Berkeley three-clause, numbered 1/2/3.

The comparator is FreeBSD source revision
`d106002967eff712894ab8b5337bbbff106ce5e0`:

- [accept(2)](https://github.com/freebsd/freebsd-src/blob/d106002967eff712894ab8b5337bbbff106ce5e0/lib/libc/sys/accept.2), particularly portable explicit flag setup, nonblocking empty-queue results and interrupted accept.
- [fcntl(2)](https://github.com/freebsd/freebsd-src/blob/d106002967eff712894ab8b5337bbbff106ce5e0/lib/libc/sys/fcntl.2), descriptor/status flag access and nonblocking I/O.
- [getsockopt(2)/setsockopt(2)](https://github.com/freebsd/freebsd-src/blob/d106002967eff712894ab8b5337bbbff106ce5e0/lib/libc/sys/getsockopt.2), per-socket SO_NOSIGPIPE behavior.
- [send(2)](https://github.com/freebsd/freebsd-src/blob/d106002967eff712894ab8b5337bbbff106ce5e0/lib/libc/sys/send.2), short transfer/error semantics and lack of a peer-delivery guarantee.

These manuals retain Berkeley three-clause notices numbered 1/2/4. They are not
bundled source inputs. SDK declarations do not establish callable PS5 symbols,
process permissions or native behavior. C uses sin_len on known Darwin/FreeBSD
builds; another reviewed BSD target must explicitly select
`PS5_BT_LISTENER_SOCKADDR_HAS_LEN` if its compiler does not identify that layout.
Rust never invents or duplicates the sockaddr field offsets.

## Development checks and activation gates

```sh
cargo test -p bt-listener --features native-posix
cargo test -p bt-listener --features external-posix
cargo clippy -p bt-listener --features native-posix --all-targets -- -D warnings
cargo clippy -p bt-listener --features external-posix --all-targets -- -D warnings
cargo check -p bt-listener --features external-posix --target x86_64-unknown-none
python3 tests/control/run.py
python3 tests/control/run.py --sanitize
```

Four native tests cover actual loopback listener/client lifecycle, repeat close,
accepted EOF, fresh client reuse, invalid preflight, the real authenticated HTTP
pump, and idle deadline cleanup. Five external-ABI tests supply synthetic native
functions for dirty successful bind, dirty accept, invalid I/O count, false
close-success reports and poisoned replacement acceptance. Compile-fail checks
cover thread ownership and overlapping accepted-client borrows. C conformance
injects each setup stage, short I/O, EINTR, WouldBlock, invalid counts, stale or
exhausted tokens, descriptor zero and interrupted close. A separate child sets
SIGPIPE's default disposition and writes to a disconnected local socket; it
must receive EPIPE rather than terminate. The implementation never changes the
global signal handler. The harness has a 30-second run ceiling and uses no
console or LAN target; its only TCP bind is `127.0.0.1:0`.

These checks establish host behavior and the project-owned ABI. They do not
establish a PS5 linked listener, browser interoperability on a console, target
export resolution, permissions, kernel resource bounds, usable Bluetooth/audio
activation or coexistence. Those require a reviewed target adapter/link and a
currently authorized bounded hardware experiment, with stop/cleanup and durable
outcome evidence. The established diagnostic remains independently selectable.
