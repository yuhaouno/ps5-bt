# bt-l2cap

Allocation-free, safe `no_std` basic-header framing, received point-to-point ACL
recombination in caller storage, and one bounded initiator-side Classic
Basic-mode channel state machine. The channel machine handles Connection,
MTU-only Configuration and Disconnection signaling against scripted peers. It
does not provide a live transport, signaling multiplexer, responder allocator,
outbound ACL fragmentation, SDP/AVDTP, pairing or recovery loop.

See [HCI-L2CAP.md](../../docs/HCI-L2CAP.md) for the API contract, ownership/epoch
requirements, bounds, missing policy layers, synthetic test provenance and
Bluetooth Core 6.0 section references.
