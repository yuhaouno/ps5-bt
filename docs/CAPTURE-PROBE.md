# Bounded Avcap2 capture probe

Status: host-tested diagnostic component. Parent integration has produced one
successful preflight and one full immediate-read lifecycle on hardware; the new
optional 100 ms wait path **Requires hardware**. This does not establish
playable audio or freeze the future Rust/C audio ABI.

## Integration requirement for the current hardware window

The probe deliberately accepts only two project-owned resolver callbacks. The
parent integration must call it synchronously while runtime
`minimal-syscall-v6` still has its existing-loader kernel resolver initialized:

1. `lookup_current` is the existing `ps5_bootstrap_lookup`. The probe requests
   only `sceKernelLoadStartModule` from the current process's libkernel handles.
2. `lookup_module` is a small parent-owned wrapper around the already-linked
   SDK v0.45 `kernel_dynlib_dlsym(-1, module, name)`. It must clear `*out`, reject
   invalid arguments or an uninitialized resolver, use only the nonnegative
   Avcap2 module handle passed by the probe, and return zero only with a non-NULL
   address. `kernel_dynlib_dlsym` derives the NID from the supplied export name
   using the already-selected SDK `nid.c`; the probe needs no new offsets or NID
   table. This wrapper belongs beside the current bootstrap resolver because
   `kernel_resolver_ready` is private there. A non-kernel-resolver build must
   report unavailable rather than invent another lookup path.
3. Construct `ps5_capture_probe_ops` from those two helpers. The first hardware
   artifact should call `ps5_capture_probe_preflight`, which stops after loading
   and resolving and never enters the private capture ABI. Only after reviewing
   that evidence should a separate explicitly selected artifact call
   `ps5_capture_probe_run`. Encode either result with
   `ps5_capture_probe_encode_report` while the bootstrap log is live. Do not log
   either scratch buffer. Run the probe once; there is no retry or restart.
4. Set the optional `trace` field to the parent's durable lifecycle logger:

   ```c
   void trace(enum ps5_capture_probe_phase phase, uint32_t ordinal,
              int entering, int64_t raw_status);
   ```

   The probe calls it immediately before and after every current/module lookup,
   module load, lifecycle call, read and cleanup call. `ordinal` is zero except
   for the seven Avcap2 resolutions (`0..6` in the order below). `entering` is
   exactly one before and zero after. Entry status is zero; exit status is the
   raw resolver/native return. Successful module loads are traced as zero rather
   than exposing their returned module handles. The callback receives no
   address, native handle, buffer or content. If the process terminates inside a
   native call, the unmatched durable begin marker identifies that phase.
   Setting `.trace = NULL` disables tracing without changing probe behavior.
5. The optional fourth field is
   `int (*wait_before_read)(uint32_t microseconds)`. When non-NULL, the full
   probe calls it exactly once with `100000` after successful Start and before
   ReadAudio. The call receives durable WAIT begin/end trace markers. A nonzero
   return is preserved as `raw_wait_status`, makes WAIT the primary failure,
   skips ReadAudio, and proceeds through Stop/Close/Terminate exactly once.
   NULL preserves the original immediate-read sequence. The callback is
   synchronous and non-reentrant; it must not start a worker or retain state.

Equivalent wrapper logic, for review rather than blind copying, is:

```c
if (!out || !name || !name[0] || module < 0 || !kernel_resolver_ready)
    return a_project_local_failure;
*out = 0;
unsigned long address = kernel_dynlib_dlsym(-1, (unsigned)module, name);
if (!address)
    return a_project_local_failure;
*out = (void *)(uintptr_t)address;
return 0;
```

This is current-process export resolution through kernel read/write already
provided by the installed loader. It does not acquire initial kernel access,
inspect another process, change credentials or syscall-origin permissions, or
add a kernel patch chain. The unsafe loader transport and firmware structure
invariants remain exactly those in `RUNTIME-REVIEW.md`.

Target compilation must retain the reviewed C flags, especially
`-ffreestanding -fno-builtin -mno-red-zone -mgeneral-regs-only`. This source has
no floating-point type, instruction request, sample conversion or SIMD
assumption. The 48 kHz stereo f32 interpretation remains **Requires hardware**.

