# Bounded Classic connection and SSP candidate

Status: implemented portable candidate, **Requires PS5 hardware**. `bt-link`
contains original safe `no_std` Rust, fixed-size state and one 25-byte command
slot. It depends only on `bt-core` and `bt-hci`, has no platform imports, allocator,
listener, persistent storage or transport implementation. This does not make the
PS5 adapter usable and is not headset support or an audio/coexistence result.

## Supported path

One explicit selected peer, an externally assigned nonzero connection epoch,
and caller-supplied controller-supported packet types enter `LinkSession::new`.
The session serializes Create Connection, Authentication Requested, Set
Connection Encryption and Read Encryption Key Size. `ready_connection()` exposes
`(ConnectionHandle, epoch)` only after all of the following:

- The exact commands entered owned transport storage and received their matching
  successful Command Status/Complete acknowledgements.
- The selected peer's Connection Complete supplied an ACL handle. Foreign peers,
  wrong handles, old epochs and wrong acknowledgement kinds/opcodes are rejected.
- A known local bond key was explicitly supplied, or an approved new SSP key
  notification with supported type 4, 5, 7 or 8 was received. Authentication
  Complete alone cannot authorize an unknown controller-cached key. Entering
  SSP invalidates prior stored-key authorization; that exchange requires its
  own supported new Link Key Notification after explicit approval.
- Authentication succeeded, encryption became enabled, and the explicit key-size
  query returned at least the configured minimum (7..16 octets). Configure 16
  unless a deliberate reviewed interoperability policy says otherwise. A v2
  Encryption Change is decoded but does not bypass the query.

New pairing offers DisplayYesNo, no OOB, General Bonding with MITM not required.
A protected local UI must actually display the six-digit value and permit
approval/rejection. Even headset Just Works requires an explicit caller approval;
there is no arbitrary-peer autoaccept. SSP authentication and encryption do not
prove MITM resistance, Secure Connections, or headphone identity beyond the
selected address and the user's pairing procedure. This is a conservative
headset-compatible association subset, not a complete Classic security manager.

`NeedKey` and `NeedConfirmation` carry epoch-bound request IDs. `supply_key` and
`approve` reject stale IDs. The bond supplier must select the protected local
record for this selected peer and validate supported key-type/provenance policy;
passing an arbitrary/debug key as a stored `SecretKey` violates that boundary.
A missing key produces Link Key Request Negative Reply to allow SSP. New keys
arrive as `ProvisionalBond`, to be retained privately and persisted only after
Ready. Failed/cancelled pairing must discard that provisional record. A future
bond store is responsible for secure persistence and forget-device; this crate
neither writes keys nor implements encryption at rest.

Legacy PIN, SSP passkey-entry and OOB requests receive their explicit negative
reply, then bounded Disconnect. Debug, changed-without-provenance, legacy
combination and reserved key types fail explicitly. Incoming connection request
acceptance, inquiry/discovery, remote names, passkey display/entry, OOB, link-key
change/refresh, controller configuration and automatic reconnect are absent.
These need separate reviewed implementations; do not present them as supported.

## Command and dispatcher boundary

`prepare_command` writes one complete HCI command, without H4, into caller
storage; `command_len` supplies the exact length. Short storage is unchanged.
Repeating preparation, including after `Busy`, returns identical bytes and the
same `CommandTicket`. `confirm_accepted(ticket)` is mandatory and is called once
only after the entire command entered the single owner's transport storage.
This is separate from the controller's subsequent Command Status/Complete.
No second command is available while that acknowledgement remains outstanding.
Command Complete peer/handle fields immediately following Status remain mandatory,
valid, and correlated even on failure (Core Vol 4 Part E section 4.5). Only the
trailing key-size byte may be absent/undefined for a failed key-size query; it is
never used as a measured size on failure. Unknown HCI Command (status 0x01) may
legally use either acknowledgement kind. This candidate accepts opcode-correlated
Command Status for normally-Complete commands and status-only Command Complete
for normally-Status commands, which have no normal Complete routing schema. Known
normally-Complete commands still require their specified peer/handle in Complete
error responses, including status 0x01. Wrong-kind success never consumes pending
state. An unsupported Create does not start a connection or justify a cancel.

Command credits in decoded events are raw observations; this crate never grants
itself a command or ACL allowance. The enclosing owner controls command credits,
event routing/masks, SSP enablement and transport integrity. Controller SSP and
normal non-debug operation must already be source-reviewed/configured; this
candidate does not change shared controller state implicitly.

The outer dispatcher must preserve foreign/system traffic, retain events rejected
with `CommandBusy`, and retry them in bounded order after the owned command ack.
A candidate pairing exchange may deliver an IO request while a key-reply ack is
outstanding; it must not be dropped. The caller must impose bounded queue limits
and report loss. Other unexpected ordering is explicit and is not treated as a
successful discard. HCI has no wire epoch: the caller must establish a fresh
epoch, quarantine/drain old traffic and never relabel stale events on reconnect.
Constructing another session with the same epoch can also duplicate ticket IDs
and is forbidden. This primitive does not prove a PS5 dispatcher exists.

