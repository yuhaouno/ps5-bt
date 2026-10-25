# Portable SDP discovery

Status: safe, `no_std`, allocation-free host-tested framing, data-element
parsing and one bounded A2DP Audio Sink discovery exchange in `bt-sdp`. The
crate has no dependencies or platform interfaces. It does not open an SDP
L2CAP channel, connect a headset, start AVDTP or establish hardware support.

## A2DP Sink discovery client

`A2dpSinkDiscovery` encodes ServiceSearchAttributeRequest (`0x06`) for Audio
Sink UUID `0x110B`. Its AttributeIDList contains only ServiceClassIDList
(`0x0001`), ProtocolDescriptorList (`0x0004`),
BluetoothProfileDescriptorList (`0x0009`) and optional SupportedFeatures
(`0x0311`). The caller selects a per-response `MaximumAttributeByteCount`, a
whole-response byte limit, node/depth limits and a response-chunk limit. The
fixed implementation ceiling is sixteen chunks. Request output is
caller-provided and remains unchanged if it is too small.

The caller supplies the accumulation buffer when constructing the client and a
separate scratch buffer when accepting a final response. Intermediate chunks
are appended only after transaction, epoch, byte, chunk, progress and
continuation checks succeed. The final chunk is combined and fully parsed in
scratch storage first; only a semantically valid result is committed to the
accumulation buffer or returned. This prevents malformed final data from
publishing a partial service record. The caller retains ownership of both
buffers; there is no allocation, recursion or per-response heap state.

Each outgoing request has an explicit caller-selected transaction ID; a
continuation request must differ from the request it follows. A
response must match the currently outstanding ID and connection epoch.
Continuation tokens remain opaque, are copied into a fixed sixteen-byte field,
and are returned byte-for-byte in the next explicitly requested PDU. Every
nonempty token is remembered for the bounded exchange, so immediate and longer
token cycles fail. A non-final zero-byte chunk, chunk/byte overflow or a
continuation on the last permitted chunk also fails. Receiving continuation
does not automatically send another request or extend the whole-exchange
deadline.

Completed AttributeLists must contain exactly one service record. The record
must contain each required attribute once, advertise Audio Sink, use an L2CAP
descriptor with PSM `0x0019`, include an AVDTP descriptor and version, and
include an A2DP (`0x110D`) profile descriptor and version. SupportedFeatures is
accepted only as a bounded unsigned 16-bit value. Attribute IDs, L2CAP PSM,
AVDTP/profile versions and SupportedFeatures must use the exact SDP UInt16 wire
encoding; numerically equal UInt8/UInt32 encodings are rejected. Duplicate, unexpected,
ambiguous multiple-record, wrong-service, wrong-PSM and wrong-profile data is
rejected. The returned `SinkService` contains the two advertised versions and
optional feature bits; it is discovery metadata, not negotiated AVDTP state or
proof that a remote device works.

Time is an opaque monotonically interpreted `u64` supplied by the caller.
`poll_timeout` must be called by the owner; there is no internal clock, retry or
background task. The same whole-exchange deadline is checked on every response.
Cancellation and disconnect are explicit, clear accumulated logical state, and
never send traffic. A disconnected epoch cannot restart until a strictly newer
epoch is supplied. Epochs are caller metadata absent from SDP packets, so the
transport dispatcher remains responsible for assigning them correctly and for
quarantining stale channel traffic.

## Packet boundary

`Pdu::parse` borrows exactly one five-byte-header-bearing SDP PDU. It validates
the big-endian parameter length and rejects truncation, concatenation and extra
bytes. Raw PDU IDs, including reserved IDs, remain available for diagnostics;
generic framing does not validate their parameters. `expect_transaction` checks
an explicitly supplied request ID. Matching an ID does not establish channel,
connection, epoch or native traffic ownership.

`ServiceSearchAttributeResponse::parse(bytes, expected_transaction,
maximum_attribute_bytes)` checks response ID `0x07`, the current transaction,
the response byte count against the supplied request maximum, and the complete
continuation field. The request maximum must be at least seven; response chunks
may contain zero or one byte because continuation can split a data element at
any octet. The discovery client separately rejects a zero-byte non-final chunk
as no progress. `ContinuationState::parse` accepts exactly one count-prefixed
field containing zero through sixteen opaque bytes; `bytes()` exposes only the
token and `is_complete()` reports an empty token. No token internals are
interpreted or trusted.

**A response's `attribute_lists` slice is a chunk, potentially ending anywhere
inside a data element. It is not independently validated as a complete tree.**
The generic framing parser does not assign it semantics. The discovery client
collects correlated chunks in bounded caller-owned storage and validates the
complete tree and requested A2DP attributes before returning a result. An empty
continuation state by itself still does not establish a valid record.