## Exact bounded operation

`ps5_capture_probe_run` performs one non-reentrant, synchronous sequence:

1. Resolve `sceKernelLoadStartModule` through `lookup_current`.
2. Load `/system/common/lib/libSceIpmi.sprx` first, then
   `/system/common/lib/libSceAvcap2.sprx`. Preserve each signed module return and
   secondary `res` output. A negative module return or nonzero secondary result
   stops the probe at that load phase.
3. Resolve, in order, `sceAvcap2Initialize`, `sceAvcap2Terminate`,
   `sceAvcap2OpenAudio`, `sceAvcap2Close`, `sceAvcap2Start`, `sceAvcap2Stop` and
   `sceAvcap2ReadAudio` from the returned Avcap2 module handle. Preserve the
   first raw resolver failure and the successfully resolved bit mask.
   `PS5_CAPTURE_CALL_RESOLVE_AVCAP2` distinguishes an actual resolver attempt;
   reports emit `module_lookup_status=na` when Avcap2 loading failed before it.
4. Call Initialize once. On exact zero, OpenAudio once with a private internal
   `0x38`-byte block: size `0x38`, unknown `+0x08 = 0x18`, source `2`, mode `0`,
   unknown `+0x1c = 0xffffffff`, all other bytes zero, kind `0`.
5. On exact-zero OpenAudio plus a non-NULL handle, Start once. On exact-zero
   Start, call the optional settle hook once with exactly 100,000 microseconds.
   A nonzero wait result skips ReadAudio and enters cleanup. With no hook, or
   after a successful hook, call ReadAudio once with `READ_NOWAIT = 1`, an
   internal 8,192-byte buffer and internal 16-byte metadata block whose first
   little-endian u32 is initialized to 16. There is no polling or retry.
6. Consume each definitely owned resource once: Stop only after successful
   Start, Close only after successful OpenAudio with a handle, and Terminate
   only after successful Initialize. A failed OpenAudio's dirty handle is not
   assumed owned. Every attempted cleanup status is retained. Cleanup failure
   is `cleanup_uncertain=1` and is never retried blindly.
7. Volatile-wipe the native metadata and audio scratch before returning. No
   sample byte, metadata byte, native handle or private block appears in the
   result or report. Loaded process modules are not unloaded: neither pinned
   source established safe ownership/unload semantics for already-loaded/shared
   modules, and the payload process is expected to return immediately.

The internal static scratch avoids an 8 KiB loader-stack assumption and avoids
heap allocation. Consequently, this probe is explicitly non-reentrant. A nested
call returns `PS5_CAPTURE_PROBE_BUSY` before touching scratch or invoking a
callback. This is not a general capture session and must not be placed in a
background audio loop.

`ps5_capture_probe_preflight` shares steps 1-3 and then returns success with
`capture_invoked=0`, `read_kind=not_called`, and no Initialize/Open/Start/Read/
Stop/Close/Terminate calls. This is the recommended first hardware gate. It
still loads both modules and therefore still depends on their process/module
lifetime behavior.

Trace callbacks are synchronous and must return. They may write fixed metadata
markers through the existing lifecycle logger, but must not allocate, resolve
more symbols, invoke capture, expose addresses, or retain callback arguments.
Trace failure cannot be reported through this callback signature; the parent
logger must use its existing durable-error accounting.

## Hardware evidence to date

Reported from the parent-controlled Slim / firmware 13.40 run on 2026-10-24:

- Preflight loaded Ipmi as module `18` and Avcap2 as module `19`, with both
  secondary `res` values zero, and resolved all seven required Avcap2 exports.
- The full **no-wait** one-read diagnostic returned zero from Initialize, Open,
  Start, Stop, Close and Terminate. Its immediate ReadAudio returned raw
  `0x81950002` (`empty`), so it returned no PCM bytes and stored/dumped none.
- The user confirmed normal console/controller behavior after the run.

This is narrow lifecycle evidence, not proof of the requested PCM format,
capture timing, menu/game mix, repeatability or playable audio. The optional
100 ms settle-hook variant was completed only after the hardware window ended
and **has never run on PS5**. Its behavior remains Requires hardware.

## Units, statuses and claims

