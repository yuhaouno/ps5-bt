# Fixed native application-directory setup candidate

Status: implemented as an independently selected candidate with synthetic host
fault checks. On Slim/13.40, the schema-2 run rejected root with only
`PARENT_WRITE` (mask 4), after successful open/fstat and with clean close.
Traversal beyond root, protected namespace acceptance, actual metadata layout
and durability remain **Requires hardware**. See [HW4-EVIDENCE.md](HW4-EVIDENCE.md).
Preparing or building this candidate does not authorize a console launch. It is
not Bluetooth transport or PCM support.

## Purpose and fixed scope

`platform/ps5/application/setup_namespace.c` prepares only
`/data/ps5-bt/control`, the protected directory required by the application
FileStore. The API accepts a reviewed resolver, cancellation callback and report;
it accepts no path, UID, permission, command or arbitrary file input. Root and
`/data` must already exist. Only missing `ps5-bt` and `control` components may be
created. No existing directory is chmod/chown/unlinked, and no lock or key file
is created or removed. There is no entropy read, network listener, capture,
encoder, radio operation or system-process/driver modification in this procedure.

The caller must keep initialized matching POSIX imports, their current-thread
errno slot and callable current-process resolver valid for the entire procedure,
including cleanup. It must serialize calls: reentry is rejected. The existing
23-export import table is unchanged. Setup resolves `mkdirat` separately once;
nonzero full-width resolver status or a null symbol rejects it without a call.
No alternate export or syscall fallback is attempted.

The separately selected build is `--application-setup --loader-kernel-resolver`,
producing `ps5-bt-application-setup.elf` for slot `ps5-bt-application-setup`.
Use the final kit manifest/runbook for its exact artifact identity, slot and
current execution selection. It does not silently run before another candidate.
Without a successful setup or another established safe native preparation,
the original application private-directory prerequisite still applies.

## Source, layout and permission contract

`mkdirat`'s function-pointer type comes from the unmodified pinned SDK declaration
`int mkdirat(int, const char *, mode_t)` in `sys/stat.h`, revision
`a7685ce8e97ce1f8a4f38253a41d54241c9124f1`. The same reviewed 43-header inventory
used by the application supplies `stat`, native UID/mode types and flags.
Their original notices remain intact; this is a declaration/layout selection,
not native-export availability proof. New setup code and its tests are original
GPL-3.0-or-later. See [PS5-POSIX-IMPORTS.md](PS5-POSIX-IMPORTS.md) for the existing
source/license and callable-lifetime review.

Traversal holds descriptors for `/`, `data`, `ps5-bt` and `control`. Every open
uses `O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC`. Every non-root component
is inspected relative to its held parent with `AT_SYMLINK_NOFOLLOW` before open;
its opened descriptor must have the same device/inode. A symlink, nondirectory
or substitution fails. There is no string traversal through unvalidated parents.

Existing parents use the same policy as FileStore: directory owned by native
root or the payload's `geteuid()` identity; group/other write permission is
rejected unless the sticky bit is set. Therefore an existing root-owned `0755`
`ps5-bt` parent can be admitted. A **newly created** component and the **final**
`control` directory must instead have exactly `0700`, including absence of
special mode bits, and match the payload's native UID. `mkdirat(..., 0700)` is
followed by fresh no-follow metadata and opened-descriptor validation. A native
umask or unexpected ownership that changes the resulting contract causes
rejection; setup does not repair it.

Only ENOENT for the two allowed components permits creation. Root/data absence,
EINTR, permission failures, EEXIST races and all other failures stop without
retry. Each confirmed creation is followed by `fsync` of the new directory and
its parent. A sync error remains an error; neither successful host tests nor a
returned native sync proves power-loss durability. Created directories persist
on failure/cancellation and are reported. A failed creation call can have an
uncertain native side effect; a zero creation mask is not a filesystem rollback
proof. No rollback removal or permission relaxation is attempted.

FTP directory visibility or creation does not prove native UID/mode. The
candidate itself must validate those facts. Do not substitute speculative
`SITE CHMOD`, broad permissions, deleting `.p5bt.lock`, or changing an existing
owner for this contract.

## Budgets, cancellation and cleanup

There are at most 32 normal-operation checkpoints, including the one optional
export lookup and UID observation. Current successful paths use 13 checkpoints
when both components exist, 17 when one is missing and 21 when both are missing.
There are at most four separate cleanup close attempts, in reverse descriptor
order. Each descriptor is consumed before its one close call; even close(EINTR)
is not retried. Cancellation is checked before normal operations and before
returning the final outcome; cleanup itself continues after cancellation.

