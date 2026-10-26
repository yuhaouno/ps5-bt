# Runtime review: minimal-syscall-v6

Reviewed 2026-10-24. Baseline and explicit loader-kernel-resolver builds retain
the project entry/linker and fixed SDK v0.45 revision
`a7685ce8e97ce1f8a4f38253a41d54241c9124f1`. See
[HW1-REPAIR.md](HW1-REPAIR.md) for the hardware result and limitations.

## Entry and syscall contract

The trusted pre-existing loader supplies the SDK payload argument structure,
callable first-field callback/veneer, live pipe/socket descriptors, kernel data
base and writable payloadout. Their lifetime spans this synchronous call.
They are not arbitrary untrusted pointers. Invalid/null arguments return 120.

The entry preserves the caller stack/frame, aligns the stack and clears BSS.
It probes the supplied callback as the fixed SDK does. A callback that resolves
sceKernelDlsym to another public entry is normalized in a local argument copy;
a getpid veneer leaves the output null and passes unchanged to the SDK.
The SDK selects its documented +0xa syscall entry; no alternate offsets are
searched. **The runtime does not read libkernel code bytes.** A readable-code
assumption was invalid on the recorded hardware: byte-read probes terminated
before their hold, while using the supplied veneer produced persistent logs.
Execute-only text is the inferred explanation, not a dumped firmware finding.

The unmodified SDK syscall bridge is wrapped by project
`bootstrap/syscall_checked.S`. The wrapper captures carry immediately, preserves
all six arguments and call-stack alignment, and returns raw positive errno
failures as negative errors. mmap errors retain MAP_FAILED (-1) for SDK callers.
A synthetic x86-64 executable checks all six arguments, carry-set positive
errors, carry-clear negative returns, stack alignment and mmap failure.
It runs on macOS through available x86-64 execution and on x86-64 Linux.

## Optional existing-loader kernel resolver

`make ps5-hw1` or `build_ps5.py --loader-kernel-resolver` explicitly includes
unmodified SDK `crt/kernel.c` and `crt/nid.c`, plus their hash-verified headers
`kernel.h`, `kernel_iommu.h`, `elf.h`, `nid.h`, `klog.h`.
Each retains John Törnblom's GPL-3.0-or-later notice (2024; kernel_iommu.h 2026).
No stock CRT, patch.c, runtime module loader or proprietary binary is linked.
Hashes are enforced by tools/build_ps5.py and included in the manifest/bundle.

The runtime first opens the persistent log, records kernel initialization begin,
calls `__kernel_init(args)`, and records its exact result. Failure returns 128
without entering Rust. On success, failed ordinary export lookup may fall back
to `kernel_dynlib_dlsym(-1, handle, name)` for the current process only.
This uses the already-supplied pipe/socket kernel-R/W transport. It does not
establish initial kernel access or mutate process credentials/syscall bounds.
Kernel setters and __patch_init are rejected by the ELF inspector in both modes.
This describes the current diagnostic variants, not a permanent ban on reviewed
minimal own-process permission adapters under AGENTS.md. A future adapter needs
a separately selected build, explicit import/ABI/layout review and PS5
validation; do not weaken baseline inspection or import stock patch.c silently.

### Unsafe invariants and hardware scope

The SDK borrows rwpipe/rwpair, kpipe_addr and kdata_base_addr. Pointers must
describe the installed loader's valid, still-open transport. Reads use that
transport, including writes to its pipe/socket control state; this is not a
general promise of zero kernel writes. Descriptors are never closed by us.

The pinned source supplies the firmware table and layouts. For 13.40 its
allproc address is data-base + 0x28C9E80; process traversal reads pid at +0xBC,
credentials at +0x40 and vmspace at +0x200. Dynamic-loader metadata follows
the exact dynlib structures in pinned kernel.h. Init also derives DMAP/IOMMU
state using pinned helpers. These are SDK invariants, not invented offsets.
No UID/authid/capability/syscall-bound setter is called. Other firmware or
loaders remain Requires hardware even if the SDK has a table entry.

The source trusts kernel dynlib sizes/pointers and traverses kernel structures;
host mocks cannot prove these lifetimes or compatibility. The recorded
Slim/13.40 environment initialized this path and resolved callable write,
_exit, notification and sysctlbyname exports. That validates this tested path,
not every SDK helper, malformed-kernel behavior or firmware layout.

## Lifecycle and results

The fresh per-run /data/ps5-bt-hw1.log is opened before export lookup.
It records initialization, startup, Rust entry, primary exit and final status.
The raw lookup failure is logged with symbol and signed result. Start/finish
notifications use the fixed reviewed notification ABI; API success is distinct
from human confirmation of on-screen display. The file is closed on completion.

A missing stderr is recorded as
`event=stream_log_unavailable fallback=persistent`. A healthy persistent log
allows the diagnostic's expected Unsupported result 2 to remain final.
Persistent open/write/close failure remains 126; notification failure remains
127. Early initialization/resolution failures retain their primary status.
120 invalid args; 121 SDK syscall init; 122 write lookup; 123 _exit lookup;
125 is reserved from the removed byte guard; 128 loader kernel init.
124 is a legacy stream-only status; durable failure takes precedence as 126.
The final value is stored in loader-owned payloadout.

