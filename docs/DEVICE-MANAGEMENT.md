# Selected device and protected local bond management

Status: portable implementation candidate; **Requires PS5 hardware**. `bt-device`
is original safe `no_std` Rust with no allocator or external dependencies. It owns
one selected peer, one `bt-link::LinkSession`, one pending approval and one fresh
provisional bond. It provides no inquiry, device-name lookup, filesystem, network
listener, controller access, transport ownership or HCI allowance. No protected
PS5 storage implementation exists yet, and no live pairing result is claimed.

## Application boundary

`DeviceManager<S: BondStore>` owns both the selected link and store. The application
uses `select(peer)`, then `start(epoch, now_us, LinkConfig)`. `prepare_command(now_us,
output)` checks the finite link deadline before writing a full HCI command.
`command_len` and `confirm_accepted` retain the link's exact acceptance protocol;
the caller still submits it only with externally established command allowance.
`receive(epoch, now_us, packet)` feeds actual HCI events into that owned session,
consumes secret-bearing link notices internally, and returns non-key notices.
Callers cannot pass a fabricated Ready or bond notification to this manager.

`NeedKey` triggers a protected store lookup for the current peer. Confirmed absence
queues the real negative key reply, permitting the existing SSP flow. An I/O error
is not absence. Bad version/length/identity/security metadata is not an absent
bond either: both conditions produce explicit errors and initiate bounded link
cancellation. `status().cleanup_error` preserves a cancellation error separately.
There is no fallback to a hardcoded PIN, unvalidated key, automatic approval, or
replacement of unreadable storage.

`NeedConfirmation` returns a selection/epoch/request-bound opaque `ApprovalToken`
and numeric value. The protected local UI can also call `pending_confirmation()`
to retrieve the still-active prompt. `approve(token, now, bool)` drives the real
link reply. Old selection tokens, stale requests, revoked devices and later
connection epochs cannot approve another session. Pairing prompts and selected
identity are protected local management data, not public trace/status data.

`link()` returns read-only access for application session binding.
`ready_connection()` excludes revoked selection and is the live audio/channel
gate. The enclosing application must reevaluate it after **every** event, tick,
stop or error, including when `receive` returns a storage error after the link
became Ready. Successful pairing and successful persistence are separate outcomes;
a save failure does not change the already authenticated current radio link.
A later encryption revalidation can temporarily revoke link readiness.

`cancel(now)` discards pending approval/provisional keys and drives bounded link
cleanup. `cancel_at_last_observation()` is a conservative stop when acquiring a
new clock sample fails: it uses only the last observed monotonic timestamp; the
next real tick may immediately exhaust that cleanup budget. No elapsed time is
invented. A regressing timestamp immediately sets sticky `clock_failed`, revokes
Ready, drops approval/provisional authority and initiates cancellation at the last
observed time. Subsequent nonregressing calls may finish bounded cleanup but cannot
select/start a new session in that failed clock context. Cancellation errors remain
observable, including prior transport uncertainty; no credits are reconstructed.
`fail_transport` forwards raw uncertain transport failure and discards
provisional secrets; it cannot grant retries or reconstruct credits.

## Bond record and policy

The local v1 record is exactly 32 bytes, all in protected storage:

| Offset | Bytes | Meaning |
| --- | --- | --- |
| 0 | 4 | ASCII `P5BD` magic |
| 4 | 1 | Record version 1 |
| 5 | 1 | Required link-authenticated/encrypted evidence flags, exactly 3 |
| 6 | 1 | SSP key type: 4, 5, 7 or 8 only |
| 7 | 1 | Previously verified encryption key-size lower bound, 7..16 octets |
| 8 | 6 | Selected BD_ADDR in HCI wire order |
| 14 | 2 | Reserved zero |
| 16 | 16 | Local bond key; secret, never public |

Every field, exact record length and expected peer are checked before a key can
reach `LinkSession::supply_key`. Legacy/debug/changed/unknown key types are rejected.
`BondPolicy` adds a minimum verified key-size bound, authenticated SSP key type
when `require_mitm` is set, and P-256/SC key type when
`require_secure_connections` is set. These latter fields constrain stored/new
**key provenance**, not an independent measurement of current controller mode,
wireless identity, authenticated encryption implementation or interoperability.
They do not add an SSP mode absent from `bt-link`; stronger policies may explicitly
reject a headset's Just Works result. The current link must still authenticate,
encrypt and pass its own key-size query. Set the minimum to 16 unless deliberately
reviewing another interoperability policy.

The record stores the configured minimum actually verified by the link's Ready
gate, not an invented exact key size. For example, a gate checking at least 12
may store 12 even if the actual response was 16. A later policy requiring 16 will
reject that older metadata rather than reinterpret it as a 16-byte measurement.
`BondRecord::new` is a protected store/import boundary for declared prior evidence;
creating/parsing bytes cannot establish hardware evidence. The record format is
neither encrypted nor integrity-protected by itself and is not a LAN protocol.
Its store must protect confidentiality, integrity and access before returning it.

