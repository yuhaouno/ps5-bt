# MVP orchestration state

Last updated: 2026-10-26. The explicit HW-1 diagnostic now executes on the
recorded Slim/13.40 environment. MVP audio is not operational.

The user selected Sony **WH-1000XM4** (over-ear) as the first planned headset.
It has not been discovered, paired or tested by this project; no compatibility
claim or Bluetooth address is recorded.

## Current development authority, 2026-10-26

The user currently has no PS5 test environment. Do not infer access from the
historical LAN window below. Implement coherent candidates with PS5 validation
pending; functional, audio-quality, latency, coexistence and stability acceptance
remains on PS5, and a Linux Bluetooth backend is not a prerequisite.
AGENTS.md permits scoped reviewed transitional C protocol modules, independent
experimental capture adapters and minimal LAN discovery/pairing/forget-device
controls. Fault recovery is bounded per episode; local bond keys may be securely
persisted but never logged, exposed or committed. Source-backed isolated
transport experiments have separate admission from production support; no
competing-reader, timer-credit or system-driver exception is implied. Existing
tests, implementations and hardware facts below are unchanged by this policy.

## Discovery and management continuation, 2026-10-26

This supersedes software omissions in older snapshots below. No new PS5 window,
ELF execution, pairing, playback or coexistence evidence is added. Rust remains
the mainline; C is limited to reviewed opaque filesystem/socket adaptation.

- `a772eb9`: bounded Inquiry/Cancel, validated EIR names and stable device IDs.
- `83e04c9`: concrete protected POSIX bond files, exact ownership and atomic
  durability boundaries; external target linking is separate from host builds.
- `20e424d`: typed v2 LAN device/pairing service, static page and finite socket
  pump; actual-state actions, protected requests and urgent/primary ownership.
- `e8ce47f`: portable management/application runner and shared-credit ACL pumps.
  Covers real wire discovery-to-connect, exact SSP decision mapping, retained
  input, healthy partial-PDU ordering, aggregate deadlines and sticky clock loss.
- `306256f`: the optional POSIX listener supplies a single exclusively borrowed client,
  exact descriptor cleanup and native loopback-to-service integration. It is
  not wired into the PS5 runtime or advertised as an active console capability.

Read DEVICE-DISCOVERY.md, BOND-FILE-STORE.md, LAN-MANAGEMENT.md,
CONTROL-LISTENER.md and APPLICATION-RUNNER.md. Root owns integration/commits;
three reused workers implemented/reviewed disjoint modules without staging or
running console operations. Do not presume these workers remain active later.

Verification: 348 ordinary workspace Rust tests plus one compile-fail check
pass, with fmt, strict Clippy and whole-workspace freestanding checks. The new
`make application-candidates-test` passes discovery/control/runner suites,
separate native and external store/listener suites, both native-feature Clippy
variants, external freestanding checks, C ASan/UBSan fault harnesses and three
mock DOM/fetch page scenarios. Native store has five filesystem checks; native
listener has four real loopback checks. Additional owner compile-fail checks
and fake ABI fault cases run only with their explicit features. The two real
host-codec pipeline checks also pass (18 audio tests with feature enabled).
Each implementation commit was validated from its independent staged snapshot.

Full `make check` is not a pass: it reaches the unchanged x86 syscall execution
check and stops with macOS errno 86 on this ARM host. C sanitizers, freestanding
runtime archive inspection and build/source-package/test-kit self-tests pass
independently. No new target ELF was linked or deployed. These are development
checks, not console acceptance or a measured stability improvement.

Remaining development: source-reviewed native capture format/timestamp adapter,
target SBC owner wiring, concrete platform scheduler/readiness integration,
durable per-run diagnostic outcomes and independently selectable PS5 test
candidates. The portable runner is a loop body, not a complete native runtime.
Filesystem calls are synchronous and must stay out of the per-frame audio path.
Native transport/dispatcher ownership, real command allowance/ACL reservation,
private capture ABI, target imports and real headset/controller acceptance remain
unresolved hardware gates. Next advance the capture/encoder seams and executable
bounded candidates; do not switch to competing reads or timed credit refill.
No PS5 deployment or new push is authorized by this continuation.

## Application continuation wave, 2026-10-26

The new portable application driver owns device/security, channel/SDP scheduling
and source signaling, and drives normalized PCM-to-SBC handoff through the owned
media sender. Three disjoint workers completed device, channel and continuous
audio modules; lead integration and independent reviews covered their boundaries.

