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

Wireless DualSense input/haptics before/during/after, human-visible notification,
deliberate panic handling and repeated long-run stability require confirmation.
Bluetooth controller identity, native traffic ownership, HCI/ACL credits,
pairing, tone, capture and audio are not implemented by this diagnostic.
