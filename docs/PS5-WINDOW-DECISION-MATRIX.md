# PS5 window: rapid test and decision matrix

Prepared 2026-10-26 for the reported PS5 Slim / 13.40, PayloadManager 0.6.3
and WH-1000XM4 environment. Reconfirm the actual machine and current execution
window before uploading or launching. This document grants no execution authority.

The plan `tools/window_test_plan.json` selects six independent artifacts.
The intended sequence is a separately selected fixed-directory setup candidate,
then three application candidates. Two standalone candidates isolate failures
only after a clean, reviewed outcome. Each item is a separate manual launch;
save its log before another launch truncates `/data/ps5-bt-hw1.log`.

There is **no admitted usable Bluetooth connection backend**, no ready safe
active-HCI test artifact, and no artifact that verifies the native PCM format.
The earlier native L2CAP result, errno 47, remains a transport blocker. Passing
all items below cannot establish headset connection, end-to-end audio or playback.
Do not add a radio test or enable `NativeAudio` on the strength of raw capture.

## New testing machine and console prerequisites

A Git clone supplies source, not a runnable kit. `build/`, `target/`, `.sdk/` and
`.tools/` are ignored. Transfer a verified prepared handoff, following
[WINDOW-HANDOFF.md](WINDOW-HANDOFF.md), or provision the pinned toolchain and
rebuild/package the selected artifacts using the offline doctor/build workflow
in [WINDOW-PREPARER.md](WINDOW-PREPARER.md), [BUILDING.md](BUILDING.md) and
[SOURCE-DISTRIBUTION.md](SOURCE-DISTRIBUTION.md). Source packages do not include
compiler installations. Retain the ELF, matching build manifest, source bundle,
kit manifest/runbook and independently communicated archive SHA-256 together.
Never relabel an old kit as a new build or mix manifests between machines.

Before any console operation, inspect the local kit and deployment dry-run.
Read-only preflight and collection are described in
[CONSOLE-WINDOW.md](CONSOLE-WINDOW.md); every helper defaults to dry-run.
Deployment and manual launch require current authorization. An upload does not
launch the payload. The read-only helper cannot prepare native directories,
create PayloadManager slots, or establish their native owner identity.

| Prerequisite | Required evidence | If missing |
| --- | --- | --- |
| Artifact identity | Matching selected ELF size/SHA-256, clean-source build manifest, source package and kit identity | Prepare/export or rebuild offline; do not launch a guessed binary |
| Console window | Current machine/environment and permitted selections confirmed | Keep the plan offline |
| Native store directory | `/data/ps5-bt/control` is a directory, exact mode `0700`, owned by the payload's native `geteuid()` identity; protected no-follow parents | Select the reviewed setup candidate when available, or use an independently established safe native setup; FTP visibility is insufficient |
| PayloadManager slots | Existing schema-2 local slots with exact filenames below, plus supported PayloadManager installation metadata | Use [PAYLOADMANAGER-SLOTS.md](PAYLOADMANAGER-SLOTS.md); its supported upload creates full metadata; FTP helpers only validate existing slots |
| Remote ELF | Read-back hash matches the selected local ELF; matching SIZE alone leaves hash Unknown | Stop before launch; repair only the intended slot under current deployment authority |
| Observations | Wireless controller checked before/during/after; witnessed manual launch and saved log before next launch | Retain Unknown for anything unobserved; never manufacture a fresh-run claim |

The exact setup/application slots under `/data/pldmgr/payloads/local/` are:

| Slot | `meta.json` filename field |
| --- | --- |
| `ps5-bt-application-setup` | `ps5-bt-application-setup.elf` |
| `ps5-bt-application-control` | `ps5-bt-application-control.elf` |
| `ps5-bt-application-generated` | `ps5-bt-application-generated.elf` |
| `ps5-bt-application-observe` | `ps5-bt-application-observe.elf` |

Each metadata record must have `schema: 2`, `kind: "local"` and the exact
`filename` above. These are the helper's minimum checks, not a claim that this
is the full PayloadManager 0.6.3 slot-creation schema. Use its supported setup
workflow; do not send a raw ELF to a guessed port or treat an old window's
network reachability as current evidence. Fallback slots are respectively
`ps5-bt-sbc-continuous` / `ps5-bt-sbc-continuous.elf` and
`ps5-bt-capture-stream` / `ps5-bt-capture-stream.elf`.

The application does not repair directory permissions. Its FileStore traverses
every component with held directory descriptors and `O_NOFOLLOW`; final mode
and native UID must match exactly. Parents must satisfy its protected-owner /
writability policy. `.p5bt.lock` is an owned private lock; Busy is not authority
to delete the lock or evict an owner. Existing file `fchmod` calls do not prepare
the directory. Never substitute FTP creation, speculative `SITE CHMOD`, broad
permissions, or an assumed root UID for native validation.

