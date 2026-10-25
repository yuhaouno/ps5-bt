# Bounded AVDTP initiator session

Status: implemented and host-tested as allocation-free signaling logic. It has
not opened a live L2CAP channel, contacted a headset, run on PS5 or produced
audible output.

## Scope and protocol baseline

`bt-avdtp` is safe `no_std` Rust for the A2DP Source's initiator path. It follows
AVDTP 1.3 sections 8.2--8.21 and 9 for signaling headers, transaction labels,
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

The session accepts one complete Single signaling packet per L2CAP SDU. The
header type can parse and encode Start, Continue and End packets, but the state
machine rejects them as `FragmentationUnsupported`: it has no caller-provided
reassembly object yet, and silently treating a fragment as a message would be
unsafe. This is a deliberate bounded subset, not a claim that peers never
fragment. A future reassembler must enforce packet count, equal Start/Continue
sizes, End size, ordering, label/message consistency, total capacity and an
explicit timeout before passing one reconstructed message to this session.

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
state transitions.

This crate does not provide SDP discovery, L2CAP channel creation, media-channel
binding, authentication/pairing, pacing, ACL fragmentation, HCI credits or
controller ownership. Live activation remains gated on those layers and on the
repository's explicit PS5 transport-ownership evidence. A caller must not send
media merely because Open signaling was accepted; it must first establish the
media transport, call `transport_opened` for the same epoch, receive Start
acceptance, and still obey lower-layer backpressure and ownership rules.
