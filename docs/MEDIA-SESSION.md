# Bounded media send session

Status: implemented and host-tested as allocation-free `no_std` orchestration.
It has not sent an HCI packet, opened a live media channel or produced sound.

`bt-media-session` joins the already-encoded media boundary without taking
ownership of the encoder, clock or transport. One call validates complete SBC
frames and every caller-provided capacity before committing an RTP sequence or
sample timestamp. It then creates one complete L2CAP Basic PDU and copies that
PDU into the outbound fragmenter's caller-owned storage.

The caller repeatedly prepares one whole ACL frame and confirms advancement
only after its single-owner transport accepts that complete frame. A rejected
transport attempt can be retried byte-for-byte. Acceptance does not mean the
controller transmitted the frame or returned a credit; this crate neither
tracks nor synthesizes credits.

Before the first accepted fragment, an explicit stop may cancel the queued PDU
with accounting. Once any fragment is accepted, ordinary stop is rejected so a
partial PDU cannot disappear. Only a typed terminal transport failure or a
strictly newer connection epoch can terminate it, and both return accepted-byte
and fragment counts. A reconnect requires a fresh media session so RTP state is
not silently reused across epochs.

The host tests cover success over three ACL fragments, exact prepare retry,
stale handle/epoch rejection, cancellation boundaries, terminal failure,
epoch replacement and atomic MTU/output/storage failures. Live integration
still requires a negotiated AVDTP media CID and MTU, an externally proven HCI
ACL data maximum, explicit clock pacing, a single-owner transport and real
credit reconciliation.
