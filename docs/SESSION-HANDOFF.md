# Active hardware window and durable handoff

## New-machine test preparation, 2026-10-26

Start with [TEMP-PS5-TEST-HANDOFF.md](TEMP-PS5-TEST-HANDOFF.md) and
[PS5-WINDOW-DECISION-MATRIX.md](PS5-WINDOW-DECISION-MATRIX.md). The current default
`tools/window_test_plan.json` contains application-setup, application-control,
application-generated, application-observe, then conditional standalone
sbc-continuous/capture-stream. The full inventory now contains ten candidates;
the older three-item startup plan and previous kit paths below describe earlier
artifacts, not the new default kit identity.

The user authorized completing/committing/pushing this offline preparation and
plans to enter the same Slim/13.40, PayloadManager 0.6.3 and XM4 environment after
this conversation. No current console connection, upload or launch follows from
that plan. The final response and generated kit/preparation/transfer manifests
record the actual completed source identity; do not infer one from a path.
Existing commits through `9234a70` were pushed and remotely verified before this
preparation. Historical statements about earlier local-only commits below are
superseded by that push, not new hardware evidence.

The new separately selected setup artifact only validates/creates the fixed
private namespace using pinned SDK declarations and payload native identity.
It never changes existing permissions or radio/system state. The application
still requires its protected directory and independently verified imports,
entropy, store/listener, clock and cleanup. Python-only portable transfer or a
fresh clone with pinned build tools supplies all six matching ELF/source pairs.
Keep payloads, transfer archives, raw logs and private state ignored, outside Git.

Native directory/export behavior, durability, capture format/native time,
Bluetooth routing/allowance and controller coexistence remain Requires hardware.
There is no usable connection backend or safe active-HCI artifact; local tests
cannot pair the XM4 or demonstrate playback. Stop on uncertain cleanup. The
unchanged x86 syscall fixture remains unavailable on the ARM development host;
full `make check` must not be reported as passing. No native test is executed by
these preparation tools.


Updated: 2026-10-26. Read AGENTS.md and docs/MVP-STATE.md first.

## Current authority, 2026-10-26

The user plans a later console window; it is not currently confirmed. All earlier windows below are
historical and do not authorize a new connection, deployment or run. Continue
MVP implementation with PS5 validation pending. Functional, quality, latency,
coexistence and stability acceptance remains on PS5; do not require a Linux
Bluetooth backend. The current AGENTS.md governs project-wide scope over older
research-stage sequencing: scoped reviewed C protocol reuse, independent
experimental capture, local bond storage, minimal pairing/device controls and
fault-episode recovery are permitted. Isolated source-backed transport
experiments may measure unresolved facts under their documented admission
plan, without implying production support or a competing-reader/timer-credit
exception. This documentation change adds no live backend or hardware result.

## Next-window application workload continuation, 2026-10-26

The user asked to push existing commits first, authorized further parallel offline
work, and confirmed the planned upcoming environment remains Slim/13.40,
PayloadManager 0.6.3 and WH-1000XM4. The push succeeded and was independently
read back: origin/feat/mvp-bringup = 5acae454ec756c8ef265c0ac61e828cfc18c0179.
The subsequent commits below remain local. There is no currently confirmed online
console window and no deployment, FTP connection, launch or hardware result in
this continuation. The old window is not execution authority. Workers finished;
root retained integration/commit ownership.

- `5691f30`: fixed-storage observed-clock generated PCM workload using the existing
  Rust scheduler and a borrowed persistent BlockEncoder, with retained complete
  frames and an explicit hash/discard sink.
- `e2b8a52`: bounded raw CaptureOwner observation, no PCM inference, separate host
  and opaque native time, byte/status/unknown-gap counts and explicit caller cleanup.
- `9834c04`: actual native factory consume/reopen on the same storage under one
  persistent Recovery and local resource epoch. HRTB scoped access prevents native
  owner escape. This is a tested library candidate, not automatic main-loop
  reconnect or an additional hardware payload.
- `6517192`: carry validated intra-quantum time through the portable loop and driver,
  start Stop budgets at that accepted instant, reject new work after global
  deadline and preserve accepted time for clock-free failure cleanup.
- `00129a1`: separately selected application-generated/application-observe targets,
  actual protected store/listener/clock startup, local audio lanes and numeric
  results. Generated full success requires 750 blocks, 57,750 bytes and the pinned summary
  and a fresh post-discard completion observation before its exclusive two-second bound.
  Current raw observation selects a 256-call ceiling, 100 ms first settle, 10 ms interval,
  exclusive two-second, at most 190 scheduled reads and no catch-up; historical diagnostics
  and BOUNDED 20-read/100 ms helper remain unchanged. Fresh clocks before audio steps
  prevent stale pre-network/pre-encode timestamps admitting late work.

