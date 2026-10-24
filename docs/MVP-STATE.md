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

The user confirmed on-screen notifications and wireless DualSense during the
diagnostic. Own USB enumeration has now read 1286:2059 with HCI-class interface
groups 0 and 2. Capture preflight resolved all seven exports; the first full
capture lifecycle completed with one immediate Empty read and clean shutdown.
No PCM was acquired. See HW2-EVIDENCE.md and its complete numeric logs.
Future target is 192.168.1.111; the hardware window has ended. Continue offline.
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
| C1b | EXECUTED (PARTIAL) | lead + user | own 1286:2059 four-interface descriptor log, clean close and normal pad; three other opens failed; binding extension unverified |
| C2 | BLOCKER | lead + platform worker | native HCI command/event/ACL ownership and enforceable credits; upstream read competition is not routing |
| C3 | EXECUTED | lead | export resolved on Slim/13.40; single query returned -1; version/controller remain Unknown |
| H1 | LIFECYCLE PASS | user + lead | loader kernel init 0, Rust entry and final 2 on Slim/13.40; user confirmed visible notifications and normal wireless input |
| H2 | PARTIAL | user + lead | USB descriptors and normal pad confirmed; HCI queries/ownership/credits remain unverified and gated by C2 |
| C4 | LIFECYCLE PASS | capture worker + lead + user | Ipmi/Avcap2 load, seven resolves, initialize/open/start/cleanup succeed; one immediate Empty read; settled-read test requires hardware |
| E1a | DONE | sdp_framing + independent/lead review | bounded SDP PDU/continuation/data-element foundation, nine parser tests; no discovery client |
| E1b | PARTIAL (HOST) | lead | bounded initiator-side Classic L2CAP Connection/MTU Configuration/Disconnection state machine passes scripted peers; responder allocation, ACL fragmentation, SDP discovery, Classic baseband connection/pairing/key lifecycle and live activation remain |
| F1 | PARTIAL (HOST) | lead | bounded complete-frame SBC-over-RTP packetization passes MTU/backpressure/sequence/sample timestamp tests; AVDTP negotiation/open/start/stop and media binding remain |
| G1a | DONE | sbc_audit + independent/lead review | validated shared SBC selections and LGPL/runtime audit; implementation follows in G1b |
| G1b | DONE (HOST) | codec worker + lead | real generic libsbc with caller-storage C ABI and safe Rust owner; 208 vectors, 12 decodes, replacement-archive linking; PS5 ABI/distribution remain gated |
| G2a | DONE (HOST) | lead | caller-owned stereo i16 PCM queue, absolute nominal sample deadlines, bounded counted silence/rebuffer/expiry; ten media tests and four synthetic scenarios; codec example in T1, no live capture/transport |
| T1 | PARTIAL (HOST) | lead | real PCM-to-SBC example emits one 77-byte frame and wraps it in 90-byte RTP/A2DP, 94-byte L2CAP and 98-byte synthetic ACL; pacing, fragmentation and hostile scripted headset/session remain |
| H7 | HARDWARE | user + lead | 440 Hz SBC A2DP tone audible on real headset, after controller/connection/negotiation |
| H8 | HARDWARE | user + lead | actual PS5 PCM audible through the proven tone pipeline |
| I1a | DONE | build_docs_integration + lead | pinned build/source-package instructions and exact baseline loading/log checklist |
| I1b | READY (MVP) | control/integration worker | thin LAN control/status interface with real unavailable states and bounded commands; no false pairing/audio capability |

Completed waves: upstream/runtime research and real minimal ELF; portable HCI,
L2CAP, SBC selections and SDP framing; host-only descriptor diagnostics; separate
native-query ELF. Each was reviewed and tested before atomic integration.
Do not declare hardware completion from desktop simulation or an ELF link.

## Persistent blockers and next integration target

The baseline diagnostic has passed its executable lifecycle gate on one
environment. Human-visible notifications and wireless DualSense were confirmed;
deliberate panic and audio coexistence remain untested. Source/host evidence does not generalize
that firmware/loader compatibility.

The next platform checks are the read-only binding extension and settled
capture read, in a newly confirmed hardware window. Those have not executed.
Native HCI command/event/ACL ownership and enforceable credits remain unresolved;
existing upstream competing readers do not establish shared routing.
The first offline PCM queue, nominal sample timeline and recovery simulation
are implemented under AUDIO-RECOVERY.md. The real host PCM-to-SBC path is now
implemented under SBC-IMPLEMENTATION.md. Next offline work needs RTP/AVDTP
packet/session boundaries and explicit capture conversion. Tone, pairing, captured PCM and live audio
remain later milestones; simulation does not authorize a shared-reader backend.
