# 2026-10-26 application window: retained failure and slot cleanup

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

The next bounded candidate adds a six-bit rejection mask under setup ABI 2/log
schema 2, with the same native operations, protection and cleanup bounds. See
the [admission and bit meanings](APPLICATION-SETUP.md#admitted-schema-2-follow-up).
Prepare only the single-item setup plan, keep the old kit, verify the new hash
and collect before any next launch. The new mask requires hardware evidence;
it must not be inferred retrospectively from this schema-1 run. No other profile
has acquired a new result and no application workload is cleared by this test.

Native PCM format/time, Bluetooth ownership/channel availability and allowance
remain gated. No pairing, headset playback or audio acceptance was established.
