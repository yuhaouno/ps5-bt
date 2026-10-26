# Testing strategy

Host correctness, hardware behavior, audio quality, performance and headset
interoperability are separate evidence tracks. Never turn a host mock pass into
a hardware claim. Run `make check`; see [BUILDING.md](BUILDING.md) for setup.

## Imported versus checked-syscall openat comparison

`make application-raw-openat-test` checks the separately selected
[route comparison](RAW-OPENAT.md). Test full-width checked returns independently
from imported POSIX errno, exact syscall number/all six argument slots, dependent
stat skipping, first-error preservation, cancellation after failure, reverse
consume-once cleanup and sticky ownership uncertainty. An out-of-range positive
return must not be truncated into a descriptor or reported as clean cleanup.
Collector tests must preserve these distinctions and reject contradictory masks
and apparently successful incomplete profiles. Fixtures call synthetic adapters;
no host or console filesystem operation establishes native compatibility.

## Fixed directory open comparison

`make application-open-compare-test` runs strict C11 and ASan/UBSan core/main
fixtures for the [separate open comparison](OPEN-COMPARE.md). Check root prerequisite
failures, all combinations of independent case failures, skipped dependent stats,
continued second-case evidence, retained first error, cancellation even after an
earlier error, reverse consume-once cleanup and sticky uncertainty. No fixture
opens host root/data. Collector tests must reject contradictory attempted/case
masks and cannot report success after any native case error. Actual target behavior
and controller observations remain hardware requirements.

## Namespace observation candidate

`make application-namespace-test` checks the separately selected read-only
root/data procedure and its C main orchestration under strict C11 and ASan/UBSan.
Synthetic faults cover each fallible call, cancellation boundaries, reverse
consume-once cleanup, retained primary errors, sticky uncertainty, predicate
rejections and field disagreement. Runtime fixtures check the dedicated fixed
log and unchanged baseline log; collector tests distinguish procedure completion
from native ABI/store acceptance. These checks do not access host `/data` or a
console. Hardware still needs the [bounded admission](NAMESPACE-PROBE.md), exact
artifact identity and real controller observations.

## Current automated checks

The discovery/management continuation adds bounded Inquiry fixtures, protected
file-store native and fake-ABI cases, HTTP service/page/pump checks, actual
loopback listener checks and application runner/ACL boundary regressions.
See DEVICE-DISCOVERY.md, BOND-FILE-STORE.md, LAN-MANAGEMENT.md,
CONTROL-LISTENER.md and APPLICATION-RUNNER.md for their individual commands,
source baselines and hardware obligations. Run `make application-candidates-test`
on a Unix development host with the pinned Rust/C tools and Node.js for the
combined optional suites. This target never contacts a console. Explicit native
features use host headers only; external adapter features are checked freestanding without linking
or assuming PS5 imports. C fault harnesses cover ASan/UBSan; mock DOM/fetch
checks cover page state and full-width IDs. Neither process-crash checkpoints
nor loopback I/O establishes PS5 durability, scheduling or coexistence.

- Application owner: 19 checks cover selected-peer security through SDP/channel
  progression and Start/Stop/restart, normalized audio handoff and partial drain,
  foreign/stale traffic, security/binding revocation, failed initialization,
  forget, clock failures, total media/disconnect budgets and live retry/stability.
- Device manager: 19 checks plus one secret-record compile-fail check cover
  protected record validation, matching Ready-gated persistence, approval tokens,
  load/save/delete failures, forgotten-bond revocation and fail-closed clocks.
- Channel scheduler: 16 checks cover SDP and separate signaling/media channels,
  complete-PDU acceptance, continuation and epoch/ID/MTU bounds, clean reopen,
  cancellation, late callbacks and bounded peer-close replies.
- Continuous audio: 16 default checks cover sample pacing, bounded concealment,
  rebuffer/expiry and retained ownership. Two additional real-codec checks run
  with `cargo test -p bt-audio --features host-libsbc --locked`; the continuous
  case byte-compares 750 blocks/96,000 generated PCM frames with an independent
  persistent encoder and accounts for 2,250 synthetic ACL fragments. This does
  not establish PS5 capture, encode timing, audible playback or coexistence.

