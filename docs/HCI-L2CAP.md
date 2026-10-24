# Portable HCI and L2CAP framing

Status: HCI framing plus bounded L2CAP basic framing, recombination and an
initiator-side Classic Basic-mode channel machine are implemented and tested on
the host. No hardware, controller ownership, live connection or audio support
is established by these components.

## HCI contract

`bt-hci` is safe, `no_std`, allocation-free Rust with no platform dependency.
`Command`, `AclPacket` and `Event` consume exactly one header-bearing HCI packet
without H4, matching `bt_core::BluetoothTransport`. Truncation, concatenation
and declared-length mismatches fail. Command/ACL encoders use caller buffers,
return the written length, leave unused output untouched and do not modify
output when capacity is insufficient. Payloads exceeding the wire length field
are rejected before serialization.

`Opcode` preserves all raw values, including unknown and zero event opcodes;
its field constructor checks widths, not command support. `ConnectionHandle`
accepts only `0x0000..=0x0eff`. Typed ACL PB/BC flags reject reserved/previously
used encodings and the forbidden non-flushable broadcast start. ACL framing
alone does not establish link type, role, direction, routing or ownership.
PB=0 is accepted for host-to-controller framing and is invalid for ordinary
controller-to-host ACL traffic. PB=2 is the received start marker; its name does
not imply that received LE traffic is automatically flushable. PB=3 is rejected
under the chosen Core 6.0 baseline, rather than treated as a complete PDU.

Existing `Event::parse`, `LocalVersion::parse` and `READ_LOCAL_VERSION` retain
their behavior. Parse failures remain `ParseError`; new handle/flag variants
distinguish framing failures. Construction/serialization uses existing
`bt_core::Error` categories. Generic `CommandComplete` and `CommandStatus`
retain raw opcode, status and controller command-credit observations. Fixed
capability decoders cover supported commands/features, extended features and
buffer sizes. Command failures remain `ControllerStatus(raw)` and undefined
failure data is not interpreted. Raw bitmaps and capacities (even unexpected
zeros) are retained; the policy layer must determine whether they are usable.
The caller must correlate returned feature pages with its outstanding request.

`CompletedPackets` validates the entire event before exposing an iterator of
handle/count pairs. It preserves every entry, including foreign handles, and
does not aggregate or replenish credits. A controller-wide capacity or reported
completion is not an owned transmission reservation. HCI does not carry a
connection epoch in this event: the future ordered dispatcher must resolve
ownership and handle reuse before accounting, and must not relabel queued
old-session events as current-session completions. No credit ledger, command
dispatcher or active transport is implemented here.

## L2CAP basic-header and recombination contract

`bt-l2cap` is safe, `no_std` and allocation-free, depending only on portable
`bt-core` and `bt-hci`. `BasicFrame` frames one PDU with a little-endian payload
length and destination CID, checks exact length, and rejects the null CID.
Other raw CID values are retained: the caller must select the applicable link
type and channel context before dispatch. A parsed basic header does not prove
Basic mode or validity of the enclosed signaling/data. The encoder checks
capacity before writing and preserves output on rejection. Wire payloads are at
most 65,535 bytes, excluding the four-byte basic header.

`Reassembler` recombines one received point-to-point PDU at a time in a mutable
caller buffer, including its basic header. Its state is constant-sized; memory
is bounded by the supplied slice. A decoded header that exceeds this capacity
fails before that fragment changes storage. This storage bound is not a
negotiated per-channel MTU. PB=2 starts and PB=1 continues; broadcasts, PB=0,
missing starts, length overshoots and a new start while a PDU is pending fail.
Headers split at any octet boundary, zero-length starts/continuations and zero
payload PDUs are supported. Completion returns a view borrowing the buffer;
Rust prevents another push/reset while that view remains in use.

Every push supplies an explicit `Connection { handle, epoch }`, which must
match both the instance and packet handle. Each interleaved connection needs
its own storage/instance and an ordered dispatcher. Reset abandons an old
context only with a strictly increasing epoch, including when switching handles;
epoch wrap is rejected. Epochs are caller metadata, absent from HCI packets.
The caller must attach context before reuse, drain/quarantine stale traffic,
and never label old packets with a new epoch. This API cannot detect a caller
that falsely labels old traffic as current. After epoch exhaustion the caller
needs a new, independently established transport/session context.

All rejected pushes/resets leave state and storage unchanged. Errors are
protocol diagnostics, not successful drops or replacements for transport
`bt_core::Error`. The future connection loop must surface integrity failures
and impose bounded receive deadlines; this primitive has no clock, timeout,
retry or recovery policy. In particular, it reports a start that interrupts an
unfinished PDU rather than silently replacing it after a possible flush/loss.
Reliable operation and explicit flush recovery remain future policy work.

