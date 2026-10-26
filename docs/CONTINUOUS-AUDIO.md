# Continuous normalized PCM scheduling candidate

`bt-audio` implements an allocation-free, safe `no_std` continuously driven
PCM -> complete SBC block scheduler. It reuses `bt_core::media::PcmQueue` and
`SampleClock`, and can feed `bt-session::SourceSession::queue_sbc_frames_at`.
The default feature owns no capture ABI, codec C state, platform clock, socket,
transport or HCI credits. It does not activate the PS5 diagnostic or establish
headset playback, audible continuity, latency or wireless controller coexistence.

## Storage, format and clock

The caller supplies three disjoint mutable allocations for the scheduler's
entire lifetime: timestamped `TimedStereoFrame` queue storage, `[[i16; 2]]`
render scratch and retained encoded-byte storage. Capacity is checked before
construction. There is no heap or per-block allocation. Each pull calls the
encoder at most once for one complete negotiated SBC block. One block per media
packet is a conservative integration candidate, not a measured optimal packet size.

Input is already normalized, known-rate stereo i16. `push_pcm(first_sample,
samples)` uses monotonic positions relative to stream sample zero; it never
accepts native PS5 bytes, guesses a float format or interprets native timestamps.
Full, overlapping, reordered or already-rendered input is rejected without
replacing old samples. Conversion and native capture layout/units remain an
explicit upstream adapter obligation.

`Config.parameters` is an existing validated stereo or joint-stereo SBC
selection; it must match the persistent `BlockEncoder`. `epoch_us` is the chosen
nominal delivery time of sample zero, not a guessed capture timestamp. The
scheduler derives every due time from the absolute sample index, rounding upward
once; it does not accumulate rounded block durations. Future blocks wait until
their sample deadlines. Input buffering relative to capture and encoder execution
cost still require actual PS5 measurement; this component cannot promise an
acoustic delay from a nominal clock.

`max_lateness_us` sets an exclusive expiry at `due + max_lateness`. Late,
wholly unencoded windows are skipped to the first unexpired block boundary with
exact expired stored-PCM and skipped timeline counts. `SampleClock` is nominal:
there is no physical clock-drift estimator, resampler or adaptive delay policy.
An optional absolute `deadline_us` bounds a candidate run. Overflow and regressing
caller-supplied monotonic time are explicit terminal failures.

## Complete-block ownership handoff

The usable owner loop is:

1. Supply normalized PCM through `push_pcm` whenever capture has produced it.
2. Call `pull(now_us, &mut encoder)`. `WaitUntil` is a real wait boundary, not
   permission to busy-spin; it includes earlier run/rebuffer deadlines.
3. `Ready(EncodedBlock)` identifies one complete retained frame with opaque
   ticket, full-width `first_sample`, nominal due/expiry and real/concealed counts.
   Borrow it using `encoded_frame(ticket)` immediately after a fresh pull.
4. Synchronously call `SourceSession::queue_sbc_frames_at(block.first_sample,
   &[frame], rtp_scratch, l2cap_scratch)`. Whole-block successful queueing transfers
   ownership to Source; call `confirm_accepted(now, ticket)` exactly once.
5. A whole-block rejection uses `reject(now, ticket, Backpressure::Busy)` or
   `NoCredits`. Polling preserves the same encoded bytes/ticket without another
   encode until expiry. Credit status is supplied by the real owner; this enum
   never reads, estimates or changes a credit ledger.
6. Unknown acceptance/other terminal consumer failure uses `consumer_failed`.
   It must not be relabeled Busy or retried. The caller must reconcile its
   downstream operations independently. Do not issue another pull/stop between
   synchronous queue acceptance and confirmation; the scheduler cannot observe
   an undisclosed ownership transfer.

A known successful handoff is recorded before reporting newly detected bad/late
confirmation time. Accepted bytes are never taken back or offered again, even
if confirmation detects run expiry or a regressing clock. Late acceptance is
counted explicitly. Tickets are scoped to one scheduler instance/stream; reuse
across instances is prohibited. A stale ticket cannot consume newer output.

Before handoff, an expired encoded block is wholly unsent and can be dropped,
with exact block/PCM-frame counts. After handoff, **only Source owns that media**.
If Source has a partially accepted L2CAP PDU, scheduler expiry/catch-up cannot
replace it, discard its remainder or refund any HCI credits. Source continues
its existing drain/failure/accepted-prefix accounting. Its full-width
`queue_sbc_frames_at` preserves actual PCM sample positions in RTP across gaps;
a shortened sequential timestamp would conceal missing audio duration.