Non-Busy transport-send errors have uncertain acceptance. `fail_transport`
records the raw error, invalidates Ready and prohibits replay or further local
commands, including guessed cleanup. External transport reconciliation/teardown
is required. The API does not claim that loss can be repaired by a timer.

## Deadlines, cancel and cleanup outcomes

The caller supplies monotonic microseconds to every state-changing operation and
calls `tick` while waiting. Clock regression is rejected. Connection, aggregate
security (authentication + approval + encryption + size query) and cleanup have
finite configured deadlines, checked at `now >= deadline`; events do not refresh
them. No loop sleeps, retries, resets the radio, or creates a new connection.
A later encryption-change event during Ready revokes readiness and starts one
bounded key-size verification. Encryption-off immediately invalidates readiness.

`cancel` has one cleanup budget. Repeated calls cannot extend it. Before Create
Connection is accepted, cancel is local. Once accepted, it waits for the command
ack, then issues Create Connection Cancel and waits for both its Command Complete
and the original Connection Complete cancellation outcome. If the original
failure outcome arrives before the cancel acknowledgement, cleanup remains in
`Stopping` under its original deadline until the acknowledgement is known.
`Notice::Closed` reports the link outcome; terminal command cleanup still requires
`State::Closed` and no pending command. Connection-success races are accounted for: an unsent cancel is replaced by Disconnect; an already
accepted cancel must finish before Disconnect. Status 0x0B on that cancel is an
observable `CancellationRace`, not lost controller status.

An established handle receives one Disconnect with reason 0x13; successful
Disconnection Complete releases it. Spontaneous disconnect revokes Ready; any
accepted command still outstanding keeps its correlation in `Stopping` until
acknowledged or the cleanup budget expires. A timeout/failure retains failure,
known handle and command metadata where applicable. It does not assert successful
controller cleanup. The caller must report the durable result and establish the
next bounded transport reconciliation experiment rather than silently reopen.
There is no destructor issuing hardware operations. Exactly-once native storage
release remains the enclosing transport owner's responsibility.

`SecretKey`, `Event`, `Notice`, pending commands and `LinkSession` have no Debug
implementation. `Peer` Debug is redacted. Key exposure is explicit for a protected
store/encoder. Command output and incoming key-bearing frames are secret-bearing;
never log them or pass them to LAN status/traces. Best-effort safe Rust clearing
of internal buffers is not guaranteed compiler-resistant zeroization and cannot
erase caller, controller or durable-storage copies.

## Checks and eventual PS5 validation

Run `cargo test -p bt-link`, `cargo clippy -p bt-link --all-targets -- -D warnings`
and `cargo check -p bt-link --target x86_64-unknown-none`. Synthetic tests cover
stored-key reconnect, explicit SSP approval/rejection, provisional keys, exact
wire bytes, acknowledgement correlation, foreign/stale traffic, security gates,
legacy negative replies, cancellation races, finite deadlines, partial command
uncertainty, spontaneous-disconnect correlation and malformed event bounds.
`tests/lifecycle.rs` contains the full selected-peer command/event sequences in
`connect`, `finish_security` and `ready` helpers. Values, handles and keys are
synthetic, not sanitized hardware captures.

PS5 validation still requires a reviewed usable transport with owned command
allowance/routing, event-mask/SSP prerequisites, current execution authorization,
finite operation/time budget and artifact identity. Check wireless controller
operation before/during/after, protected UI approval, real headset authentication,
encryption/key-size response, cancellation races, repeated start/stop and durable
outcomes. No hardware window is currently assumed and no command was sent.

## Specification provenance

Original implementation; no upstream source copied/adapted, no new dependency.
Primary baseline inspected 2026-10-26:
[Bluetooth Core Specification 6.0](https://www.bluetooth.com/specifications/specs/core-specification-6-0/),
[Vol 4 Part E, HCI functional specification](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-60/out/en/host-controller-interface/host-controller-interface-functional-specification.html):

- 4.5: failed completion routing fields, optional trailing results and vendor
  choice of Command Status/Complete for Unknown HCI Command.
- 5.4.1 and 5.4.4: complete command/event wire headers without H4.
- 7.1.5–7.1.7: Create Connection, Disconnect and Create Connection Cancel,
  including packet-type reserved bits, cancel/final-connection event ordering
  and the Connection Already Exists race.
- 7.1.10–7.1.11, 7.1.13: link-key reply/negative reply and PIN negative reply.
- 7.1.15–7.1.16: authentication and encryption commands/async completion.
- 7.1.29–7.1.31: IO capability and user confirmation reply/negative reply.
- 7.1.33 and 7.1.35: unsupported passkey and OOB negative replies.
- 7.5.7: Read Encryption Key Size (OGF 5, OCF 8).
- 7.7.3, 7.7.5–7.7.6, 7.7.8: connection, disconnection, authentication and
  v1/v2 encryption events; failed result fields are not interpreted as success.
- 7.7.14–7.7.15: Command Complete/Status, exact opcode correlation.
- 7.7.22–7.7.24 and 7.7.40–7.7.45: PIN/key requests, key types, SSP IO,
  numeric confirmation, unsupported pairing methods and pairing-complete status.

These references define protocol fields and selected ordering, not Bluetooth
qualification, controller ownership, headset compatibility or PS5 acceptance.
