# Milestone roadmap

Milestones are evidence gates, not dates or promises. A compiling archive, a
linked ELF, execution on a PS5, and stable audio are different achievements.
The current dependency graph, worker ownership and blockers are tracked in
[MVP-STATE.md](docs/MVP-STATE.md).

Development policy, 2026-10-26: unavailable PS5 access does not block coherent
candidate implementations. Prioritize application lifecycle integration,
connection/pairing/bond storage, channel and AVDTP media binding, continuous SBC
scheduling, bounded fault-episode recovery, experimental capture adaptation,
diagnostics and minimal LAN pairing/device controls. Reviewed scoped C protocol
modules may be transitional implementations behind a versioned ABI. These are
authorized work packages, not newly completed features.

Functional, quality, latency, coexistence and stability acceptance remains on
PS5. Retain development checks without requiring a Linux Bluetooth backend.
Source-backed isolated transport experiments use AGENTS.md's admission rules;
production ownership/credit guarantees still gate support. No console window
is authorized by an offline build. Next-window planning records the reported
Slim/13.40, PayloadManager 0.6.3 and XM4 environment; it does not authorize a
current deployment/run. The 2026-10-25 window below is historical.

The separate `ps5-application-control` candidate now assembles the real resource
factory, protected store, persistent listener, fallible clock and Rust application
loop into a target-linked ELF. It uses an existing private 0700 directory,
ephemeral loopback and fresh unlogged authorization; no Bluetooth, capture or codec
operation occurs. Startup unwinds partial resources and store leases observably;
Stop, clock faults and execution budgets remain bounded. PS5 imports, entropy,
filesystem/scheduling and controller coexistence are still Requires hardware.
See [APPLICATION-STARTUP.md](docs/APPLICATION-STARTUP.md).

Two additional application variants now reuse that startup: generated mode
schedules/discards 750 real target-SBC blocks with observed clock pacing, while
raw observation keeps format/time Unknown and admits at most 256 calls in an
exclusive two-second interval (up to 190 scheduled polls with the selected 100 ms settle).
They are separately selectable target candidates, not Bluetooth or playback
acceptance. See [generated work](docs/APPLICATION-GENERATED.md) and
[observation](docs/APPLICATION-OBSERVE.md).

Scoped [native resource rebuilding](docs/NATIVE-RESOURCE-REBUILD.md) now closes
and reopens actual store/listener resources on the same storage under one
persistent recovery controller. Host tests preserve fault budgets, local epochs,
Stop and cleanup uncertainty. Automatic main-loop reconnect and a separate
rebuild console artifact are not implemented.

The usable Bluetooth connection backend remains a critical blocker: native
L2CAP returned errno 47 in the recorded probe, and there is no ready safe
active-HCI artifact. Local application diagnostics cannot establish routing,
credits, headset playback or XM4 compatibility.

The Rust `ps5-app` candidate now constructs actual reviewed native resource
owners in stages and unwinds them in reverse on startup failure. Control-only,
unknown-format capture observation and explicit generated-SBC modes are separate;
native audio still requires verified format and no radio authority is granted.
See [PS5-RESOURCE-FACTORY.md](docs/PS5-RESOURCE-FACTORY.md).

A persistent listener service now safely lends its owned native client to
reusable HTTP state, accepts successive clients under total budgets and checks
Stop before accept/read. This closes the single-client wiring gap offline;
The control-only target now links that listener; PS5 import/permission behavior
and any LAN/radio/audio activation remain separate gates.
See [LISTENER-SERVICE.md](docs/LISTENER-SERVICE.md).

Five Rust candidates—capture-stream, continuous SBC and the three application
variants—embed matching build identities.
A bounded local collector records actual numeric outcomes, cleanup, interruption
and explicit controller observations without copying private log text; host
persistence does not prove PS5 durability. Nine independently selectable kit artifacts have matching source support;
the codec selections additionally retain replacement/relink materials. See
[DIAGNOSTIC-RESULTS.md](docs/DIAGNOSTIC-RESULTS.md).
The new startup plan selects only application-control/generated/observe from
that inventory. Its intended assembly directory is
`build/ps5/next-window-kit-20261026`; identity exists only after clean-commit
assembly succeeds. The read-only console helper defaults to dry-run and never
creates slots/directories, uploads, launches or changes permissions.

A separate fallible native clock/wait candidate now provides cached reviewed
exports and a single <=1ms requested wait slice per step. Host fixtures and
freestanding/object inspection pass; native timing/readiness integration remains
pending. See [PLATFORM-CLOCK.md](docs/PLATFORM-CLOCK.md).

The portable application loop now binds concrete HTTP/control/input/audio/send
owners under finite quotas and global Stop. Factory-based resource reconstruction
consumes capture/encoder cleanup, retains failure budgets and uses fresh epochs;
a rebuilt application accepts an already-spent retry ticket without spending it
again. Native resource construction now has an independent control-only assembly;
production routing/readiness and capture/audio activation remain gated by
transport/format evidence. See [APPLICATION-LOOP.md](docs/APPLICATION-LOOP.md).