Both new target ELFs linked/inspected. Independent staged snapshots passed fmt,
whole-workspace tests (437 ordinary + one compile-fail), strict Clippy and whole-workspace
freestanding checking. Native fixtures include 33 ps5-app tests + one borrow-escape
compile-fail, seven runtime host tests, real successive TCP clients, full 750-block
persistent codec summaries, repeated raw capture through the actual Rust/C owner,
mid-work Stop, failed final encoder-stage initialization and fresh deadline jumps.
C import/entry sanitizer tests pass. Source-package/build/kit self-tests,
36 local-result tests and 12 fake-FTP window-helper tests pass. The final staged
serialization/marker update was separately tested. Full make check's unchanged
x86 syscall execution fixture remains unavailable on ARM macOS (errno 86); this
was not weakened, silently skipped or called a pass. These are offline checks.

Read NEXT-WINDOW-READINESS.md before the upcoming console window. Final clean-HEAD
builds/source bundles follow this handoff commit. The selected three-item kit is
build/ps5/next-window-kit-20261026; full nine-item inventory is kept separately.
Manifests carry the actual source revision and hashes. New helpers default to
no network and no output writes; explicit execution only performs bounded
read-only slot/hash preflight or private no-clobber log/result collection.
They neither prepare directories/slots nor upload or launch. Host fsync does not
prove PS5 persistence and matching build identity does not prove log freshness.

An existing private 0700 /data/ps5-bt/control directory owned by this payload
identity is required; current existence/protection is unverified. Matching
PayloadManager slots must also be prepared before
the application tests can run. Native imports/entropy/protection/timing/cleanup
remain Requires hardware. If application setup fails cleanly, independent
continuous-SBC/one-read-Rust-capture candidates remain separate fallbacks; uncertain
cleanup or changed controller input stops the window, rather than another test.

All three application procedures passing does not complete the audio chain:
there is no usable admitted Bluetooth connection backend/active HCI test artifact,
the prior L2CAP create returned errno 47, and native PCM format/time units remain
Unknown. Raw bytes or successful encoding cannot promote NativeAudio. Real route/
owned allowance (or source-proven system-managed channel ABI), format verification,
headset pairing/playback, quality/latency/stability and coexistence still need
separate evidence and may require further implementation. Passing these tests
does not guarantee a headset connection in a few hours or after one adjustment.
Rust remains the mainline.

## Native startup integration continuation, 2026-10-26

This supersedes the software omissions in the older dated snapshots below.
Three disjoint workers implemented/reviewed factory, listener, startup and result
work; root remained the only integration/commit owner. Workers are finished.
No deployment, console connection/run, push, native audio activation or new
hardware/coexistence evidence was authorized or performed.

- `f0854d8`: persistent bounded sequential-client HTTP service using actual
  listener ownership, fixed buffers and retained client-close uncertainty.
- `09a4b1c`: concrete Rust `ps5-app` factory for reviewed native clock, protected
  store, listener, capture and target SBC owners, with explicit modes and reverse
  consume-once cleanup. Unknown PCM cannot enter NativeAudio; no transport or
  credit constructor exists.
- `fa938b8`: independent selected-export POSIX bridge, exact 43-header SDK
  closure/notices and bounded cancel-aware entropy acquisition. Missing exports,
  unsafe device/format or uncertain close fail explicitly; no guessed syscall,
  predictable authorization token or privilege/driver changes.
- `0409478`: per-turn application state/store lending and finite startup drive.
  Initial Stop preempts normal clock/startup work; committed teardown advances
  bounded clock/wait operations so a sticky latch cannot freeze StopTimeout.
  Clock/wait/budget faults abort then consume at most six cleanup steps.
- `a01cd04`: separately selected control-only target application ELF, real
  store/listener/clock startup, Rust Application/Runner, Stop entry points and
  numeric outcomes. Earlier HTTP close uncertainty remains sticky through final
  cleanup and poisons reuse; the atomic guard rechecks poison after acquisition.
  Build/source/kit/local-result tools recognize this independent identity.

The candidate requires an already existing private mode-0700 directory at
`/data/ps5-bt/control` owned by the payload identity. It does not create/relax that
directory. It uses ephemeral loopback with fresh unlogged authorization, at most
5,000,000 observed microseconds, 20,000 turns, 10,000 waits, 64 clients and
100,000 us request/Stop budgets. It invokes no Bluetooth, capture or codec
operation. Its native imports, entropy, filesystem protections/durability,
scheduling and controller behavior remain Requires hardware. This is a startup
candidate, not LAN deployment, playback or measured timing.