- Source-session binding: 22 checks join Classic Ready/context, SBC negotiation,
  media binding and the existing media sender, including fragmented responses,
  Start acceptance, Busy/credit stops, healthy partial-PDU drain, total stop
  deadlines, cleanup confirmation, full-width sample gaps/RTP wrap, clean restart,
  stale/expired offers, clock failure and sticky
  integrity escalation into the recovery controller. No live channels, capture,
  encoder scheduling, real credits or PS5 audio result are established.

- Selected-peer Classic link/security: 31 wire/lifecycle checks cover exact
  HCI commands and acceptance, peer/handle/epoch correlation even on failed
  acknowledgements, Unknown Command acknowledgement alternatives, stored-key
  versus fresh SSP authorization, negative unsupported pairing replies,
  authentication/encryption/key-size gating, cancel/connection/disconnect races,
  bounded cleanup and stale responses. Addresses/keys in fixtures are synthetic.
  These checks do not establish PS5 pairing, ownership or protected persistence.

- Owned ACL ledger: exact accepted/completed invariants, zero and full u16
  reservations, atomic malformed/foreign/duplicate/over-completion failure,
  stale epoch quarantine, cumulative counter exhaustion, Busy retry, all other
  backend errors with explicit uncertain outcome, and no clock refill.
  Media submission tests bind that ledger to real fragment cursors: Busy both
  before/after a prefix, exhaustion, terminal prefix cancellation, first-send
  uncertainty, idle failure and wrong owner/data-limit rejection. These are
  synthetic reservations and completion events, not PS5 ownership evidence.
- Native L2CAP availability boundary: all exports must resolve before calls;
  immediate errno capture, descriptor zero, control failure, one close attempt,
  cleanup uncertainty, atomic reports and real main ordering. Run
  `make native-l2cap-probe-test`; the same scenarios run under ASan/UBSan.
  Callback tests do not establish PS5 socket implementation or controller routing.
- LAN control boundary: exact four-route HTTP/1.1 parsing, byte/header/line
  ceilings, bearer/Host/Origin/custom-header enforcement, duplicate/smuggling/
  body rejection, single pending command ownership, stale completion rejection,
  failure/cancel state and atomic redacted status JSON. These are host tests of a
  protocol model; the v2 service/page and optional native listener have additional
  checks documented above. There is no PS5 command execution.
- Real host-only PCM -> Rust/C -> libsbc: eight Rust checks, including
  compile-fail !Send/!Sync; exact lengths/storage, rejected-no-state-change,
  raw backend errors and poisoning; 208 byte-identical encoder vectors and 12
  reference decoder checks; C ASan/UBSan and allocator-import rejection;
  separately rebuilt replacement archive linking. `make sbc-codec-test` also
  runs the 77-byte real SBC example and the complete 90-byte RTP/A2DP ->
  94-byte L2CAP -> three 44/44/18-byte synthetic ACL submissions with one Busy,
  two credit stops and three scripted completions. No headset, capture, real
  transport or target execution.
- Generated-signal SBC self-test: two independent init/finish cycles encode the
  same two fixed 48 kHz joint-stereo frames, compare both cycles byte-for-byte
  and require prefix `0x9cfd206f` plus four-frame FNV-1a digest `0x14e0ec79`.
  Invalid/short/unaligned/overlapping storage fails closed. The core is covered
  by ASan/UBSan and freestanding target compilation; it is not yet PS5 runtime
  evidence.
- SBC-over-RTP packetization: exact RTP/A2DP header bytes, one/three complete
  frames, negotiated MTU and output backpressure, retained marker and state on
  failure, frame count/length/sync rejection, dynamic payload/SSRC validation,
  and sequence/sample-timestamp wrap. It is not bound to a live AVDTP/media
  transport and makes no timing claim.
- AVDTP initiator: exact command/header bytes, conservative SBC selection and a
  scripted configure/open/start/suspend/restart/close/abort lifecycle; malformed
  and reserved bits, peer reject details, stale epoch/label/signal, collision,
  timeout/disconnect and short-output atomicity. The legacy single-message API
  still rejects fragments; the packet API adds 19 bounded reassembly/integration
  tests for packet counts, full-SDU lengths, fixed deadlines, cancellation
  quarantine, stale CID/epoch/transaction generation and fragmented responses.
  No live L2CAP channel, headset interoperability or PS5 execution is implied.
- Media send orchestration: complete SBC frame to RTP/L2CAP/ACL success across
  multiple fragments, byte-identical prepare retry, stale/foreign connection
  rejection, pre-send stop, partial-send cancellation gate, typed terminal
  failure, epoch invalidation and atomic MTU/scratch/retained-capacity failure.
  Acceptance is not controller completion and no test invents HCI credits.