The separately selected `application-setup` candidate implements fixed-path
setup with [its own contract](APPLICATION-SETUP.md). It traverses held descriptors
from `/` through `/data`, creates only missing fixed `ps5-bt` and `control`
components at `0700`, and consumes descriptors once. Newly created components
and final `control` must match the native payload UID and exact `0700`; an
existing protected root-owned `0755` ps5-bt parent can be admitted. It never
chmod/chowns existing paths, creates keys, opens a listener or touches radio/
capture. `mkdirat` uses the pinned SDK declaration and a separately resolved
export; a missing export is a clear negative prerequisite result. Consult the
final kit identity before selection; offline checks are not hardware proof.
Without this artifact, the existing manual native-directory prerequisite remains.

## Candidate sequence and acceptance limits

Use a 15-second external observation ceiling per launch. Native synchronous
calls cannot be preempted by a Rust deadline; a missing return at that ceiling
is incomplete evidence and stops the sequence. Every completed item also needs
the exact identity-bearing outer lifecycle, its own successful inner profile,
known clean owned cleanup, and truthful controller observations. The outer
`payload_return status=2` is the expected diagnostic seam, not audio success.

| Selection | Bounded work | Passing evidence establishes | Next bounded step |
| --- | --- | --- | --- |
| `application-setup` | Fixed local directory setup/validation; at most 32 normal checkpoints plus four consume-once closes, per final kit contract | Protected path accepted under this native payload identity; no application/audio evidence | Save complete result, then application-control |
| `application-control` | Actual native imports/entropy, protected FileStore, ephemeral IPv4 loopback listener and Rust owner loop; maximum 5,000,000 observed us, 20,000 turns, 10,000 waits, 64 clients; each requested wait at most 1,000 us | Selected local startup, finite runtime and cleanup executed | application-generated |
| `application-generated` | Same startup plus observed-clock scheduling, persistent target SBC and discard sink; 750 blocks / 96,000 stereo frames in an exclusive 2,000,000-us work interval | Exact generated encoding and actual-clock admission/completion in this application | application-observe |
| `application-observe` | Same startup plus raw capture: 100-ms first settle, 10-ms cadence, exclusive 2,000,000-us interval, 256 hard read-call ceiling; at most 190 polls under exact regular timing, fewer when late, no catch-up burst | Raw capture lifecycle/status/byte/cadence evidence; Empty-only completion is possible | Review raw outcome; format remains Unknown |
| Fallback `sbc-continuous` | At most 750 generated blocks, 2,000,000 observed us, persistent real SBC, explicit finish, discard sink | Standalone codec wiring/bitstream and execution bound; scheduler times are synthetic, so physical pacing is not verified | Compare with the application's exact failed stage; fix offline before another app attempt |
| Fallback `capture-stream` | One immediate NOWAIT raw read, at most 2,000,000 observed us, explicit stop/close | One-read Rust/native capture wiring and cleanup; immediate Empty is inconclusive about available audio | Compare with repeated app observation; do not infer PCM or loop indefinitely |

The fallbacks bypass application entropy/store/listener startup, not cleanup or
coexistence requirements. Select them independently only after reviewing a clean
failure and its retained evidence. Neither fallback is permission to continue
after uncertain cleanup, controller changes, or missing final lifecycle.

Generated application acceptance requires all 750 blocks discarded, 57,750 bytes,
SBC prefix `9cfd20cb`, whole-stream FNV-1a `7784d1fb`, exact summary matches, no
concealment/rebuffer/expiry, and a **fresh post-discard completion observation**
before the exclusive work deadline. Matching counters without that observation
are incomplete timing evidence. The profile is joint stereo, 48 kHz, 16 blocks,
8 subbands, loudness allocation and bitpool 32. No sound is expected: there is
no capture input, RTP/ACL output or headset connection.

Raw Data, Empty, Zero, Overrun, Ended and Error remain distinct. Data establishes
bytes only, not PCM samples, channel layout, sample rate or timestamp units.
Overrun represents an unknown sample gap; do not invent a lost-frame count.
Empty/Zero are observable native outcomes, not PCM success. Capture bytes are
scrubbed and must not be added to shared reports or source control.

## Interpret the result before selecting another launch

