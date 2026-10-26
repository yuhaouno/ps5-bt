# Bounded A2DP source session binding

`bt-session::SourceSession` is a safe `no_std`, allocation-free integration of
the existing SBC AVDTP initiator and retained media sender. It has no capture,
encoder, socket, USB reader, HCI dispatcher or PS5 backend. Development checks
do not demonstrate a headset connection, continuous audio or controller coexistence.

## Inputs and ownership

An externally established `Channel` carries the connection handle/epoch, local
and remote dynamic CIDs, and negotiated peer MTU. This is evidence supplied by
the single owner, not proof obtained from those numeric fields. An SDP
`SinkService` must have been discovered on the same authenticated connection.
The caller retains signaling transport, L2CAP establishment, authentication,
native traffic preservation, command/ACL credits and exactly-once cleanup.

`from_link` requires the portable `bt-link` manager's authenticated, encrypted,
minimum-key-size-checked `ready_connection()` and an exact matching channel
context. Before each source operation, the same owner must gate traffic whenever
`ready_connection()` no longer matches, including security revalidation that has
not produced a terminal failure, and forward later link failures. The constructor
does not retain a physical link ownership token. `new` supports
an externally managed authenticated link, such as a future native-channel
backend, with the same explicit preconditions. No such PS5 backend is established.

Configuration declares conservative SBC bitpool (2..53), dynamic RTP payload
type, nonzero SSRC, initial sequence/sample timestamp and nonzero microsecond
budgets. Settings are not measured/recommended PS5 tuning values. Native PCM
format/timestamp validation and sample-clock pacing remain separate work.

## Negotiation and media

1. Construction prepares Discover. `prepare_signal(now, output)` checks the
   exclusive deadline and copies one complete AVDTP SDU, retaining identical
   bytes/ticket until `confirm_signal_accepted(ticket, now)`. Call it with a fresh
   monotonic time immediately before submission; do not send an old saved offer.
   Whole-SDU Busy is retryable without advancing; any other transport-send
   error is terminal with acceptance effects requiring reconciliation.
2. Accepted Discover selects the first available audio Sink. Accepted Get
   Capabilities selects the existing 48 kHz stereo/joint-stereo, 16-block,
   eight-subband Loudness SBC subset at the fixed requested bitpool. Set
   Configuration and Open follow only their matching successful responses.
   There is no hidden endpoint fallback or infinite peer negotiation loop.
3. Accepted Open enters `AwaitMedia`, with its own finite deadline. Only an
   externally established separate media channel with the same connection epoch
   and distinct CIDs may bind. Invalid MTU, ACL data maximum or insufficient
   retained storage does not claim the channel is open or start playback.
4. Binding creates the existing RTP/L2CAP/ACL `MediaSession` from negotiated
   parameters, then emits Start. No SBC frames may queue before accepted Start.
5. `queue_sbc_frames` and `transmit_next` reuse the existing sender/owned credit
   ledger. They do not encode or pace samples. Busy and zero credits retain the
   PDU, while terminal sends preserve accepted-prefix cancellation evidence.

The caller supplies a signaling-channel `SignalingReassembler` to
`receive_signal_packet`. Partial responses return `false` and never advance the
source phase. Complete responses use the same initiator semantic validation as
the preserved Single-packet `receive_signal`. Do not mix receive paths within
an active fragmented message. Context/transaction/malformed errors are surfaced;
the owner must handle them, not silently discard control traffic. Reassembly
timeout or clock failure invalidates this application session.

`queue_sbc_frames_at(first_sample, ...)` carries the scheduler's full-width PCM
sample position relative to stream sample zero. It rejects overlap and overflow;
RTP wraps at u32 independently. Explicit gaps from expired wholly unsent audio
advance the timestamp without inventing packets or markers. Rejected frames,
capacity errors and Busy leave both sample positions and output unchanged. A
retained PDU cannot be replaced, even before any fragment has been accepted.

After accepted Close and `media_closed`, `restart(now)` negotiates again over
the existing signaling channel. It preserves submission/transaction generations,
requires a fresh local media CID and the same established ACL data limit, and
reuses caller storage only after confirmed real cleanup. A peer may reuse its
remote CID. Construct a fresh scheduler for sample zero; old controller
completions remain with the external credit owner, without refunds. Restart
cannot revive a failed or cleanup-pending session.

## Stop, terminal failure and recovery

Stop immediately prevents new media. An entirely unaccepted PDU may be cancelled
with counts. A healthy partially accepted PDU enters `Draining`, then Suspend
and Close only after its final fragment is accepted. An aggregate stop deadline
is retained across repeated Stop, drain, Suspend, Close and media cleanup;
per-signal deadlines can expire earlier. Accepted Close still requires
`media_closed` for the exact channel after actual native cleanup before Stopped.

Negotiation cancellation, timeout, clock regression or transport loss ends local
usability. `cancel()` remains available without a functioning clock. A terminal
failure can abandon a partial PDU only with exact accepted-prefix accounting.
This **does not** tear down a physical link, flush a controller, refund credits
or authorize sending a new PDU on the old context. The owner must perform real
teardown, cancel/reap native work, reconcile ownership/credits where necessary,
quarantine old traffic and establish a newer epoch before constructing a new
session. Expired waiting/cleanup outcomes are not successful cleanup.

`Snapshot` retains the first user-facing cause separately from a sticky typed
terminal cause. Later integrity/credit uncertainty escalates the latter, even
after an initial cancellation or when no PDU was queued. `recovery_failure()`
maps this into `bt_core::recovery`: uncertainty requires reconciliation, while
an externally confirmed disconnection may use the retryable cleanup path.
Polling/resetting local state cannot satisfy either safety requirement.
`take_cancellation` consumes retained evidence once; a returned transmit error
may also contain that same evidence, not a second cancellation to count.

## Checks and limits

Twenty-two development checks cover negotiation/channel binding, the Classic
Ready boundary, fragmented responses and assembly expiry, Busy/no-credit stops,
partial-PDU drain, stop deadlines, terminal prefixes, stale acceptance, expired
offers, clock failure and recovery classification. Payloads/identities/keys in
fixtures are synthetic; SBC framing fixtures are not encoding/audio evidence.
Checks include formatting, Clippy and the freestanding target. Application
status is not wired into `bt-control` or the PS5 diagnostic; their unavailable
operations remain unavailable.

Remaining work includes actual transport/dispatcher ownership, complete channel
and SDP scheduling, inquiry/bond persistence, capture and encoder integration,
sample pacing and the LAN owner loop. Real headset pairing, SBC tone, captured
audio, latency, recovery and wireless DualSense behavior all require PS5 tests.
