# Portable management and transport runner

`bt-runner` joins the actual discovery state machine, typed LAN management,
selected-device application, protected bond lookups and connection-scoped ACL
pumps. It is original safe, allocation-free `no_std` Rust. It does not open a
PS5 transport, grant ownership, discover controller credits, instantiate native
capture or activate playback. Default capabilities are all Unavailable.

## Caller-driven scheduling

The platform supplies one monotonic microsecond clock, fixed buffers, actual
capabilities, fresh nonzero service/scan/link epochs and the protected store in
`Application`. Each `Runner::step` performs bounded state progression and claims
at most one management request. It retains primary and urgent tickets until
their respective outcomes: accepting a pairing decision does not complete
Connect, and requesting cancellation does not prove Inquiry or connection
cleanup has completed. The v1 queue and v2 management queue are serialized.
`controls()` supplies the two queues to `bt_control::service` under the same
exclusive owner borrow.

Search emits genuine Inquiry commands through `CommandSink`; selection uses a
quiescent, unexpired discovery result and its observed page-scan repetition.
The runner maps private Bluetooth addresses to service-local opaque IDs. Fresh
scans retire old IDs. Store errors remain typed diagnostics and Unknown bond
metadata rather than being interpreted as missing records. Forget operates on
the selected mapping even after its discovery observation expires, and reports
the actual protected deletion outcome. Native paths and key material never
enter the HTTP contract.

SSP approval IDs map to the exact current `ApprovalToken`, including its peer,
epoch and challenge identity. Replaced or expired challenges cannot authorize
another attempt. Snapshot generations change only for actual v2 observations;
commands still carry exact current generations and available actions. Clock
regression is sticky: the runner immediately revokes commands/approval, publishes
failure at the last monotonic instant and completes both bounded ticket slots. The page
cannot make transport/capture/codec/playback Ready. Failure and uncertain
cleanup remain visible independently of the coarse legacy connection state.

`receive_hci` requires explicit Discovery/Application routing and the matching
epoch. HCI bytes themselves carry no epoch: the caller must establish true
ownership and quarantine old/system traffic, rather than relabeling it. The
runner does not consume an arbitrary controller input stream. Before a handle
exists, `pump_application_commands` offers one command per call and retains
channel/signaling offers for the connection-scoped pump.

## Shared ACL output and retained input

Create `OutputPump` only for the application's exact secured connection and
reviewed ACL data limit. Supply one externally established `AclCreditOwner`
shared by signaling and media. A whole channel/signaling PDU is copied into
bounded owner storage before its application offer is confirmed; the subsequent
ACL fragments use actual acceptance/completion accounting. Busy accepts zero
bytes. Any other uncertain transport failure is terminal and forbids replay.

The owner repeatedly services control and audio fairly. Once media has an
accepted prefix, control returns MediaDrain until that PDU completes. While a
control PDU is buffered, audio returns ControlPending. A healthy partial PDU is
never silently discarded or interleaved with another PDU on the same handle.
Control timeout is aggregate from handoff, not refreshed by each fragment.
Clock failure, connection/context mismatch, malformed credit completion or
deadline expiry revokes integrity, reports exact cancellation accounting and
fails the application. Elapsed time never restores credits.

`InputPump` reassembles one owned connection's ACL input and retains a complete
Basic frame if its application consumer rejects it. New input is Busy until
delivery succeeds or an explicit `confirm_forwarded` attests that another
owner copied it. The initial fragment's aggregate deadline includes retained
delivery time. A failed/expired inbox remains available for diagnosis; success
is never inferred from discarding it. Foreign handle/epoch input remains the
caller's routing responsibility. Actual queue loss must be reported as
transport failure, never hidden by this pump.

## Platform obligations and verification

These primitives are a finite-step loop body, not a native wait/poll scheduler.
The platform still owns event routing, real command allowance and ACL
reservation, bounded readiness waits, capture/encode staging, explicit pump
outcomes, native resource cleanup and durable test identity/outcomes. Store
calls are synchronous: never place filesystem mutation on the per-frame audio
path. Replacing a closed application requires proven cleanup, preserved recovery
policy and newer routing epochs. No automatic reopen/reconnect is implemented.

Run `cargo test -p bt-runner`, `cargo clippy -p bt-runner --all-targets -- -D
warnings`, and `cargo check -p bt-runner --target x86_64-unknown-none`.
Synthetic fixtures exercise discovery-to-connect bytes, capability gates,
ticket cancellation, stable snapshots, result expiry, protected deletion,
retained input, exact shared credits, partial-PDU ordering and aggregate failure
budgets. These checks establish development behavior only. Live PS5 event
routing, SSP/headset compatibility, storage, playback, timing and DualSense
coexistence all remain **Requires hardware**. The established diagnostic and its
Unsupported backend are unchanged.

## Concrete bounded loop and audio assembly

[APPLICATION-LOOP.md](APPLICATION-LOOP.md) extends this original step-body with
`MainLoop`, typed `ApplicationLoop`, owned `AudioPipeline` and `RebuildOwner`.
Actual HTTP, management, owned HCI/ACL/completions, capture, persistent encoding
and media submission have independent per-turn quanta. Native resource factories
and verified input routing remain external prerequisites. The versioned capture
adapter preserves unknown format/time, and attaching a Rust pump never grants
transport ownership or credits. Global shutdown can cancel discovery or an
in-flight HTTP request, prevents new capture/encode work, and drains a healthy
partially sent media PDU before protocol teardown; a terminal abort preserves
accepted-prefix and reconciliation evidence.