- USB probe callbacks: actual-name selection, node/entry limits, exactly-once
  cleanup, partial metadata, active-configuration/driver-name validation and
  main failure mapping. `make usb-probe-test` performs no real USB operation.
- Capture preflight/lifecycle callbacks: resolve/load/open/read failures, raw
  status preservation, phase tracing, optional one-shot settling wait, byte/frame
  bounds and consume-once cleanup. `make capture-probe-test` uses synthetic
  callbacks; see HW2-EVIDENCE.md for the distinct real no-delay lifecycle.
- Capture observation core: caller-driven single-read steps, Empty/Overrun/Data
  accounting, exact call/deadline stops, cancellation, clock regression,
  invalid-length/error termination, atomic local validation and exactly-once
  cleanup token. The host test supplies synthetic time and reads; it is not a
  PS5 capture result or permission to infer a PCM format.
- Capture observation runner: exact immediate/Empty/Overrun/Data sequencing,
  20-call/two-second exhaustion, failed clock/wait/read, cleanup faults and
  clock regression; existing one-read/preflight modes remain covered. Main's
  clock bridge tests native error, dirty lookup, invalid timespec and overflow.
- PCM sample deadlines across five rates, capacity/wrap/stale/overflow rejection,
  bounded concealment across block boundaries, atomic rebuffer, retained future
  samples and exact expiry counts; 10,000 deterministic queue operations against
  a reference. `make media-recovery-test` also asserts four synthetic gap/stall
  scenarios, with no codec, RF, HCI credits or audible-quality claim.
- HCI fixture decode and byte-order assertions; every truncation/trailing byte;
  foreign events/opcodes and failed controller status; exhaustive declared event
  lengths versus available lengths (256 x 258 framing cases).
- Real C main -> Rust staticlib -> C adapter -> Rust decoder. A HOST-ONLY fake
  exercises the success path and 13 failures: ABI mismatch, unknown backend
  status, zero token, failed send, timeout, dirty timeout outputs, integrity loss,
  oversized claimed receive length, unknown kind, truncation, foreign opcode,
  failed controller command and shutdown failure. Cleanup is checked once.
- C stub conformance: no fake session on unsupported init, zero receive outputs,
  null output rejection and explicit unsupported operations.
- USB metadata parsing and atomic bounded reports: truncations, resource limits,
  duplicate endpoints/alternates and deterministic mutations. Run
  `make usb-descriptors-test` or `make check-c-sanitize` for ASan/UBSan.
- Native query boundary and real C main ordering: exact one-call read-only
  arguments, resolver/query/length failures and atomic reports. `make
  native-query-test` also links the baseline main without query dependencies;
  `make check-c-sanitize` covers the optional path with ASan/UBSan.
- Formatting, cargo check, Clippy warnings denied, and freestanding archive compilation;
  inspect every object as little-endian ELF64 x86-64 relocatable code.
- Build-tool failure paths and source-package clean-state, provenance, content
  and deterministic-output tests; `make check-build-tools` needs no SDK download.
- MVP kit plan self-tests pin `.111`, reject the historical target and path
  escapes, enforce bounded candidate metadata and verify hashing. Assembly is
  separately exercised against real clean-HEAD diagnostic manifests after the
  final commit; dry-run deployment performs no network operation.

None proves C can safely access hardware. The Rust wrapper depends on C obeying
pointer/capacity/lifetime rules; validation after FFI cannot undo an invalid
write. C descriptor tests run under AddressSanitizer/UndefinedBehaviorSanitizer
in CI. Add Rust Miri where useful for pure Rust code. Deterministic mutations
are regression coverage, not a coverage-guided fuzz campaign or ownership proof.

## Runtime v6 verification

The builder compiles/runs startup tests with and without each of the explicit
loader-kernel-resolver and capture flags. Synthetic cases cover module-handle
lookup, dirty/null/failed symbols, current-process fallback, kernel-init
failure, missing loader transport, durable log errors and unavailable stderr.
`make check-build-tools` also runs the actual x86-64 syscall wrapper against a
fake bridge: all six arguments, stack alignment, carry-set positive errno,
carry-clear negative result and MAP_FAILED are checked. This is a host ABI test;
no syscall is executed by that test. Hardware evidence is in HW1-REPAIR.md.

## Portable expansion