| Evidence | Classification / stop decision | Concrete next step |
| --- | --- | --- |
| Complete expected profile, clean cleanup, matching identity and observed controller behavior | Candidate-specific pass only | Save the raw log and correlated JSON; advance one row above |
| Setup rejects an existing mode/UID/type/symlink, or app store rejects a missing/unsafe path | Negative prerequisite result, not Bluetooth failure | Preserve stage/errno/cleanup; inspect the actual native path through an authorized reviewed mechanism; do not relax permissions |
| Store Busy or uncertain prior owner | Ownership unresolved | Stop and reconcile the owner; do not delete `.p5bt.lock` or retry launches as a repair |
| Missing/dirty/null export, entropy unavailable, or validated native ABI failure | Concrete startup negative if outer lifecycle and cleanup complete | Use the mapping below; fix the exact source-backed prerequisite. A separately selected clean fallback may isolate an unaffected module |
| Generated summary differs, or codec cleanup fails | Codec/integration negative | Preserve exact counters/digest/finish result. After clean cleanup, standalone SBC can distinguish local codec from application startup/pacing |
| Correct generated bytes/digest but absent final completion observation | Incomplete timing evidence; no pacing pass | Inspect retained observed times/deadline; fix clock/admission logic offline before a bounded rerun |
| Any expired generated offer or expiry counter | Failed admission/timing contract, even with matching bytes/digest | Preserve exact expiry/failure counters; fix deadline admission before another bounded run |
| Standalone SBC passes while integrated generated fails | Standalone codec passed; app startup/actual pacing/integration remains unresolved | Compare exact startup and work-stage results; synthetic-time fallback cannot clear the app pacing gate |
| Observe completes with only Empty/Zero | Raw procedure completed; no nonempty capture evidence | Preserve counts/cadence/cleanup. Review native availability/source contract; do not call PCM verified or repeat without a new hypothesis |
| Repeated Data with clean completion | Raw byte/lifecycle evidence only | Design a separately admitted source-backed format-confirmation experiment; no ready format-confirmation artifact exists today |
| Overrun/unknown gap, terminal capture failure or clock regression | Stop the current experiment and review its cause | Preserve raw status, read count and cleanup; do not continue encoding or infer samples/time units |
| Controller changes, cleanup uncertainty, or missing final lifecycle at external ceiling | Stop the entire sequence | Preserve logs, reconcile native resources/coexistence; another launch is not cleanup proof |
| Matching build ID without witnessed current run | Freshness Unknown | Keep the record as unproven-current evidence; witness a later authorized manual launch and save before replacement |
| FTP failure / truncated collection / `.partial.log` | Collection failed or partial; not proof the payload failed to enter Rust | Preserve partial input and transport outcome; a bounded read-only recollection may recover the same log before any next launch |
| All candidates pass | Local startup/encoding/raw observation established within each contract | Transport ownership/channel/backend and native format evidence still block an end-to-end headset test |

A matching text build ID correlates a log with build inputs; it is not an
execution attestation and cannot distinguish two runs of the same build.
`--run-observation observed` records an operator statement only. The helper's
`ps5_log_freshness` remains Unknown. Unknown controller observations remain
Unknown even when a numeric profile passes. Save unique private raw logs and
results; never overwrite a previous run to make a sequence appear complete.

## Exact native import triage

The original application bridge resolves all 23 exports before opening a
descriptor. For a non-cancelled **stage-2** lookup failure, convert
`application_import_resolved` from its logged hexadecimal value to decimal:
that many preceding exports succeeded; the attempted export is this index.

| Index | Export | Index | Export | Index | Export |
| --- | --- | --- | --- | --- | --- |
| 0 | `__error` | 8 | `getsockname` | 16 | `fstatat` |
| 1 | `open` | 9 | `setsockopt` | 17 | `fchmod` |
| 2 | `openat` | 10 | `recv` | 18 | `flock` |
| 3 | `fcntl` | 11 | `send` | 19 | `fsync` |
| 4 | `socket` | 12 | `close` | 20 | `renameat` |
| 5 | `bind` | 13 | `read` | 21 | `unlinkat` |
| 6 | `listen` | 14 | `write` | 22 | `geteuid` |
| 7 | `accept` | 15 | `fstat` | — | — |

Stage 1 validates init/reentry. Stage 3 validates the current-thread errno
provider after all 23 resolutions: failure with resolved=23 is **not** a missing
24th export. Cancellation status 1 can occur before/after lookup and is not a
missing-symbol finding. Stages 4/5/6 cover entropy open/type/read; stage 7 is
entropy-close failure when it becomes primary; cleanup flags separately retain
close failure after an earlier cause. Stage 8 is import-table shutdown.
Preserve status bits, raw errno, full lookup/return high/low halves, resolved
count and cleanup flags/errno together; do not reduce all failures to one errno.

The setup candidate's optional `mkdirat` resolution is separate from this
23-export table; use its own report. `clock_gettime`/`usleep` and capture module
exports are also separate adapters. Read [PS5-POSIX-IMPORTS.md](PS5-POSIX-IMPORTS.md)
and the selected manifest/source before interpreting those failures. The pinned
SDK declaration set is revision
`a7685ce8e97ce1f8a4f38253a41d54241c9124f1`; declarations are not native-export
availability proof. No guessed alternate name, native layout, syscall, broader
credential scope or system-driver change is a fallback. The import table cannot
be reinitialized after successful use in the same process.
