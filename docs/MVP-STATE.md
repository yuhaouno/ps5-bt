# MVP orchestration state

Last updated: 2026-10-24. The explicit HW-1 diagnostic now executes on the
recorded Slim/13.40 environment. MVP audio is not operational.

## Current milestone and baseline

Runtime minimal-syscall-v6 removes unsafe executable-byte inspection, normalizes
raw syscall errors, and adds an explicit existing-loader kernel resolver.
Kernel initialization returned 0, Rust entry was logged, notifications returned
API success and the durable log ended with final 2. Ordinary write lookup had
failed with -1. The native query resolved its export, called once and returned
-1; controller/version remain Unknown. No credential setter is required by the
observed diagnostic path.

Next: human notification/wireless DualSense confirmation, then bounded read-only
USB enumeration. Active HCI operations remain gated on native traffic ownership
and credits. See HW1-REPAIR.md and RUNTIME-REVIEW.md. Earlier reports describe
their historical software/transfer baselines and are not current runtime state.

## Shared interfaces and integration rules

- Retain `bt_core::BluetoothTransport`: complete header-bearing command/ACL
  buffers without H4; one ordered, tagged receive stream into caller storage;
  bounded millisecond wait. Do not split receive into competing native readers.
- `bt_core::PcmSource` is the current AudioSource contract: declared format,
  interleaved f32 caller buffer, whole frame count, optional monotonic timestamp
  and discontinuity. Do not pass floats by value through the freestanding C ABI.
- `Error` categories and raw backend codes remain authoritative. Unknown is not
  success; no fake credits, silent queue loss, resets or unbounded retry loops.
- C ABI v1 and pointer/lifetime/token rules in ABI.md apply. A worker proposes
  changes centrally before editing another owner's contract.
- Lead owns Cargo.toml/lock, README, CHANGELOG, ROADMAP and this board. Workers
  do not stage, commit, reset, stash or modify others' paths. Lead reviews and
  commits each completed feature with tests/docs. Tests use the pinned Rust
  environment documented in BOOTSTRAP-REPORT.md until tooling is made durable.
- License review precedes source reuse. Existing-loader access and necessary
  own-process permission changes follow AGENTS.md; no new exploit chain,
  proprietary binary/key, unauthorized codec, GUI or other MVP expansion.

## Task graph

| ID | State | Owner | Task / dependency / evidence |
| --- | --- | --- | --- |
| B0 | DONE | lead | inspect repository and repeat documented baseline |
| A1 | DONE | platform_research + lead review | exact SDK/loader pins, startup/syscall contract, metadata-only probe and capture boundaries, native routing blocker and per-file license review; see PS5_PLATFORM.md |
| B1 | DONE | build_toolchain + independent/lead review | real inspected diagnostic ELF, original minimal entry/linker, three hash-verified SDK syscall inputs; no stock permission-changing CRT |
| D1a | DONE | hci_l2cap + lead review | safe HCI command/event/ACL and capability/completion types; malformed-input tests; no credit accounting or live transport |
| D1b | DONE | hci_l2cap + lead review | bounded L2CAP framing/reassembly in caller storage, tied to explicit handle/epoch; no signaling/channel establishment |
| A2 | DONE | native_hci_research + lead review | source-backed single read-only native stack-version query; no PS5 broker found in bounded inventory; see NATIVE-HCI-RESEARCH.md |
| C1a | DONE | platform_descriptors + lead review | bounded descriptor parser/report, hostile host tests and ASan/UBSan; no live USB access |
| C1b | WAITING | platform worker | actual read-only descriptor probe after verified runtime operations/SDK headers; no endpoint takeover |
| C2 | BLOCKER | lead + platform worker | native HCI command/event/ACL ownership and enforceable credits; upstream read competition is not routing |
| C3 | EXECUTED | lead | export resolved on Slim/13.40; single query returned -1; version/controller remain Unknown |
| H1 | LIFECYCLE PASS | user + lead | loader kernel init 0, Rust entry and final 2 on Slim/13.40; visible notifications and wireless input await user confirmation |
| H2 | HARDWARE | user + lead | HW-2 descriptor/HCI data and DualSense before/during/after; requires C1/C2 |
| E1a | DONE | sdp_framing + independent/lead review | bounded SDP PDU/continuation/data-element foundation, nine parser tests; no discovery client |
| E1b | WAITING | connection worker | bounded Classic connection/pairing/key lifecycle and A2DP SDP discovery after packet interfaces |
| F1 | WAITING | AVDTP/A2DP worker | typed SBC negotiation/open/start/close and RTP packetization after D1/E1 contracts |
| G1a | DONE | sbc_audit + independent/lead review | validated shared SBC selections and LGPL/runtime audit; no encoder imported |
| G1b | WAITING | codec worker | proven libsbc encoder behind narrow wrapper, after bounded initialization storage and relinking plan; per-frame allocation prohibited |
| T1 | WAITING | integration worker | hostile scripted transport/headset and PCM-to-SBC-to-RTP-to-ACL simulation once module contracts exist |
| H7 | HARDWARE | user + lead | 440 Hz SBC A2DP tone audible on real headset, after controller/connection/negotiation |
| H8 | HARDWARE | user + lead | actual PS5 PCM audible through the proven tone pipeline |
| I1a | DONE | build_docs_integration + lead | pinned build/source-package instructions and exact baseline loading/log checklist |
| I1b | WAITING | documentation worker | pairing/configuration/audio workflow only when those capabilities exist |

Completed waves: upstream/runtime research and real minimal ELF; portable HCI,
L2CAP, SBC selections and SDP framing; host-only descriptor diagnostics; separate
native-query ELF. Each was reviewed and tested before atomic integration.
Do not declare hardware completion from desktop simulation or an ELF link.

## Persistent blockers and next integration target

The baseline diagnostic has passed its executable lifecycle gate on one
environment. Human-visible notifications, wireless DualSense and deliberate
panic still require confirmation. Source/host evidence does not generalize
that firmware/loader compatibility.

The next platform work is read-only USB metadata through a verified runtime.
Native HCI command/event/ACL ownership and enforceable credits remain unresolved;
existing upstream competing readers do not establish shared routing.
Tone, pairing, capture and audio remain later milestones. No active protocol
expansion is required for this runtime repair.
