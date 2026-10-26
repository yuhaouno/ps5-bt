# Read-only root/data namespace consistency diagnostic

Status: independently selected experimental candidate. Native execution and
native layout validity remain **Requires hardware**. The 2026-10-26 operator
asked for autonomous preparation and upload, with manual execution after returning.
This admission describes the candidate, not a result or authority for a future
console window. See [HW4-EVIDENCE.md](HW4-EVIDENCE.md) for the preceding failures.

## Question and permitted mechanism

The schema-2 setup run returned only root `PARENT_WRITE` (mask 4) under the
compiled SDK interpretation. This experiment asks whether descriptor and
parent-relative metadata observations agree for root and data. It may reveal
inconsistent views; agreement cannot independently establish the native ABI,
mount/ACL protection, or whether an attacker can replace the root-to-data edge.
It does not bypass the application's protected-parent rule or accept a store.

The original C adapter in `platform/ps5/application/namespace_probe.c` calls
only already reviewed `geteuid`, `open`, `openat`, `fstat`, `fstatat` and `close`
through the existing selected-export POSIX bridge. The export table remains 23
entries, resolved before resource acquisition; unused entries are not called.
There is no new module, export scan, syscall, upstream implementation or SDK
header dependency. The pinned declarations and 43-header closure are those of
SDK `a7685ce8e97ce1f8a4f38253a41d54241c9124f1`, described with per-file license
obligations in [PS5-POSIX-IMPORTS.md](PS5-POSIX-IMPORTS.md). All new project code
is GPL-3.0-or-later; existing header notices remain unchanged.

The C implementation uses the target's `struct stat`, `uid_t`, flags and type
macros, not hardcoded native layouts. Native stat buffers start zeroed. The
reviewed synchronous functions receive valid local storage for each call; neither
those buffers nor descriptor values escape into Rust or outlive the procedure.
Matching imported functions and the current-thread errno slot must remain live
until all close attempts complete. Reentry and concurrent use are forbidden.
The same existing-loader resolver is used; no credential or driver mutation is
introduced. Missing imports or loader access are concrete failures.

## Finite operation sequence

| Stage | One normal operation | Evidence retained |
| --- | --- | --- |
| 1 | `geteuid()` | Owner used only for local predicates; never emitted |
| 2 | `open("/", O_RDONLY \| O_DIRECTORY \| O_NOFOLLOW \| O_CLOEXEC)` | Root ownership bit |
| 3 | `fstat(root)` | Root descriptor predicate bits |
| 4 | `fstatat(root, ".", ..., AT_SYMLINK_NOFOLLOW)` | Root-relative predicates and agreement bits |
| 5 | `fstatat(root, "data", ..., AT_SYMLINK_NOFOLLOW)` | Data path predicate bits |
| 6 | `openat(root, "data", same flags)` | Data ownership bit |
| 7 | `fstat(data)` | Data descriptor predicates/agreement and root/data device equality |
| 8 | No native operation | All seven normal operations completed |

The fixed paths are not caller input. No file below data is opened. Descriptor
opens remain directory-only and no-follow. Unlike setup, this observation-only
candidate deliberately continues its finite read-only sequence when a predicate
or comparison differs; those observations never authorize file access by the
application. Native-call failure stops normal calls immediately. There is no
mkdir, chmod, chown, sync, lock, rename, unlink, entropy read, listener, capture,
encoder or Bluetooth operation in this probe. The existing runtime still writes
its bounded diagnostic log and sends start/finish notifications.

There are at most **seven normal operations and two reverse consume-once close
attempts**, with no retry. Cancellation is checked before normal operations and
at completion; cleanup is uncancellable. Consume a descriptor before its close,
even for EINTR. Preserve the first primary result separately from cleanup failure;
close uncertainty latches a process-local refusal for later calls. Import
shutdown is not cleanup proof and does not reset that latch.

The outer import initializer has its existing finite 23-export lookup sequence.
Native synchronous calls cannot be preempted by an elapsed timer. Use a
**15-second external ceiling** and check wireless controller input before,
during and after. A missing return, controller change or uncertain cleanup stops
the sequence; another payload is not a recovery action.

## Report ABI and interpretation

`namespace_probe.h` defines independent ABI `0x00010000`, an 80-byte report of
20 fixed-width primitive fields. No raw stat, UID/GID, path, inode, device number,
pointer, key or capture bytes are emitted. The log schema is 1:

`event=application_namespace_begin schema=1 namespace=root-data access=read-only radio=Unavailable capture=Unavailable`

