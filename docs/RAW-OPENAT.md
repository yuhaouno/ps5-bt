# Checked raw-openat route comparison

Status: separately selected, source-backed experiment prepared on 2026-10-27.
Native execution and syscall-number compatibility on firmware 13.40 remain
**Requires hardware**. The operator authorized preparation/upload with manual
execution later. This document grants no future hardware window or launch
authority. Existing setup, namespace and open-comparison bounds are unchanged.

## Hypothesis and source review

The retained open comparison found imported `openat(root, "data", flags)`
returning -1/errno 1 while absolute `open("/data", flags)` and its fstat succeeded.
Earlier imported root-dot fstatat also returned -1/errno 1. This does not prove a
credential restriction, a native stub implementation, or a safe storage path.

This candidate compares the imported openat call against the **same root
descriptor, name, flags and mode** passed to the existing checked raw syscall
bridge. The hypothesis is that native-export and checked-syscall invocation
routes may differ. The source mapping is explicit, not firmware compatibility
evidence: SDK `a7685ce8e97ce1f8a4f38253a41d54241c9124f1`
`libc/syscalls.c:1901–1906` implements openat with number 499;
`include/freebsd/sys/syscall.h:420` corroborates that mapping. The original
`libc/syscalls.c` retains John Törnblom's GPL-3.0-or-later notice and has SHA-256
`82bb957e859b7fcba00f6d777fe753a7e6fce7bac864f42f730b130fda187a77`.
It is retained verbatim as reference-only source, never compiled or linked.

The pinned SDK symbol inventories distinguish libkernel surfaces:

| Reference-only source | Relevant labels | SHA-256 |
| --- | --- | --- |
| `sce_stubs/libkernel_sys.c` | fstatat line 602, openat line 1034 | `bf48b2a70948abbdd01bd8976586c75501836a08943db4f13c2b0ab728b824fa` |
| `sce_stubs/libkernel.c` | _openat line 278; no fstatat/openat labels | `668eedf57ebae86843bc2fcf3ca912c9937025dc39eefbfef46585eb8fa61e45` |
| `sce_stubs/libkernel_web.c` | _openat line 278; no fstatat/openat labels | `7251d1bab1967fd9ae2a75327272e39fb393f87e445b9e75f5e1efba7f022a5a` |

These generated inventories are not native implementations or proof of an EPERM
stub. No alternate symbol is selected from them. The current-process resolver
and existing POSIX bridge remain unchanged. Source review retains all these
reference revisions/hashes. The build's `sdk_review_sources` inventory contains
only `libc/syscalls.c`, retained verbatim without compiling it; the stub inventories
are documented references and are not build-manifest inputs. Linked SDK inputs
retain their separate identity and license checks.

`platform/ps5/bootstrap/syscall_checked.S` already preserves six arguments and
captures carry before C can change it. Through the selected SDK `crt/syscall.c`
it calls the loader-provided libkernel syscall entry; checked errors become
negative errno values. This experiment adds **no syscall instruction**, native
text inspection, stock CRT, permission patch, credential write, syscall-origin
range change, driver change or new kernel-access mechanism. Initial loader
kernel access and current-process resolver lifetime remain prerequisites.
See [RUNTIME-REVIEW.md](RUNTIME-REVIEW.md).

The original GPL-3.0-or-later `raw_openat.c` uses the same reviewed native
declarations, stat layout and no-follow flags as the existing application bridge.
No raw native structure enters Rust. All stat buffers start zeroed. Their
predicate/equality results do not independently validate native layout, actual
UID/mode, mount identity, ACLs or protection against path replacement.

## Calls and bounds

| Stage | Normal operation | On native failure |
| --- | --- | --- |
| 1 | geteuid, retained locally only | No error return |
| 2 | Imported open of root | Stop prerequisites |
| 3 | Imported fstat of root | Stop prerequisites |
| 4 | Imported openat of fixed data name | Retain failure; skip stage 5 |
| 5 | Imported fstat of that descriptor | Retain failure; continue independent case |
| 6 | Checked raw syscall 499 on same root/data | Negative error: retain failure; skip stage 7 |
| 7 | Imported fstat of established checked descriptor | Retain failure |
| 8 | No call: independent cases exhausted | Does not erase earlier failure |

