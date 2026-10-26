# Bounded owner loop and capture/audio assembly

`bt-runner` now provides a finite `MainLoop`, a typed `ApplicationLoop` lane
adapter, a concrete `AudioPipeline`, and resource reconstruction tied to the
existing `bt_core::recovery::Recovery`. These are safe, allocation-free `no_std`
Rust implementations. Native PS5 factory installation, owned Bluetooth routing
and actual audio format evidence remain absent; no playback capability becomes
Ready because these implementations exist.

## Assembly and bounded scheduling

The enclosing platform constructs fixed storage, its protected store, one
persistent Recovery, the application, discovery/management runner, listener or
accepted HTTP client, owned input dispatcher and reviewed command/ACL adapters.
`MainLoop<LoopResources>` supplies the acquisition order Store, Transport,
Listener, Capture, Encoder. One startup step attempts one resource. A failed
acquisition must unwind its own partial native work and preserve its diagnostic;
the loop unwinds earlier successful acquisitions in reverse order. Cleanup
consumes each resource before calling its callback, including on failure, so
later polling and Drop cannot retry the same close. Native factories may defer
stream-specific activation until negotiation; reserving a resource slot is not
permission to start capture or infer negotiated parameters.

During Running, per-turn quotas (each 1..64) bound Network, Control, Input,
Capture, Encode and Send. Each callback performs one bounded quantum. A stop
latch is examined before and after every quantum, and the default work order
processes HTTP/control before capture or encoding. A lane without immediate
work returns false and gives the remaining turn to later lanes. Suggested wakes
are bounded by max_wait_us, the experiment deadline and the earliest supplied
absolute microsecond deadline. Quota exhaustion yields immediately. There are
no internal sleeps or timer-based credit refills. Each backend call still needs
a reviewed finite/native return contract; a quota cannot interrupt a blocking
native call.

`ApplicationLoop` binds these lanes to the existing implementations:

| Lane | Concrete work per quantum |
| --- | --- |
| Network | One `ConnectionPump` nonblocking HTTP read/write against the runner's actual queues |
| Control | One `Runner::step` and at most one discovery/application command, complete control-PDU handoff or control ACL fragment |
| Input | One retained-inbox delivery, or one owned dispatcher read plus HCI/ACL/completion routing |
| Capture | One read/normalization or one retained PCM block queue offer |
| Encode | One scheduler pull, at most one persistent real codec call |
| Send | One already encoded offer handoff or one media ACL fragment |

Before a secured link exists, the adapter drives discovery and command-only
connection/pairing. `attach_connection` binds exact connection-scoped InputPump,
OutputPump and the externally established shared credit owner. Wrong epoch or
handle owners are rejected. `attach_audio` accepts a fresh pipeline only for the
application's exact negotiated SBC parameters. A stopped pipeline may be
replaced with a fresh one for a later stream; this never silently reuses its old
sample origin or spent capture budget. These attachment points are explicit
factory work, not an ownership authority.

The `OwnedInput` dispatcher contract only accepts traffic already separated from
system/foreign traffic by a proven mechanism. A native receive error or queue
loss must remain an error. A retained packet is never overwritten: failed HCI or
completion routing retains its metadata/bytes, and a fully reassembled packet
moves ownership to InputPump's retained inbox before the raw ACL fragment can be
reused. The adapter scrubs consumed input and command scratch. Diagnostics must
not expose the private bytes, addresses or keys.

HTTP errors such as a partial request timing out close and disable that client,
record its error and leave audio running. Uncertain client cleanup is surfaced
to the enclosing owner. A subsequent accepted client needs a fresh HTTP pump;
this adapter does not itself perform a PS5 accept call or guess readiness APIs.

## Actual capture to scheduler to codec seam

`AudioPipeline` takes an opened CaptureOwner, explicit validated CaptureBridge,
AudioScheduler, FinishEncoder and one raw read buffer, then starts capture once.
Validation requires identical encoder/scheduler parameters, matching input and
SBC sample rates, the exact SBC PCM block length, and bounded raw buffer size.
There is no resampler: a declared 44.1 kHz source cannot enter a 48 kHz stream.
Native Avcap2's unknown format cannot construct a validated bridge.

A capture quantum preserves chunk remainders and pushes at most one full stereo
i16 block. Queue Busy retains the exact block and prevents another native read.
Empty is counted and yields without pretending PCM arrived. Overrun with unknown
sample loss records the event but requires a new stream epoch; the Encode lane
checks that condition in the same turn, stops resources and refuses new encoded
or concealed blocks. Ended and native Error are immediate terminal results.
Raw read storage is scrubbed after normalization.

Encode invokes the persistent BlockEncoder through AudioScheduler. A retained
unaccepted frame is never regenerated. Send admits only an unexpired existing
scheduler offer or an already owned media PDU, so it cannot bypass the separate
encode quota. OutputPump keeps the signaling/media ordering and real shared
credit ledger. Busy and NoCredits remain observable without restoring credits.

