# Native L2CAP availability probe

Status: independently selected diagnostic, 2026-10-25. PS5 execution and native L2CAP
availability remain **Requires hardware**. Build and test results must be
recorded separately before deployment; this document is not an execution claim.

## Purpose and architecture decision

The next useful transport question is whether this process can create a native
L2CAP channel socket. A working native L2CAP route could let the system retain
HCI commands, event delivery, connection handles and controller credits while
Rust owns SDP, AVDTP, RTP/A2DP and audio policy. This directly serves headset
connection and conservative SBC playback. It does not require forcing every
PS5 adapter through the existing raw-HCI `BluetoothTransport` ABI v1.

This diagnostic checks socket availability only. It does not discover a headset,
connect, pair, start audio or prove controller association. Successful creation
would justify review of a separate native L2CAP adapter boundary, not activation
of the current HCI backend. `BluetoothTransport` remains Unsupported.

Creating and immediately closing an unbound socket is distinct from submitting
an HCI command or competing for USB input. It still allocates kernel state, so
it is a bounded availability experiment rather than a claim of zero side
effects. The FreeBSD source below motivates its narrow scope; the PS5 kernel's
implementation and behavior remain **Unknown**.

## Fixed source review and provenance

The SDK reference is `ps5-payload-dev/sdk` revision
`a7685ce8e97ce1f8a4f38253a41d54241c9124f1` (v0.45). Its root license is
GPL-3.0-or-later; imported headers retain their own notices.

| Reference | Reviewed fact | License of referenced file |
| --- | --- | --- |
| [SDK socket header][sdk-socket] | `socket(int, int, int)`; `AF_INET=2`, `AF_BLUETOOTH=36`, `SOCK_DGRAM=2`, `SOCK_SEQPACKET=5` | Berkeley three-clause notice, numbered 1/2/4 |
| [SDK Internet header][sdk-in] | `IPPROTO_UDP=17` | Berkeley three-clause notice |
| [SDK Bluetooth socket header][sdk-btsocket] | `BLUETOOTH_PROTO_L2CAP=135` | Maksim Yevmenkin BSD two-clause notice |
| [SDK unistd header][sdk-unistd] | `close(int)` returns `int` | Berkeley three-clause notice |
| [SDK errno header][sdk-errno] | `int *__error(void)` and the source-declared error constants | Berkeley/USL three-clause notice, numbered 1/2/4 |
| [SDK libkernel symbol inventory][sdk-kernel] | Names `socket`, `close` and `__error` | SDK root terms; symbol inventory is not an implementation |

The behavioral comparator is `freebsd/freebsd-src` revision
`d106002967eff712894ab8b5337bbbff106ce5e0` (`releng/11.4`):

- [L2CAP implementation][fb-l2cap], BSD two-clause: attach (lines 1873-1971)
  reserves buffers, allocates/registers a PCB and sets it CLOSED. Close calls
  disconnect; disconnect/detach (1850-1853, 2274-2347) send a disconnect request
  only when state requires it. Incoming listener selection (2577-2580) requires
  `SO_ACCEPTCONN`, which plain socket creation does not set.
- [Generic socket syscall implementation][fb-syscalls], Berkeley three-clause
  notice numbered 1/2/4: `kern_socket` (131-167) creates the local socket through
  `socreate`. `SOCK_NONBLOCK` causes an internal `FIONBIO` operation, so this
  probe uses no creation flags. The flag would not provide a hard syscall
  deadline anyway.
- [Raw HCI implementation][fb-raw], BSD two-clause: attach (848-891) registers
  default Command Complete/Status filters. Input (448-515) duplicates matching
  packets into socket queues. This is useful comparator evidence, but raw HCI
  creation is outside this experiment and does not prove a PS5 broker.

The references establish declared signatures, constants and FreeBSD behavior.
They do not establish PS5 export availability, protocol registration, security
policy, native controller attachment or equivalence to FreeBSD. No upstream
implementation, header or stub code is vendored, compiled or linked for this
new diagnostic. Existing bootstrap/runtime provenance remains documented in
[UPSTREAM.md](UPSTREAM.md) and [RUNTIME-REVIEW.md](RUNTIME-REVIEW.md).

## Exact operation and stop rules

Resolve the three declared functions through the already reviewed bootstrap
resolver before creating any socket. A failed resolver status, NULL symbol,
missing errno accessor or invalid errno storage stops the test without a socket
call. Do not use guessed syscall numbers, addresses, alternate API names or
new module loading.

The only socket sequence is:

1. Create one control socket with `socket(2, 2, 17)` (IPv4 UDP). Capture the raw
   return and errno immediately, before logging or any other native call. A
   negative return stops the test; Bluetooth creation is not attempted.
2. For every nonnegative control descriptor, including **fd 0**, consume its
   ownership and attempt `close(fd)` exactly once. Capture close return and
   errno immediately. Continue only after successful close.
3. Create one Bluetooth socket with `socket(36, 5, 135)` (native L2CAP). Capture
   return and errno immediately. A negative return is terminal; no retry or
   alternate family, type or protocol is attempted.
4. For every nonnegative Bluetooth descriptor, including **fd 0**, consume its
   ownership and attempt `close(fd)` exactly once, recording return and errno
   immediately. Stop afterward.