Offline verification: workspace formatting/check/tests/strict Clippy pass, with
409 ordinary Rust outcomes and one compile-fail outcome. Feature-enabled actual
POSIX factory fixtures, five native-runtime host tests, seven driver startup
regressions, capture/clock fixtures and target feature checks pass. Actual two
successive authenticated TCP clients cross per-turn lending without native
radio/audio calls. C import/entropy and main orchestration ASan/UBSan tests,
protected-store/listener fault suites, page checks, codec reference/decoder and
replacement archive checks, Rust archive inspection and 26 local-result tests
pass. Implementation commits were checked from independent staged snapshots;
the full staged native application was linked/inspected as an x86-64 ET_DYN ELF
with only relative relocations, no imports/TLS/constructors and reviewed segment
permissions. These checks prove neither native API behavior nor headset support.

Full `make check` stops at the unchanged x86 syscall execution fixture on this
ARM macOS host: errno 86, Bad CPU type in executable. Earlier suites pass;
build/source/kit tests and archive/C-sanitizer checks were run separately. The
unavailable execution check was not weakened or reported as passing.

After this documentation commit, final clean-HEAD builds and corresponding
source bundles are generated for the seven independently selected kit candidates
and the baseline. The local kit is `build/ps5/mvp-startup-kit-20261026`; its own
manifest/runbook records exact source revision and hashes rather than a
self-referential tracked artifact hash. Building/assembling it performs no network
connection to a console and creates no remote slot. Keep earlier kits historical.
See APPLICATION-STARTUP.md, PS5-RESOURCE-FACTORY.md, LISTENER-SERVICE.md,
PS5-POSIX-IMPORTS.md and DIAGNOSTIC-RESULTS.md.

Next offline integration can expand the actual factory's gated capture/generated
codec modes into separately selected bounded application experiments, or wire
resource rebuild to an admitted owned backend. Actual native format/time units,
owned Bluetooth route/command allowance/ACL reservation (or proven system-owned
channel ABI), native cleanup/reopen, usable authorized LAN configuration and PS5
headset acceptance remain open. Preserve unknown evidence and admission gates.
No current console window or execution authority may be inferred from this work.

## Capture/audio integration continuation, 2026-10-26

This supersedes software omissions in older snapshots below. The three disjoint
workers implemented capture, target codec and application-loop code; root owned
integration, source/target builds and all commits. Follow-up workers completed
clock/wait and local results. Workers are finished; do not assume they remain
active or that historical hardware access is current. No PS5 deployment, push,
execution, headset playback or new coexistence evidence was authorized/performed.

- `0aa6ed2`: separate target-libsbc feature, persistent Rust BlockEncoder and
  bounded 750-block generated-only continuous target SBC diagnostic. The codec
  archive stays outside the Rust archive for replacement/relinking.
- `3dd74aa`: experimental capture ABI and exactly-once Rust resource owner,
  explicit format evidence/normalization, independent time domains and retained
  block bridge. A separate one-read Rust/C target ELF preserves Unknown native
  PCM/time, while the established capture diagnostics remain selectable.
- `575f9f8`: finite MainLoop and actual HTTP/control/input/capture/encode/send
  ApplicationLoop assembly, Stop across phases and observable cleanup/metrics.
  Resource factory rebuild keeps fault tickets, attempt/time budgets and fresh
  epochs; rebuilt applications join the already-spent attempt without spending
  twice. A real capture-fixture-to-persistent-SBC-to-ACL test checks exact frames,
  chunk remainders, queue/transport Busy, explicit credits and partial-PDU Stop.
- `1c0bdd7`: separately selected fallible cached clock/wait ABI, checked native
  time and one <=1ms requested wait slice; host fixture and target-object checks.
- `67bb1c1`: embedded build identity and unique run markers for the two new Rust
  diagnostics, bounded local JSON result collection, actual cleanup/termination
  checks, explicit controller observations and host fsync/no-clobber persistence.

Read CAPTURE-STREAM.md, SBC-CONTINUOUS.md, APPLICATION-LOOP.md,
PLATFORM-CLOCK.md and DIAGNOSTIC-RESULTS.md. README/ABI/build/testing/upstream,
changelog and roadmap now distinguish implementation, target candidate and PS5
acceptance. The six-item kit adds continuous SBC and capture-stream without
replacing the original four; every distributed codec ELF still needs its exact
source/notices and working replacement-link materials.