- `233a451`: explicit sample gaps/RTP wrap and clean source restart after actual
  media cleanup; retained submissions, fresh local CID and unchanged credit owner.
- `4f64aa3`: selected-device approval and protected bond store/record lifecycle,
  matching Ready-gated save, forget revocation and clock-failure cleanup.
- `3d90ef2`: SDP/channel scheduling, acceptance-gated publication, bounded cleanup
  and fresh media reopen; late acceptance cannot hide cleanup responsibility.
- `50f7a5f`: continuous normalized PCM scheduling and optional real host encoder;
  complete-block handoff, bounded conceal/rebuffer/expiry and retained backpressure.
- `bt-app`: explicit Connect/Start/Stop/restart/disconnect progression, security
  and channel-loss gates, finite media/cleanup budgets and live recovery policy.

Current verification: 289 ordinary workspace Rust tests plus one compile-fail
check, two additional audio real-codec checks, fmt/check/Clippy and freestanding
checking pass. Each feature was validated from its staged snapshot. Full Make
check availability remains separately recorded in the latest handoff.

No PS5 activation or playback result is added. Remaining: concrete preserved
transport/dispatcher authority, inquiry, PS5 protected filesystem, native capture
format/timestamp and target encoder adaptation, platform event loop, LAN listener/
page/control integration and real pairing/audio/coexistence/stability tests.
See [the application contract](APPLICATION-OWNER.md). Older software omissions
below describe their recorded snapshot; hardware facts remain unchanged.

## Implementation wave, 2026-10-26

The lead integrated three disjoint workers and the source-session binding:

- `50826cd`: per-fault recovery with cleanup/reconciliation tickets, finite
  attempt/time budgets, backoff and stable-success reset.
- `58789ea`: caller-storage AVDTP receive reassembly, shared semantic validation,
  fixed deadlines and channel/epoch/transaction-generation isolation.
- `2baf62f`: selected-peer Classic connection/security, explicit SSP approval,
  stored-key lookup, authentication/encryption/key-size gating and bounded cancel.
- `bt-session`: binds Classic Ready and discovered Sink/channel metadata to SBC
  negotiation and existing media submission. Healthy partial-PDU Stop drains
  before Suspend/Close; terminal prefixes and integrity reasons remain observable.

All 214 workspace Rust tests, Clippy, formatting, workspace freestanding checks,
C sanitizers, codec checks and Rust archive inspection passed on this machine.
The full `make check` is not a pass: unchanged x86 syscall execution stops with
macOS errno 86 on this ARM host. Other build/source/kit self-tests are recorded
in the handoff. These are development results, not PS5 acceptance. The PS5
runtime still links the diagnostic/stub, not these new application candidates.

