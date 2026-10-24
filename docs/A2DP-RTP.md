# Bounded SBC-over-RTP packetization

Status: implemented and tested on the host. This is packet construction, not an
AVDTP session, live L2CAP transport or PS5/headset playback result.

## Contract

`bt-a2dp` is safe, allocation-free `no_std` Rust. `SbcRtpPacketizer` receives an
already validated `SbcParameters` selection, the negotiated media-channel MTU,
a dynamic RTP payload type, SSRC, first sequence and first sample timestamp. The
SBC frame size and PCM frames per encoded frame come from the shared parameter
model; for the conservative joint-stereo 48 kHz/16-block/8-subband/bitpool-32
selection these values are 77 bytes and 128 sample frames.

One call accepts one through fifteen borrowed, complete SBC frames and writes a
single caller-owned packet:

```text
12-byte RTP v2 header | 1-byte A2DP SBC media header | complete SBC frames
```

RTP multibyte fields use network byte order. CSRC, extension and padding are not
supported. Payload type must be in the dynamic 96..127 range and SSRC must be
nonzero. The SBC media header keeps fragmentation/start/last/reserved bits zero
and stores the complete frame count in its low nibble. Each supplied frame must
have the negotiated length and `0x9c` sync byte. Full SBC CRC/syntax validation
remains the encoder/decoder boundary's responsibility.

The configured MTU includes both headers and every frame. Oversized packets are
rejected; this layer never fragments a frame or creates a partial L2CAP PDU.
Output capacity, every frame boundary and total size are preflighted before the
first write. Any error preserves output, sequence, timestamp and pending marker.
On success, sequence advances by one with RTP wraparound and timestamp advances
by `frame_count * pcm_frames_per_sbc_frame` with 32-bit wraparound. It never uses
bytes, channel samples, elapsed wall time or a guessed capture clock as the RTP
timestamp unit.

The marker bit is caller-controlled through `mark_next_packet`. It is consumed
only by a successful packet. The packetizer does not infer talkspurts or capture
discontinuities from PCM contents or time; the future AVDTP/media session must
apply its selected marker policy explicitly. Reconfiguration creates a new
packetizer instead of changing codec length or MTU mid-packet.

## Validation and limits

Six packetizer tests cover exact single-frame bytes, marker retention across
backpressure, three-frame packing, MTU rejection, zero/over-limit frame counts,
bad size/sync, dynamic payload/SSRC validation and sequence/timestamp wrap. A
host-only integration example now runs the actual caller-storage libsbc path:

```text
128 stereo PCM frames -> 77 SBC bytes -> 90 RTP/A2DP bytes
    -> 94 L2CAP bytes -> 98 HCI ACL bytes
```

The example parses the final ACL and L2CAP frames back and verifies that the
real encoded SBC bytes survived unchanged. The handle, CID, SSRC and timestamps
are synthetic. No packet is sent and no timing, RF, headset or audible result is
claimed.

Baseline references: [A2DP 1.4](https://www.bluetooth.com/specifications/specs/advanced-audio-distribution-profile-1-4/),
section 4.3 media packet format and its SBC media payload header; and
[RFC 3550](https://www.rfc-editor.org/rfc/rfc3550), section 5.1 RTP fixed header
and sequence/timestamp/SSRC fields. This original implementation copies no
upstream protocol code. Passing these synthetic tests is not Bluetooth
qualification or interoperability evidence.

Still missing: AVDTP endpoint discovery/negotiation and stream state, RTP/media
channel binding, pacing/deadlines, outbound L2CAP-to-ACL fragmentation, transport
backpressure scheduling, HCI credit accounting and live transport ownership.
Those layers must preserve this packet's all-or-nothing boundary; they may drop
expired PCM or a complete unsent packet before fragmentation, never a partially
sent PDU or a completion event.