Verification on this ARM macOS host: 396 ordinary workspace Rust checks plus
one compile-fail check pass, with formatting, strict Clippy and whole-workspace
freestanding checking. The explicit host codec suites, actual capture and clock
Rust/C fixtures, both new native C sanitizer harnesses, 53 default runner tests,
one real-codec runner pipeline test, 17 local-result parser/write-fault tests,
baseline C sanitizers, store/listener/page candidates and Rust archive inspection
pass. Every implementation batch was checked from its independent staged
snapshot. Both new target ELFs were linked and inspected; the continuous SBC
baseline replacement link passed. These are development checks, not console
acceptance, real-time performance or measured continuity improvements.

Full `make check` remains unavailable: after passing its earlier suites it stops
at the unchanged x86 syscall execution test with macOS errno 86 (Bad CPU type).
The test was not weakened or skipped. Build/source-package/kit tests and the
remaining independent suites pass separately. Final clean-HEAD artifact/source
packages and kit are generated after this documentation commit; use their exact
manifests and hashes, never a dirty development artifact or historical hash.

Remaining: install a concrete native PS5 resource factory, listener acceptance/
readiness and owned input routing; establish transport command allowance/ACL
reservation or a proven system-owned channel ABI; confirm actual capture format,
metadata units, clock/wait behavior and backend cleanup/reopen on PS5. Unknown
format still cannot enter the encoder. The clock/wait module adds no seventh
payload. Actual tone, headset playback, latency/quality/long-duration stability,
PS5 persistence and wireless-controller coexistence require a fresh authorized
hardware window. Do not replace these prerequisites with competing reads,
guessed native fields/credits, timer refill or system-driver mutation.

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

## Application integration continuation, 2026-10-26

This supersedes older software omissions; hardware facts and diagnostic artifacts
are unchanged. Root owns integration/commits. Workers completed device, channel
and continuous audio, cross-reviewed one another, and made no staging/deployment.
Do not assume workers or any console window are still active in a later session.

Completed commits: `233a451` sample gaps/clean restart, `4f64aa3` device/bonds,
`3d90ef2` channel/SDP scheduling, `50f7a5f` continuous PCM/SBC. Application owner
integration and this handoff follow together. Read APPLICATION-OWNER.md,
DEVICE-MANAGEMENT.md, CHANNEL-SCHEDULER.md and CONTINUOUS-AUDIO.md for boundaries.

- The single portable driver now progresses selected-peer security, SDP cleanup,
  signaling, explicit Start, separate media, SBC handoff, bounded Stop/restart
  and disconnect. A peer/security binding loss gates media immediately.
- The source carries full-width sample positions across RTP wrap, and reuses
  stopped media storage only after actual cleanup and a fresh local CID. Peer
  remote CID reuse is legal. No old completion credit is refunded.
- Device clock failure revokes Ready/approval/provisional authority while retaining
  cleanup errors; late channel acceptance retains radio-visible cleanup evidence.
- Audio has 16 default checks plus two real host-codec checks. The long case uses
  750 blocks/96,000 generated normalized frames and 2,250 completion-accounted
  synthetic ACL fragments. This is not native capture, PS5 timing or audio proof.
- Application recovery retains accepted attempt tickets, polls episode deadlines,
  requires external working-playback attestation (not Start ack) before stable
  success, and cannot bypass safety/backoff/exhaustion with a new driver. Finite
  queue-to-last-fragment budgets prevent endless Busy/zero-credit media stalls.

Verification: 289 ordinary Rust tests plus one compile-fail check; fmt/workspace
check/Clippy/freestanding checks and the two audio feature checks pass. Independently
validated staged snapshots ensure untracked later modules cannot mask dependencies.
Full `make check` was attempted again and stops at the unchanged x86 syscall
execution test with macOS errno86 on this ARM host. It is not a full pass.
`make check-c-sanitize rust-archive` and build/source-package/test-kit self-tests
pass independently; existing codec vectors and C/Rust seam checks passed before
that Make stop. No new PS5 ELF was linked or run in this continuation wave.

Still absent: a concrete PS5 protected store, inquiry/device search, native PCM
format/timing validation and target encoder wiring, system-coordinated transport,
live platform owner loop and LAN listener/page/management binding. The PS5 runtime
still uses the established diagnostic/stub; no hardware milestone is promoted.
Next: platform store/capture/codec seams and minimum control integration can
advance offline; source-backed transport experiments need a concrete mechanism
and a fresh hardware window. Functional/audio/coexistence/stability acceptance
remains on PS5. No unverified old deployment window authorizes a new run.

## Implemented continuation wave, 2026-10-26