A real `ProvisionalBond` notice is tagged internally with selected peer, selection
generation and connection epoch. It stays private until **that same owned link**
reaches authenticated/encrypted Ready. Only then may it be saved. A cancellation,
failed/closed link, rejection, transport uncertainty or forget drops that pending
record. Save failure retains it only for an explicit `retry_save(now)` while the
same link remains Ready. Further Ready/revalidation notices do not silently retry
failed persistence. Once the link closes the provisional record is discarded.

## Protected store contract

`BondStore` has synchronous bounded `load`, `save_atomic` and `delete_atomic`
methods using fixed 32-byte caller storage. Implementations may return
`Unavailable`, a numeric raw backend error, or `DurabilityUnknown`. The operation
is retained in `StorageFailure`, and error/status objects contain no key bytes.

A production store must provide all of the following before it can be enabled:

- Own protected local namespace and reviewed permissions; reject unsafe paths,
  symlink substitution and wrong record ownership. Peer selection must never
  become an arbitrary user-provided file path.
- At most the documented fixed record length on load, with no retained borrowed
  buffer or key logging. `None` means confirmed absence after a successful lookup.
- Atomic whole-record replacement on save and an explicit durability boundary
  including required data and directory/metadata persistence. Returning success
  before that boundary violates the trait. Error may leave the old or new whole
  record; it never justifies claiming the new record was durably accepted.
- Idempotent, durable deletion. Confirmed absence is success. A successful unlink
  alone must not be equated with durable deletion without platform evidence.
- Finite I/O bounds and single serialized ownership, no per-audio-frame storage
  activity, and best-effort cleanup of transient secret buffers. API names alone
  do not prove these properties on PS5.

There is deliberately no guessed PS5 filename, permission API, fsync/rename
sequence, encryption-at-rest algorithm, kernel offset or filesystem shim here.
The platform store remains **Unavailable** until independently reviewed.

## Forget, reconnect and failure outcomes

`forget(now)` first revokes all lookup/save/approval authority and drops the
provisional key, then attempts link cancellation and durable deletion. Its
`ForgetOutcome` reports cleanup and persistence separately. A late accepted
key-size response, pairing event or Ready cannot resurrect the forgotten record.
If deletion fails, `forget_pending` stays set; selection changes, reconnect and
save are blocked until explicit `retry_forget()` succeeds. Retry does not restart
radio cleanup or extend its budget. If deletion succeeds but radio cleanup fails,
forget is durably recorded while the old link still requires reconciliation.
The application must report both outcomes rather than claim complete cleanup.

The in-memory revocation protects this manager's lifetime. A failed durable
deletion may leave the old bond after a process restart; that is an explicit
failed-forget outcome, not durable revocation. Retain/report the failure and do
not reconstruct the manager to bypass it. A future platform design may add durable
tombstones, but this version does not pretend to provide them.

After successful forget, a new explicit selection is required before pairing
again. Selection cannot replace a live or uncertain link; the old link must be
Closed with no command acknowledgement outstanding. Every new start uses a
strictly greater nonzero connection epoch. A normal reconnect keeps the selected
peer and performs a fresh protected lookup. It does not reuse a cached plaintext
key or automatically retry a failed connection. External transport quarantine,
recovery budgets and ownership remain outside this manager. Taking/dropping the
manager or its store does not perform native cleanup.

`DeviceStatus` reports only booleans, link state, persistence outcome and numeric
errors. `selected_peer()` deliberately exposes the minimal identity to protected
management callers; Peer Debug remains redacted. Manager, record and provisional
key types have no Debug/Clone implementation. Encoding and HCI command buffers
contain secrets and must never enter Git, diagnostics or HTTP status. Internal
safe clearing is best effort; compiler-resistant zeroization and erasing platform
copies are not claimed.

## Development checks and PS5 acceptance

Run `cargo test -p bt-device`, `cargo clippy -p bt-device --all-targets -- -D
warnings`, and `cargo check -p bt-device --target x86_64-unknown-none`. Tests use
only synthetic addresses/keys and a fake bounded store. They drive actual
`bt-link` wire/state transitions for stored reconnect, explicit SSP approval,
provisional save gating, save/delete failures, late completion after forget,
request/selection/epoch isolation, policy rejection and cancellation. Record tests
cover every length truncation, trailing bytes and all raw key-type values.

These checks are development evidence. PS5 acceptance still requires a reviewed
protected store, usable transport ownership/allowance, real headset pairing,
reboot reconnect/forget durability, error injection where safe, bounded cleanup,
wireless DualSense checks and current execution authorization. No PS5 environment
or storage operation was used for this implementation.

Original implementation; no upstream code copied or adapted. Bluetooth key-type
and connection/security provenance follows the exact Core 6.0 references in
[CLASSIC-LINK.md](CLASSIC-LINK.md), particularly Vol 4 Part E sections 7.7.23–24
and 7.7.40–45. The v1 local record and store contract are project-defined; no
Bluetooth qualification or PS5 filesystem compatibility follows from them.