These bounds limit calls, not their native duration. A synchronous native call
cannot be preempted by the procedure. Use the kit's independent 15-second
external observation ceiling; preserve an incomplete log and stop the sequence
if return is missing. Check wireless-controller operation before/during/after,
even though this payload issues no radio traffic.

The first primary error or cancellation is retained when cleanup also fails.
Cleanup failure has its own descriptor mask, errno and raw return. Any uncertain
close latches a process-local refusal: later setup calls return -3 without
native calls and report the prior cleanup mask. The latch is not cleared by a
new Stop, successful later import shutdown, or another setup invocation. This
prevents an invisible retry, but does not reconcile the uncertain descriptor.
A new payload launch is not cleanup proof; reconcile first.

A known-clean repeat is idempotent: existing protected directories are validated
without another mkdir or creation fsync. Successfully created directories are
persistent scoped state, not resources to be removed during ordinary cleanup.

## Versioned report and result interpretation

`setup_namespace.h` defines independent ABI `0x00020000`, a 72-byte primitive
report with log schema 2. It preserves the first 64 bytes' field offsets and
appends `validation_failed_mask` plus a zero reserved word. The original
64-byte ABI `0x00010000`/schema-1 artifacts remain in their immutable kits;
callers must compile against the matching version and size. The local collector
accepts both log schemas without inventing missing schema-1 detail.
Native `stat`, paths, UID, tokens, keys, pointers and capture bytes are
not copied into the report. All numeric raw failures and cleanup evidence remain local
diagnostic fields; do not replace them with a generic Bluetooth error.

| Field / value | Meaning |
| --- | --- |
| `stage` 1 | Validation, optional lookup and UID acquisition |
| `stage` 2 / 3 / 4 / 5 | Root / data / ps5-bt / control traversal |
| `stage` 6 | Normal traversal completed; cancellation/cleanup still determine the result |
| `status` 0 | Complete verified procedure and known clean owned cleanup |
| `status` 1 | Cancellation; any created components remain reported; not setup acceptance |
| `status` -1 | Native lookup/operation or primary cleanup failure |
| `status` -2 | Invalid argument or checkpoint-budget violation |
| `status` -3 | Reentry or prior uncertain cleanup; inspect `prior_cleanup_mask` |
| `status` -4 | Type, ownership, mode or device/inode validation failed |
| `created_mask` bits 0 / 1 | Confirmed creation of ps5-bt / control |
| `verified_mask`, `opened_mask`, `closed_mask` bits 0..3 | Root, data, ps5-bt, control; `closed_mask` means a close was attempted, not that it succeeded |
| `cleanup_failed_mask` | Current uncertain close attempts; success requires zero |
| `prior_cleanup_mask` | Earlier uncertainty blocking this invocation; success requires zero |
| `raw_lookup` | Full signed 64-bit optional mkdirat resolution result |
| `raw_errno`, `raw_return` | Primary traversal/native operation evidence |
| `cleanup_errno`, `cleanup_raw_return` | First failed close's separate errno/raw result; errno zero does not erase a nonzero raw failure |
| `operations` | Normal checkpoints used, bounded by 32 |
| `validation_failed_mask` | Schema 2 only: failed source predicates below; nonzero exactly when the primary status is -4 |

The rejection mask identifies checks over the **compiled SDK layout**, not an
independent measurement of the native layout, actual UID or permissions:

| Bit | Rejected predicate |
| --- | --- |
| `0x01` | Metadata is not interpreted as a directory |
| `0x02` | Parent owner is neither root nor the payload's observed effective UID |
| `0x04` | Group/other writable without the sticky bit |
| `0x08` | Before/opened device or inode differs |
| `0x10` | Final or newly created component is not owned by the payload UID |
| `0x20` | Final or newly created component does not have exact mode 0700 |

Before opening a non-root component, only its type is checked. After successful
`fstat`, every applicable original predicate contributes a bit, including
overlapping failures. Root can produce only bits 0x01/0x02/0x04. Identity needs
a non-root opened component; private-owner/mode checks apply only to final or
newly created components. No new native call or cancellation checkpoint is added.
Both local `stat` buffers are zero-initialized so incomplete writes cannot turn
old stack contents into reported predicates; this does not validate a native ABI.
The first policy mask survives cleanup failure or later cancellation. Other
outcomes, including reentry/prior-uncertainty refusal, retain a zero mask. No
global rejection state is retained. Descriptor counts, 32 checkpoints, four
consume-once closes and all protection rules are unchanged.