This section supersedes older missing-software statements below; hardware facts
and artifact acceptance remain unchanged. Three collaboration workers completed
AVDTP reassembly, selected-peer Classic link/security and per-fault recovery.
The lead integrated `bt-session` source negotiation/media binding and reviewed
all work; workers did not stage, commit, push or deploy. No worker/hardware
window should be presumed active in a later session.

Completed commits: `50826cd` recovery, `58789ea` receive reassembly, `2baf62f`
Classic link/security. Source-session integration follows with this handoff.
The current workspace has 214 ordinary Rust tests, plus the unchanged eight
host-codec checks, 208 encoder and 12 decoder vectors. The new modules remain
portable candidates; the PS5 diagnostic and Unsupported adapter are unchanged.

- Connection manager supports only a selected peer, known stored bond or
  explicit SSP confirmation, followed by authentication/encryption/key-size
  checks. Inquiry, durable bond storage and incoming connection acceptance are
  not implemented. Raw event routing/command-credit authority are prerequisites.
- Source binding requires established authenticated link, SinkService and
  channel metadata. It joins negotiation and credit-aware media submission,
  retains total stop budgets and terminal prefix evidence, and maps uncertainty
  into recovery reconciliation. Capture, encoding, sample pacing, channel/SDP
  scheduling and the LAN owner loop remain absent.
- Code review fixed prior stored-key authorization leaking into new SSP, early
  Closed during cancellation races, failed HCI acknowledgement shape/ownership,
  expired signaling offers and lost integrity classification. These fixes have
  synthetic regressions, not measured PS5 outcomes.

The pinned Rust 1.90 toolchain, rustfmt, Clippy and freestanding target are now
installed at `/Users/shouheng/.cargo/bin`; add that directory to PATH explicitly.
Workspace fmt/check/test/Clippy, whole-workspace freestanding checking,
`make check-c-sanitize rust-archive`, codec checks and build/source/kit tool
self-tests passed. `make check` stopped at unchanged
`tools/test_syscall_checked.py`: this ARM Mac cannot execute the generated x86
binary (errno 86). Do not claim a full pass, weaken that test or infer a syscall
regression from the missing host execution facility. Each feature's staged
snapshot was validated separately so untracked later work could not mask deps.

Next implementation: protected bond persistence and discovery/selection,
actual channel/SDP owner scheduling, experimental capture/target encoder/pacing
and minimal LAN management. Transport ownership still needs a source-backed
mechanism before a bounded candidate can be prepared; no broker is invented.
Future functional/audio/coexistence/stability acceptance remains on PS5.

## Latest: resumed LAN window and four completed candidates

This section supersedes the historical offline-only status below. The user
explicitly renewed hardware access from the PS5 LAN and authorized tests.
Target remains `.111`; FTP 2121 and the existing service at 9180 responded.
Only the four PS5-BT diagnostic slots were created/uploaded. Each uploaded ELF
was read back and SHA-256 verified; the user launched each manually and
confirmed completion with normal wireless controller behavior. Logs were saved
before the next launch. See HW3-EVIDENCE.md for exact identities and raw logs.

- USB binding, clean `942eb9d`: active configuration succeeds; driver names
  remain Unknown for all four interfaces, three other opens fail, cleanup
  succeeds, final 1 records incomplete inventory. HCI remains Unsupported.
- Settled capture, clean `942eb9d`: one read after 100 ms returns 8,192 bytes;
  all lifecycle/cleanup statuses 0, cleanup_uncertain=0, final 2.
- Generated-signal SBC, clean `34ea256`: two lifecycles/four 77-byte frames,
  prefix `9cfd206f` and digest `14e0ec79` exactly match host; codec/cleanup 0,
  final 2. No hardware encode duration was measured.
- Capture observation, clean `34ea256`: immediate Empty, one wait, then 8,192
  bytes; clock calls return 0, last-read-start elapsed 104,093 us; clean cleanup,
  final 2. Format and timestamp units remain unknown; no samples were retained.

New reviewable commits: `1745e97` media send orchestration, `b787f39` bounded
observation core, `6acedd9` deterministic codec self-test core, `f8a4bb8`
freestanding SBC headers/report, `a79bc77` first nonempty hardware evidence,
`3581c2e` replaceable target codec diagnostic, `9b9c8b4` binding gate update,
and `34ea256` independently selected bounded observation ELF.

The final trace-format fix preserves decimal read ordinals above 9; the earlier
one-digit formatter affected only later observation attempts, not the two-read
hardware result. Capture artifacts rebuilt after this fix have different
bytes and remain unexecuted until matched to a fresh log. Original tested
`942eb9d` and `34ea256` artifacts keep their recorded acceptance.

