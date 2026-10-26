# Single-owner application driver

`bt-app::Application` is a safe, allocation-free `no_std` application driver.
It owns the selected-device manager, channel/SDP coordinator, source session
and signaling reassembler, and borrows a persistent fault-episode `Recovery`.
It has no PS5 entry, native reader, socket listener, capture ABI or transport
implementation. The diagnostic's Unsupported backend remains unchanged.

## Driving one owner

Construct `DeviceManager` with a protected `BondStore`, then `Application` with
explicit policies and caller storage. Nested source/reassembly configuration
is checked before the live flow; a later construction failure that consumes
storage is terminal, never a retryable Connected state with empty buffers.
Store requirements are in [DEVICE-MANAGEMENT.md](DEVICE-MANAGEMENT.md).

The enclosing platform loop must route known owned packets to this one driver,
preserve system/foreign traffic and supply fresh monotonic microseconds. Each
call performs bounded work; no method sleeps, starts a thread or retries I/O.
Wait/poll scheduling belongs to that loop. Busy or foreign/stale errors leave
the original input with its dispatcher: retain/retry or route it appropriately,
with a bounded queue whose overflow is an explicit integrity failure. Do not
silently drop control packets or claim this application owner coordinates with
the system Bluetooth owner by itself.

1. `connect(peer, epoch, now)` selects one known peer and drives Classic security.
   Search/inquiry is still absent. SSP notices and `pending_confirmation` expose
   bounded approval tokens to protected management only. Keys never enter status.
2. After authenticated/encrypted/key-size-checked Ready, `tick` creates the
   channel coordinator, opens SDP, discovers the Sink and closes SDP before
   opening AVDTP signaling. `Connected` waits for an explicit `start(now)`.
3. Start creates the source/reassembler, negotiates SBC, opens a separate media
   channel only after AVDTP Open, and waits for Start acceptance. It does not
   infer capture format or claim audible playback.
4. `negotiated_parameters` allows construction of a matching persistent encoder
   and normalized stereo-i16 `AudioScheduler`. `pump_audio` either prepares one
   complete block or submits one media ACL fragment, preserving Source-owned
   PDUs under Busy/zero credits. It never reads the transport or refills credits.
   A finite queue-to-last-fragment budget applies even under permanent Busy or
   zero credits; individual fragments/retries never extend it. Expiry terminates
   the attempt with exact accepted-prefix evidence and no credit refund.
5. Stop prevents new media, drains a healthy accepted prefix, sends Suspend and
   Close, then waits for actual L2CAP media cleanup before `Stopped`. Start can
   negotiate again over the same signaling channel using a fresh local media
   CID; create a fresh scheduler for sample zero. Peer remote-CID reuse is legal.
6. Disconnect closes media first, then signaling, then the Classic connection.
   Repeated requests retain one aggregate deadline. `Closed` means the owned
   protocol lifecycle is confirmed closed, not that an external native adapter
   has cancelled/reaped its own transfers or reconciled controller allowances.

Channel/SDP limits are in [CHANNEL-SCHEDULER.md](CHANNEL-SCHEDULER.md), source
rules in [SOURCE-SESSION.md](SOURCE-SESSION.md), and sample pacing/encoder limits
in [CONTINUOUS-AUDIO.md](CONTINUOUS-AUDIO.md). Media reopen consumes finite fresh
CID/transaction ranges; exhaustion requires clean teardown and a newer epoch.

## Submission and receive boundaries

`prepare(now, output)` offers at most one unit:

| Submission | Output framing | External requirements |
| --- | --- | --- |
| Command | complete HCI command, no H4 byte | command authority/allowance; may contain a protected local bond key |
| Channel | complete L2CAP Basic PDU | ordered complete-owner handoff, correct ACL fragmentation and established credits |
| Signal | complete AVDTP SDU with remote CID/context | Basic framing/fragmentation and ordered complete-owner handoff |