The native ReadAudio return is retained as all 32 bits in `raw_read_status` and
reported as eight hexadecimal digits. Known upstream observations are classified
without erasing the raw value:

| Raw u32 | Probe classification | Meaning used here |
| --- | --- | --- |
| positive signed value | `data` or `invalid_length` | bytes returned, never frames |
| `0` | `zero` | zero returned; distinct from the known Empty code |
| `0x81950002` | `empty` | no data in this one nonblocking call |
| `0x81950004` | `overrun` | no bytes returned; discontinuity set |
| `0x81950008` | `ended` | terminal native failure for this probe |
| `0x81950001` | `invalid` | native failure; no restart or credential fallback |
| any other negative value | `error` | raw unknown native failure |

Only a positive result no greater than 8,192 and divisible by eight is accepted
as complete frames under the **requested but unverified** stereo-f32 model:
`requested_frames = read_bytes / 8`. The report names both units explicitly.
No returned data is interpreted as float, no finite-sample check is performed,
and no channel count, sample rate, PCM format, capture clock or timestamp is
claimed. `timestamp_present` is always zero. A successful probe proves only
that this call sequence completed with the recorded metadata on that run.

The primary failure remains the first failure in the forward path. Later cleanup
errors remain visible through their raw fields and `cleanup_uncertain`; they do
not replace an earlier read/open/start failure. If the forward path succeeded,
the first cleanup failure becomes the primary `cleanup_failed` result.

## Upstream provenance and licensing

The audit used immutable primary-source revisions already recorded in
`UPSTREAM.md`:

- FGGstore/FGG-PlayPods
  `d726e1c32568048aae66f3218c0540adc357e408`, `src/capture.c`: Ipmi-before-
  Avcap2 dependency, library paths, seven exports, `0x38` OpenAudio layout,
  source 2/mode 0 request, byte-return bounds, known status values, and
  consume-once cleanup behavior. Root `LICENSE` is GNU GPL version 3 and the
  inspected file has no later-version grant. `capture_probe.c` is therefore
  marked conservatively `GPL-3.0-only`.
- X-F1REBALL-X/HearBridge-PS5
  `cd5b7fce1b8226d8850cdd306437550098c6a62b`, `src/avcap2.c`: matching export
  names, paths and layout; 8,192-byte reads; 16-byte metadata with size preset;
  and independent matching status observations. Root `LICENSE` plus `NOTICE`
  state GPL-3.0-or-later.
- ps5-payload-dev/sdk
  `a7685ce8e97ce1f8a4f38253a41d54241c9124f1`, `crt/kernel.c`, `kernel.h` and
  `nid.c`: current-process module export resolution used by the proposed parent
  wrapper. These files are already the hash-verified GPL-3.0-or-later
  loader-resolver selection; this change copies none of their source.

No upstream retry loop, credential/authid fallback, format/content heuristic,
sample conversion, heap-backed session, audio dump, alternate library search,
or hidden restart was adopted. No proprietary SPRX is copied, downloaded from a
console or redistributed; the probe requests already-installed system modules
by path at runtime.

## Completed checks

Completed on 2026-10-24 Asia/Shanghai from baseline Git HEAD
`1ca499f` (`feat/mvp-bringup`); the checkout was clean before these four owned
files were added. No PS5 was contacted and no build/deployment/shared file was
changed by this worker; the parent subsequently integrated and ran the hardware
artifacts summarized above.

- Strict host build and fake lifecycle test:

  ```sh
  cc -std=c11 -Wall -Wextra -Wpedantic -Werror \
    -Iplatform/ps5/diagnostics \
    platform/ps5/diagnostics/capture_probe.c \
    tests/platform/capture_probe_test.c -o /tmp/ps5-capture-probe-test
  /tmp/ps5-capture-probe-test
  ```

  Result: `PASS bounded metadata-only capture probe lifecycle and cleanup`.
  Cases cover success, resolution-only preflight, optional NULL tracing and
  NULL waiting, exact begin/end trace order and raw returns, the exact 100,000
  us wait argument, failed-wait cleanup without Read, nested-call Busy, all seven
  resolution positions,
  dirty failure symbols/handles, both module-load failures, lifecycle failures,
  nonzero Ipmi/Avcap2 secondary results with no subsequent load/private call,
  resolver-status `na` when Avcap2 loading prevents all resolver attempts,
  known read statuses, arbitrary error, invalid byte lengths, exact cleanup
  order, all cleanup failures, first-failure preservation, byte/frame math, no
  sample bytes in reports, canaries and every short report capacity.