The UDP control separates a general socket/export/process-access failure from
a Bluetooth-specific failure. It has no peer and transmits no datagram. At most
one diagnostic socket is owned at a time. A successful call's errno can be
stale; report errno as unknown for successful calls and capture it only on failure.
Record raw statuses and whether each stage ran; do not manufacture errno for an
uncalled stage or infer successful cleanup from process return alone.

A close failure, including EINTR, makes final resource disposition **uncertain**.
The diagnostic must not retry a possibly consumed descriptor, create the next
socket, or silently report clean cleanup. Resolve/log/report failures cannot
skip an already owned close attempt. Descriptor ownership is never inferred
from truthiness, and no loader-owned descriptor may be closed.

There is no `bind`, `connect`, `listen`, `accept`, `send`, `recv`, socket read or
write, `ioctl`, `setsockopt`, raw HCI socket, Netgraph request, USB endpoint
operation, permission/credential change, reset, detach, fallback or retry.
Existing bounded durable logging and notification are separate bootstrap
facilities; they do not use the diagnostic sockets.

The expected socket calls are local and immediate. **15 seconds is the external
observation ceiling, not an in-kernel hard timeout or a promise of cancellation
while a native call is blocked.** If the end notification/log is absent, do not
launch again; retrieve evidence and establish the process/resource outcome.

## Separate artifact and hardware run

The independently selected build is:

```sh
make ps5-native-l2cap-probe
```

The ELF stem is `ps5-bt-native-l2cap-probe`, with matching manifest, map and
inspection/disassembly reports under `build/ps5/`. Its variant must be explicit
in the manifest; baseline and existing diagnostic artifacts remain separately
selected. Build/host checks establish neither socket availability nor controller
coexistence on PS5.

Use a separate PayloadManager local metadata slot named
`ps5-bt-native-l2cap-probe`, targeting `ps5-bt-native-l2cap-probe.elf`. Validate
that exact metadata and the clean-source artifact identity before uploading;
read back and verify the remote ELF before its manual launch. Do not overwrite
another test's slot or launch through the upload operation. The deployer must
not silently create or repair mismatched metadata.

Before launch, preserve the previous `/data/ps5-bt-hw1.log`, record the ELF hash,
manifest/source revision, console/firmware and current wireless DualSense state.
Launch once, observe input during the test, then retrieve the new durable log
before another candidate runs. Require start/end markers, resolver results,
control and Bluetooth stage outcomes, close attempt/results and final outcome.
Confirm normal wireless input afterward. Stop on a controller anomaly,
incomplete outcome or uncertain cleanup. None of these checks constitutes a
headset interoperability test.

## Result branches and next evidence gate

| Observed result | Interpretation and next step |
| --- | --- |
| Resolver/errno boundary unavailable, or UDP creation fails | General call-path/process availability is unresolved. Audit the precise symbol, raw status and immediate errno before another candidate. Do not interpret this as Bluetooth absence. |
| UDP close fails | Cleanup is uncertain; Bluetooth stage must be skipped. Establish disposition before any repeat. |
| UDP succeeds and closes; Bluetooth fails with `EAFNOSUPPORT`, `EPROTONOSUPPORT` or `ESOCKTNOSUPPORT` | This requested native socket is unavailable through the current process and call path. Preserve the exact error; do not scan alternatives or infer that all PS5 Bluetooth services are absent. |
| Bluetooth fails with `EPERM` or `EACCES` | A permission boundary was encountered. Require concrete source/process evidence and a separately reviewed narrow experiment; no automatic elevation or security-policy change. |
| Other Bluetooth failure | Preserve the original errno and stage. Resource errors and ABI/process errors require their own diagnosis; no invisible retries. |
| Bluetooth creates and closes successfully; controller remains normal | Socket availability only. Next review must establish the native controller route, exact L2CAP sockaddr/options/MTU/record semantics, security/pairing behavior, cancellable nonblocking connection/cleanup and coexistence. Prepare a separately named bounded channel experiment only after that review. |
| Any close failure, incomplete log or controller anomaly | Outcome or cleanup is uncertain. Stop and resolve it; do not promote capability or run another candidate. |

A later native-channel adapter needs an explicit ABI and ownership contract.
Keep protocol integrity errors observable and fail closed on unknown native
delivery. Native L2CAP may remove the need for this payload to own HCI credits;
it does not waive connection, cancellation, pairing or coexistence evidence.
Direct USB/raw-HCI routes still require their own demonstrated ownership and
credit contract. No result here authorizes an HCI command, headset connection
or playback automatically.

[sdk-socket]: https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/include/freebsd/sys/socket.h
[sdk-in]: https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/include/freebsd/netinet/in.h
[sdk-btsocket]: https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/include/freebsd/netgraph/bluetooth/include/ng_btsocket.h
[sdk-unistd]: https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/include/freebsd/unistd.h
[sdk-errno]: https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/include/freebsd/sys/errno.h
[sdk-kernel]: https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/sce_stubs/libkernel.c
[fb-l2cap]: https://github.com/freebsd/freebsd-src/blob/d106002967eff712894ab8b5337bbbff106ce5e0/sys/netgraph/bluetooth/socket/ng_btsocket_l2cap.c
[fb-syscalls]: https://github.com/freebsd/freebsd-src/blob/d106002967eff712894ab8b5337bbbff106ce5e0/sys/kern/uipc_syscalls.c
[fb-raw]: https://github.com/freebsd/freebsd-src/blob/d106002967eff712894ab8b5337bbbff106ce5e0/sys/netgraph/bluetooth/socket/ng_btsocket_hci_raw.c