Both opens use exactly `O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC`, mode 0.
All six raw syscall argument slots are explicitly promoted to long: root fd,
fixed name address, flags, mode 0 and two further zero slots. No arbitrary input
or file content is accepted. The probe creates no path and calls no fstatat,
entropy, listener, capture, encoder or radio operation. Runtime logging and
notifications retain their existing separate behavior.

There are at most seven normal calls and three reverse consume-once close
attempts, no retry. Only successful opens establish ownership. Each dependent
stat is skipped after an open failure; a stat failure does not release ownership.
The two case descriptors remain held until cleanup. Cancellation before any
actual call stops further work even when an earlier failure stays primary.
Skipped stats consume no checkpoint. Cleanup is uncancellable and never retries
close, including EINTR. First primary and first cleanup failures remain separate.

Synchronous native calls cannot be preempted by a Rust timer. Every launch needs
a **15-second external ceiling**, identity-bearing lifecycle, immediate local log
collection and truthful wireless-controller observations before/during/after.
Stop on a missing return, controller change or ownership/cleanup uncertainty.

## Report and unknown ownership

Independent ABI `0x00010000` is 136 bytes, with 34 fixed-width 32-bit fields.
ABI/reserved are not logged; reserved is zero. The schema-1 begin marker is:

`event=application_raw_openat_begin schema=1 namespace=root-data access=read-only cases=imported-checked radio=Unavailable capture=Unavailable`

Numeric fields use `application_raw_openat_`. Primary signed-64 return bits are
`raw_return_lo/hi`; checked signed-64 bits are `checked_open_lo/hi`. Imported
errors are sign-extended, not truncated. Primary `errno` belongs to a failed
imported call; checked negative returns preserve their own -errno semantics and
**never read the unrelated native errno slot**. A prior imported failure remains
primary even if a later checked result supplies additional evidence.

Checked-open kinds are 0 unattempted, 1 representable owned descriptor, 2 negative
checked error, 3 nonnegative result above INT32_MAX, and 4 alias of a still-held
descriptor. Kind 3/4 stops immediately at stage 6 without stat or an invented
close. An imported open alias to held root similarly stops at stage 4. The probe
closes only previously established distinct descriptors exactly once, records
`raw_ownership_unknown=1`, and latches refusal. Unknown results cannot be repaired
by casting them, closing a guessed number or restarting the payload. A later
guard report carries `prior_raw_ownership_unknown`; current/prior close uncertainty
has separate masks and errno/raw fields.

`attempted_mask` bit stage-minus-one records actual calls; operations is its
popcount. Opened/closed/observed masks use root 1, imported 2, checked 4; observed
means successful fstat, closed means an attempt. Reject bits are type 1,
parent-owner 2 and group/other write without sticky 4. Data agreement bits are
device 1, inode 2, mode 4, UID 8, GID 16, only after both data stats. Each
same-device result is 0 unobserved, 1 different, 2 equal relative to root.
No native UID/GID, path, stat bytes, inode/device number, key or pointer is emitted.

Status is 0 no native/cleanup errors; 1 cancellation absent an earlier error;
-1 native/cleanup failure; -2 invalid/budget; -3 reentry/prior uncertainty;
-4 ownership anomaly absent an earlier primary error. Cleanup-only failure has
failed stage and primary return zero. Every uncertainty prevents acceptance even
when a primary error was retained instead of -4.

## Build and later manual execution

```sh
python3 tests/application/run_raw_openat.py --cc cc
python3 tools/prepare_ps5_window.py --doctor \
  --plan tools/raw_openat_test_plan.json --llvm-config /opt/homebrew/opt/llvm@18/bin/llvm-config
python3 tools/prepare_ps5_window.py --build \
  --plan tools/raw_openat_test_plan.json --llvm-config /opt/homebrew/opt/llvm@18/bin/llvm-config \
  --kit-dir build/ps5/raw-openat-window-kit
```