## Data elements and resource bounds

`DataElement::parse(bytes, Limits)` validates exactly one entire tree before
returning borrowed views or typed scalar values. Nil, unsigned/signed integers
through 128 bits, UUID16/32/128, text, Boolean, sequence, alternative and URL are
represented separately. Invalid/reserved type and size combinations fail.
Integers use network byte order and signed values are sign-extended. Text and
URL values remain uninterpreted bytes. Boolean decoding follows the SDP receiver
rule: every nonzero byte represents true, including values a sender should not
emit. UUID values retain their wire width; Bluetooth base-UUID normalization is
not performed.

The caller must supply all three parse bounds:

- `max_bytes`: complete encoded input size, including all headers/descendants.
- `max_elements`: number of nodes, including root and empty containers.
- `max_depth`: element depth, with root at one, between one and `MAX_DEPTH` (16).

Validation uses a fixed sixteen-entry slice stack, without recursion or heap
allocation. It visits each node once after an initial root framing check. Every
node and nested boundary must fit the same tree; a child cannot borrow bytes
past its container. Oversized input fails before traversal. Reaching a node or
depth bound returns a distinct error without exposing a partially validated
tree. Variable 8/16/32-bit lengths are accumulated with checked `usize`
arithmetic; payload slicing avoids adding a hostile length to a header size. On
the tested 64-bit hosts/target, every `u32` length is representable and oversized
declarations return `Length`, not a wrapping index. `LengthOverflow` preserves
failure if a length is unrepresentable on a narrower pointer width. No 32-bit
target run is claimed.

Sequence/alternative variants carry `Elements`, a fused iterator of immediate
children. It returns `Result<DataElement, ParseError>` and never recursively
revalidates children. Its private backing slice can only come from validated
parsing. Each pass is bounded by its container's encoded size; callers control
how often they repeat traversal. The initial limits do not meter arbitrary
application work after a successful parse. All input storage remains borrowed
and immutable. There are no hidden globals.

## Validation and remaining work

Synthetic parser tests cover all 256 type/size descriptor octets, truncation and
extra bytes for legal forms, Boolean values, signed extrema, UUID byte order,
nested boundaries, exact byte/node/depth limits, child iteration, maximum
8/16-bit values and hostile 32-bit lengths. PDU tests cover maximum-size frame
truncation, trailing bytes, declared lengths, unknown IDs, transaction mismatch
and continuation field lengths. One-byte response chunks are accepted by the
framing layer for later bounded concatenation.

Scripted discovery tests cover exact request bytes; success across arbitrary
one-byte and larger splits; new transaction IDs for continuation requests;
every full-response truncation; stale transaction and epoch; immediate and
multi-token continuation loops; zero-progress continuation; chunk, total-byte,
node and depth limits; short request/scratch buffers; duplicate attributes;
wrong service, PSM and profile; timeout, cancellation, disconnect and strictly
newer epoch reset. Rejected final responses do not append to accumulated bytes
or transition to Complete. These are synthetic peers, not interoperability or
live query results.

Run `cargo fmt -p bt-sdp --check`, `cargo check -p bt-sdp --all-targets`,
`cargo test -p bt-sdp`,
`cargo clippy -p bt-sdp --all-targets -- -D warnings`, and
`cargo check -p bt-sdp --target x86_64-unknown-none` using the repository's
pinned toolchain. These are correctness/compilation checks, not a fuzz campaign,
protocol qualification, interoperation test or PS5 execution.

Transaction allocation, clock scheduling, SDP ErrorResponse interpretation and
a multiplexer for more than one outstanding request remain owner policy or
future work. L2CAP channel negotiation/routing, native HCI ownership, pairing,
AVDTP, codecs and every hardware gate remain outside this client. The caller
must not send its encoded requests until the SDP channel, traffic ownership and
transport integrity are independently established. No automatic retry or
recovery policy is supplied, and no live SDP query is claimed.

## Specification provenance

Original implementation; no upstream source was copied or adapted. The parser
baseline was inspected on 2026-10-23 against the fixed
[Bluetooth Core 6.0, Vol 3, Part B, SDP specification](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-60/out/en/host/service-discovery-protocol--sdp--specification.html).
The baseline is sections 3.1–3.3 (data elements), 4.1–4.3 (byte order, framing,
transaction and continuation), and 4.7.1–4.7.2 (request and response fields).
A2DP service attribute semantics were inspected against the
[Bluetooth A2DP 1.4 specification](https://www.bluetooth.com/specifications/specs/advanced-audio-distribution-profile-1-4/),
sections 5.1–5.2. The specifications define the wire and record
format; no Bluetooth qualification, interoperability or conformance claim
follows from implementing these primitives.
