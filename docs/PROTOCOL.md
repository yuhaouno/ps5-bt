# Protocol implementation plan

The only initial protocol code frames complete HCI events and decodes a successful
Read Local Version Information Command Complete. It does not constitute an HCI
host stack. Its fixtures are synthetic, not console captures. Opcode `0x1001`
is OGF Information Parameters / OCF 1; wire command is `01 10 00` without H4.
An event begins with event code and parameter length. Command Complete adds
command credits, little-endian opcode and command-specific return parameters.
Preserve HCI command failures separately from parse failures.

Primary baseline: [Bluetooth Core 6.0](https://www.bluetooth.com/specifications/specs/core-specification-6-0/),
Vol 4 Part E, sections 5.4.4 (events), 7.4.1 (local version) and 7.7.14
(Command Complete). See [HCI specification HTML](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-60/out/en/host-controller-interface/host-controller-interface-functional-specification.html).
Version numbers returned by hardware are numeric observations, not audio quality
ratings. No conformance or qualification claim is made.

Next implement command-credit-aware dispatch and capability queries only after
the platform can preserve native traffic. Then bounded ACL reassembly by owned
handle/epoch; Classic L2CAP signaling/configuration and MTUs; SDP data elements,
continuations and A2DP sink discovery; AVDTP transaction labels, endpoint/media
codec capabilities and typed negotiation/open/start/suspend/close states; RTP
sequence/timestamp/frame boundaries; and A2DP Source with SBC. Specification
revision/section and tests accompany each addition. For AVDTP/RTP do not confuse
control signaling packets with the media transport channel.

All external lengths/identifiers are checked before indexing, allocating or
changing state. Bound recursion, reassembly size, channels, transactions and
timeouts. Unknown packet kinds are not guessed. Treat disconnect/reconnect as
new epochs; stale completions must not replenish new-session credits. Shared
capacity is not ours to reserve unilaterally. Do not copy an upstream protocol
implementation wholesale.

Pairing, authentication, encryption, explicit user confirmation, key protection
and reconnect identity are required before real headset support. Never log keys
or auto-accept arbitrary pairing requests. No codec is implemented at bootstrap.
