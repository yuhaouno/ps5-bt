# Portable HCI and L2CAP framing

Status: HCI framing implemented and tested on the host. L2CAP framing is the
next isolated addition. No hardware, controller ownership, connection or audio
support is established by these parsers.

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

## Provenance and validation

Original specification implementation; no upstream code was copied or adapted.
All added packets are inline synthetic test data, without device identifiers,
keys, captures or compatibility claims. The existing synthetic local-version
fixture remains unchanged. Tests cover byte order, wire-size maxima, every
command parameter-length/available-length combination, every maximum ACL packet
truncation, bad flags/handles, capability truncations and controller failures,
and retention of foreign completion entries. No fuzz campaign or hardware test
is claimed.

Baseline inspected on 2026-10-23: [Bluetooth Core 6.0](https://www.bluetooth.com/specifications/specs/core-specification-6-0/).
[Vol 4, Part E, HCI functional specification](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-60/out/en/host-controller-interface/host-controller-interface-functional-specification.html):

- 5.4.1: command packet header and opcode layout.
- 5.4.2: ACL packet header, handle range and PB/BC restrictions.
- 5.4.4: event header and parameter length.
- 7.4.1–7.4.5: version, supported commands/features, extended features and buffer size.
- 7.7.14–7.7.15: Command Complete and Command Status.
- 7.7.19: Number Of Completed Packets.

These references specify the framing baseline, not conformance or qualification.
Native command/event routing, enforceable ACL ownership/capacity, transport loss
behavior and PS5 coexistence all remain **Requires hardware** or **Unknown**.