The bounded pipeline can hold one Source PDU plus one scheduler block and the
caller PCM queue. That bounds memory but does not alone bound the age of an
already accepted Source PDU: real owner scheduling and Source stop/failure rules
remain necessary. This candidate does not silently discard a Source-owned PDU,
even when it has not yet accepted its first ACL fragment.

## Underrun, rebuffer, cancellation and counters

`PcmQueue` performs counted zero concealment up to the configured consecutive
missing-frame limit. Its over-limit render is atomic: no PCM consumed, no output
written and no encoder call. The scheduler returns `Rebuffer { first_sample,
deadline_us }`; resume on new input or a bounded owner timer, not an invisible
retry loop. The rebuffer episode deadline is retained across repeated polls and
timeline catch-up. Successful rendering ends that episode; expiry fails explicitly.
Zero concealment is a simple policy primitive, not a click-free audio claim.

`Snapshot` exposes phase, failure, next sample, queue occupancy, pending block,
rebuffer deadline and checked cumulative counters. `underrun_windows` counts
distinct evaluated sample windows. `rebuffer_returns` counts polling outcomes,
not audible dropouts; `rebuffer_episodes` counts entries into rebuffer state.
`expired_pcm_frames` counts discarded stored input, `skipped_sample_frames`
counts windows never rendered, and `expired_encoded_frames` counts rendered
blocks dropped before handoff. These categories are distinct, not interchangeable
measurements of audible interruption. Rendered-source/concealment counts remain
observable even if the following codec call fails.

Codec errors and incorrect output length/sync are terminal. The caller owns the
persistent encoder lifetime and exactly-once finish. `stop()` is clock-independent
and idempotent, prevents future input/encoding and counts discarded local queued
PCM/retained encoded blocks; a prior failure remains observable. It does not stop
capture, finish the codec or touch Source. To stop the application, the owner
must separately stop capture, call Source's healthy drain/stop or explicit
terminal cancel, reconcile uncertainty and clean native resources.

## Optional existing real host encoder

`host-libsbc` adds `HostSbcEncoder`, which owns the existing safe `bt-codec`
encoder plus a caller-borrowed i16 interleaving scratch slice. It copies exactly
one stereo block into that scratch and delegates to the existing checked C seam.
Encoder storage, !Send/!Sync behavior, poison handling and exactly-once finish
remain those of `bt-codec`. A short scratch or wrong input length is rejected.

This imports no new upstream source. It reuses the already reviewed selected
BlueZ libsbc 2.2 component and provenance/license/relink obligations in
[SBC-IMPLEMENTATION.md](SBC-IMPLEMENTATION.md) and [UPSTREAM.md](UPSTREAM.md).
The host feature retains `TARGET == HOST`: enabling it for the freestanding
PS5 target is intentionally unsupported by that host feature. The independent
`target-libsbc` feature now selects the same persistent `SbcEncoder` against a
reviewed external target archive. [SBC-CONTINUOUS.md](SBC-CONTINUOUS.md) describes
the separately built Rust diagnostic; the short C self-test remains selectable.

## Development checks and PS5 acceptance

```sh
cargo test --locked --offline -p bt-audio
cargo test --locked --offline -p bt-audio --features host-libsbc
cargo clippy --locked --offline -p bt-audio --all-targets --features host-libsbc -- -D warnings
cargo check --locked --offline -p bt-audio --target x86_64-unknown-none
```

Sixteen default checks cover 1,000 paced synthetic blocks, retained Busy/no-credit
bytes, exact expiry/catch-up, concealment/rebuffer recovery/budgets, stale tickets,
known late acceptance, clock/codec failures, capacity rejection and independent
Source partial-PDU ownership. Two additional real-codec feature checks include
750 blocks / 96,000 generated normalized PCM frames / 2,250 synthetic accepted ACL
fragments, byte-compared with a second persistent existing encoder and with
explicit synthetic completions. That is two seconds of nominal sample positions,
not two seconds of audio playback, real-time performance or RF behavior.

Real PS5 acceptance still needs established transport/channel/credit ownership,
reviewed native PCM conversion, target codec execution, capture/encode timing,
actual headset playback, bounded failure/stop cleanup and wireless pad coexistence.
All audible-gap, quality, latency, drift and stability outcomes remain
**Requires hardware**. No Linux Bluetooth backend or desktop audio demonstration
is introduced or required.