Stop closes capture and explicitly consumes FinishEncoder once, recording both
cleanup results in PipelineCleanup. Repeated Stop and Drop retain the same result,
including target codec finish errors. A parameter-only stopped encoder slot
rejects any accidental encode; it allows existing healthy Source-owned partial
media to drain through the application's generic pump after codec cleanup.
Per-stream PCM/codec storage is fixed; no per-frame allocation is introduced.

## Global stop and exit

`Runner::request_shutdown` latches command unavailability, revokes pairing
approval, cancels Inquiry when active and requests the application's ordered
disconnect. It does not discard a healthy accepted media prefix. Protocol
quiescence settles claimed and unstarted management tickets as Cancelled.
`abort_shutdown` remains usable without a valid clock, preserves failure and
requires external cleanup/reconciliation.

`ApplicationLoop::request_stop` also cancels the active partial HTTP request and
stops capture/encoding before further lane work. Its poll_stop performs at most
one owned receive, control output and media drain before checking protocol
quiescence. `MainLoop` applies one aggregate stop deadline, never refreshed by
repeated requests. Expiry or a regressing/overflowing clock causes one terminal
abort, then consume-once resource cleanup. Stop remains available during startup,
scanning, pairing, codec work, HTTP processing and partial sends. A protocol
Closed result is never reported as proof that external native transfers were
reaped or that uncertain HCI credits were reconciled.

LoopReport retains cause, forced-abort and stop errors, per-resource cleanup
results, acquired state, per-lane counts and continuity counters. Queue high-water,
Empty/overrun/known-gap counts, encode failures, send waits, rebuffer episodes and
reconnect reasons are redacted observability, not acoustic continuity measures.
Counter saturation is explicit. PipelineSnapshot retains native capture cleanup,
bridge unknown-gap accounting and audio scheduler counters. Durable serialization,
build identity and crash outcome belong to the experiment recorder.

## Rebuilding resources without resetting a fault budget

`RebuildOwner<RebuildFactory>` owns the actual old and newly opened resource.
It invokes the real consume-once close before acknowledging cleanup, spends the
exact offered Recovery attempt before reopening, advances the checked connection
epoch and retains every failed partial open for the next cleanup quantum.
`CaptureEncoderResources` is a concrete resource implementation which calls
CaptureOwner.close and FinishEncoder.finish and records both results. A factory
can construct fresh capture/codec resources for the new epoch with fixed buffers.
Unknown cleanup blocks automatic retry. IntegrityLost always awaits explicit
matching SafetyTicket + old-epoch reconciliation; passing time, clean close,
new handle allocation and resetting local credit counters are insufficient.

Keep the **same Recovery instance** when retiring/rebuilding Application and
resources. After RebuildOwner has accepted/opened an attempt, construct the new
application and call:

```text
Application::connect_with_accepted_attempt(peer, new_epoch, now_us, attempt_ticket)
```

That method checks the exact existing InFlight ticket instead of spending a
second attempt. Fresh routing and sample/connection epochs are separate from
resetting Recovery. The application continues the remaining original episode
budget through security/channel negotiation and audio. Once the application's
actual working-resource and accepted-media requirements plus external observation
are satisfied, use Application.confirm_recovered_working. For standalone resource
candidates without Application, RebuildOwner.confirm_working supplies the same
explicit success attestation. Use one success owner, never both. A successful
open alone never starts stable-success observation or resets attempts. A later
fault retains prior attempts/deadline until the documented stable interval passes.

Explicit exhaustion, Stop, epoch overflow and clock failure prevent reopening;
active native resources still receive one cleanup attempt. Factory operations
must be individually bounded and cancellable. Native reopening, credit safety,
format, headset behavior and improved continuity remain **Requires hardware**.

## Checks and remaining platform work

Run:

```sh
cargo test -p bt-runner --locked
cargo test -p bt-runner --features host-libsbc --locked
cargo clippy -p bt-runner --all-targets -- -D warnings
cargo check -p bt-runner --target x86_64-unknown-none --locked
```

Tests use declared PCM, fake lifecycle adapters, actual Rust state machines and
an optional real persistent libsbc encoder. They check actual HTTP Stop before
another capture/encode quantum, raw/inbox ownership, shared completion credits,
partial-media drain after explicit codec finish, queue Busy/chunk remainders,
unknown loss, terminal reads, rate/block mismatch, failed initialization,
consume-once cleanup errors, per-turn quotas and aggregate stop. Rebuild tests
exercise successful/partial failed factory opens, actual CaptureOwner/codec pair
cleanup, exact epochs, stale evidence, preserved attempt/time budgets and
exhaustion. Synthetic traffic and reference-encoder equality are offline checks.

An installed native PS5 LoopResources factory, listener acceptance/readiness,
source-backed waits, valid native PCM format and actual transport ownership are
still separate integration requirements. The baseline Unsupported payload and
existing capture/SBC diagnostics remain selectable. No PS5 hardware run,
Bluetooth playback, audio quality, latency or DualSense coexistence acceptance
is established here.
