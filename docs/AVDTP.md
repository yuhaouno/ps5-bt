# Bounded AVDTP initiator session

Status: implemented and host-tested as allocation-free signaling logic. It has
not opened a live L2CAP channel, contacted a headset, run on PS5 or produced
audible output.

## Scope and protocol baseline

`bt-avdtp` is safe `no_std` Rust for the A2DP Source's initiator path. It follows
AVDTP 1.3 (adopted 2012-07-24) sections 8.2--8.21 and 9 for signaling headers, transaction labels,
commands, capabilities and state transitions. SBC fields follow A2DP 1.4 section
4.3.2. The implementation is original and imports no upstream protocol code.

The supported setup sequence is:

```text
Discover -> Get Capabilities -> Set Configuration -> Open
         -> caller establishes media L2CAP transport -> Start
         -> Suspend/Start or Close -> caller releases transport
```

Abort provides bounded teardown after a stream has been configured. Discovery
and capability collection do not change stream state. Set Configuration encodes
Media Transport plus one SBC Media Codec capability. Open acceptance enters
`TransportOpening`; it is not called `Open` until the caller explicitly reports
that the media transport channel was established. Likewise, accepted Close and
Abort retain teardown states until the caller reports transport cleanup.

The official baselines are the Bluetooth SIG's
[AVDTP 1.3](https://www.bluetooth.com/specifications/specs/a-v-distribution-transport-protocol-1-3/)
and [A2DP 1.4](https://www.bluetooth.com/specifications/specs/advanced-audio-distribution-profile-1-4/).
The receive reassembler was checked directly against the
[adopted AVDTP 1.3 PDF](https://www.bluetooth.org/DocMan/handlers/DownloadDoc.ashx?doc_id=260860),
sections 8.3, 8.4.1--8.4.6 and 8.20.4 (printed pages 58--61 and 75), on
2026-10-26. The catalog also requires Errata Correction 23224 for compliance
claims; this project makes none. No upstream reassembly source was imported.
Passing these tests is not a Bluetooth qualification or interoperability claim.

## Bounded wire contract

The typed header parser preserves the four-bit transaction label, all four
packet types and all four message types. Signal identifiers are typed for every
AVDTP 1.3 command; the raw six-bit value is retained until validation rather
than silently masking unknown or reserved identifiers. A General Reject is
correctly treated as its one-octet header without a Signal Identifier and is
correlated to the pending transaction label. Reserved header, SEID, endpoint
and codec bits must be zero. Endpoint IDs enforce the valid `1..=0x3e` range.
Discovery responses expose a borrowed iterator and select only an available
audio Sink.

The original `Initiator::receive(epoch, bytes)` still accepts a complete Single
packet and rejects fragments as `FragmentationUnsupported`. The new
`receive_packet` path accepts one signaling L2CAP SDU at a time through a
caller-owned `SignalingReassembler`, then uses the same semantic response parser.
It returns `None` until the message is complete. The caller does not synthesize
a Single header, concatenate wire headers or validate the same response again.

## Receive reassembly

`SignalingContext { epoch, channel_id }` identifies one ordered signaling
channel; `channel_id` is its local dynamic L2CAP CID (`>= 0x0040`). The caller
must establish this binding rather than label arbitrary traffic as belonging to
the channel. Every packet must match both fields. Reusing a CID after reconnect
requires a newer epoch. The standalone reassembler validates wire structure;
`Initiator::receive_packet` additionally checks the pending label, signal and
response type before accepting Single or Start data. Partial messages are bound
to the initiator's internal transaction generation, so an abandoned partial
cannot satisfy a later transaction after the four-bit wire label wraps. Do not
mix standalone `push` and `receive_packet` within a partial message.

The storage borrow bounds the assembled payload, without allocation or retained
input pointers. `ReassemblyLimits` bounds packet count, complete SDU size and a
nonzero timeout in microseconds. The fixed deadline starts with Start and never
extends on Continue. Caller timestamps must be monotonic; deadline overflow and
clock regression are explicit errors. A complete message and its parsed outcome
borrow storage only until the next mutable access, enforced by Rust lifetimes.

Wire checks enforce Start/Continue/End order, matching label and message type,
the declared packet count including Start and End, full Start/Continue **SDU**
length equality, and End no larger than Start. The different header lengths
mean equal SDU lengths do not mean equal payload contributions. Start needs at
least two packets for this fragmented sequence; the eight-bit count can reach
255. Header-only fragment payloads are permitted when the sequence is otherwise
valid. Reserved signal identifiers and malformed headers are rejected.

Single or another Start never replaces an active partial message. Rejected
packets preserve partial accounting and buffer bytes. `partial()` exposes the
declared/received counts, accepted payload bytes and deadline. `cancel` and
`expire` return that accounting once; cancellation or expiration of a partial
quarantines the receiver until `reset` receives a strictly newer epoch. An
expired packet is rejected at the exclusive deadline without consuming the
pending Initiator command. The caller must also use `timeout` or `disconnected`
to invalidate that session, then reset the reassembler to the same newer epoch.
No method retries or silently abandons a pending command.

AVDTP 1.3 section 8.3 permits a same-SEID Abort command to interrupt a fragmented
message. This initiator-response path has no peer-command/acceptor handler:
incoming commands remain explicit collision/unexpected-command errors and do
not replace a partial response. Supporting that exception needs a separately
bounded peer-command path. Outbound command fragmentation is also not provided.

For example, after emitting Discover for epoch 7:

```rust
use bt_avdtp::{Initiator, ReassemblyLimits, SignalingContext, SignalingReassembler};

let context = SignalingContext { epoch: 7, channel_id: 0x0040 };
let mut storage = [0u8; 256];
let mut receiver = SignalingReassembler::new(context, &mut storage, ReassemblyLimits {
    max_packets: 8, max_packet_bytes: 48, timeout_us: 1_000_000,
}).unwrap();
let mut initiator = Initiator::new(7);
initiator.discover(&mut [0u8; 2]).unwrap();
assert!(initiator.receive_packet(&mut receiver, context, 10,
    &[0x06, 2, 0x01, 0x04]).unwrap().is_none());
let outcome = initiator.receive_packet(&mut receiver, context, 20,
    &[0x0e, 0x08]).unwrap().unwrap();
```

Every command is written to caller-owned storage. Size, state and pending-
transaction checks happen before output or session state changes. There is one
outstanding command at a time and no queue, allocator, timer, sleep or retry.
Short output, malformed input, wrong epoch, stale label/signal, collision and
invalid transition leave the transaction and output unchanged. A valid Accept
or Reject consumes exactly the matching transaction. Reject results preserve
the peer's error byte and, where present, the failing service category or SEID.

The caller binds an epoch to the signaling connection and supplies that epoch on
every response and transport completion. Timeout requires the exact outstanding
label; timeout and disconnect both require a strictly newer epoch and clear all
transaction, endpoint, codec and stream state. Delayed responses from the old
connection therefore cannot complete a new transaction. There are no hidden
retries; policy may reconnect and begin again only as an explicit new epoch.

## Conservative SBC negotiation

Capability parsing walks complete category/length/value elements, rejects
truncation and duplicate categories, requires Media Transport, and recognizes
only an audio SBC Media Codec capability for this path. Unsupported codecs and
parameter sets remain explicit errors. It requires the peer to advertise:

- 48 kHz;
- stereo or joint stereo (joint stereo is preferred);
- 16 blocks, 8 subbands and loudness allocation; and
- an ordered bitpool range within the A2DP `2..=250` bounds.

Selection takes one caller-chosen bitpool inside the peer range and validates it
through `bt-core::SbcParameters`, including the A2DP bitrate ceiling. Set
Configuration advertises that one fixed value as both minimum and maximum; the
encoder cannot silently vary outside the negotiated value. The resulting codec
configuration is retained by the session for the caller to bind to the encoder
and RTP packetizer. Content protection, reporting, recovery, multiplexing,
header compression, reconfiguration, delay reporting and additional codecs are
not selected.

## Tests and remaining transport gates

The crate's host tests cover exact headers and command bytes, a scripted Sink
through streaming and bounded close, suspend/restart, abort, peer reject detail,
short/malformed packets, output backpressure, pending-command collision, stale
label/signal, timeout, disconnect, epoch reset, unsupported codec and invalid
state transitions. Reassembly tests additionally exercise two/three/255-packet
messages, short End, empty fragment payloads, declared-count and SDU-length
violations, wrong type/label/epoch/CID, reserved headers, capacity failure without
overwrite, exclusive deadlines, clock regression/overflow, explicit cancel/reset,
mixed transaction ownership and stale partials after label wrap. Integration
tests cover fragmented Discover/capabilities/rejects and configured/open/start
transitions through `receive_packet`, preserving the media-channel opening gate.

Development checks are `cargo test -p bt-avdtp`, `cargo clippy -p bt-avdtp
--all-targets -- -D warnings`, and `cargo check -p bt-avdtp --target
x86_64-unknown-none`. They do not establish PS5 behavior or headset compatibility.

This crate does not provide SDP discovery, L2CAP channel creation, media-channel
binding, authentication/pairing, pacing, ACL fragmentation, HCI credits or
controller ownership. Live activation remains gated on those layers and on the
repository's explicit PS5 transport-ownership evidence. A caller must not send
media merely because Open signaling was accepted; it must first establish the
media transport, call `transport_opened` for the same epoch, receive Start
acceptance, and still obey lower-layer backpressure and ownership rules.
