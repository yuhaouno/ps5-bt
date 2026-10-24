# Testing strategy

Host correctness, hardware behavior, audio quality, performance and headset
interoperability are separate evidence tracks. Never turn a host mock pass into
a hardware claim. Run `make check`; see [BUILDING.md](BUILDING.md) for setup.

## Current automated checks

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

None proves C can safely access hardware. The Rust wrapper depends on C obeying
pointer/capacity/lifetime rules; validation after FFI cannot undo an invalid
write. C descriptor tests run under AddressSanitizer/UndefinedBehaviorSanitizer
in CI. Add Rust Miri where useful for pure Rust code. Deterministic mutations
are regression coverage, not a coverage-guided fuzz campaign or ownership proof.

## Runtime v6 verification

The builder compiles/runs startup tests both with and without the explicit
loader-kernel-resolver flag. Synthetic cases cover fallback lookup, kernel-init
failure, missing loader transport, durable log errors and unavailable stderr.
`make check-build-tools` also runs the actual x86-64 syscall wrapper against a
fake bridge: all six arguments, stack alignment, carry-set positive errno,
carry-clear negative result and MAP_FAILED are checked. This is a host ABI test;
no syscall is executed by that test. Hardware evidence is in HW1-REPAIR.md.

## Portable expansion

The SDP foundation tests exact PDU/transaction/continuation framing, all element
type/size descriptors and Boolean bytes, integer extrema, truncations and explicit
byte/node/depth limits. See [SDP.md](SDP.md). Response chunks are deliberately
separate from complete-tree validation; no discovery/session pass is claimed.

Implement unit/state tests with each protocol component. Include malformed
lengths, nested SDP limits, L2CAP fragmentation/MTU bounds, foreign/stale handles,
transaction timeouts, AVDTP collisions and disconnects at each transition. Use a
virtual monotonic clock and bounded scripted transport; no sleeps in state tests.
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