Tests cover ordinary and forwarding callbacks, resolution errors, stream/file/
notification failures, synthetic kernel-resolver fallback and init failure.
Optional native-query tests remain separate. The hardware baseline completed
with primary/final 2; the native query resolved sysctlbyname, called the single
OID once, returned -1 and preserved final 2. No errno was captured by that helper.

## Remaining gates

The user confirmed visible notifications and normal wireless input during the
diagnostics. Haptics workload, deliberate panic handling and long-run stability
remain untested. The baseline still has no Bluetooth transport or capture.

The separate USB metadata diagnostic uses only read-only SDK-defined metadata
ioctls. The capture-only module helper accepts a nonnegative current-process
module handle, never a PID or arbitrary address, and falls back to the same
audited SDK NID resolver with PID fixed to -1. It clears failed output pointers
and performs no credential or permission setter. Main passes only the handle
returned for its own loaded Avcap2 module. Four host startup variants exercise
this helper's ordinary, null, dirty-failure and kernel-fallback paths.

The capture preflight loads already-installed Ipmi then Avcap2 through a
resolved libkernel export; no proprietary module is copied or redistributed.
The one-read variant calls the inspected private capture ABI, whose buffer and
lifecycle invariants are in CAPTURE-PROBE.md. A later optional settling callback
resolves SDK-declared `int usleep(useconds_t)` with a u32 argument and permits
exactly one 100000-us call; lookup/wait failures remain visible and trigger
cleanup without a read. OS calls themselves are trusted synchronous boundaries,
not hard real-time guarantees. Modules are not blindly unloaded.

Own USB descriptors and capture lifecycle have recorded evidence in
HW2-EVIDENCE.md; HW3-EVIDENCE.md adds nonempty settled/bounded reads and a target
generated-signal SBC self-test. Native PCM format/timing, traffic ownership,
HCI/ACL credits, pairing, tone and audio playback remain unverified. A descriptor's
second HCI-class interface is not a proven independent transport.

## Experimental capture-stream selection, 2026-10-26

The independent `PS5_BT_CAPTURE_STREAM` build exposes the identical reviewed
`ps5_bootstrap_module_lookup` implementation already selected by
`PS5_BT_CAPTURE_PROBE`. Its host fault cases run under both flags. It adds no
resolver mechanism, kernel offset, permission mutation, module unloading or
transport ownership. Rust's separately selected one-read capture owner calls
the new opaque capture ABI; the baseline and older diagnostic stay separate.
Native units remain Unknown and execution requires a current hardware window.


## Independent native application-control selection

The explicit `PS5_BT_APPLICATION_CONTROL` selection retains the baseline entry,
loader-kernel resolver, syscall bridge and existing diagnostic logging/notification
limits. It adds no credential/syscall-origin setter, system-driver change or new
kernel offset. Its use of `ps5_bootstrap_module_lookup` is the same reviewed
current-process-only helper; capture constructors remain disabled in this mode.
No radio command, native capture call or SBC encoding occurs.

Before entering Rust, the C entry resolves the fixed POSIX export table and errno
slot against exact pinned SDK declarations. Missing or dirty exports fail before
any descriptor. Entropy uses one read-only, no-follow/nonblocking character-device
open and at most 16 progress-making reads for 16 bytes; failure/Stop still consumes
close once, rejects uncertainty and scrubs temporary secrets. No proprietary import
library, target libc, guessed syscall or alternative unreviewed entropy source is
used. Native API availability/entropy quality remain Requires hardware. Full source,
header/notices and export lifetimes are in PS5-POSIX-IMPORTS.md.

Rust then acquires the concrete cached clock, existing protected FileStore and
persistent listener. The private `/data/ps5-bt/control` directory must already
exist with mode 0700 and reviewed native protections; startup neither creates
it nor treats path existence as durability. An ephemeral loopback bind supplies
the exact Host/Origin policy and a fresh unlogged bearer. This is a control-only
startup probe, not LAN deployment or Bluetooth/audio support. Bounds are fixed in
APPLICATION-STARTUP.md: finite runtime/turn/wait/HTTP/Stop policies and consume-once
reverse cleanup. Kernel calls are trusted synchronous boundaries, not preemptible
hard-time guarantees.

Unsafe Rust invariants are exclusive atomic access to stable disjoint static
buffers, valid export/errno lifetimes on the same native thread, no reentrant
native callbacks, bounded readable secret input and no surviving reference after
return. The atomic entry guard must check poison while owning exclusivity before
borrowing storage. StoreSlot/StoreLease keep moved application state independent
of native resource addresses, and restore the actual store before factory cleanup.
Stop remains a separate atomic latch and cannot be reset at entry. Cleanup certainty
includes prior client-close/HTTP errors and the factory's final results; later close
success cannot certify an earlier uncertain descriptor. Scrub guards cover caller/
owned tokens and HTTP buffers. Uncertainty poisons reuse in the same process.

The builder source/header/ELF inspections and host fault tests establish a reviewable
candidate only. No application-control execution, native persistence, timing or
controller coexistence result follows from earlier HW-1/capture/codec runs. A fresh
explicit hardware window, artifact identity, durable entered/final outcomes and
controller checks before/during/after remain mandatory; no deployment occurs here.
