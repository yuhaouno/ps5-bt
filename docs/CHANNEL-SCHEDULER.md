# Bounded channel and SDP scheduling

`bt-channel` composes the existing `bt-l2cap::ClassicChannel` initiator and
`bt-sdp::A2dpSinkDiscovery` into a selected-connection coordinator. It is original
safe Rust, `no_std`, with no allocator. This is an executable host-tested
candidate. Live PS5 transport, headset interoperability and controller
coexistence remain **Requires hardware**.

## Ownership and integration boundary

The constructor receives one externally established `Connection` (handle and
nonzero epoch), three distinct reserved local CIDs, negotiated-receive-MTU policy,
fixed budgets, caller-owned transmit storage and caller-owned SDP accumulation
storage. The owner must reserve those local CIDs across its entire connection;
this module cannot discover another owner's channels. There are exactly three
slots: SDP, AVDTP signaling and AVDTP media. No native channel backend, HCI event
reader, ACL credit ledger or Bluetooth authority is supplied.

`prepare_sdu(now, output)` exposes one complete L2CAP Basic PDU, including the
four-byte length/CID header. `PreparedSdu.len` includes that header;
`destination_cid` and `connection` describe its destination. Output is copied
only after capacity and deadline checks. Repeated preparation returns identical
bytes and ticket until acceptance, cancellation or failure. Short output leaves
the destination unchanged.

The external single owner must copy **all** bytes into bounded FIFO storage
before calling `confirm_accepted(ticket, now)` exactly once. `Busy` accepts
nothing and retains the same offer. Whole-PDU acceptance is an owner-storage
handoff, not controller completion or proven ACL credits. The owner is responsible
for ordered ACL fragmentation, retention of healthy partial PDUs, controller
flow control, event demultiplexing and exactly-once transport cleanup. An
uncertain native send must call `fail_transport` and reconcile or terminate the
owned connection. It cannot be retried as `Busy` or resolved with timed credits.

The optional `queue_data(role, now, payload)` serializes signaling/application
SDUs through the same single pending slot. Its later `SduKind::Application`
acceptance must also be correlated with the application's own command ticket;
the coordinator does not validate AVDTP transaction state. Media may instead use
an external bounded PDU scheduler once the binding is established, provided
ownership, FIFO ordering and pending partial-PDU obligations remain coordinated.

## Application sequence

1. Establish and authenticate the selected link externally, construct the
   coordinator, then call `begin_discovery(now)`.
2. Feed complete Basic PDUs through `receive(connection, now, bytes, scratch)`;
   drain/confirm prepared outbound PDUs. SDP opens automatically, and one
   ServiceSearchAttribute request is active at a time. Continuations retain a
   single discovery deadline and receive fresh transaction IDs.
3. `ServiceDiscovered` reports the validated Audio Sink service and queues SDP
   disconnection. Wait for `sdp_closed()` before `open_signaling(now)`.
4. `Notice::Opened { role: Signaling, binding }` provides handle/epoch, local and
   remote CID, local receive MTU and peer receive MTU. Bind that context to the
   AVDTP source. `Notice::Data` borrows only the input payload; the owner must
   deliver it before releasing/reusing its receive storage.
5. After the source has obtained AVDTP Open acceptance for this session, call
   `open_media(now)`. The coordinator enforces an open signaling channel; the
   application enforces the AVDTP prerequisite. Bind the resulting media channel
   before AVDTP Start and streaming.
6. Stop the source, finish any healthy partially sent media PDU and complete its
   Suspend/Close sequence before `close_media(now)`. Notify the source that media
   is closed only after `media_closed()` becomes true. Signaling may remain open
   for another start. `close_signaling(now)` requires media to be closed.
7. For connection teardown, call `cancel(now)`, continue receiving and draining
   cleanup traffic, and wait for `is_closed()`. Then the external owner can tear
   down its authenticated link. Terminal failure instead requires explicit outer
   reconciliation; the coordinator does not manufacture successful cleanup.

`binding(role)` is available only after both directions of configuration are
complete and the reciprocal Configuration Response has been accepted. It is
revoked during cancellation, role closure and terminal failure. The per-role
`*_closed()` getters distinguish a previously used, cleanly closed slot from a
slot that never opened. A peer Disconnect remains a cleanup obligation until its
response is accepted, even though the lower channel has already cleared its CID.

## Correlation, limits and failure

Responses require the selected handle/epoch, channel and exact outstanding
identifier, and cannot arrive before whole-PDU command acceptance. A response
to a locally queued but unaccepted request returns `ResponseBeforeAcceptance`.
SDP additionally correlates its transaction and bounded continuation state.
Bindings publish only after accepted reciprocal replies, hiding the lower
initiator's eager command-construction transition from the application.

