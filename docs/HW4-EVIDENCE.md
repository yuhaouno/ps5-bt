# 2026-10-26 window: application blocker, SBC and capture outcomes

The user confirmed the current Slim / firmware 13.40, PayloadManager 0.6.3,
WH-1000XM4 environment and authorized uploads/tests. The manager's actual
`http://192.168.1.111:8084/version` returned `0.6.3`. This dated record does not
authorize a future window.

## Build and deployment identity

Clean source HEAD: `20a5d665abd4c6a5fccb3125c622b65cb14f1220`.
The initial doctor could not locate LLVM without an explicit configuration.
Rust 1.90.0, its target/components and Homebrew LLVM 18.1.8 were already installed;
doctor passed with `/opt/homebrew/opt/llvm@18/bin/llvm-config`. No toolchain
installation was necessary. `prepare_ps5_window.py --build` completed all 13
ordered build/package/assembly steps and validated the six-item kit:

`build/ps5/next-window-kit-20261026-20a5d66`

Its adjacent `-preparation/identity.json` records `status: ready`. Each ELF passed
the builder's synthetic startup and structural checks and has matching source
materials. These are local build checks, not hardware acceptance. Full
`make check` was not run in this window; its previously recorded ARM-host x86
execution limitation remains unresolved.

All six exact local slots were initially absent. The version-reviewed manager
`mode=new` API imported them. The first upload succeeded, but the client wrongly
assumed its response was JSON and stopped. Read-only metadata and complete ELF
hash verification reconciled that upload before continuing; no duplicate upload
was attempted. All six subsequently passed metadata, size and SHA-256 readback.
No upload launched a payload.

## Operator report and evidence loss

The user reported executing all ELFs and normal controller operation throughout.
Per-item logs were not collected between these manual launches. Their exact
order, individual return times and individual outcomes were not recorded.
The single fixed log retained **application-generated**, not capture-stream.
The initial capture-stream collection therefore correctly reported
`identity_mismatch`; recollection against the matching generated manifest
reported `complete_failure`. Both saved inputs have SHA-256:

`db6a2391c1fdd3fcdbac93ebd1dfc3e14235fe5bc0f54152f9bb2f050d367a70`

Private raw logs/results remain under ignored `build/ps5/console-window/`,
including `application-generated-jffcns45.log` and its JSON result. No raw log,
token, key or capture data is committed. The collector retains
`ps5_log_freshness: Unknown`: build identity and the operator statement do not
independently attest a particular launch. Other candidates' outcomes remain
Unknown, even with reported normal controller behavior.

## Retained generated-application failure

ELF SHA-256: `07b0df7104f65da1ccfcd8026ac8e5802cfcc63cf06d4200a707f26ce1978575`.
Embedded build ID: `b35bd5bd7c24c98ee6afdadc6bf74b1bc3d6fb8a8b067add07e6a83d5f47daaf`.

The log contains matching begin/finish identity and final payload status **1**.
All 23 POSIX exports resolved; entropy completed a 16-byte read and clean close.
The factory acquired the clock, made one clock observation, then failed Store
acquisition before Listener/Capture/Encoder acquisition or generated work:

| Field | Value / source interpretation |
| --- | --- |
| `failed_resource` | 1: Store |
| `error_stage` | 2: `PS5_BT_STORE_DIRECTORY` |
| `error_bits` | `0xfffffffe`: `PS5_BT_STORE_IO` (-2) |
| `native_status_bits` | 1: raw errno, EPERM in the pinned SDK |
| `error_flags`, native cleanup values | All zero |
| `active_mask`, `cleanup_mask`, `cleanup_failed_mask` | 0, 1, 0 |
| `cleanup_uncertain` | 0; import shutdown and preflight cleanup completed |

In `platform/store/bond_file_store.c`, this directory-stage I/O error can arise
from initial root `open`, a component `openat`, or a subsequent `fstat`.
The precise call/component is **Unknown**. It is not the explicit UID/mode
policy rejection (-4 with errno 0), and the lock/flock stage was not reached.
The report does not justify chmod/chown, deleting a lock, changing credentials
or replacing a native API. No SBC result or pacing success follows.

## Slot cleanup and subsequent setup run

A setup-only rerun was requested to obtain its finer stage/mask evidence, but
the user could not identify the entry in the overflowing frontend list and
requested removal first. The first rerun request therefore produced no evidence.