The complete local suite passed after the SBC integration; the observation
integration additionally passed strict/sanitized lifecycle/runner/main tests,
clock-regression cancellation, source-package self-tests and actual ELF links.
The workspace has 113 ordinary Rust tests plus eight feature-on codec checks,
208 encoder and 12 decoder vectors. Two independently rebuilt target codec
archives replacement-link to identical self-test ELF bytes. Extracting the
`34ea256` SBC corresponding-source package into a separate temporary project
also rebuilt the exact tested ELF. The package contains source/notices,
application objects, Rust/default codec archives and working replacement script.

Preserve original tested packages/manifests separately from final-HEAD rebuilds.
Rebuild all four candidates and packages after this documentation commit, then
assemble the ignored kit without editing manifests. Rebuilt artifacts do not
inherit hardware results unless the exact tested bytes match. Temporary workers
are complete; do not assume they or this hardware window remain active in a
future session without checking current state and user availability.

Remaining path: native capture format/timestamp evidence, measured encode
duration, Classic connection/pairing/key lifecycle, AVDTP reassembly/binding,
sample/credit pacing and actual LAN listener/page. The source-proven single-
owner HCI broker/ACL-credit contract is still the activation blocker. There is
no ready active-HCI/headphone candidate; do not invent a broker, competing USB
reader, reset or timed credit refill to bypass it.

## 2026-10-25 relay completion

The new lead completed and committed nine reviewable increments after
`ba93307`; the latest offline protocol wave adds:

- `6a3cfca`: bounded authenticated LAN request/status contract with one
  serialized command ticket; no socket listener, page or PS5 operation.
- `1a94fac`: acceptance-confirmed outbound L2CAP-to-HCI ACL fragmentation that
  retains a partially sent PDU until completion or explicit terminal/epoch
  cancellation; no capacity discovery or credit ownership.
- `875d99d`: bounded A2DP Audio Sink SDP discovery with exact UInt16 service
  semantics, epoch/deadline correlation and finite continuation history.
- `e55f2e3`: bounded AVDTP initiator session for conservative SBC discovery,
  configuration and lifecycle transitions; fragmented signaling, live media
  binding and hardware use remain absent.

The preceding relay increments were:

- `0e32102`: allocation-free Classic L2CAP Connection, MTU Configuration and
  Disconnection signaling with an epoch-bound initiator channel and six
  scripted-peer tests.
- `0c2ecf5`: `bt-a2dp` complete-frame SBC-over-RTP packetization plus a real
  host path from 128 stereo PCM frames through 77 SBC bytes, 90 RTP/A2DP bytes,
  94 L2CAP bytes and 98 synthetic ACL bytes.
- `4ef1d00`: post-HW2 HCI ownership audit. The executed native query returned
  -1 and FreeBSD-shaped SDK socket headers still do not prove a PS5 broker,
  ACL-send route, bounded dispatcher or credits. Active transport remains
  Unsupported; the binding diagnostic is the next executable evidence step.
- `40b6127`: clean-manifest hardware-test kit assembly and guarded FTP upload.
  The plan pins `.111`, binding before settled capture, 15-second external
  ceilings and log/controller stop rules. It is dry-run by default and never
  launches a payload.

Last green checks at `e55f2e3`: `make check`, `make check-c-sanitize`,
`make sbc-codec-test`, `make check-build-tools`, the `bt-a2dp` freestanding
check, the `bt-avdtp` freestanding check, and Git diff/format checks. The
workspace suite has 104 ordinary Rust
tests; the host codec feature adds eight Rust checks plus 208 encoder vectors,
12 decoder checks, sanitizers, forbidden-import inspection and replacement
archive relinking. These are host/build results, not PS5 playback evidence.

The binding and settled-capture candidates, manifests and corresponding-source
packages were rebuilt from the clean kit commit and assembled under ignored
`build/ps5/mvp-test-kit/`. Rebuild them once more if HEAD changes after this
handoff update; never edit a manifest. The kit's `kit-manifest.json` is the
authoritative final hash list. Run `python3 tools/mvp_test_kit.py --deploy` for
a network-free dry run. Only after the user confirms the console window and the
two exact PayloadManager slots exist with matching schema-2 metadata may the
lead consider `--deploy --execute`; it uploads only and still does not launch.

The temporary collaboration workers from these protocol turns are no longer
live. Their changes were reviewed and corrected by the integration owner before
the commits above; the final AVDTP audit reported no blocking issue. Do not
assume `/root/acl_fragmenter`, `/root/avdtp_session`, `/root/hci_ownership`,
`/root/l2cap_signaling`, `/root/rtp_packetizer` or `/root/sdp_discovery` can be
resumed. The two older local worker chat IDs below remain historical references.

