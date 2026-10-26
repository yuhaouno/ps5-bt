# Audio continuity and transport integrity

Approved direction: 2026-10-24. This supersedes interpretations of earlier
bootstrap restrictions that would prohibit all media recovery, initialization
allocation, separate execution stages or offline work until hardware is ready.
It does not establish a Bluetooth broker or relax native traffic preservation.

## Recovery boundary

| Condition | Response | Must not happen |
| --- | --- | --- |
| PCM temporarily absent | bounded, counted silence/concealment; rebuffer beyond its configured limit | unlimited silence reported as successful capture |
| PCM too old to meet its playout time | discard with exact frame counts before encoding/fragmentation | unbounded backlog or dropping an already-partially-sent PDU on a healthy connection |
| Persistent clock drift | measure occupancy/clock mismatch; independently validate resampling later | inventing credits or assuming a larger queue fixes clock-rate mismatch |
| Owned capture session fails | bounded, cancellable recovery after confirmed resource cleanup | hidden restart loops or using uncertain handles |
| Raw HCI events/ACL lost, handle ownership unknown, credits unreconciled | explicit transport failure; reconcile by a verified mechanism or end the session | timed credit refill, pretending traffic was preserved, blind controller reset |
| Headset disconnects | bounded reconnect with a new epoch and exact cleanup | old completions credited to the new connection |

Receive integrity remains strict. Media continuity policy is distinct and may
be tolerant; neither changes what USB completion or HCI completion means.
An explicit terminal transport failure or strictly newer connection epoch may
cancel the unsent remainder of a partially accepted PDU, invalidate the old
session and report the exact accepted prefix. This never implies completion or
refunds uncertain outstanding credits; ordinary stop is not reconciliation.

Recovery budgets apply per fault episode and must define attempt/time limits,
backoff, stop/cancel, waiting/exhausted states and a stable-success condition
before a new budget is available. A failed attempt cannot reset its own budget.
Exhaustion need not terminate the payload: it may remain visibly unavailable
until an explicit user retry or documented new external trigger. Do not turn
this into an invisible retry loop or require a restart after every recoverable
gap. These are implementation rules, not a claim of improved audible continuity.

## Development lanes and acceptance

Offline PCM buffering, sample-clock timing, codec boundary tests, packetization
and scripted recovery may proceed while native ownership is investigated.
Source-backed isolated transport diagnostics follow the experimental admission
rules in [AGENTS.md](../AGENTS.md), without needing to pre-prove the facts they
are intended to measure. No active competing-reader backend is thereby approved.
An eventual native dispatcher/driver-path adaptation needs a separate scope/ABI
review; changing this process's credentials alone does not create such a dispatcher.

Functional, audio-quality, latency, coexistence and stability acceptance remains
on PS5. Development checks remain useful, but a Linux Bluetooth backend is not
a prerequisite and synthetic outcomes cannot substitute for console results.

Use bounded caller storage and explicit deadlines. Initialization allocation
can be fallible and bounded; the real-time per-frame path must not allocate.
Single transport ownership does not require capture/encode/transport to block
one another. SIMD is neither mandatory nor banned permanently; each optimized
unit needs a reviewed calling convention and verified runtime support.

First prove continuity with conservative negotiated SBC, then reduce latency,
then compare higher-quality configurations. Compare against HearBridge on the
same hardware/headset/workload: audible gaps per hour, longest gap, external
end-to-end latency distribution, actual codec parameters and wireless pad
behavior. No superiority claim follows from implementation language or tests.

## Implemented offline foundation

`bt_core::recovery::Recovery` now implements the fault-episode budget separately
from PCM rendering. Its safety tickets distinguish cleanup from integrity/credit
reconciliation, attempt acceptance spends budget exactly once, and stable success
is required before automatic reset. Exhaustion/stop retain outstanding safety
requirements; no method opens resources, sleeps, resets the controller or grants
credits. See [the controller contract](RECOVERY-CONTROLLER.md) for API, 13
development tests and PS5 evidence limits.