- The same host suite under AddressSanitizer and UndefinedBehaviorSanitizer:

  ```sh
  cc -std=c11 -Wall -Wextra -Wpedantic -Werror \
    -fsanitize=address,undefined -fno-omit-frame-pointer \
    -Iplatform/ps5/diagnostics \
    platform/ps5/diagnostics/capture_probe.c \
    tests/platform/capture_probe_test.c \
    -o /tmp/ps5-capture-probe-test-sanitize
  /tmp/ps5-capture-probe-test-sanitize
  ```

  Result: the same PASS, with no sanitizer diagnostic.

- LLVM 18.1.8 compiled `capture_probe.c` for `x86_64-sie-ps5` using the existing
  target C flags. The output was an ELF64 x86-64 relocatable object, had no
  undefined symbols, and its disassembly contained no XMM/YMM/ZMM register use.
  This is an object-level check, not an integrated ELF or hardware result.

## Remaining dependencies and exact continuation

The following work remains with the parent/integration owner:

1. Review these four files and the conservative GPL-3.0-only file decision.
2. Review the parent-owned module lookup, `usleep` resolution and build/main
   wiring against this final four-callback API. Keep all helpers limited to the
   explicit loader-kernel-resolver capture diagnostic; do not expose raw kernel
   helpers or Avcap2 handles to Rust or add offsets, credentials or fallbacks.
3. Repeat startup/link/map/import/relocation inspection and host runtime tests
   after incorporating this final wait-enabled source. Verify the final ELF
   preserves `-mgeneral-regs-only` and rejects setters, patch initialization,
   proprietary inputs and unexpected imports. Log only phase metadata and the
   encoded report, never PCM or the 16-byte native metadata.
4. If another hardware window becomes available, test the 100 ms settle-hook
   variant separately. Record exact Git/artifact hashes, model/firmware/loader,
   WAIT begin/end, the full encoded line, bootstrap lifecycle, visible
   notifications, return-to-loader, and DualSense before/during/after. Do not
   pair a headset or open HCI traffic.
5. Treat missing module permission, resolver failure or any lifecycle status as
   the result. Do not elevate credentials, retry under another authid, restart
   capture, search alternate module paths, dump libraries, or change kernel
   policy. Cleanup uncertainty requires ending the run, not a second close.
6. Even a positive whole-frame read leaves format, channel order, rate,
   timestamp origin, pacing, menu/game mix behavior, silence semantics and
   repeatability **Requires hardware**. A later separately reviewed test may
   measure those properties without storing PCM; this probe alone must not be
   connected to `PcmSource` or advertised as live audio.

Hardware verification outstanding: the 100 ms wait variant, positive byte
returns, requested format/channel/rate, metadata/timestamp meaning, menu/game
mix, repeated-process behavior and longer-run cleanup effects. The recorded
one-shot lifecycle and user-confirmed normal behavior do not establish those
facts; host fakes and target object inspection establish no hardware behavior.

Module lifetime and `LoadStartModule` secondary-result note: at both pinned
revisions, applications call LoadStartModule in Ipmi-then-Avcap2 order and do not
unload either module. PlayPods gates on the signed function return and does not
use `res`; HearBridge logs both values and confirms the module by a subsequent
handle lookup, but likewise does not assign a success/failure meaning to `res`.
This probe preserves `res` verbatim and fails closed unless the function return
is nonnegative **and** `res` is exactly zero. A nonzero Ipmi result prevents the
Avcap2 load; a nonzero Avcap2 result prevents export resolution and every
private capture call. Nothing is retried or unloaded. This does not claim a
specific undocumented meaning for other `res` values; they remain
**Unknown / Requires hardware**. The recorded hardware results were both zero. Preflight
performs no Avcap2 Initialize or cleanup, avoiding an unproven terminate of an
already-initialized capture service.