Copy to established owned transport storage before `confirm_accepted`. Busy
rejects the entire offer and retains byte identity. A partially accepted Basic
PDU in an outer fragmenter is not Busy for the whole offer: retain and finish
that outer PDU, or report a terminal uncertain transport outcome. Never retry
the same SDU as a new PDU after uncertain partial acceptance. Every other send
error invalidates the application and requires reconciliation. Output buffers
containing commands must be protected and scrubbed after copying.

`receive_hci` handles selected-peer link/security events. The outer dispatcher
must route controller completion events to its real credit owner and preserve
unrelated events. `receive_l2cap` receives one bounded, complete Basic PDU with
an externally established handle/epoch; ACL reassembly and ownership proof are
outside this driver. Channel notices feed source signaling/reassembly directly.
Peer loss of signaling/media binding gates new Source traffic immediately.
Unsupported signaling commands and foreign media input are surfaced, never
silently acknowledged. The channel candidate currently accepts exactly one
supported signaling command per Basic PDU; see its documented subset.

## Faults, cleanup and retry budgets

Security revalidation that temporarily leaves Ready gates the old source even
without a terminal link error. Clock regression, expired operations, uncertain
submission, unexpected channel closure and encoder failure stop application
usability. `cancel` remains clock-independent. Cancellation prefix evidence is
available once through `take_cancellation`; local cancellation never refunds
credits or proves actual teardown. Fault handling is conservative: the active
protocol attempt becomes terminal and requires external reconciliation.
Even a peer-initiated close can require the outer owner to complete/reap its
pending reply and tear down the attempt; Failed is not a cleanup success.

The external `Recovery` survives driver reconstruction. Connect cannot bypass
AwaitSafety, backoff, exhaustion or stopped policy; it accepts the exact offered
attempt ticket before execution. Tick polls active episode deadlines and stable
success. A recovered working resource must be externally observed, then explicitly
attested with `confirm_recovered_working`. Streaming and at least one complete
accepted media PDU are necessary conditions, not headset/audibility evidence.
Only this explicit attestation starts stable-success observation; an open call
or Start acknowledgement does not reset budgets. A later fault keeps the same
episode bounds. Resource reconstruction is implemented by the external `bt-runner::RebuildOwner`
candidate described in APPLICATION-LOOP.md; this protocol owner does not itself
open native resources.

After fault, the outer owner must finish actual cleanup and reconcile native
routing/credits before satisfying the exact recovery safety ticket, advancing
the epoch and reconstructing the attempt. `into_device` does not clean native
resources or authorize reusing a failed LinkSession. A failed manager must be
retired/recreated only after that externally confirmed teardown; keep the same
Recovery across this operation. User retry after exhaustion is explicit.

## Checks and pending platform work

Twenty-one application checks cover the full selected-peer stored-key handshake,
SDP/channel progression, explicit Start, SBC negotiation/media binding, PCM block
handoff, Busy/zero-credit behavior, healthy partial drain, clean Stop/Start,
disconnect order/deadlines, foreign/stale packets, security revocation, invalid
nested initialization, forget, clock failure and recovery acceptance/stability.
They use synthetic identities, protected-memory fixtures, clocks and SBC framing;
the separate audio feature checks exercise real host encoding.

The separate runner now binds listener/HTTP, management, discovery and declared
capture/continuous SBC candidates. A protected POSIX store, experimental Avcap2
owner and target SBC link exist, but native PS5 acquisition/routing and capture
format/timestamp evidence remain pending. PS5 pairing, test tone, captured audio, latency, long playback and
wireless DualSense coexistence require fresh hardware tests.

### Accepted resource-reopen attempt handoff

`connect_with_accepted_attempt(peer, epoch, now_us, ticket)` joins only the exact
`InFlight(ticket)` in the same caller-owned Recovery after resource reopening.
It requires an idle application and no attached attempt, rejects merely offered
or stale tickets, retains the episode deadline/attempt count, and assigns failure
to that existing attempt if device setup fails. It does not infer successful
playback, clear cleanup requirements or prove that an epoch is native-authorized.
The reconstruction factory supplies the newer epoch after actual cleanup.
`confirm_recovered_working` remains gated on Streaming, accepted media and an
explicit external working-playback observation.