The SDP tests cover exact PDU/transaction/continuation framing, all element
type/size descriptors and Boolean bytes, integer extrema, truncations and explicit
byte/node/depth limits. The A2DP Sink client adds exact request bytes, arbitrary
chunk splits, full token-history loop detection, transaction/epoch/deadline
binding, atomic final validation and strict service/PSM/profile/UInt16 semantics.
See [SDP.md](SDP.md). These scripted peers do not establish a live query or
interoperability result.

Implement unit/state tests with each protocol component. Current AVDTP tests
cover collision, timeout, disconnect and stale responses around the bounded
single-packet session. Continue with malformed lengths, nested SDP limits,
L2CAP fragmentation/MTU bounds, foreign/stale handles and future AVDTP
fragment reassembly transitions. Use a virtual monotonic clock and bounded
scripted transport; no sleeps in state tests.
Check resource bounds and make every retry/failure observable.

Add a pinned cargo-fuzz/libFuzzer harness when parser surface grows; seed it
with synthetic fixtures and sanitized, redistributable HCI captures. Cover the
entire parse/state boundary, not only a parser that discards all results. Track
corpus provenance, crash minimization, byte/time/memory limits and regressions.
Meaningful properties include no panic, exact framing, bounded allocation,
round-trip validity where applicable, and no state change on rejected frames.
The bootstrap's exhaustive framing test is not a substitute for future fuzzing.

Captured traces must remove keys, addresses/serials, user information and unrelated
traffic; preserve length/framing and record sanitization so regressions remain
meaningful. Do not commit Sony firmware, library files, game data or PCM captures.

## Current host limitation, 2026-10-26

The preceding capture/audio/loop wave recorded 396 ordinary Rust checks plus
feature suites; current native application/resource suites are listed below.
On this ARM macOS host, `make check` stops at unchanged `tools/test_syscall_checked.py`
because the host cannot execute its generated x86 binary (errno 86, Bad CPU
type). This is unavailable host execution, not a passing syscall test or PS5
result. Keep the test intact; report the full Make target as blocked and run
remaining independent checks explicitly. Build/source-package/kit and local
result self-tests pass separately.

## Hardware and audio gates

Follow H0/H1/H2 in [HARDWARE-TEST.md](HARDWARE-TEST.md). Establish descriptor
access and native traffic ownership before production command/event traffic.
Separately selected, source-backed diagnostics may investigate unresolved
routing/ownership facts under [AGENTS.md](../AGENTS.md); the facts they intend
to measure need not already be established. Known API/layout and traffic bounds,
finite budgets, stop/cleanup, controller checks and a current authorized hardware
window are required. This supplies no blanket competing-reader, timer-credit or
system-modification exception. Fault tests
must verify cancellation, pending transfer storage, lost events, receive queue
overflow, clean close, invalidated sessions and return-to-loader. No silent
reset/reopen after integrity loss.

Functional, audio-quality, latency, coexistence and stability acceptance stays
on PS5. Continue compilation/static and relevant automated development checks;
they do not establish console support, and a Linux Bluetooth backend is not a
development prerequisite. Candidate implementations may be committed with
explicit PS5 validation pending. Preserve existing diagnostic baselines.

After minimal audio works, test headset power cycles, unexpected disconnects,
bounded reconnect, controller reconnect during audio, wireless DualSense input
and haptics workload, Wi-Fi load and RF congestion, PCM starvation/overrun,
unsupported codec capabilities and malformed packets. Record which hardware and
firmware combinations ran each case. Test repeated startup/shutdown and failed
partial initialization. Unknown outcomes remain Unknown.

Stage soak tests at 1 h and 8 h with explicit workload, memory high-water mark,
queue levels, underrun/overrun counts, USB and HCI completions per handle,
reconnects, encode timing and cleanup result. Define thresholds before testing;
do not omit failures or call a short tone a soak test.

For audio quality use known generated signals, a pinned reference decoder,
codec bitstream checks and measurements appropriate to the signal. Compare
end-to-end latency distributions using a repeatable external timing setup plus
capture/buffer/encoder/queue/pacing telemetry. Report receiver uncertainty,
sample count, method and confidence; HCI completion time is not acoustic latency.
No numeric quality or latency results exist yet.

## Continuous Rust target SBC checks