Remaining offline critical path: Classic baseband connection, explicit pairing
and key storage; AVDTP signaling reassembly and live media-channel binding;
transport pacing; reviewed target libsbc archive/self-test and its relink
distribution materials; experimental capture format/timestamp adapter; and the
bounded LAN listener/static page wired to actual owner state.
HCI activation still requires a source-proven single-owner dispatcher and the
one-command/credit acceptance experiment in NATIVE-HCI-RESEARCH.md. No raw USB
reader, timed credit refill or guessed socket broker is an alternative.

## Historical offline authority (superseded by renewed window above)

The morning hardware window ended, after which offline development resumed.
The later explicit LAN message renewed access and produced the results above.
Parallel work remains authorized for the scoped MVP.
Target is **192.168.1.111**, never the historical .10 address.
Prioritize finishing diagnostics/build tests and the real Rust-to-SBC offline path.
That baseline is complete at `62e3c39`. The user has now requested a new lead
chat to continue implementation and prepare the 2026-10-25 PS5 window. Read
MVP-NEXT-RUN.md for the authoritative relay plan and worker continuation roster.
Future sessions must check actual user availability instead of treating this
historical plan as a continuously open hardware window.
No change was approved to the native HCI ownership/credit integrity gate.
Do not enable a competing-reader backend or timed credit refill.

## Confirmed access

- FTP: 2121, anonymous connection works. Use `curl --request LIST` for listings;
  this server rejects curl's `--list-only`/NLST with 502.
- ps5upload: 9120; its Web UI: 9180. These are NOT elfldr's raw ELF protocol.
- elfldr 9021 is not listening. Do not blindly send ELF bytes to 9120.
- PayloadManager local slots: `/data/pldmgr/payloads/local/`.
- Existing baseline slot: `ps5-bt-hw1/ps5-bt-hw1-v6.elf`, schema-2 meta.json.
- Each diagnostic replaces `/data/ps5-bt-hw1.log`; retrieve before the next run.
- SDK full source: `/tmp/ps5-bt-upstream.dpjS9B/sdk`, verified Git revision
  `a7685ce8e97ce1f8a4f38253a41d54241c9124f1`. Temporary path is not permanent.
- LLVM: `/opt/homebrew/opt/llvm@18/bin/llvm-config` (18.1.8).

## Parallel ownership (shared working directory; do not overwrite)

- Main chat: build integration, runtime, deployment, common docs, final commits.
- Capture chat `01a16bd9-053d-7013-b602-44d5be34bfc7`: exclusive
  `platform/ps5/diagnostics/capture_probe.{c,h}`,
  `tests/platform/capture_probe_test.c`, `docs/CAPTURE-PROBE.md`.
- Codec chat `01a16bd9-07f6-7b83-8038-6b4ddc7ac94d`: exclusive
  `third_party/sbc/`, `platform/codec/`, `tests/codec/`, `tools/test_sbc.py`,
  `docs/SBC-IMPLEMENTATION.md`, and `crates/bt-codec/`.
- USB subagent `/root/usb_probe`: exclusive `usb_probe.{c,h}` (optional core),
  its tests and docs/USB-PROBE.md.
- Workers do not stage/commit/deploy. Main must review all source/license/test
  evidence. No actual captured audio, addresses, pairing keys or proprietary
  modules enter source control.

## Resume procedure

1. Inspect status and the latest 20 commits; preserve concurrent changes.
2. Read this file and each worker's implementation/handoff document.
3. Inspect compact `wait_threads` snapshots for the two chat IDs above; do not
   create duplicates. User authorized messages to these tasks for coordination.
4. Use the local evidence under docs/evidence and build/ps5; do not assume the
   console is available. Rebuild diagnostics without deploying. A future live
   run needs a newly confirmed window and matching artifact/log provenance.
5. Review capture callbacks/module ABI before wiring any hardware operation.
6. Commit independently reviewable completed changes with tests/docs. Do not
   stage unfinished workers or package a dirty source tree as corresponding source.

## Latest completed baseline

HEAD before this wave: `1ca499f` (offline PCM queue and recovery), preceding
`d3f0a0b` (layered recovery policy), `7580087` (working diagnostic lifecycle).
44 Rust tests, four PCM simulations, `make check`, C ASan/UBSan and PS5 HW-1
build passed in the previous wave. No playable audio exists yet.

## Live results in this wave (read this first on resume)