The experimental Rust capture owner now has a separate Avcap2 ABI, explicit
format/time evidence, declared-input normalization and retained block bridge.
`ps5-capture-stream` exercises one raw read through the new Rust/C seam with
finite budgets and no format inference. Native units and continuous hardware
behavior remain Unknown; see [CAPTURE-STREAM.md](docs/CAPTURE-STREAM.md).

The Rust target SBC seam now has a separately selected `target-libsbc` feature
and `ps5-sbc-continuous` candidate: generated PCM traverses the persistent Rust
scheduler/codec owner with finite operation/time budgets. Host checks and target
ELF/replacement links are development evidence; continuous PS5 execution,
captured-audio integration and headset acceptance remain pending. See
[SBC-CONTINUOUS.md](docs/SBC-CONTINUOUS.md).

Prior discovery/management wave: implements bounded Inquiry/Cancel, concrete
optional POSIX bond files, the minimal LAN device/pairing service/page, a
single-client native listener candidate and the portable management/ACL runner.
Commands remain capability-gated; no native PS5 backend is activated. Native
capture/target encoder adaptation, platform scheduling, durable run orchestration
and executable hardware candidates can still advance before console access.
M2/M3 acceptance remains unchanged. Details and current verification are in
[the durable handoff](docs/SESSION-HANDOFF.md).

Prior snapshot: the earlier continuation wave on 2026-10-26 joins
selected-device pairing/bond management,
channel/SDP scheduling, continuous normalized PCM-to-SBC pacing and a portable
single-owner application driver. Clean Stop/Start uses fresh local media CIDs;
finite media/disconnect/recovery budgets remain observable. All 289 ordinary
workspace Rust tests and one compile-fail check pass, plus two feature-enabled
real-codec pipeline checks. These candidates do not promote M2/M3: the live PS5
transport, protected filesystem store, inquiry, native capture/target encoder
integration, LAN listener/page and hardware acceptance remain pending.

The earlier 2026-10-26 implementation wave added bounded selected-peer Classic security,
AVDTP receive reassembly, source negotiation/media binding and per-fault recovery.
Those were host-checked candidates, not completed M2/M3 audio milestones. At
that snapshot, inquiry, persistent bonds, channel scheduling, capture/encode/
pacing and the LAN owner loop remained. All 214 workspace Rust tests and
freestanding checks passed;
full `make check` is blocked by this ARM host's unavailable x86 syscall-test
execution (errno 86), separately from PS5 verification.

Prior hardware baseline: runtime v6 completed HW-1 executable lifecycle on the recorded
Slim/13.40 environment: existing-loader kernel initialization, Rust entry,
durable log and final Unsupported 2. The native query ran but returned -1.
The user confirmed visible notifications and wireless DualSense operation.
Host-only PCM staging, nominal sample timing and bounded recovery are implemented
with deterministic gap/stall scenarios. Own USB metadata and Avcap2 lifecycle
have executed; a 100 ms settled capture read now returned 8,192 bytes with clean
cleanup. Native PCM format/timing remain unknown. Native HCI ownership/credits
still gate production traffic; isolated diagnostics have separate admission.
See [hardware evidence](docs/HW1-REPAIR.md) and
[layered recovery](docs/AUDIO-RECOVERY.md). Subsequent target: 192.168.1.111.
See [current USB/capture results](docs/HW3-EVIDENCE.md). The user renewed the
hardware window from the PS5 LAN on 2026-10-25.
The first real offline PCM-to-SBC path is now host-tested: bounded queue,
safe Rust owner, C caller-storage wrapper and pinned generic libsbc. Its first
complete SBC frame now passes through bounded RTP/A2DP, L2CAP and synthetic ACL
framing. A reusable send cursor now preserves complete media PDUs across ACL
backpressure and requires explicit whole-fragment acceptance. A bounded AVDTP
initiator signaling session is host-tested, but this is
not M3a: there is no live media transport or headset output. A separate
generated-signal target codec self-test now links with replaceable archive and
application-object/source materials; its two lifecycles/four generated frames
have now executed on PS5 with exact host digest and clean cleanup. Encode
duration and live media integration remain unverified.
The next implementation and 2026-10-25 hardware preparation plan is
[MVP-NEXT-RUN.md](docs/MVP-NEXT-RUN.md). Milestones gate evidence and activation,
not independent host protocol, target-adapter or minimal-control-page development.
An initiator-side Classic Basic-mode L2CAP channel now passes scripted
Connection/MTU Configuration/Disconnection tests, but has no live transport,
AVDTP media-channel binding or hardware evidence. A bounded A2DP Sink SDP discovery exchange
now passes scripted continuation and service-record tests but has no live query.
Its outbound fragmenter now preserves one partially accepted PDU until complete
or explicitly terminated and emits exact PB=0/PB=1 ACL frames against a proven
external data limit. A separate owned ACL reservation ledger and one-attempt
media submission driver now pass host tests and the real SBC example, including
Busy retry, zero-credit backpressure and exact completion accounting. They do
not establish a hardware reservation, native routing or sample-clock pacing.
The thin LAN foundation now has a host-tested authenticated request boundary,
single-owner command tickets and redacted status serialization. At that earlier snapshot it had no listener or rendered page. The later
service, resource factory and control-only application candidate above supersede
that implementation limit without supplying radio or audio acceptance.

