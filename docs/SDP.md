# Portable SDP parsing foundation

Status: safe, `no_std`, allocation-free host-tested framing in `bt-sdp`.
No session, service discovery, headset connection or hardware support is
established. The crate has no dependencies or platform interfaces. It is a
foundation for later A2DP sink record interpretation, not an A2DP implementation.

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
contain at least two attribute bytes. `ContinuationState::parse` accepts exactly
one count-prefixed field containing zero through sixteen opaque bytes;
`bytes()` exposes only the token and `is_complete()` reports an empty token.
No token internals are interpreted or trusted.

**A response's `attribute_lists` slice is a chunk, potentially ending anywhere
inside a data element. It is not independently validated as a complete tree.**
The future client must collect chunks in bounded caller-owned storage and only
then call `DataElement::parse`. This crate neither stores nor concatenates
responses. It does not check attribute ordering, attribute ID/value pairing,
service class membership, protocol descriptors, requested attributes or profile
semantics. Even an empty continuation state does not establish a valid record.

## Data elements and resource bounds

`DataElement::parse(bytes, Limits)` validates exactly one entire tree before
returning borrowed views or typed scalar values. Nil, unsigned/signed integers
through 128 bits, UUID16/32/128, text, Boolean, sequence, alternative and URL are
represented separately. Invalid/reserved type and size combinations fail.
Integers use network byte order and signed values are sign-extended. Text and
URL values remain uninterpreted bytes. Boolean decoding follows the SDP receiver
rule: every nonzero byte represents true, including values a sender should not
emit. UUID values retain their wire width; Bluetooth base-UUID normalization is
future interpretation work.

The caller must supply all three bounds:

- `max_bytes`: complete encoded input size, including all headers/descendants.
- `max_elements`: number of nodes, including root and empty containers.
- `max_depth`: element depth, with root at one, between one and `MAX_DEPTH` (16).

Validation uses a fixed sixteen-entry slice stack, without recursion or heap
allocation. It visits each node once after an initial root framing check. Every
node and nested boundary must fit the same tree; a child cannot borrow bytes
past its container. Oversized input fails before traversal. Reaching a node or
depth bound returns a distinct error without exposing a partially validated tree.
Variable 8/16/32-bit lengths are accumulated with checked `usize` arithmetic;
payload slicing avoids adding a hostile length to a header size. On the tested
64-bit hosts/target, every `u32` length is representable and oversized declarations
return `Length`, not a wrapping index. `LengthOverflow` preserves failure if a
length is unrepresentable on a narrower pointer width. No 32-bit target run is
claimed.

Sequence/alternative variants carry `Elements`, a fused iterator of immediate
children. It returns `Result<DataElement, ParseError>` and never recursively
revalidates children. Its private backing slice can only come from validated
parsing. Each pass is bounded by its container's encoded size; callers control
how often they repeat traversal. The initial limits do not meter arbitrary
application work after a successful parse. All input storage remains borrowed
and immutable. There are no output mutations or hidden global state.

## Validation and remaining work

Inline synthetic tests cover all 256 type/size descriptor octets, every
truncation and an extra byte for each legal type/size form, all Boolean values,
signed extrema at all integer widths, UUID byte order/width, child iteration,
malformed nested boundaries, exact byte/node/depth limits, and depth 16 versus
17. Length tests exercise maximum 8/16-bit strings with every truncation,
small 16/32-bit declarations, and large `0x80000000`/`0xffffffff` declarations.
PDU tests exercise every maximum-size frame truncation, trailing bytes, every
16-bit declared length against a fixed buffer, raw unknown IDs and transaction
mismatch. Continuation tests cross every count byte with available field lengths
zero through nineteen. Response tests cover type, transaction, request maximum,
short/oversized byte counts, missing/truncated/extra/oversized continuation, and
a valid chunk that is not a complete data element.

Run `cargo fmt -p bt-sdp --check`, `cargo check -p bt-sdp --all-targets`,
`cargo test -p bt-sdp`, `cargo clippy -p bt-sdp --all-targets -- -D warnings`,
and `cargo check -p bt-sdp --target x86_64-unknown-none` using the repository's
pinned toolchain. These are synthetic correctness/compilation checks, not a fuzz
campaign, protocol qualification, interoperation test or PS5 execution.

Request encoding, complete attribute-list interpretation, A2DP sink selection,
bounded continuation collection/round counts, duplicate/no-progress detection,
new transaction allocation, deadlines, SDP error responses and the single
outstanding request policy remain future work. L2CAP channel negotiation/routing,
native HCI ownership, pairing, AVDTP, codecs and all hardware gates remain
outside this parser. No automatic retry or recovery policy is supplied.

## Specification provenance

Original implementation; no upstream source was copied or adapted. Inspected
on 2026-10-23 against the fixed
[Bluetooth Core 6.0, Vol 3, Part B, SDP specification](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-60/out/en/host/service-discovery-protocol--sdp--specification.html).
The baseline is sections 3.1–3.3 (data elements), 4.1–4.3 (byte order, framing,
transaction and continuation), and 4.7.1–4.7.2 (request maximum and response
chunk fields). The specification defines the wire format; no Bluetooth
qualification or conformance claim follows from implementing these primitives.