Feature/flag is `--application-raw-openat`, ELF/slot stem
`ps5-bt-application-raw-openat`, with dedicated fixed log
`/data/ps5-bt-raw-openat.log`. Other diagnostics keep their existing logs.
Build/package from clean committed source; inspect exact identity and read back
the uploaded hash. Upload does not launch. While the operator is away, leave
manual launch/controller checks pending; preparing a candidate does not supply
those observations. Repeating this artifact truncates its own log.

If checked access succeeds while imported access fails, the evidence isolates an
invocation-route difference for these exact inputs. If both fail, retain both
errors and stop this hypothesis; do not change permissions or loop over guessed
syscalls. Even complete matching metadata cannot clear protected storage, native
ABI, capture format, Bluetooth ownership/credits or headset playback gates.


## Validation before manual execution

Strict C11 and ASan/UBSan raw core/main suites pass, including full signed-64
errors, descriptor 0/INT32_MAX, wide-positive and alias uncertainty, cancellation,
all 64 normal-fault combinations and reverse close-failure accounting. Existing
setup, namespace and open-comparison strict/sanitized regressions pass. An
independent temporary harness generated 35,528 reports from the actual C core
and synthetic adapters; collector contract/cleanup classifications matched all
of them. These are synthetic checks, not PS5 evidence.

Builder/source-package/kit self-tests and collector 65, console 22, preparer 16,
portable-handoff 20 tests pass. Target precommit compile/link, native-header
closure, runtime log/error fixtures and ELF inspection pass. Disassembly confirms
number 499 enters the existing checked wrapper; the review-only libc file is
not a linked input. The final deployment must use a fresh clean-commit rebuild.
Full `make check` remains unavailable on this ARM host at its documented x86
syscall execution fixture; it is not reported as passing. Native route behavior,
store protection/ABI, PCM format and Bluetooth ownership/allowance remain pending.


## Uploaded manual-run checkpoint, 2026-10-27

Clean source `e328d8574dee69f949697c565033541486a2f09b` passed the toolchain doctor
and all three sequential build/package/assembly steps. Existing pinned tools
sufficed; no installation was needed. The ready kit is
`build/ps5/raw-openat-window-kit-e328d85`, with matching preparation identity.
The clean target build again passed runtime synthetic checks, target header
closure, link and ELF inspection. Reference-only SDK source bytes in the final
bundle match their pinned hash and are not a linked input.

- ELF SHA-256: `68af865401fba4e3efaeb9b071f556690d4a87e9f3490cbd169778c08c65c93c`.
- ELF size: 75144 bytes.
- Embedded build ID: `fb99f5f9e596d2de140a7104fe66506218ce1e87c53b85207b6947b3dee8ea25`.
- Build manifest SHA-256: `e9d2376871558de5d1374852418e36a90988fde2c2c566ea10e8b7bb81100333`.
- Matching source bundle SHA-256: `9769d68fb261ba39b393b5178c26f3a681dec747ade153bed96d2a882a3570c6`.

Within the ongoing authorized window, the manager again reported 0.6.3 at
.111:8084. Its reviewed `mode=new` upload created the absent exact slot.
Read-only FTP preflight verified metadata, size and full ELF SHA-256; output is
`build/ps5/raw-openat-preflight-e328d85.json`. After saving the old comparison
result and verifying this upload, the exact completed open-compare slot was
removed through the reviewed manager route. Final inventory is saved in
`build/ps5/raw-openat-final-inventory-e328d85.json`: only
**`local/ps5-bt-application-raw-openat`** remains among PS5-BT entries, with all
unrelated slot records unchanged. Previous local kits/logs remain intact.

**No agent launch or raw-openat hardware outcome.** Refresh and manually run the
single entry once, observe wireless input before/during/after, and stop on a
missing return at 15 seconds or any input change. Immediately after the operator
reports execution, collect without launching another payload:

```sh
python3 tools/console_window.py --kit build/ps5/raw-openat-window-kit-e328d85 \
  --collect --candidate application-raw-openat --execute \
  --controller-before normal --controller-during normal --controller-after normal \
  --run-observation observed
```

Use only actually reported observation values; otherwise keep Unknown. The log
is `/data/ps5-bt-raw-openat.log`. This checkpoint grants no future window, and
matching upload/build identities do not clear store, PCM or Bluetooth gates.
