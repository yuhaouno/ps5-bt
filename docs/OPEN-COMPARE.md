# Fixed directory open comparison diagnostic

Status: independently selected experimental candidate; native results **Require
hardware**. It follows the retained [root-dot fstatat failure](NAMESPACE-PROBE.md#observed-root-dot-fstatat-failure-2026-10-26).
The operator authorized autonomous preparation/upload and subsequent manual
execution. This admission does not grant a future console window or clear any
application, PCM or Bluetooth gate.

## Question and reviewed mechanism

Root `open`/`fstat` succeeded, but `fstatat(root, ".", AT_SYMLINK_NOFOLLOW)` returned
-1/errno 1. Source review found no argument-forwarding mismatch. This new candidate
compares `openat(root, "data", flags)` and `open("/data", flags)` without invoking
fstatat again. Both use the same target-declared read-only, directory, no-follow
and close-on-exec flags. This can distinguish an observed open-route asymmetry
from a blanket inability to open that target. It cannot determine why a call
fails or make an unprotected path safe for storing keys.

`platform/ps5/application/open_compare.c` is original GPL-3.0-or-later. Native
operations use the existing selected-export bridge and SDK
`a7685ce8e97ce1f8a4f38253a41d54241c9124f1` declarations/43-header dependency closure.
No API, offset, layout, upstream component or imported syscall is added.
[PS5-POSIX-IMPORTS.md](PS5-POSIX-IMPORTS.md) records source/license/ABI review;
per-file notices remain intact. Current-thread imported functions/errno and
local stat storage must remain valid throughout the synchronous call and cleanup;
there is no pointer retention or concurrent/reentrant access. The existing-loader
resolver requires the already supplied loader kernel channel and changes no
credentials, syscall permissions, system process, driver or dispatcher.

Unlike store traversal, this explicitly selected experiment may observe root/data
despite reported protection rejection. Absolute `/data` can traverse an unprotected
root **only for this read-only experiment**: no contents or descendant/private
file is opened, no directory is created, and no permission rule is relaxed.
No `mkdir`, `chmod`, `chown`, `rename`, `unlink`, lock, sync, entropy read, listener,
codec, capture, radio or fstatat operation is invoked. The baseline runtime's
bounded log writing and notifications remain enabled.

## Call budget, case failures and cleanup

| Stage | Fixed call | Failure behavior |
| --- | --- | --- |
| 1 | `geteuid()` | Keep UID local; no error return |
| 2 | `open("/", flags)` | Prerequisite failure stops normal calls |
| 3 | `fstat(root)` | Prerequisite failure stops normal calls |
| 4 | `openat(root, "data", flags)` | Record error; skip stage 5 only |
| 5 | `fstat(relative)` | Record error; proceed to independent absolute case |
| 6 | `open("/data", flags)` | Record error; skip stage 7 only |
| 7 | `fstat(absolute)` | Record error |
| 8 | No native call | Independent case sequence exhausted, even if a case failed |

`flags` is exactly `O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC` from the
pinned target headers. Only successful opens confer descriptor ownership;
negative opens never feed a stat or close. Zeroed stat snapshots are evaluated
only after successful calls. Successful descriptors remain held for comparison,
then close in reverse absolute/relative/root order. A failed stat does not erase
ownership of its descriptor. Seven normal calls and three close attempts are
hard maxima; failed-open dependent stats are skipped, not retried.

Proceeding to the second independent case after a known first-case error is a
separately admitted observation, not a recovery loop. The first primary native
error and failed stage remain visible while later per-case errors are preserved
separately. Cancellation before each actual call stops work even if a previous
case already set primary failure; it never becomes permission to run the next
case. A skipped dependent stat consumes neither a call nor a checkpoint.
Cleanup is uncancellable and each owned descriptor is consumed before its single
close attempt. Any close uncertainty latches refusal for later calls in the same
process; neither import shutdown nor restarting a payload proves reconciliation.

The outer initializer retains its finite 23-export lookup sequence. Native calls
cannot be preempted by a timer. Use the **15-second external ceiling**, watch
wireless controller input before/during/after, and stop on missing return,
controller change or uncertain cleanup. Do not automatically run the application
because one access path succeeded.

## Primitive report and acceptance

`open_compare.h` defines independent ABI `0x00010000`, a 120-byte report containing
30 fixed-width primitive fields. ABI/reserved are not logged; reserved stays zero.
No UID/GID, native stat bytes, device/inode number, path, key or pointer is emitted.
Per-case successful open returns may contain descriptor numbers. The log begins:

`event=application_open_compare_begin schema=1 namespace=root-data access=read-only cases=relative-absolute radio=Unavailable capture=Unavailable`

Numeric fields use `application_open_compare_`; raw returns/statuses are encoded
as eight hexadecimal digits retaining signed i32 bits. Stage identifies the
checkpoint reached, which cancellation may stop before its call. `attempted_mask`
bit `(stage - 1)` marks each actual normal call, and `operations` is its popcount.
Opened/closed/observed masks use root 1, relative 2, absolute 4; observed means
successful stat. Closed means attempted, not necessarily successful.

`relative_open_raw/errno`, `relative_stat_raw/errno`, and corresponding absolute
fields preserve each attempted case result. Missing attempted bits mean skipped
or not reached; zero fields without those bits are not success. Successful calls
have zero report errno; the UID is never copied into a numeric return.
`failed_stage`, primary raw/errno and status retain the first native failure.
Cleanup-only failure has failed stage and primary raw zero, plus separate cleanup
raw/errno. Current/prior cleanup masks remain distinct from primary errors.

- Rejection bits, interpreted through the compiled SDK layout: type 1, parent
  owner 2, group/other write without sticky 4. They do not stop this observation.
- `data_match_mask`: relative-vs-absolute metadata agrees on device 1, inode 2,
  mode 4, UID 8, GID 16; zero until both stats succeed.
- `relative_same_device` / `absolute_same_device`: comparison with successful root
  stat, 0 unobserved, 1 different, 2 equal. None establishes a mount boundary or
  path-replacement protection.
- Status: 0 all calls successful; 1 clean cancellation absent prior error;
  -1 native/cleanup failure; -2 invalid/budget; -3 reentry/prior uncertainty.

A full successful profile requires stage 8, attempted mask 127, operations 7,
opened/closed/observed masks 7, zero native/cleanup errors, clean import shutdown
and matching outer lifecycle. If either case fails, classification remains
`complete_failure` even if the other supplies useful metadata and stage 8 is
reached. Preserve those independent observations; do not hide the failure behind
an overall success. Predicate rejection/mismatch still cannot clear store or ABI
acceptance, which remain Unknown under every outcome.

| Observed cases | Interpretation / next decision |
| --- | --- |
| Relative open fails, absolute open/stat succeeds | Access-route asymmetry for these exact calls/flags; no general permission or safe-store claim |
| Both open/stat succeed | Earlier root-dot fstatat failure is narrower than all descriptor-relative opens; compare only observed SDK-view fields |
| Both fail | Retain separate raw errors/stages; no inferred credential repair or alternate API |
| Successful stats disagree | View/race/native ABI unresolved; no protected-store acceptance |
| Cleanup uncertain, controller change or missing lifecycle | Stop and reconcile; do not launch another test as recovery |

## Build and deferred manual run

```sh
python3 tests/application/run_open_compare.py --cc cc
python3 tools/prepare_ps5_window.py --doctor \
  --plan tools/open_compare_test_plan.json \
  --llvm-config /opt/homebrew/opt/llvm@18/bin/llvm-config
python3 tools/prepare_ps5_window.py --build \
  --plan tools/open_compare_test_plan.json \
  --llvm-config /opt/homebrew/opt/llvm@18/bin/llvm-config \
  --kit-dir build/ps5/open-compare-window-kit
```

The independently selected builder flag is `--application-open-compare`; exact
slot/ELF stem is `ps5-bt-application-open-compare`. It reuses the finite Rust
baseline/Stop seam without starting the Rust application. Existing setup and
namespace code/bounds and the default six-item plan are unchanged. Commit, build
from clean source, package matching materials, then upload and read back the full
hash under current authorization. Upload is not execution.

This artifact uses its own fixed `/data/ps5-bt-open-compare.log`. Existing logs
remain unchanged. The plan/collector accepts only the exact singleton candidate
and fixed path; no arbitrary log input or mixed-path plan is admitted. Run once:
repeating this same candidate overwrites its own log. While the operator is away,
finish preparation/upload without waiting, leaving launch and controller checks
pending. When the operator reports completion, collect immediately against the
exact kit, using only actually reported observation values:

```sh
python3 tools/console_window.py --kit build/ps5/open-compare-window-kit \
  --collect --candidate application-open-compare --execute \
  --controller-before normal --controller-during normal --controller-after normal \
  --run-observation observed
```

Retain private raw logs and correlated JSON; freshness remains Unknown. Host tests,
ELF inspection and a matching upload do not establish native execution, firmware
compatibility, protected store durability, Bluetooth authority or audio playback.

## Validation record

Strict C11 and ASan/UBSan core/main tests pass. The synthetic core covers all
64 combinations of six fallible normal-call failures, each interrupted call,
all cancellation points including after a retained earlier error, all seven
close-failure masks, positive/errno-zero failure preservation, descriptor zero,
partial stat output, each predicate/comparison field, reentry and known-clean
repeat. Original setup and namespace strict/sanitized suites still pass.
The collector has 60 passing tests, console helper 20, preparer 15 and portable
handoff 19; builder/source/kit self-tests pass. Target compile/link, pinned header
dependency closure, runtime synthetic log/error tests and final ELF inspection
pass. Native comparison outcome remains untested. Full `make check` was not run;
the previously documented ARM-host x86 syscall-fixture limitation remains.

## Uploaded manual-run checkpoint, 2026-10-27

Clean source `86391641d28947f40e9201cb35e70d6a97134810` passed doctor and all
three build/package/assembly steps. The immutable kit is
`build/ps5/open-compare-window-kit-8639164`; its adjacent preparation identity is
`ready`. Existing pinned tools sufficed; no installation was needed.

- ELF SHA-256: `98a63a4342218a1f199a3ab61169051a9e6f226deceb11e1d9f82874f51eeb4f`.
- ELF size: 75104 bytes.
- Embedded build ID: `908401d2fd3b72aa419f7bd58cf3569144aab59f2de1b168545a942242d41ca4`.
- Build manifest SHA-256: `0ac84d1a753b6ecbbf9cd9503b511bf85c9366e52c0092f43a59b39a10ed7d39`.
- Matching source bundle SHA-256: `baa0a213ab98e4a972ada5c96fd452269d4cdcf8fe92cb72de417d1cf64b7765`.

Within the ongoing authorized window, PayloadManager version/inventory again
confirmed 0.6.3 at .111:8084. The absent exact slot was imported with the reviewed
`mode=new` route. FTP preflight verified matching metadata, size and full ELF
SHA-256; saved output is `build/ps5/open-compare-preflight-8639164.json`.
After preserving both previous namespace collections and verifying the new
upload, the completed namespace slot was removed through the reviewed manager
route. Final inventory contains only **`local/ps5-bt-application-open-compare`**
among PS5-BT slots; all unrelated slot records are unchanged. Original kits and
logs remain intact, including the remote dedicated namespace log.

**No agent launch or new comparison hardware result.** The operator should
refresh and manually run `ps5-bt-application-open-compare` once, observe the
wireless controller before/during/after and stop if return is missing at 15 seconds
or input changes. When completion is reported, immediately collect the dedicated
`/data/ps5-bt-open-compare.log` with the command above and
`--kit build/ps5/open-compare-window-kit-8639164`. Do not treat this preparation
record as authority for a future session or as permission to clear any store,
format, Bluetooth ownership or credit gate.
