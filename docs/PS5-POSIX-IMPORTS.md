# Selected POSIX exports for the native application candidate

Status: source/layout reviewed, host-fault tested and target linked. Native
export availability, ABI behavior, filesystem protection, entropy and socket
permissions remain **Requires hardware**. No console was contacted. This bridge
is selected only by the separate application-control build and does not change
the baseline, transport ownership, credentials, system driver or dispatchers.

## Narrow implementation and lifetime

`platform/ps5/application/posix_bridge.h` exposes only project-defined primitive
records/callbacks. Its independent ABI value is `0x00010000`; the 48-byte report
retains stage, project status, native errno, full resolver/raw return, resolved
count and separate cleanup flags/errno. There are no native structures, paths,
keys, tokens or pointers in that report.

`posix_imports.c` derives native function-pointer types from the selected target's
own headers. Init resolves all 23 fixed exports through the existing current-
process resolver before any descriptor acquisition. A failed or dirty/null
resolution clears the local candidate table and returns failure. No callable
pointer from a failed lookup is used. The errno provider must return a valid
current-thread slot. The same single thread and export/errno lifetimes must
outlive every factory resource; no callback may race or reenter the bridge.

| Selected exports | Narrow existing use |
| --- | --- |
| `__error` | preserve immediate native errno in existing C adapters |
| `open`, `openat`, `fcntl` | fixed store/entropy paths, reviewed native flags and descriptor setup |
| `socket`, `bind`, `listen`, `accept`, `getsockname`, `setsockopt`, `recv`, `send` | existing bounded IPv4 TCP listener only |
| `close`, `read`, `write` | consume owned descriptors and bounded records/entropy |
| `fstat`, `fstatat`, `fchmod`, `flock`, `fsync`, `renameat`, `unlinkat`, `geteuid` | existing protected bond-store implementation |

There is no symbol scan, alternate guessed API, dynamic command supplied by
HTTP, new syscall number or module loader. Each export name has a declaration
in the inspected SDK; this does not prove it is exported or behaves compatibly
on PS5. Missing functions fail before resource acquisition. The original TCP
and store implementations remain unchanged: a preincluded project redirect
header binds their existing calls to this table. Native `stat`, sockaddr,
flags and errno values remain private to C, never copied into Rust.

`fcntl` admits exactly the four operations used by the existing listener:
F_GETFL/F_SETFL/F_GETFD/F_SETFD. Getter calls do not read a nonexistent variadic
argument; setter calls forward the promoted integer. Open/openat read a mode
argument only with O_CREAT. `memchr` is an original bounded byte loop; the
existing runtime supplies memcpy/memset/memcmp. There is no imported target libc
or proprietary import library. Target objects retain only the explicit project
bridge and those byte-memory dependencies.

Shutdown clears cached pointers only after the caller has consumed every
factory resource, including separately recorded uncertain cleanup. It acquires
or releases no native descriptor itself. Successful initialization cannot be
repeated in the same payload process, including after shutdown. It cannot be
used as an invisible reconnect/reset mechanism. A caller violating the lifetime
contract is an integration error; an erased function table is no cleanup proof.

## Source/layout and license review

The SDK source revision remains
`a7685ce8e97ce1f8a4f38253a41d54241c9124f1` (v0.45). Exact files/hashes for the
43-header dependency closure are in `tools/application_sdk_headers.json`.
The build checks the actual dependency union against this inventory. It uses
Clang resource headers and the pinned target headers, never host native layouts.
No selected SDK header is modified. The existing SDK subset is not expanded
with stock CRT, permission patches, libc, binaries or new kernel code.

Top-level errno.h/fcntl.h and stdint.h are upstream header links. Standalone
provisioning expands them to the original sys/errno.h, sys/fcntl.h and
sys/stdint.h bytes respectively, preserving their complete original notices.
The project preincludes target headers before generic POSIX feature macros so
pinned FreeBSD cdefs selects its documented default BSD visibility for flock,
S_ISVTX and native types. No flag value or structure layout is invented.

The reviewed headers retain their per-file Berkeley/USL, Mike Barcroft,
NetBSD, WIDE Project, FreeBSD Foundation and other original notices. They are
not uniformly licensed by the SDK root. Several previously selected type
headers and the additional x86/_align.h retain advertising clauses; two-clause
and three-clause variants coexist. The source package retains every selected
header verbatim, including those differing obligations. Root GPL text never
replaces header notices. New bridge/redirect/test code is original
GPL-3.0-or-later. These materials are a recorded source selection, not legal,
patent or platform compatibility clearance.

The SDK declarations inspected include errno.h/sys/errno.h, fcntl.h/sys/fcntl.h,
unistd.h, stdio.h, sys/file.h, sys/stat.h, sys/socket.h and netinet/in.h, with
all target type/endian/cdefs dependencies retained by the inventory. Prior
behavior/layout reviews remain in BOND-FILE-STORE.md and CONTROL-LISTENER.md.
No whole upstream application is imported.

## Ephemeral authorization secret

The candidate does not use a source-code token, log a token or fall back to a
predictable value. A single open of `/dev/random` requests read-only,
O_NOFOLLOW/O_NONBLOCK/O_CLOEXEC flags and must identify a character device.
At most 16 progress-making reads collect exactly 16 bytes; zero, oversized
claims, EINTR, WouldBlock or any error stop without an internal retry. One
owned close is attempted even after failure/cancellation; uncertain close
prevents token use. Only after clean close are the 16 bytes encoded as 32
lowercase hex characters for the HTTP bearer policy. All secret temporaries
are scrubbed with volatile writes and never enter status, logs or reports.

The behavioral reference is FreeBSD source
`d106002967eff712894ab8b5337bbbff106ce5e0`,
[sys/dev/random/randomdev.c](https://github.com/freebsd/freebsd-src/blob/d106002967eff712894ab8b5337bbbff106ce5e0/sys/dev/random/randomdev.c):
its two-clause Mark R V Murray notice and nonblocking unseeded failure behavior
were inspected. This is reference only; no RNG implementation is copied.
The PS5 node/entropy/permission behavior remains unknown. Failure is a concrete
prerequisite failure, never permission to weaken authentication or read another
unreviewed source. This is no entropy-quality measurement or claim about a
specific PS5 RNG implementation. The candidate binds loopback only; explicit
LAN configuration/authorization remains a separate integration step.

A synchronous cancellation callback is checked before/after resolution and
between entropy operations. Stop before open issues no descriptor call. Stop
after open still consumes the owned close and wipes buffers. Cancellation is
separate from native failure and cleanup uncertainty. These are finite operation
budgets, not proof a native kernel call can be preempted or cannot oversleep.

## Offline checks and later execution

`tests/application/posix_imports_test.c` uses synthetic exports and explicitly
simulates fresh processes between cases. It tests every missing/dirty/null
export, full-width resolver status, null errno, reentry, use before/after active
lifetime, finite partial entropy reads, wrong node type, short/error/oversized
reads, interrupted close, simultaneous primary/cleanup failure and Stop during
resolution/entropy. C ASan/UBSan pass. No entropy or console device is opened by
that harness. Target source/header compilation and final ELF link have passed;
there are no unresolved native imports or target libc requirements.

An eventual application run still needs a current authorized console window,
the established loader resolver and callable export lifetimes, an existing
private 0700 `/data/ps5-bt/control` directory and a finite experiment budget.
The candidate creates no directory or privilege change and exposes no secret.
Save exact identity-bearing logs and check wireless input before/during/after.
Unknown transport/capture/credits stay gated even if local resources succeed.