## Classic signaling and channel contract

The same crate parses and encodes exactly one BR/EDR signaling command for
Connection, Configuration and Disconnection request/response codes. Identifiers
must be nonzero, dynamic CIDs start at `0x0040`, and PSM bit rules are checked.
Configuration supports the Basic-mode MTU option only: absent means the 672-byte
default, values below 48 are rejected, and continuation flags, duplicate MTUs
and all unimplemented options fail explicitly. This deliberately does not
pretend to support ERTM, streaming mode, QoS, flush timeout or extended windows.
Short output and malformed input do not partially write a command.

`ClassicChannel` owns one initiator-side channel for an explicit HCI handle and
connection epoch. It emits one Connection Request, correlates the exact response
identifier and source CID, emits one MTU Configuration Request, accepts the
peer's reciprocal configuration, then opens only after both directions succeed.
Its `peer_mtu` is therefore a parsed signaling result for that synthetic session,
not a controller or headset capability claim. It can initiate or answer one
Disconnection exchange and preserves the request CID order in the response.

The caller owns time and serialization. `timeout` applies only to the exact
pending identifier; there is no timer, retry or hidden reopen. Connection and
disconnection timeouts close local state. A configuration timeout retains the
known remote CID so the caller can explicitly cancel with a Disconnection
Request. Rejections are visible events. A newer connection can replace an old
context only through a strictly increasing epoch. Foreign handles/epochs, stale
identifiers, wrong CIDs, invalid transitions and constructed invalid commands
leave channel state unchanged. Output-capacity failures also preserve the
identifier counter, so retry emits the same transaction rather than creating a
phantom gap.

This is not a signaling multiplexer, responder-side channel allocator, outbound
ACL fragmenter, SDP client, AVDTP session or pairing implementation. The caller
must wrap encoded commands in CID `0x0001`, enforce signaling and data MTUs,
apply explicit deadlines, and never send them until HCI ownership and credits
are established. At most one locally initiated transaction is pending in this
minimal machine; a separate bounded dispatcher must split and order compound
signaling packets.

## Provenance and validation

Original specification implementation; no upstream code was copied or adapted.
All added packets are inline synthetic test data, without device identifiers,
keys, captures or compatibility claims. The existing synthetic local-version
fixture remains unchanged. Tests cover byte order, wire-size maxima, every
command parameter-length/available-length combination, every maximum ACL packet
truncation, bad flags/handles, capability truncations and controller failures,
and retention of foreign completion entries. No fuzz campaign or hardware test
is claimed. L2CAP tests cover every maximum-frame truncation, every declared
length against a fixed available length, all two/three-fragment boundaries of a
synthetic PDU (including split/empty headers), exact capacity, interleaved
handles, epoch changes, and unchanged storage/state on rejection. Six additional
scripted signaling tests cover exact connection/configuration/disconnection
bytes, reciprocal MTU configuration, peer and local disconnects, every
truncation of a configuration command, unsupported options/continuations,
short-output atomicity, rejection, timeout/cancel, stale identifiers/CIDs,
foreign epochs and constructed-invalid commands. These are synthetic peers,
not interoperability results.

Baseline inspected on 2026-10-23: [Bluetooth Core 6.0](https://www.bluetooth.com/specifications/specs/core-specification-6-0/).
[Vol 4, Part E, HCI functional specification](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-60/out/en/host-controller-interface/host-controller-interface-functional-specification.html):

- 5.4.1: command packet header and opcode layout.
- 5.4.2: ACL packet header, handle range and PB/BC restrictions.
- 5.4.4: event header and parameter length.
- 7.4.1–7.4.5: version, supported commands/features, extended features and buffer size.
- 7.7.14–7.7.15: Command Complete and Command Status.
- 7.7.19: Number Of Completed Packets.

[Vol 3, Part A, L2CAP specification](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-60/out/en/host/logical-link-control-and-adaptation-protocol-specification.html):

- 1.4: fragment/start and recombination terminology.
- 2.1: CID namespace and null destination prohibition.
- 3 and 3.1: basic header, length, byte order and Basic mode payload.
- 7.2.1–7.2.2: ordered fragments per logical link, interleaving across links,
  recombination and length integrity checks.
- Signaling packet formats and channel configuration state requirements for
  Connection, Configuration and Disconnection commands.

These references specify the framing baseline, not conformance or qualification.
Native command/event routing, enforceable ACL ownership/capacity, transport loss
behavior and PS5 coexistence all remain **Requires hardware** or **Unknown**.
