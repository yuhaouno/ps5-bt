# Testing strategy

Host correctness, hardware behavior, audio quality, performance and headset
interoperability are separate evidence tracks. Never turn a host mock pass into
a hardware claim. Run `make check`; see [BUILDING.md](BUILDING.md) for setup.

## Current automated checks

- LAN control boundary: exact four-route HTTP/1.1 parsing, byte/header/line
  ceilings, bearer/Host/Origin/custom-header enforcement, duplicate/smuggling/
  body rejection, single pending command ownership, stale completion rejection,
  failure/cancel state and atomic redacted status JSON. These are host tests of a
  protocol model; there is no listener, browser page or PS5 command execution.
- Real host-only PCM -> Rust/C -> libsbc: eight Rust checks, including
  compile-fail !Send/!Sync; exact lengths/storage, rejected-no-state-change,
  raw backend errors and poisoning; 208 byte-identical encoder vectors and 12
  reference decoder checks; C ASan/UBSan and allocator-import rejection;
  separately rebuilt replacement archive linking. `make sbc-codec-test` also
  runs the 77-byte real SBC example and the complete 90-byte RTP/A2DP ->
  94-byte L2CAP -> 98-byte synthetic ACL framing example. No headset, capture,
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
  timeout/disconnect, short-output atomicity and explicit fragment rejection.
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

## Hardware and audio gates

Follow H0/H1/H2 in [HARDWARE-TEST.md](HARDWARE-TEST.md). Establish descriptor
access and native traffic ownership before command/event traffic. Fault tests
must verify cancellation, pending transfer storage, lost events, receive queue
overflow, clean close, invalidated sessions and return-to-loader. No silent
reset/reopen after integrity loss.

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