## Admitted schema-2 follow-up

The [2026-10-26 setup run](HW4-EVIDENCE.md) opened root and obtained `fstat`
return 0, then rejected its metadata at stage 2 with clean close. Its schema-1
report cannot identify which predicate rejected it. The schema-2 hypothesis is
that the mask can distinguish those source-level conditions without exposing
raw metadata or changing permissions. Real `stat` layout and underlying cause
remain unresolved even if a particular bit is reported. The subsequent clean
`a3c2fc1` run observed only mask 4; no `/data` traversal or creation occurred.
This isolated the source predicate without validating native layout or clearing
protection. The separately selected [read-only consistency probe](NAMESPACE-PROBE.md)
now implements the next observation; this setup's checks remain unchanged.

Use the single-item `tools/setup_test_plan.json` with `prepare_ps5_window.py`
to build/package a fresh committed source revision. Keep the prior kit and log.
Prerequisites are the existing loader kernel-R/W resolver, reviewed 23 POSIX
exports plus `mkdirat`, and a current confirmed operator window. Only the fixed
setup traversal/possible missing-component creation above is admitted; there is
no new command, radio, capture or privilege operation. Native synchronous calls
retain their existing limitation. Observe the 15-second external ceiling and
controller before/during/after; stop on timeout, controller change or uncertain
cleanup. Collect immediately, review the mask and primary/cleanup results before
another launch. Do not infer a native layout fix, chmod/chown or credential
change from a predicate bit. A new build must be hash-verified before selection.

A success report requires verified/opened/closed masks all `0x0f`, zero current
and prior cleanup-failure masks, and the complete matching identity-bearing
outer lifecycle. Use the collector's setup profile and explicit controller
observations. Missing final lifecycle, ambiguous identity, incomplete collection
or unobserved freshness remains incomplete/Unknown; `payload_return status=2`
alone is not success. See
[PS5-WINDOW-DECISION-MATRIX.md](PS5-WINDOW-DECISION-MATRIX.md) for the next selected
application test and stop rules.

A missing optional `mkdirat` export or rejected existing permissions is a useful
negative prerequisite result. It does not justify a guessed API, additional
privilege scope, chmod/chown repair, or Bluetooth activation. Setup success only
clears this native directory prerequisite for the observed payload identity;
application import/entropy/store/listener, codec/capture and transport gates
remain independent.

## Offline fault checks

`tests/application/setup_namespace_test.c` includes the actual import bridge and
setup implementation with synthetic native functions, nodes and descriptors.
It never opens or writes the host's `/data`. It checks exact no-follow/cloexec/
directory flags, no root/data creation, both/one/no-missing-component paths,
protected existing parents, strict created/final modes and ownership, symlink/
nondirectory rejection, device/inode substitution, dirty/null/full-width lookup
errors, reentry, every fallible normal operation including EINTR, every
cancellation checkpoint, persistent confirmed creation, reverse consume-once
cleanup, primary-plus-cleanup failure, and sticky refusal after uncertainty.

`tests/application/setup_main_test.c` separately exercises the actual C main
branch with orchestration stubs: init failure/cancellation produces one import
report group, successful init produces two groups and exactly one shutdown,
setup success/cancellation/failure and shutdown failure preserve their events
and numeric reports, and aborting stubs prove that neither entropy nor the Rust
application is invoked. The existing generic baseline Rust probe remains the
finite status-2 seam. These mocks never resolve native exports.

Run both suites, each strict and sanitized, with:

```sh
python3 tests/application/run_setup.py --cc cc
```

The runner uses private temporary outputs and bounded compiler/test processes.
Strict C11 `-Wall -Wextra -Werror -pedantic` and ASan/UBSan executions pass.
The schema-2 follow-up covers each rejection bit, simultaneous root failures,
primary-plus-close failure, incomplete native stat output, and zero per-call
mask on cancellation/reentry/native errors. The 48 collector tests preserve
legacy schema-1 results and reject malformed/incoherent new masks; 16 window
tests and 13 preparer tests pass. Builder/package/kit self-tests and a target
link/structural inspection also pass. Rebuild/package from the final clean
commit before deployment; these offline checks are distinct from the subsequent
schema-2 hardware rejection recorded above.
These tests establish bounded logic and error preservation, not PS5 export,
permission, durability, coexistence or native cleanup acceptance. Target
compilation/link and kit identity are integration checks reported with the
actual artifact; the bounded native outcome now recorded in HW4 does not validate
unexecuted traversal, creation or durability paths.