Media may reopen after confirmed closure. Every reopen receives a fresh local
CID and a fresh disjoint block of local signaling identifiers; old local-CID
data cannot enter the replacement channel. Peer-assigned remote CIDs may be
reused after closure. Active channels cannot share a remote CID. Identifiers,
CIDs, SDP transactions and acceptance tickets do not wrap: exhaustion is an
explicit error requiring a new coordinated connection/session context.

The channel budget spans Connect and both configuration directions. Discovery
has one fixed budget across all chunks. Local closure/cancellation and peer
Disconnect replies use bounded cleanup budgets; peer traffic never extends an
existing earlier deadline. `tick(now)` and `prepare_sdu(now, ...)` fail at the
deadline, including equality. Repeating `cancel` does not reset its budget.
Normal cancellation drops only wholly unaccepted setup/application offers,
preserves required replies and accepted requests, obtains an unknown remote CID
from a pending accepted Connect, and closes media before signaling before SDP.

A matching acceptance callback is evidence of a completed handoff even when its
timestamp is late or regresses. It consumes the offer and records the outstanding
remote obligation before returning a terminal timing error. It cannot then be
replayed or cancelled as an unsent Connect. Other clock regressions reject the
operation without changing protocol state; the owner must stop, reconcile or
call the clock-independent `fail_transport` rather than invent a corrected clock.
`failure()` retains the first terminal cause. Timeout preserves potentially live
remote allocations; `needs_cleanup()` remains true when cleanup is unknown.
`fail_transport(Busy)` is rejected because Busy means no handoff occurred.

Storage, MTUs, chunk count/size, total attribute bytes, parser nodes/depth and
deadline arithmetic are bounded. Transmit storage must hold at least 47 bytes
(the largest supported SDP request plus Basic header); larger application SDUs
require suitable caller capacity. The configured local receive MTU is at least
48 and must accommodate the SDP chunk bound plus its worst-case response
overhead. Payload MTUs exclude the four-byte Basic header. Per-packet SDP parse
scratch is supplied to `receive`; insufficient scratch is an explicit error.
There is no per-frame allocation or hidden retry loop.

## Foreign traffic and candidate limitations

`ForeignConnection` and `ForeignChannel` leave the borrowed input available to
the outer dispatcher. `Busy` also requires retaining and retrying the same
received packet in order after draining the pending outbound SDU. No event
reader consumes native/system traffic on this module's behalf. The outer owner
must distinguish foreign traffic, unsupported packets and actual transport loss;
it must not silently discard control events or route another connection's data
into this coordinator.

The existing signaling parser accepts exactly one supported command in a Basic
PDU. Compound signaling PDUs, inbound Connection Requests, additional signaling
commands, advanced L2CAP modes and open-channel reconfiguration are unsupported
and return explicit errors. These are interoperability limitations for this
candidate, not claims about what every headset sends. The owner must retain the
input for dispatch/diagnosis and apply bounded failure policy for unsupported
traffic belonging to this session. AVDTP commands and media receive semantics
remain the source application's responsibility.

## Verification and provenance

Host tests cover the full SDP/signaling/media path, continuation correlation,
acceptance and Busy ordering, peer MTUs, stale and foreign contexts, short-output
atomicity, fresh media reopen with peer-CID reuse, active CID collisions, local
and accepted cancellation, premature peer closure, peer-reply cleanup deadlines,
fixed deadlines, uncertain handoff and late/regressing acceptance callbacks.
Run `cargo test -p bt-channel`, `cargo clippy -p bt-channel --all-targets -- -D warnings`
and `cargo check -p bt-channel --target x86_64-unknown-none`.

No upstream source is copied or adapted. This module composes the already
reviewed primitives described in [HCI and L2CAP](HCI-L2CAP.md) and [SDP](SDP.md),
without introducing new wire formats. Their fixed baseline is Bluetooth Core
6.0, Vol 3 Part A sections 2.1, 3 and 3.1 (CIDs and Basic framing), channel
Connection/Configuration/Disconnection signaling and state requirements; and
Vol 3 Part B sections 3.1–3.3, 4.1–4.3 and 4.7.1–4.7.2 (SDP data elements,
transactions, continuation and query fields). Audio Sink record semantics use
A2DP 1.4 sections 5.1–5.2. The linked primitive documents record primary sources
and their inspection date. These references establish implementation provenance,
not Bluetooth qualification or headset/PS5 verification.
