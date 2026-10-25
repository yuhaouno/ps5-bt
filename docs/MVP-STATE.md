# MVP orchestration state

Last updated: 2026-10-25. The explicit HW-1 diagnostic now executes on the
recorded Slim/13.40 environment. MVP audio is not operational.

## Current milestone and baseline

Runtime minimal-syscall-v6 removes unsafe executable-byte inspection, normalizes
raw syscall errors, and adds an explicit existing-loader kernel resolver.
Kernel initialization returned 0, Rust entry was logged, notifications returned
API success and the durable log ended with final 2. Ordinary write lookup had
failed with -1. The native query resolved its export, called once and returned
-1; controller/version remain Unknown. No credential setter is required by the
observed diagnostic path.

The user confirmed on-screen notifications and wireless DualSense during the
diagnostic. Own USB enumeration has now read 1286:2059 with HCI-class interface
groups 0 and 2. Capture preflight resolved all seven exports; the first full
capture lifecycle completed with one immediate Empty read and clean shutdown.
The renewed 2026-10-25 window produced 8,192 bytes after a 100 ms settle with
clean cleanup and user-confirmed normal wireless input. Format/timing remain
unknown; see HW3-EVIDENCE.md and its complete numeric logs.
Current target is 192.168.1.111; the user renewed hardware access from the LAN.
Active HCI operations remain gated on native traffic ownership
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
  proprietary binary/key or unrelated expansion. The thin LAN control page and
  the conservative-SBC protocol/platform work in MVP-NEXT-RUN.md are authorized.

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
| C1b | EXECUTED (PARTIAL) | lead + user | own 1286:2059 four-interface descriptor log, clean close and normal pad; three other opens failed; all binding queries returned Unknown names in HW3 |
| C2 | IN PROGRESS | lead | native query returned -1; next bounded experiment is unconnected L2CAP socket creation/close (NATIVE-L2CAP-PROBE.md). Native route and raw HCI ownership remain unknown; no direct USB activation |
| C3 | EXECUTED | lead | export resolved on Slim/13.40; single query returned -1; version/controller remain Unknown |
| H1 | LIFECYCLE PASS | user + lead | loader kernel init 0, Rust entry and final 2 on Slim/13.40; user confirmed visible notifications and normal wireless input |
| H2 | PARTIAL | user + lead | USB descriptors and normal pad confirmed; HCI queries/ownership/credits remain unverified and gated by C2 |
| C4 | NONEMPTY READ | capture worker + lead + user | settled read returned 8,192 bytes; all lifecycle/cleanup statuses 0 and user-confirmed normal wireless input; native PCM format/timing still unknown |
| E1a | DONE | sdp_framing + independent/lead review | bounded SDP PDU/continuation/data-element foundation and A2DP Sink discovery client, with transaction/epoch/deadline/continuation and exact UInt16 service semantics; no live query |
| E1b | PARTIAL (HOST) | lead | bounded initiator-side Classic L2CAP Connection/MTU Configuration/Disconnection plus acceptance-confirmed outbound ACL fragmentation pass scripted tests; responder allocation, Classic baseband connection/pairing/key lifecycle and live activation remain |
| F1 | PARTIAL (HOST) | lead | bounded complete-frame SBC-over-RTP packetization and AVDTP Discover/Get Capabilities/Set Configuration/Open/Start/Suspend/Close/Abort pass host tests; signaling reassembly, media binding and live transport remain |
| G1a | DONE | sbc_audit + independent/lead review | validated shared SBC selections and LGPL/runtime audit; implementation follows in G1b |
| G1b | TARGET SELFTEST PASS | codec worker + lead + user | generic libsbc caller-storage path encoded four generated frames in two lifecycles on PS5 with exact host digest and clean cleanup; source/object/replacement materials tested; encode timing and live integration remain |
| G2a | DONE (HOST) | lead | caller-owned stereo i16 PCM queue, absolute nominal sample deadlines, bounded counted silence/rebuffer/expiry; ten media tests and four synthetic scenarios; codec example in T1, no live capture/transport |
| T1 | PARTIAL (HOST) | lead | real PCM-to-SBC example emits one 77-byte frame; a bounded media cursor wraps complete SBC frames through RTP/A2DP, L2CAP and acceptance-confirmed ACL fragmentation with exact retry/cancellation accounting; clock/credit pacing and hostile live headset/session remain |
| H7 | HARDWARE | user + lead | 440 Hz SBC A2DP tone audible on real headset, after controller/connection/negotiation |
| H8 | HARDWARE | user + lead | actual PS5 PCM audible through the proven tone pipeline |
| I1a | DONE | lead | pinned build/source-package instructions plus a clean-manifest, hash-checked binding/settled-capture kit; deployment defaults to dry-run and never launches |
| I1b | PARTIAL (HOST) | lead | bounded authenticated request parser, one-command ownership and redacted actual-state JSON; listener/static page and PS5 owner integration remain; no false pairing/audio capability |

Completed waves: upstream/runtime research and real minimal ELF; portable HCI,
L2CAP, SBC selections and SDP framing; host-only descriptor diagnostics; separate
native-query ELF. Each was reviewed and tested before atomic integration.
Do not declare hardware completion from desktop simulation or an ELF link.

## Persistent blockers and next integration target

Transport access is the immediate critical path: run the separate native L2CAP
availability candidate, then use the result to select route/connection ABI
review or close that particular socket branch. Do not repeat capture or SBC
self-tests without a changed implementation or a concrete unanswered question.
Next acceptance remains real headset connection, a short audible SBC tone,
then captured audio and the minimal LAN controls. A native channel backend may
leave HCI/credits under the system owner; it requires a separately reviewed
channel ABI rather than pretending the existing raw-HCI ABI was implemented.

The baseline diagnostic has passed its executable lifecycle gate on one
environment. Human-visible notifications and wireless DualSense were confirmed;
deliberate panic and audio coexistence remain untested. Source/host evidence does not generalize
that firmware/loader compatibility.

The read-only binding extension and settled capture read have now executed in
the renewed window. Driver names remain Unknown; the settled read returned
8,192 bytes. Native format/timing remain unknown. The generated-signal target
codec now passes two lifecycles/four frames with exact host digest; bounded
capture observation also passed Empty then Data using the checked clock adapter.
Native HCI command/event/ACL ownership and enforceable credits remain unresolved;
existing upstream competing readers do not establish shared routing.
The first offline PCM queue, nominal sample timeline and recovery simulation
are implemented under AUDIO-RECOVERY.md. The real host PCM-to-SBC path is now
implemented under SBC-IMPLEMENTATION.md. Complete-frame RTP packetization, SDP
discovery and the bounded AVDTP initiator session are now done; next media work
is signaling reassembly, media-channel binding, pacing and explicit capture
conversion. Tone, pairing, captured
PCM and live audio remain later milestones; simulation does not authorize a
shared-reader backend.