`bt_core::media` is safe `no_std` Rust without an allocator or transport access:

- `SampleClock` derives absolute microsecond deadlines from nominal sample
  indices, rounding upward once per deadline rather than accumulating rounded
  block durations. Checked arithmetic reports overflow. It does not estimate
  physical capture-clock drift or resample audio.
- `PcmQueue` borrows caller storage for timestamped stereo i16 frames. This is
  downstream of an eventual explicit f32-to-i16 conversion, not a replacement
  for `PcmSource`. Capacity rejection never overwrites old input. Reordered,
  overlapping and already-rendered input is rejected.
- `render` operates on an explicitly chosen sample window. Real frames reset
  the consecutive-missing counter; zero concealment is counted and bounded
  across calls. An over-limit window returns `Rebuffer` without consuming input,
  modifying output or advancing the timeline. The caller must wait or stop,
  not retry-spin. This primitive does not promise click-free concealment.
- `discard_before` drops only unencoded, unsent PCM with exact frame counts.
  Successful forward rendering separately reports skipped timeline frames and
  expired stored frames. Neither path releases HCI credit. Reconnect/new capture
  epochs require new queue/timeline state; there is no implicit restart.

`make media-recovery-test` is part of `make check`. Its 48 kHz, 128-frame
quantum and 256-frame concealment limit are synthetic fixtures, not measured
or recommended hardware settings. Each scenario spans 23,040 nominal frames:

| Script | Asserted outcome |
| --- | --- |
| Steady capture | 23,040 source frames, no silence, expiry or rebuffer |
| 256-frame capture gap (5.33 ms) | exactly 256 silence frames, no rebuffer |
| 1,024-frame capture gap (21.33 ms) | 256 silence frames, six rebuffer returns, 768 timeline frames skipped on resume |
| Sender blocked for 3,840 frames (80 ms) | 3,840 expired PCM frames discarded, 3,840 timeline frames skipped; queue high-water at most 2,048 frames |

Six rebuffer returns are six polling outcomes, not six audible interruptions.
Discarding old audio bounds backlog but loses that audio: the 80 ms stall is
not made continuous. The script chooses readiness and nominal render positions;
it does not model a real jitter buffer, encoder load, controller flow control,
RF behavior or headset buffering. Ten media tests additionally check atomic
failure, wraparound, future samples, concealment limits and clock arithmetic.
The PS5 diagnostic does not invoke this module; live adapter status is unchanged.

## Current evidence and operating context

The user confirmed visible HW-1 start/finish notifications and normal wireless
DualSense after final status 2. Deliberate panic and long-run stability remain
Untested. Future target address: **192.168.1.111**, superseding the historical
192.168.1.10 address in HW1-REPAIR.md. No connection or deployment is automatic.

HearBridge's device diagnostics were read directly on 2026-10-24 before this
offline wave: user-reported CFI-2200 B01 / 13.400.002; /dev/ugen0.2, 1286:2059,
interface 0 alt 0, event 0x82, ACL IN 0x81, ACL OUT 0x01; reported controller
capacity 7 packets of 1021 bytes. These are upstream diagnostic observations,
not PS5-BT enumeration or an exclusive credit reservation. No device addresses
or pairing material are copied into this document.

The user reports upstream WH-1000XM4 playback with the wireless pad working,
but occasional audible interruptions. A log sample included cumulative timed
fallback and media-drop counters; those are not counts of audible dropouts.
The pinned HearBridge hci_usb.c describes competing native reads; acl_pool.c
implements timed refill. rate.c/main.c already implement queue/rate/latency
adjustments. Pin/provenance remain in UPSTREAM.md; no source is imported here.
The cause of a particular audible interruption needs time-correlated capture,
queue, completion and RF observations. No further hardware tests run tonight.