The optional `bt-audio/host-libsbc` suite additionally verifies the finite
continuous diagnostic, exact 750-block bitstream digest, clock/budget failures
and cleanup; the existing independent persistent-encoder comparison remains.
`make continuous-main-test` covers C-main invocation and pinned clock layout,
raw errors, dirty lookups, invalid timespec and overflow.
`python3 tests/codec/target_archive_test.py` rejects wrong object architecture,
missing wrapper exports and allocator imports. `make ps5-sbc-continuous` performs
the actual target link, ELF inspection and baseline replacement link. The source
packager tests both independent codec variants, complete relink identities and
failure rejection. None executes on PS5 or establishes real-time encoding.

## Experimental Rust capture checks

`make capture-stream-test` covers 13 portable format/normalization/timeline/
bridge/owner cases, the actual Rust/C fixture lifecycle, !Send compile rejection
and C ASan/UBSan faults. Coverage includes byte/frame/capacity and finite float
bounds, 10,000 deterministic float bit patterns, exact retained acceptance,
known/unknown gaps, read budgets, clock failures, reentry and cleanup failures.
The host fixture's explicit reset simulates a fresh process; the production
cleanup-uncertainty latch cannot be reset. `make capture-entry-test` checks the
separate main/clock boundary. `make ps5-capture-stream` builds/inspects the actual
target ELF and does not run it. Native format/time and continuous behavior still
require PS5 evidence; Empty is no PCM success.

## Bounded loop and actual capture/codec integration

`cargo test -p bt-app -p bt-runner --locked` exercises startup unwind, per-lane
quotas, urgent Stop, finite protocol teardown and resource reopen across the same
fault episode. `cargo test -p bt-runner --features host-libsbc --locked` adds the
actual declared capture → bridge/scheduler → persistent SBC → RTP/L2CAP/ACL
integration. Four unaligned capture chunk lengths produce four frames compared
byte-for-byte with an independent persistent encoder, across twelve synthetic
accepted ACL fragments. Queue Busy, backend Busy, explicit NoCredits/completions
and healthy partial-PDU Stop retain exact ownership. This exercises a real host
codec under synthetic transport, not native capture or headset playback.

## Native clock/wait fixture checks

`make scheduler-clock-test` covers cached resolution, cancellation without native
calls, exact short/max wait requests, raw status preservation, invalid time,
conversion overflow, nanosecond regression, reentry, sticky failure and close.
The feature-off workspace still has no clock symbol dependency. The adapter C
object was compiled/inspected with pinned LLVM18 as little-endian ELF64 x86-64,
with no undefined imports or SIMD register use. This is no PS5 timing result.

## Identity and local-result checks

`python3 tests/tools/test_diagnostic_result.py` exercises the five identity-bearing
profiles—capture-stream, continuous SBC and the three application variants—including exact native read/status/flag combinations, malformed or
missing identity/configuration/cleanup, dirty manifests, truncated/interrupted
runs, numeric bounds and private atomic file/directory-fsync failure behavior.
Main-entry C tests check begin/probe/experiment/finish ordering. Source-package
checks reject missing/invalid embedded identity and mismatched candidate names.
All input logs in these checks are synthetic; controller/audio/persistence
acceptance is never inferred from them.


## Concrete native startup and control-only loop

```sh
make application-startup-test  # factory/import/Rust startup and C-main suites
# Individual Rust commands:
cargo test -p bt-runner --locked
cargo test -p ps5-runtime --features application-control-host --locked
cargo clippy -p ps5-runtime --all-targets --features application-control-host --locked -- -D warnings
cargo check -p ps5-runtime --features application-control --target x86_64-unknown-none --locked
make ps5-application-control
```

The factory suite exercises every acquisition-stage failure, reverse cleanup,
explicit native-format gates, disabled resources, external store handoff and
actual protected-store/loopback/listener/codec owners with synthetic native clock
and capture exports. The selected-export C ASan/UBSan harness covers every missing,
dirty/null import, errno/lifetime, finite entropy-device lifecycle and cancellation
with consume-once close; it opens no console device. The target build checks the
exact pinned header dependency union, actual C/Rust link and ELF structure.

Startup/driver tests cover store leases without self references, pre-entry and
between-stage Stop, persistent Stop during delayed drain, finite clock/wait/turn
faults, normal request/response continuation across borrowed loop views and
reverse cleanup. Runtime tests run actual private temporary POSIX stores and
loopback TCP clients with the real C clock context and synthetic reviewed exports.
They keep transport/capture/codec unavailable and cover ephemeral endpoint/token
bounds, clock and wait errors, prior client-close uncertainty and backing-storage
poison guards. No credential or Bluetooth operation is admitted by these tests.