Remaining: actual transport/dispatcher ownership, channel/SDP scheduling,
inquiry and protected persistent bonds, capture/encoder/sample-pacing integration
and the LAN listener/page/owner loop. No new hardware window, live backend,
headset pairing or continuity/latency improvement has been demonstrated.

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
| D1c | DONE (HOST) | lead + ownership worker + independent review | owned ACL packet ledger and one-attempt media submission: exact acceptance/completion, Busy/credit backpressure, sticky uncertainty and observable prefix cancellation; external reservation/routing still required |
| A2 | DONE | native_hci_research + lead review | source-backed single read-only native stack-version query; no PS5 broker found in bounded inventory; see NATIVE-HCI-RESEARCH.md |
| C1a | DONE | platform_descriptors + lead review | bounded descriptor parser/report, hostile host tests and ASan/UBSan; no live USB access |
| C1b | EXECUTED (PARTIAL) | lead + user | own 1286:2059 four-interface descriptor log, clean close and normal pad; three other opens failed; all binding queries returned Unknown names in HW3 |
| C2 | BLOCKER | lead | L2CAP socket experiment executed: UDP control/close succeeded, BT creation -1/errno 47, normal pad. This socket branch is unavailable in the current process; a different proven native broker or preserved USB traffic ownership remains required |
| C3 | EXECUTED | lead | export resolved on Slim/13.40; single query returned -1; version/controller remain Unknown |
| H1 | LIFECYCLE PASS | user + lead | loader kernel init 0, Rust entry and final 2 on Slim/13.40; user confirmed visible notifications and normal wireless input |
| H2 | PARTIAL | user + lead | USB descriptors and normal pad confirmed; HCI queries/ownership/credits remain unverified and gated by C2 |
| C4 | NONEMPTY READ | capture worker + lead + user | settled read returned 8,192 bytes; all lifecycle/cleanup statuses 0 and user-confirmed normal wireless input; native PCM format/timing still unknown |
| E1a | DONE | sdp_framing + independent/lead review | bounded SDP PDU/continuation/data-element foundation and A2DP Sink discovery client, with transaction/epoch/deadline/continuation and exact UInt16 service semantics; no live query |
| E1b | PARTIAL (HOST) | lead | bounded initiator-side Classic L2CAP Connection/MTU Configuration/Disconnection plus acceptance-confirmed outbound ACL fragmentation pass scripted tests; selected-peer connection/security now exists in bt-link; responder allocation, inquiry, protected persistent bonds and live activation remain |
| F1 | PARTIAL (HOST) | lead | bounded complete-frame SBC-over-RTP packetization and AVDTP Discover/Get Capabilities/Set Configuration/Open/Start/Suspend/Close/Abort pass host tests; receive reassembly and host source/media binding are now implemented; live channels/transport remain |
| G1a | DONE | sbc_audit + independent/lead review | validated shared SBC selections and LGPL/runtime audit; implementation follows in G1b |
| G1b | TARGET SELFTEST PASS | codec worker + lead + user | generic libsbc caller-storage path encoded four generated frames in two lifecycles on PS5 with exact host digest and clean cleanup; source/object/replacement materials tested; encode timing and live integration remain |
| G2a | DONE (HOST) | lead | caller-owned stereo i16 PCM queue, absolute nominal sample deadlines, bounded counted silence/rebuffer/expiry; ten media tests and four synthetic scenarios; codec example in T1, no live capture/transport |
| T1 | PARTIAL (HOST) | lead | real PCM-to-SBC example now drives 77-byte SBC through RTP/L2CAP and three ACL submissions using one synthetic owned credit, Busy retry, two credit stops and three synthetic completions; real reservation, sample pacing and live headset/session remain |
| H7 | HARDWARE | user + lead | 440 Hz SBC A2DP tone audible on real headset, after controller/connection/negotiation |
| H8 | HARDWARE | user + lead | actual PS5 PCM audible through the proven tone pipeline |
| I1a | DONE | lead | pinned build/source-package instructions plus a clean-manifest, hash-checked binding/settled-capture kit; deployment defaults to dry-run and never launches |
| I1b | PARTIAL (HOST) | lead | bounded authenticated request parser, one-command ownership and redacted actual-state JSON; listener/static page and PS5 owner integration remain; no false pairing/audio capability |

Completed waves: upstream/runtime research and real minimal ELF; portable HCI,
L2CAP, SBC selections and SDP framing; host-only descriptor diagnostics; separate
native-query ELF. Each was reviewed and tested before atomic integration.
Do not declare hardware completion from desktop simulation or an ELF link.

## Persistent blockers and next integration target

Transport access is the immediate critical path: the native L2CAP candidate
returned errno 47 after successful UDP control cleanup, closing that particular
socket branch for this environment. Do not repeat capture or SBC
self-tests without a changed implementation or a concrete unanswered question.
Next acceptance remains real headset connection, a short audible SBC tone,
then captured audio and the minimal LAN controls. A native channel backend may
leave HCI/credits under the system owner; it requires a separately reviewed
channel ABI rather than pretending the existing raw-HCI ABI was implemented.
There is no ready headset-connect candidate against the failed socket path.
The next executable transport test requires a concrete alternative channel or
ownership ABI; more endpoint reads, guessed protocols or permission changes
without such evidence would not resolve the blocker.

The latest bounded native follow-up found MBus attachment-management calls but
no usable channel/subscription ABI (NATIVE-HCI-RESEARCH.md). No new target run
was requested. Independently, the ACL reservation ledger and media submission
integration passed 134 workspace Rust tests, eight host-codec checks, workspace
and codec-feature Clippy, whole-workspace freestanding checking and Rust
archive inspection. The real SBC example now exercises three accepted ACL
fragments with one Busy retry, two credit stops and three synthetic completions.
This closes a software integration gap, not the C2 hardware ownership blocker.

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
is actual channel/SDP scheduling, pacing and explicit capture conversion;
receive reassembly and host media binding are implemented. Tone, pairing, captured
PCM and live audio remain later milestones; simulation does not authorize a
shared-reader backend.