- USB first run completed on .111. User replied "已运行，手柄无异常".
  Deployed ELF SHA-256:
  `f5abd0e7e8d119a5d67fda0b1dc82adc9d872d8fc5d23c41cf2d9958794b2ab9`.
  Raw log saved locally in ignored `build/ps5/hw2-usb-first.log`.
- Own code read `/dev/ugen0.2`, VID:PID `1286:2059`, config 1, total bytes
  `0x0167`, four interfaces. HCI-class interface 0 uses 81/01/82/02;
  interface 2 ALSO declares HCI class and uses 84/04/85/05. SCO interfaces
  1 and 3 each have alternates 0..5. This is a topology candidate, NOT proof
  of independent controller state, native ownership or available credits.
- Other actual nodes `/dev/ugen0.1`, `/dev/ugen1.1`, `/dev/ugen2.1` returned
  normalized -1 from open. Directory and acquired device closed with 0.
  Inventory is honestly incomplete and main final status is 1, not a startup
  failure; full lifecycle returned. No permission changes were needed.
- Capture preflight (traced) executed successfully. All seven exports resolved,
  Ipmi/Avcap2 module load and secondary results were 0/success. ELF SHA-256:
  `8b200c3771c3ffb9b783a3fc3aeb3563f5ebaa928de494ba52d6fbdb26a58a3c`.
- Capture-once (without settling delay) executed successfully:
  `ced89c5c3c9dcedd0a5a453e4ae129386d5cd2bac97a4ca3f6ca805a02c711bb`.
  Initialize/Open/Start/Stop/Close/Terminate all returned 0; immediate read was
  Empty `0x81950002`, zero bytes. User said everything was normal. This proves
  lifecycle, not PCM acquisition, format or playable audio. Local raw log:
  `build/ps5/capture-once-first.log`.
- Second USB driver-binding artifact SHA-256
  `1cf0c2395ff219ebadc7584c01789c49ddfdc3f38f0b98798d4fdb22646f71fb`
  was deployed. Its launch/result was NOT confirmed before quota exhaustion.
- The optional 100 ms capture settling change was interrupted after updating
  capture_probe.h, then completed and host-tested after the usage reset. Main
  supplies a checked libkernel usleep callback. It has never been deployed or
  run on PS5. Do not call a successful no-delay run a settled-read pass.
- Hardware user said ~40 minutes remained around 03:01 UTC (11:01 Shanghai).
  At 03:12 UTC they said quota is nearly exhausted: finish hardware tests now;
  offline development resumes in the afternoon. Do not start Web work now.
- The morning diagnostic builds recorded dirty shared-checkout state honestly.
  Do not rewrite manifests to claim clean committed provenance. Source packages
  need a later clean rebuild; integration status follows below.

## Offline continuation after usage reset

Both parallel chats completed their scoped work and are idle/frozen. The USB
subagent is also finished. Diagnostic work is integrated as `44c902c`; the
independent codec change follows in a `feat(codec)` commit. Inspect Git for
the final HEAD rather than assuming this document's earlier baseline is current.
That codec commit is `62e3c39`. Clean-source diagnostic/source-package builds
passed there; USB/capture-once bundles independently reproduced identical ELF
bytes. The rule/relay documentation update comes after that build, so rebuild
manifests from the new clean HEAD before packaging/deployment.

- Capture: fail closed on nonzero module secondary results, distinguish no
  export lookup from a successful zero status, aligned scratch, exact optional
  100 ms wait, and consume-once cleanup; strict/sanitized host tests pass.
- USB: active configuration/binding metadata and local-error distinction are
  host-tested and target-compiled, not hardware verified. No active transport.
- Codec: real `PcmQueue` -> safe Rust -> C -> generic libsbc works on the host.
  Eight Rust checks, 208 encoder vectors, 12 decodes, sanitizers and replacement
  archive linking pass. The example emits one 77-byte joint-stereo SBC frame.
  Feature-on cross builds intentionally fail; no codec enters the PS5 ELF.
- Review `docs/SBC-IMPLEMENTATION.md` for caller-storage/unsafe invariants,
  source hashes, patches and remaining target/relink/distribution gates.
- Earlier dirty hardware builds retain their original identities. Clean
  rebuilt diagnostics/source packages do not inherit those hardware results.

Next offline deliverable: bind the completed SDP, L2CAP, AVDTP and RTP pieces
through an explicit paced media orchestration path, while separately advancing
baseband/pairing, target SBC and capture-format evidence. Keep the eventual
minimal LAN page thin and honest about implemented states. The next hardware
window still needs driver-binding metadata and one settled capture read first;
nonempty PCM format/timing and HCI routing/credit ownership remain separate
blockers.