All report numbers use the `application_namespace_` prefix. Stage identifies the
normal call whose checkpoint was reached (possibly cancelled before its call),
or 8 after completion. Operations counts consumed normal checkpoints. Opened/closed
masks use root bit 1 and data bit 2; closed means **attempted**, not successful.
Current and prior cleanup masks, first cleanup errno/raw return and primary
status/errno/raw return remain separate. `geteuid` is never copied to raw return.
A cancellation immediately after open may retain its successful descriptor return.

| Field | Meaning |
| --- | --- |
| `observed_mask` | Successful stat calls: root descriptor 1, root dot 2, data path 4, data descriptor 8 |
| `*_reject` | Under compiled layout: type 1, parent owner 2, group/other writable without sticky 4; zero if not observed |
| `root_match_mask` | Root descriptor vs dot: device 1, inode 2, mode 4, UID 8, GID 16 agree; zero until both observed |
| `data_match_mask` | Data path vs opened descriptor: same five bits; zero until both observed |
| `same_device` | Root/data descriptor device comparison: 0 unobserved, 1 different, 2 equal |
| `status_bits` | Signed status encoded as u32: 0 complete procedure, 1 cancelled, -1 native/cleanup failure, -2 invalid/budget, -3 reentry/prior uncertainty |

A complete procedure requires stage 8, seven operations, observed mask 15,
opened/closed masks 3, zero current/prior uncertainty, successful import shutdown
and matching outer identity/lifecycle. Predicate rejections and comparison
mismatches are valid observations even when the procedure completes. Collector
`complete_success` means **only that this observation procedure completed**;
store protection and native ABI acceptance remain Unknown. Do not convert equal
devices, different devices, matching modes or a complete profile into a root
permission exemption or an unverified mount guarantee.

Native failures retain their call stage and immediate errno; close failures do
not replace an earlier primary failure. Import-init failure produces no probe
report and no probe call. Shutdown failure forces a failed terminal outcome.
Clean cancellation produces a cancelled outcome, never complete success.

## Build, separate log and operator handoff

```sh
python3 tests/application/run_namespace_probe.py --cc cc
python3 tools/prepare_ps5_window.py --doctor \
  --plan tools/namespace_test_plan.json \
  --llvm-config /opt/homebrew/opt/llvm@18/bin/llvm-config
python3 tools/prepare_ps5_window.py --build \
  --plan tools/namespace_test_plan.json \
  --llvm-config /opt/homebrew/opt/llvm@18/bin/llvm-config \
  --kit-dir build/ps5/namespace-window-kit
```

The builder selects `--application-namespace --loader-kernel-resolver`; its
fixed ELF/slot is `ps5-bt-application-namespace`. It links the existing finite
Rust baseline/Stop seam but does not start the Rust application or obtain an
entropy token. Original setup and the default six-item plan remain unchanged.
Only upload the committed clean-source artifact with its matching package and
manifest. Verify installed manager version, exact slot metadata and full remote
ELF hash under the current authorization before manual launch.

This artifact's runtime log is **`/data/ps5-bt-namespace.log`**, selected by its
compile-time feature. Other existing artifacts retain `/data/ps5-bt-hw1.log`.
No arbitrary path parameter is accepted. The one-item plan and collector are
restricted to this exact candidate/path combination. Running another baseline
candidate cannot truncate this separate log; repeating namespace itself still
replaces its prior log, so launch it once and collect before rerunning it. The
log's existing runtime open/write/close error handling and mode 0600 request are
unchanged; no protected-store durability claim follows.

When the operator is away, prepare and upload but leave manual launch pending:
controller observations cannot be supplied remotely. Do not wait for a reply
before finishing preparation. The operator can run the prepared entry once and
report completion/controller behavior on return. Collection then uses:

```sh
python3 tools/console_window.py --kit build/ps5/namespace-window-kit \
  --collect --candidate application-namespace --execute \
  --controller-before normal --controller-during normal --controller-after normal \
  --run-observation observed
```

Use those observation values only if actually reported. Preserve private raw log
and correlated JSON; automatic freshness remains Unknown. A changed result may
justify another separately reviewed experiment. It never enables unknown PCM,
Bluetooth ownership/credits, or headset playback.

## Validation record

Strict and ASan/UBSan core/main tests pass, including known-clean repeated calls,
all fallible native operations, cancellation before/after work, descriptor zero,
reverse close errors, reentry and sticky uncertainty. The original setup tests
also pass. The target build passes synthetic startup for baseline and namespace
log selection, pinned SDK dependency closure, final link and structural ELF
inspection. Collector, window, preparer, handoff and source/kit self-tests cover
new observations and the fixed path while retaining legacy plans. These are
local checks; no namespace hardware result has been collected. Full `make check`
was not run: the previously recorded ARM-host x86 fixture limitation remains.
