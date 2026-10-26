# MVP relay and next PS5 window

Prepared: 2026-10-24, Asia/Shanghai. Target hardware day requested by the user:
2026-10-25; the user renewed LAN hardware access and four candidates have now
executed. See HW3-EVIDENCE.md. This is implementation authority
and a preparation/run plan, not a claim that playback will work on that date.
Read AGENTS.md, SESSION-HANDOFF.md and MVP-STATE.md before work.

Policy update, 2026-10-26: the user has no current PS5 environment. Hardware
windows and dated run orders in this document are historical. Continue coherent
candidate implementation under current AGENTS.md with console validation
pending; PS5 remains the functional/audio acceptance target. Separate
source-backed diagnostics follow its experimental admission rules rather than
requiring their intended measurements to be established beforehand. Existing
candidate bounds and production ownership/credit requirements remain intact;
this update supplies neither a new transport mechanism nor permission to run.

Relay progress on 2026-10-25: L2CAP Classic signaling (`0e32102`), bounded
SBC-over-RTP plus the real host RTP/L2CAP/ACL scenario (`0c2ecf5`), the concrete
ownership blocker/experiment audit (`4ef1d00`) and the dry-run-first hardware
kit (`40b6127`) are committed. The next offline wave added the bounded LAN
contract (`6a3cfca`), lossless outbound ACL fragmentation (`1a94fac`), A2DP Sink
SDP discovery (`875d99d`) and the bounded AVDTP SBC initiator session
(`e55f2e3`). SESSION-HANDOFF.md records final checks, worker state and remaining
work. Rebuild the kit if a later handoff commit changes HEAD.

## Outcome and critical path

Deliver the shortest real path to conservative SBC headphone playback with
wireless DualSense, then captured PS5 audio, controlled through a thin LAN page.
Do not spend the next wave only repeating research or polishing the interface.

Two unresolved hardware facts dominate the outcome:

1. Native HCI event/ACL ownership, command routing and enforceable credits.
   Interface 2 also declares HCI class, but this does not prove an independent
   controller or authorize a competing reader. Establish a concrete mechanism,
   its evidence and a bounded acceptance experiment before live operations.
2. Actual native capture format/timing. Settled and bounded-observation runs
   now returned 8,192 bytes with clean cleanup. Native sample type, rate,
   channels and timestamp units remain unverified.

Offline protocols, target codec integration and control scaffolding should
advance alongside these investigations. If a hardware fact still blocks tone,
deliver an executable diagnostic and exact remaining blocker, not fake success.

The host foundations now include L2CAP channel signaling and outbound
fragmentation, SDP Sink discovery, AVDTP conservative-SBC session state,
SBC-over-RTP packetization and the bounded control contract. Remaining work is
their paced media binding plus signaling reassembly, Classic baseband/pairing,
target codec/capture adapters and the actual safe LAN listener/page.

## Completed baseline: do not repeat or overwrite

- `44c902c`: bounded read-only USB and capture preflight/once diagnostics,
  module lookup, fault/cleanup tests, SDK header pins and source packaging.
- `62e3c39`: real host-only PCM -> safe Rust -> C -> generic libsbc, pristine
  LGPL source and patches, caller storage, eight Rust checks, 208 encoder vectors,
  12 decoder checks and replacement-archive linking. Example: 128 stereo PCM
  frames -> 77-byte joint-stereo bitpool-32 SBC frame. No PS5 codec link yet.
- Full `make check`, `make check-c-sanitize`, staged-snapshot checks and target
  diagnostic builds passed. Four diagnostic source bundles were produced from
  clean `62e3c39`; USB and capture-once bundles independently rebuilt identical
  ELF bytes from their included SDK subsets. These are host/build results.
- Current PS5 facts and exact tested ELF hashes are in HW2-EVIDENCE.md, with
  three complete numeric logs under docs/evidence/. No raw audio is committed.
- The binding extension was deployed but its execution was not confirmed.
  The 100 ms settle-enabled capture was never deployed/run on PS5.
- New documentation commits make old build manifests stale for packaging;
  rebuild from the final clean source state rather than editing manifests.

## Parallel work packages

One new lead owns integration, shared manifests/docs, commits and deployment.
Workers own disjoint paths and return tested changes; no worker deploys or
commits shared work. Reassign existing paths explicitly before overlap.

