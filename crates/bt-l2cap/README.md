# bt-l2cap

Allocation-free, safe `no_std` basic-header framing, point-to-point ACL
fragmentation/recombination, and one bounded initiator-side Classic Basic-mode
channel state machine. The outbound cursor borrows one exact PDU and advances
only after whole-frame transport acceptance. The channel machine handles
Connection, MTU-only Configuration and Disconnection signaling against scripted
peers. It does not provide a live transport, credit ledger, pacing, signaling
multiplexer, responder allocator, SDP/AVDTP, pairing or recovery loop.

See [HCI-L2CAP.md](../../docs/HCI-L2CAP.md) for the API contract, ownership/epoch
requirements, bounds, missing policy layers, synthetic test provenance and
Bluetooth Core 6.0 section references.