At the user's request, the manager's reviewed `/manage:delete` interface removed
all eight PS5-BT slots: the six new candidates plus historical
`ps5-bt-native-l2cap-probe` and `ps5-bt-capture-observe`. A fresh manager inventory
confirmed zero PS5-BT slots and unchanged unrelated slot identities. Local kit,
source packages and collected evidence remain available. At that cleanup
checkpoint no candidate remained; no native private directory/lock was altered.
The inspected delete route/owner are from `itsPLK/ps5-payload-manager` revision
`3f881e8e569eba3054d1784a1962439b2eee92c2`; their source hashes match the existing
[0.6.3 review](PAYLOADMANAGER-SLOTS.md). Its GPL-3.0 license was inspected; no
implementation was copied. Exact `local/ps5-bt-*` slot keys were used. The owner
also removes those selected slots' autoload/favorite entries as part of deletion.

The user then asked to continue in the same session. Only the original canonical
setup slot was reimported and verified against its original kit:

- ELF SHA-256: `9de999a6738f30f9d72ca2bf1a3eac7b69bdb08fe795ad288fe1b53c9413b504`.
- Build ID: `f0d6937b54ecc2d875306879bcb72d9bb9c15033c14199c2135aff5eff1b259d`.
- Source HEAD: `20a5d665abd4c6a5fccb3125c622b65cb14f1220`; schema 1.

The operator confirmed return within the requested ceiling and normal controller
behavior before/during/after. Immediate collection saved private
`application-setup-727vy8yq.log` and JSON, classified `complete_failure`.
Log SHA-256: `882d64685f9c648962f0087e1934cf78e1cfe0488dbbf0ea3c2b589e4edc5bbb`.
It contains matching run begin/finish and final payload return 1:

| Setup field | Observed value |
| --- | --- |
| stage / status / errno | 2 / -4 / 0 |
| operations | 4 |
| opened / closed / verified / created masks | 1 / 1 / 0 / 0 |
| native raw return / mkdirat lookup | 0 / 0 |
| current/prior cleanup-failure masks and raw cleanup | All zero |
| POSIX imports | 23 resolved; clean shutdown |

The inspected source makes these four operations mkdirat lookup, effective UID,
root open and root fstat. Open succeeded and fstat returned 0, followed by root
policy rejection and exactly one successful close. No `/data` traversal or
directory creation occurred. This is a different failure classification from
the earlier Store I/O/errno-1 result; it does not establish a common root cause.
The rejected root predicate and actual stat layout remain Unknown. Matching
identity plus the operator report still leaves collector freshness Unknown.

## Schema-2 setup result

The follow-up was built and packaged from clean source
`a3c2fc150346db1cd6963351d49040ab380630f2` using the single-item setup plan.
Doctor and all three preparation steps passed. The separate immutable kit is
`build/ps5/setup-predicates-kit-a3c2fc1`; the original six-item kit remains intact.
Only setup was visible, and its replacement ELF passed full remote hash readback:

- ELF SHA-256: `4541dae6ab67ec8c8a2215b6da92acf47fd66d63162afef3641c6d4a52600e90`.
- Build ID: `9a468c7da9b080faa7a7866bd4e3fca6e0dcf7de40b73155b3a97426c015b9fd`.
- Manifest SHA-256: `3f61ac9a28076410df6109023642086f43771868c1f8ea704805de9c23025ec5`.
- Private log/result: `application-setup-twvp6ofq.log` / `.json`.
- Log SHA-256: `03da1dfca3577aaffee1cfd70d35098c39eb7ac2f471c57aaf1ab8e8d8a45b0b`.

After the operator reported return and normal controller input before/during/after,
immediate collection classified **complete_failure**. Stage 2, status -4,
errno 0, operations 4 and opened/closed/verified/created masks 1/1/0/0 repeat
the prior run's bounded outcome. All current/prior cleanup fields are zero;
imports resolved and shut down cleanly, with matching outer lifecycle and return 1.
The new `validation_failed_mask` is **4**, solely `PARENT_WRITE`.

Under the compiled SDK interpretation, the root metadata has group/other write
permission without sticky protection. Type and parent-owner predicates did not
fail. This does not disclose an exact mode/UID, independently verify native stat
layout, establish mount/ACL authority or prove a common cause with the earlier
Store I/O error. No `/data` traversal or directory creation occurred. Protection
was not relaxed, and no existing mode, owner or credential was changed.

## Conditional standalone SBC result

The clean setup failure allowed the matrix's independent isolation tests.
Setup was removed and only `ps5-bt-sbc-continuous` imported, then hash-verified.
This candidate deliberately used the original clean **20a5d66** kit and matching
source/manifest, not the newer setup build:

- ELF SHA-256: `62942dcdd35d4e2f49fb45bb4e882b63bd195a1e4e807f60b0d43f8c1e21ecd0`.
- Build ID: `b1a4f177f87d2596d4338e259cbe3d2bf3aa49c249c7972eb222880ade53b84f`.
- Manifest SHA-256: `663f4006d6cd1be7ef2b5b6234e7d69ec99f02e7ca79ed3549947020fa220ee7`.
- Private log/result: `sbc-continuous-dociljsj.log` / `.json`.
- Log SHA-256: `d774ca0e81495329daea92e8cae8eb1d58fedd3c885ad09fcfab7cb28fd6376c`.

The operator reported return and normal controller input before/during/after.
Immediate collection classified **complete_success** with matching outer lifecycle
and expected diagnostic return 2:

| Field | Observed value |
| --- | --- |
| outcome / cleanup attempted | 0 / 1; cleanup reported complete |
| blocks / stereo PCM frames | 750 / 96,000 |
| encoded bytes | 57,750 |
| SBC prefix / whole-stream FNV-1a | `9cfd20cb` / `7784d1fb` |
| observed elapsed microseconds | 126,564, below the 2,000,000-us bound |

This establishes the bounded generated-PCM Rust/target SBC path and its discard
output for this run. The nominal sample clock is synthetic; elapsed execution
is neither real-time pacing evidence nor acoustic/end-to-end latency. No capture,
Bluetooth submission or headset playback occurred. Integrated generated mode
remains blocked before encoding by application startup.

## Conditional capture-stream result

After saving SBC evidence, only `ps5-bt-capture-stream` was installed and
hash-verified, again from the original clean **20a5d66** kit:

- ELF SHA-256: `72ad2b60496ae41fb1cdd034409f3819d40779a7553e0596f91a8e9dcef9f986`.
- Build ID: `fc26cef520bcef76f600f66410d2331758283e3d9ffb61a262d47880265fa83b`.
- Manifest SHA-256: `ba907fcc7cdfcc222111c9b930e6de5fae3f35ae127815f8997af553530d4f5b`.
- Private log/result: `capture-stream-c6wuovit.log` / `.json`.
- Log SHA-256: `48f8e0dadcfc8c4370b0ff38e8a4181e0a19b530fabc6d1c496970aad9f0318e`.

The operator reported return and normal controller input before/during/after.
Immediate collection classified **complete_success for the procedure**, with
matching outer lifecycle and expected return 2. Exactly one immediate NOWAIT
read reported kind 3 (Empty), native status `0x81950002`, zero bytes and no
reported discontinuity. Stop/close statuses were zero; explicit cleanup and
capture completion markers were present. No second read or Empty retry occurred.
This establishes one-read Rust/native lifecycle and cleanup, not nonempty PCM,
format, native timestamp units or continuous-read behavior.

All three separately collected runs record operator observation as `observed`
and controller acceptance as `reported_normal`. Collector `ps5_log_freshness`
and audio acceptance remain **Unknown**; matching identity is not independent
execution attestation. No general coexistence or stability acceptance follows
from these brief observations.

## Final state and next bounded investigation

After retaining the capture result, its exact slot was removed using the same
reviewed manager route. Final inventory confirmed **zero PS5-BT slots** and
unchanged unrelated slots. Local kits, matching sources and private logs remain.
The original control/observe outcomes were overwritten and remain Unknown;
they were not relaunched after setup rejection. The generated startup failure
is retained; no integrated application workload passed.

The next proposed candidate is an independently selected **read-only root/data
metadata consistency probe**, not yet implemented or ready to execute. Use only
already reviewed `geteuid/open/openat/fstat/fstatat/close`: compare root descriptor
metadata with `fstatat(root, ".")`, and root-relative `data` metadata with its
opened descriptor. A proposed bound is seven normal calls plus at most two
reverse, consume-once closes, no retries and the same 15-second external ceiling.
Any native-call failure stops acquisition and triggers owned cleanup. Continuing
read-only observation after a policy rejection must be explicit in its separate
admission; it must never create directories or accept the store. Report only
predicate/agreement bits, device-equality booleans, errors and cleanup, not raw
UID/stat data. Review implementation, layout/lifetimes, finite bounds, artifact
identity and current window before execution.

Agreement could reveal consistency but cannot independently attest the native ABI
or clear the protected-parent rule. Firmware-matched ABI evidence and protection
of the root-to-data edge remain prerequisites. Do not add a root exception, change
permissions or test rename/unlink against system paths. `fstatfs` is outside the
current reviewed import/header closure and would need separate admission.
Native PCM format/time, Bluetooth ownership/channel availability and allowance
remain gated. No pairing, headset playback or audio acceptance was established.