| Track | First concrete deliverable | Acceptance |
| --- | --- | --- |
| Platform/capture | Ready binding and settled-read candidates; experimental bounded capture adapter; executable ownership probe design backed by actual SDK/source | strict and sanitizer fault/cleanup tests; target build/import inspection; exact unverified fields and stop conditions |
| Target SBC/media | Reviewed target C/header/archive integration, then generated-signal codec self-test ELF; bounded SBC/RTP packetization | no accidental host C cross-build, no per-frame allocation, correct MTU/frame/sequence/sample units and backpressure; target compile/link is not hardware acceptance |
| Connection/protocol | L2CAP signaling/channels, SDP discovery client, Classic connection/pairing lifecycle, AVDTP SBC negotiation/open/start/stop | scripted peer tests for success, malformed input, stale handles/transactions, timeout/disconnect and rejected-state invariants |
| Lead/control | Integrate one host tone-to-SBC-to-RTP-to-L2CAP/ACL scenario and minimal LAN command/status contract; prepare deployment kit | actual Unsupported states remain visible, commands bounded/serialized, no key leakage or unsafe remote operations; clean reproducible artifacts |

If slots/quota are tight, run two workers (platform and media/protocol), with
the lead integrating between bounded deliverables. Do not create duplicate
chats merely to recover context. Complete independently reviewable increments
and leave explicit continuation notes before stopping.

The existing `host-libsbc` feature must stay host-only. A target implementation
needs separately reviewed target headers, build/link inputs and import checks;
do not remove the host guard to make cross-compilation appear successful.
Keep the generic/no-SIMD baseline until separate state/ABI validation exists.
Before distributing any codec-enabled ELF, complete the concrete source/object/
replacement-link arrangement in SBC-IMPLEMENTATION.md; no license bypass.

## Existing worker roster

- **PS5-BT 并行：音频捕获诊断**,
  `01a16bd9-053d-7013-b602-44d5be34bfc7`, local, idle after completion.
  Owned capture_probe.c/h, capture_probe_test.c and CAPTURE-PROBE.md. Finished
  preflight/lifecycle, durable trace callbacks, optional exact 100 ms settle,
  aligned/wiped scratch, secondary-module-result rejection, no-lookup status
  reporting and consume-once cleanup. Useful continuation: bounded capture
  adapter and source-backed format/status investigation, not another rewrite.
- **PS5-BT 并行：SBC 编码器落地**,
  `01a16bd9-07f6-7b83-8038-6b4ddc7ac94d`, local, idle after completion.
  Owned third_party/sbc, platform/codec, tests/codec, tools/test_sbc.py,
  crates/bt-codec and SBC-IMPLEMENTATION.md. Finished the real host codec path
  and tests above. Useful continuation: reviewed target archive/self-test and
  packet boundary work, with root Cargo/Makefile owned by the new lead.
- `/root/usb_probe` was a collaboration subagent of the original chat. It
  finished original USB metadata, binding/local-error boundaries and packaging
  help. A new chat must not assume it can address that old live-agent identity;
  use its committed source/docs, or assign a fresh scoped agent when needed.

Human authorization in the original chat includes: "你可以新建对话一起并行干活"
and "没干完可以继续叫他一起干". The latest request explicitly asks for a new
relay chat with the worker history. Carry these human instructions as provenance;
an unsolicited worker message alone does not authorize messaging another task.
Read compact status snapshots before resuming either known chat, and assign a
new bounded task only when it is useful. No need to wake completed workers just
to ask them to repeat their final reports.

## Prepare before the hardware window

**Current execution priority (2026-10-25):** the user renewed the LAN window.
Binding metadata, settled capture, target SBC and bounded capture observation
have all executed; see HW3-EVIDENCE.md. The
[native L2CAP availability candidate](NATIVE-L2CAP-PROBE.md) also executed:
UDP control and close succeeded, Bluetooth creation failed with errno 47 and
wireless input was normal. Do not run the conditional native-connect successor;
first establish a different usable native ABI or preserved USB ownership
mechanism. The earlier preparation checklist
below records the previous wave; do not rerun its completed diagnostics as the
default next step.