HTTP request/response buffers and the owned bearer guard are scrubbed on return;
reports must contain no token, private path, address, sample or pointer. Primary
failure and earlier uncertain client close cannot be erased by a later successful
Listener.close. Source-package and local-result checks must bind the independent
application candidate identity, exact header inventory and numeric operation/
cleanup masks, rejecting missing or interrupted outcomes. Detailed test evidence
belongs to the final committed handoff; these commands do not prove PS5 imports,
entropy, protection, latency, durability, scheduler behavior or coexistence.


## Integrated generated and raw-observation application checks

```sh
cargo test -p ps5-app --features host-candidate --test generated --test observe --test rebuild --locked
cargo test -p ps5-runtime --features application-control-host --locked
make ps5-application-generated
make ps5-application-observe
```

Generated tests compare a persistent real encoder against an independent owner
for 750 blocks, and check observed sample deadlines, retained-frame queue Busy,
finite operations, clock/expiry/codec failures and Stop. The integrated joint
profile requires 96,000 generated stereo frames, 57,750 encoded/discarded bytes,
prefix `9cfd20cb` and FNV-1a `7784d1fb`; no frame is sent to Bluetooth. Runtime
checks retain actual clock/store/listener acquisition and explicit codec finish
around that workload, instead of substituting an isolated codec self-test.

Observation tests preserve the existing CaptureOwner budget and exact raw
Data/Empty/Zero/Overrun/Ended/Error results, no catch-up after late turns, Stop
before another read, owner interleaving rejection and consume-once cleanup
uncertainty. The selected 100 ms settle plus 10 ms interval gives up to 190 scheduled
polls before an exclusive two-second deadline; 256 is a hard call ceiling. Actual
Rust/C fixtures provide synthetic reviewed exports, not a verified PS5 format.
A nonempty buffer, elapsed host observation or Empty result establishes no PCM,
sample count, timestamp unit, codec input or audible continuity.

The scoped NativeRebuild integration uses actual protected store locks and
loopback listener open/close/reopen on the same stable storage. It checks partial
bind failure, unchanged fault budgets, explicit working/stable evidence while
owners remain live, Stop during wait/acquisition, incomplete scope poisoning,
previous cleanup uncertainty and a real C capture Stop failure. A compile-fail
case rejects a native-owner escape. No existing main loop automatically
reconnects, and no rebuild hardware artifact is added by these tests.

Target builds check distinct mode/feature selection, native imports, actual
C/Rust linkage and ELF structure. Generated codec packaging must retain matching
LGPL source/replacement/relink materials. Source/kit/result validation covers
nine inventory entries and the separate three-item startup plan, missing or
mismatched identities, bounded profile summaries, interrupted runs and uncertain
cleanup. The console helper's preflight/collection tests use mocked transports;
its default commands perform no network action and never create/upload/launch
or change permissions. Actual native 0700 ownership/protection still needs the
payload's own check. A read-only log fetch does not imply normal controller
operation: explicit before/during/after observations remain required.

The next proposed setup is the reported Slim/13.40, PayloadManager 0.6.3 and XM4.
These offline checks authorize no current deployment or run. Native L2CAP errno 47
and the absent safe active-HCI route remain critical blockers to pairing/playback
acceptance; the three application diagnostics test local integration only.

## Next-window preparation tooling

`make application-setup-test` runs strict C11 and ASan/UBSan namespace and actual
main orchestration fault tests, all using injected operations.
`make check-build-tools` includes build/source/kit checks, diagnostic-result and
fake-FTP collection tests, fresh-machine preparer faults and standalone portable
transfer validation/extraction. Tests never contact a console. The overall
check target's unchanged x86 syscall execution fixture remains unavailable on
ARM macOS; that overall target is not a pass.

For native testing use the six-item `tools/window_test_plan.json`, temporary
[TEMP-PS5-TEST-HANDOFF.md](TEMP-PS5-TEST-HANDOFF.md) and
[PS5-WINDOW-DECISION-MATRIX.md](PS5-WINDOW-DECISION-MATRIX.md). Build/inspect each
clean-source artifact, retain corresponding sources and collect one bounded log
before launching the next. Native correctness, cleanup, controller coexistence
and audio/transport acceptance require actual separately observed PS5 results.
