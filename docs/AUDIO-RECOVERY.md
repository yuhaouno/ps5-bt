# Audio continuity and transport integrity

Approved direction: 2026-10-24. This supersedes interpretations of earlier
bootstrap restrictions that would prohibit all media recovery, initialization
allocation, separate execution stages or offline work until hardware is ready.
It does not establish a Bluetooth broker or relax native traffic preservation.

## Recovery boundary

| Condition | Response | Must not happen |
| --- | --- | --- |
| PCM temporarily absent | bounded, counted silence/concealment; rebuffer beyond its configured limit | unlimited silence reported as successful capture |
| PCM too old to meet its playout time | discard with exact frame counts before encoding/fragmentation | unbounded backlog or dropping an already-partially-sent PDU |
| Persistent clock drift | measure occupancy/clock mismatch; independently validate resampling later | inventing credits or assuming a larger queue fixes clock-rate mismatch |
| Owned capture session fails | bounded, cancellable recovery after confirmed resource cleanup | hidden restart loops or using uncertain handles |
| Raw HCI events/ACL lost, handle ownership unknown, credits unreconciled | explicit transport failure; reconcile by a verified mechanism or end the session | timed credit refill, pretending traffic was preserved, blind controller reset |
| Headset disconnects | bounded reconnect with a new epoch and exact cleanup | old completions credited to the new connection |

Receive integrity remains strict. Media continuity policy is distinct and may
be tolerant; neither changes what USB completion or HCI completion means.

## Development lanes and acceptance

Offline PCM buffering, sample-clock timing, codec boundary tests, packetization
and scripted recovery may proceed while native ownership is investigated.
No active competing-reader backend is thereby approved. An eventual native
dispatcher/driver-path adaptation needs a separate scope/ABI review; changing
this process's credentials alone does not create such a dispatcher.

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