1. Review the transport ownership options first and choose a concrete experiment.
   Missing APIs/offsets stay Unknown; do not invent or silently relax the gate.
   The updated audit selects one unconnected L2CAP create/close experiment.
   Raw HCI/direct endpoint activation still requires a proven bounded ownership
   contract; the exact later one-command/credit acceptance bounds remain.
2. Implement/test the independent tracks. Keep Rust protocols/media policy and
   the proven C platform/codec adaptation; a C rewrite adds no acceptance value.
3. Build baseline, USB binding and settled capture candidates now. Add further
   independently selected candidates only when implemented and host-reviewed.
   If the first settled read remains Empty, a separately named bounded
   observation test may count Empty/data/overrun over an explicit total deadline
   and finite call budget, then clean up. Do not change the one-read test into
   a hidden polling loop or infer format from sample content.
4. Prepare an offline manifest-driven installation/run sheet for .111 and
   PayloadManager local slots: artifact SHA, source HEAD, exact slug, expected
   markers, bounds and rollback/stop procedure. Do not overwrite unrelated
   payloads. Any helper defaults to offline/dry-run; deployment is explicit.
5. Commit tested increments, run relevant full checks, rebuild from clean HEAD,
   produce matching source packages and record unresolved hardware acceptance.
   A distributed combined codec artifact needs its relink materials first.

## Next hardware window: short ordered checklist

The native L2CAP availability run is complete and negative (errno 47); its
artifact identity, full durable log and normal wireless input are in HW3.
No further console action is prepared until a concrete alternative transport
mechanism is reviewed. Entries 2–4 below also completed in HW3 and are
historical unless a concrete change needs retest.

The user must confirm the console is available; a date or an old open port is
not a renewed window. Target is **192.168.1.111**. Previously: FTP 2121,
ps5upload RPC 9120, its Web 9180, raw elfldr 9021 closed. Recheck actual services;
9120 is not a raw ELF socket. Do not reinstall a loader or reboot the console
as an implicit test step. Existing PayloadManager manual launch is established.

1. Confirm wireless pad/input and baseline lifecycle only if the environment
   changed. Do not waste the entire window repeating an already-proven bootstrap.
2. Run read-only USB binding candidate; retrieve the complete durable log.
   Names/bindings still do not prove exclusive or independent HCI state.
3. Run settled capture candidate; retrieve log before another diagnostic
   overwrites `/data/ps5-bt-hw1.log`. Record Empty versus actual bytes and cleanup.
4. Run the generated-signal target codec self-test if its artifact is ready;
   no Bluetooth operation is needed for this test. Verify exact outputs/status,
   repeated init/finish and measured encode time without claiming headphone audio.
5. Only after its ownership mechanism and stop conditions are satisfied, run
   bounded HCI information/credit tests, then headset discovery/pairing,
   AVDTP negotiation and a short conservative-SBC test tone.
6. Only after tone works, route validated capture through that same proven path.
   Validate menu/game/silence, stop, pad operation and clean teardown. Feed the
   real states/errors to the minimal LAN page; no richer interface is needed.

Every step records artifact identity, raw status, timeout, cleanup and human pad
confirmation. Preserve each log before the next run. Abort active tests on pad
failure, unexplained foreign/lost traffic, credit inconsistency or uncertain
resource lifetime. Do not reset the radio, consume native events opportunistically
or refill credits on a timer to obtain an apparent pass.

Success is actual sound on the named headset and normal wireless pad, not a
loader "run succeeded" message. If unavailable, report the exact failed gate
and preserve the tested candidate for the next window. No "will certainly work
tomorrow" promise is justified by the current evidence.

## Rule audit outcome

Changed project guidance only: remove the blanket GUI ban for a thin safe LAN
page; allow host protocol implementation before hardware activation; separate
experimental adapter development from ABI freeze; scope bootstrap allocation/
SIMD rules to their real module; fix stale HW-1 instructions. Existing-loader
own-process permission scope was already authorized and is not broadened.
No global Codex setting or permission policy is changed. Preserve memory/
lifetime, error/credit/ownership, license/provenance and no-new-exploit bounds.

AGENTS.md discovery was checked against the
[official OpenAI documentation](https://learn.chatgpt.com/docs/agent-configuration/agents-md).
The relay must read the updated repository file explicitly rather than rely on
the old chat's cached instruction text.