| Milestone | Deliverable | Exit evidence | Current state |
| --- | --- | --- | --- |
| M0 — research/bootstrap | source/license inventory, architecture, bounded ABI, desktop tests, freestanding Rust archive | reproducible host checks and artifact inspection; explicit unknowns | bootstrap implemented; validation below |
| M1 — Rust on PS5 / controller information | audited C startup, Rust entry, minimal metadata backend, bounded HCI queries only after routing gate | real console log with build hashes, USB IDs, HCI/LMP version, actual commands/features/buffers where accessible; clean return and DualSense before/during/after | Rust lifecycle, own USB descriptors and user-observed normal wireless input verified on one Slim/13.40; HCI capabilities and routing remain Unknown |
| M2 — platform transport and capture | safe native L2CAP channel or command/event/ACL transport, vendor detection, capture open/read/close | fault/cleanup tests, no hidden loss, proven native traffic routing, capture format/timing report | settled diagnostic returned 8,192 bytes; native L2CAP socket creation returned errno 47; usable transport/broker and PCM format/timing remain unresolved |
| M3a — minimal A2DP tone | Rust L2CAP/SDP/AVDTP/A2DP/RTP, explicit pairing/key storage, conservative SBC | reproducible 48 kHz stereo tone on named headset/controller/firmware, correct credits and negotiated limits | planned |
| M3b — captured audio | PS5 PCM through bounded queues to the demonstrated sink | game/menu/silence tests, capture discontinuity and queue telemetry, clean stop | planned |
| M4 — stability | bounded reconnect, starvation/overflow handling, diagnostics | staged 1 h then 8 h playback per declared target; no crashes or monotonic memory growth; no sustained underruns; headset power cycles, pad reconnects, Wi-Fi/RF load, cleanup logs | planned |
| M5 — latency/quality | measured SBC HQ/XQ experiments, pacing/buffer tuning, Game/Balanced/Hi-Fi profiles | reproducible full-path latency distributions and encoder/queue metrics; headset interoperability and regression comparison | planned |
| M6 — codecs | consider AAC and other codecs separately | implementation license/patent review, throughput/CPU/latency data and hardware demonstration before any support claim | research only |
| M7 — adaptive engine | telemetry-driven buffer/bitrate/pacing control | deterministic replay, fixed-policy baseline, stability bounds/hysteresis, controlled loss/coexistence trials | deferred |

M4 targets are proposed acceptance criteria, not measured results. Define the
underrun and memory thresholds with a baseline before starting each soak run;
publish raw counts and exceptions. A failed native-controller restoration or
unreconciled credit test blocks promotion regardless of how good audio sounds.
Fat, Slim and Pro become supported individually through evidence, not together
through a model-name conditional.

First approachable work: review source permalinks and license annotations, add
malformed-event fixtures with provenance, review translation status guides, or
record read-only hardware descriptors using the documented test procedure once
the runtime is reviewed. Do not open artificial issues for each empty crate.

Initial host validation: four Rust parser tests, 15 C/Rust seam scenarios, C stub
contract checks, fmt/check/Clippy and the freestanding archive build/inspection.
Earlier portable-wave checks: 289 ordinary Rust tests, one compile-fail check
and eight host codec checks,
208 encoder reference vectors, 12 decoder checks, replacement-archive linking,
four host PCM recovery scenarios,
descriptor/native-query C tests with ASan/UBSan, 15 ABI
scenarios, fmt/check/Clippy and archive inspection pass. The unchanged x86
syscall execution check is unavailable on this ARM host; the full Make target
is therefore not a pass. The minimal PS5 ELF
links and passes structural/failure-path checks; the explicit HW-1 variant
records Rust entry and final return 2 on one environment. This is not audio support.

The next-window application-setup candidate now covers fixed private namespace
creation/validation before the Rust resource-factory tests, with host fault
coverage and a separately identified ELF. Native export/protection/durability
acceptance remains pending; this does not change the unresolved M2 transport or
capture-format evidence. See docs/APPLICATION-SETUP.md.

Next-window readiness now includes a six-item default plan (fixed namespace,
three application workloads and two conditional isolation candidates), a
one-command local preparer and verified portable transfer for a new testing
machine. The full inventory has ten candidates; the older startup-only subset
remains available. Read docs/TEMP-PS5-TEST-HANDOFF.md before any current window.
Packaging/readiness does not close M2's native route or capture-format gates.
