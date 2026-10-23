# Minimal runtime source review

Runtime identifier: `minimal-syscall-v1`. Reviewed 2026-10-23 for an inspectable
compile/link artifact. **Hardware-untested; no PS5/firmware/loader compatibility
or deployment approval is established.** The current adapter remains Unsupported.

## Scope and dependency closure

The default SDK v0.45 CRT is excluded. `crt/crt.c` unconditionally invokes
`__patch_init`; `crt/patch.c` changes credential capabilities/attributes and
syscall permission bounds. Neither those files, `kernel.c`, kernel-R/W helpers,
the SDK `crt1.o`, SDK libc, nor dynamic SDK import libraries enter this link.
`-nostartfiles` alone is not an alternate runtime; the SDK shared-object example
also needs a normally initialized host executable.

The selected external source is [ps5-payload-dev/sdk](https://github.com/ps5-payload-dev/sdk/tree/a7685ce8e97ce1f8a4f38253a41d54241c9124f1),
revision `a7685ce8e97ce1f8a4f38253a41d54241c9124f1` (v0.45):

| Component | Selection and behavior |
| --- | --- |
| `crt/syscall.c` | Unmodified generic userland syscall adapter and initialization; consumes the loader's existing stub after a strict layout check; no permission or credential writes |
| `crt/syscall.h` | Unmodified declarations and numeric syscall definitions |
| `crt/payload.h` | Unmodified entry argument structure |

Each selected file's SHA-256 is enforced by `tools/build_ps5.py`, including when an external
source checkout is supplied. Missing files may be fetched only from immutable
revision URLs into ignored `.sdk/`. All three files carry John Törnblom's
copyright and GPL-3.0-or-later notices; they are kept intact. They are external
linked build inputs, not a new upstream Bluetooth implementation. No local SDK
patches are applied. A distributed ELF must include the selected corresponding
source, notices, GPL text and project/build sources; a link to GitHub alone is
not a completed source distribution. There is no release distribution yet.

The original `bootstrap/minimal.ld` places the three PT_LOAD segments on 16 KiB
boundaries with RX, R and RW permissions. The SDK script was tested and rejected:
its alignment depends on nonempty unwind and initialized-data sections, absent
in this subset, producing unaligned load starts at 0x13f8 and 0x4120. The original
script is neither patched nor used. No ELF loader is implemented here.

The project-owned `bootstrap/start.S` preserves the caller's frame pointer and
stack, aligns the stack to 16 bytes before a C call and returns its integer
result. `bootstrap/runtime.c` clears BSS explicitly, initializes the selected
syscall adapter, resolves existing libkernel `write` and `_exit` exports using
`SYS_sprx_dlsym` (591), then invokes the existing `main` -> Rust -> C stub seam.
Only the known alternative libkernel handles 1 and 0x2001 are tried, as in the
pinned SDK startup. Resolution failure stops startup; it does not load modules,
alter permissions, retry transport, or access any kernel-R/W argument field.

Logging uses synchronous `write(2, bytes, len)` without retaining the buffer.
A short/failed write is sticky, no write is retried, and normal return reports
124 rather than the Rust result. Panic invokes the resolved `_exit(101)`;
if that unexpectedly returns, a trap stops execution. Panic does not unwind or
claim clean return to the loader. No heap, TLS, C constructors or destructors
are used. Original byte-loop `memset`, `memcpy` and `memcmp` satisfy freestanding
Rust compiler lowering; compilation disables builtin loop replacement.

`bootstrap/bootstrap.h` exposes `ps5_bootstrap_lookup(name, out)` only during
the bootstrap's call to `main`. It performs the same fixed libkernel lookup,
clears the output on every failure, returns the final raw resolver error, and
uses local -2 invalid / -5 unavailable when no raw error exists. It never loads
a module. This C bootstrap helper does not modify the transport ABI or enable
any controller operation. No optional diagnostic query is wired into this ELF.

## Existing loader entry contract

Source inspection, not hardware verification:

* [elfldr v0.27](https://github.com/ps5-payload-dev/elfldr/blob/2a8da002e5292481ea964daa7342bbf4855f4c33/elfldr.c),
  revision `2a8da002e5292481ea964daa7342bbf4855f4c33`.
* [websrv v0.34](https://github.com/ps5-payload-dev/websrv/blob/4d35610914273f8f4fc37af2b1d5bfb069dfba5b/src/ps5/elfldr.c),
  revision `4d35610914273f8f4fc37af2b1d5bfb069dfba5b`.
  Current source `7748df9d2a43822a5ebfda149f43d4f2696d640b` was also inspected.

Their `elfldr_load` maps ET_DYN at an arbitrary base, copies PT_LOAD file bytes,
and applies section-table SHT_RELA / R_X86_64_RELATIVE relocations before entry.
Other relocation kinds are silently ignored, so our inspector rejects them and
preserves section headers. Their malloc-backed mirror does not zero BSS tails;
our entry does so before reading mutable globals.

`elfldr_prepare_exec` puts the argument pointer in rdi and pushes the original
instruction pointer as a return address. `elfldr_payload_args` stores
`payloadout` at byte 0x28. Its first field contains a resolved **getpid syscall
stub**, despite the SDK field name `sys_dynlib_dlsym`; it is not safe to call it
as a resolver by assumption. The unmodified SDK adapter detects the convention
and uses the existing stub at +0xa, exactly as upstream does. Before invoking
the adapter or any supplied callback, our entry accepts only the 12-byte prefix
`mov rax,20; mov r10,rcx; syscall`. The existing stub retains its return/error path;
we do not constrain its next instruction or invent an alternate entry offset.
This strict policy derives from SDK `SYS_getpid` and its +0xa convention;
**actual libkernel bytes, including firmware 13.40, remain Unknown**. A different
prefix stops with 125 before invocation. SDK `libc/syscalls.c` demonstrates
analogous userland syscall veneers, but does not define imported getpid.
The trusted loader must supply a readable code pointer covering these 12 bytes;
the guard is not a memory-safety boundary against a malicious loader. Our
payload adds no syscall instruction, offset discovery, permission bypass,
credential mutation or kernel patch. Existing environment permissions must
already permit symbol resolution and writing fd 2. Missing permission is a
startup failure, not an invitation to add patches.

The loaders themselves set up an already homebrew-capable process using their
own privileged operations. This project neither distributes nor implements
that prerequisite. Source ABI compatibility does not prove those loaders can
be installed or run on a particular firmware. PS5 Slim firmware 13.40 remains
an external environment question; do not present this ELF as user-test-ready
until that prerequisite and the actual loader/runtime are established.

## Results and failure codes

The existing Rust probe returns 2 for the unsupported adapter. New startup codes:
120 invalid loader arguments; 121 syscall initialization failed; 122 no write
symbol; 123 no exit symbol; 124 incomplete/failed log write; 125 unsupported
getpid stub prefix. Normal return also
stores the result in the loader-owned `payloadout`. Raw syscall resolver errors
are not treated as success. No attempt is made to clean up loader-owned kernel
descriptors because the payload never takes their ownership.

## Remaining hardware gates

Inspect exact loader entry registers, supplied stub ABI, ordinary syscall
permissions, resolved exports, stderr routing, stack/callee-saved registers,
Rust log bytes, expected unsupported result and clean return. Panic termination
requires a separate deliberate test. ELF inspection confirms separate RX/R/RW
segments; this is structural evidence, not a hardware W^X measurement.
No Bluetooth operation, controller
information, audio, DualSense coexistence or firmware support follows from this
ELF or desktop tests.
