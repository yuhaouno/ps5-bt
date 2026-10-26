# Minimal LAN management v2

`bt-control` now provides a concrete allocation-free request/response service,
a bounded nonblocking connection pump, a typed management queue and the static
HTML/CSS/JavaScript page in `web/control/`. These are host-tested candidates.
There is no bound/listening PS5 socket, native socket adapter, PS5 owner loop or
live console result supplied by this crate. Native transport, capture, headset
playback and controller coexistence remain **Requires hardware**.

## Protocol and page

The existing `Control`, `/v1/status` and three `/v1/connect`, `/v1/start`,
`/v1/stop` commands retain their state/ticket contract. The old
`http::authorize_and_route` still recognizes only those four authenticated
routes. `service::handle` additionally serves v2. It refuses a v1 command while
v2 work is pending, and vice versa; an owner using both contracts must drain and
finish both ticket types. Direct API callers must enforce the same single-owner
serialization. The shared parser also rejects header-shaped bytes following the
first empty header line; these are a forbidden body, never additional headers.

Every request is exactly one HTTP/1.1 request with an exact configured Host,
maximum 1,024 bytes, maximum 24 headers and maximum 256 bytes per header line.
Bodies, queries, pipelining, transfer coding, folded headers, bare line endings,
duplicate security/management headers and unsupported Connection tokens are rejected.
`Content-Length: 0` and `Connection: close` are optional. V2 and page assets
also accept a browser's `Connection: keep-alive` preference; every response
still closes, and the pump never processes another request. The strict legacy
parser retains its original close-only preference contract. JavaScript cannot
set Connection itself because it is a [Fetch forbidden request header](https://fetch.spec.whatwg.org/#forbidden-request-header)
(primary specification inspected 2026-10-26). The service responds
with an exact Content-Length and always closes the connection.

| Route | Access | Result |
| --- | --- | --- |
| `GET /`, `/control.css`, `/control.js` | Public, exact Host | Static page assets |
| `GET /v2/public-status` | Public, exact Host | Coarse capabilities, owner phase, cleanup flag and identity-free status |
| `GET /v2/status` | Bearer-authenticated | Version/generation, actual status, selection, current approval, available actions and command ownership/results |
| `GET /v2/devices` | Bearer-authenticated | Same generation and bounded device list with opaque IDs, names and established/unknown metadata |
| `POST /v2/scan`, `/v2/cancel-scan` | Protected mutation | Scan lifecycle request |
| `POST /v2/select` | Protected mutation | Select exact opaque device ID |
| `POST /v2/pair`, `/v2/approve`, `/v2/reject` | Protected mutation | Pairing lifecycle or exact current approval decision |
| `POST /v2/connect`, `/v2/start`, `/v2/stop`, `/v2/disconnect` | Protected mutation | Connection/audio lifecycle request |
| `POST /v2/forget` | Protected mutation | Forget selection's protected bond; may retry an owner-reported failed deletion |

Protected mutations require all of:

- `Authorization: Bearer <token>` using a nonempty owner-provisioned secret.
- Exact configured `Origin` and `X-PS5-BT-Control: 1`.
- `X-PS5-BT-Generation: <session>:<revision>` matching the current snapshot.
- For Select only, `X-PS5-BT-Device: <opaque ID>`.
- For Approve/Reject only, `X-PS5-BT-Approval: <current approval token>`.

Numeric headers use canonical unsigned decimal without whitespace, signs,
leading zeroes or overflow. Device and approval headers are rejected on other
commands. IDs, tokens, command IDs and generations are JSON strings so browser
numbers cannot truncate full-width u64 values. A 202 response means **queued**,
not successful Bluetooth execution. Stale UI and busy requests return 409;
unavailable backend operations return 503. Errors contain fixed categories,
never request bytes, tokens, addresses or keys.

The page holds the control token only in a closure, clears its password input
after unlock, and drops its token reference on lock/page exit. It never stores
the token in a URL, cookie, localStorage or sessionStorage. No credentials or
external frontend dependencies are bundled. Protected status/device fetches
must have matching generations before their controls are enabled. Buttons use
actual `available` actions; request errors clear stale UI state. The selected
headset's numeric comparison is displayed with six digits when supplied by the
owner, and otherwise asks for explicit pairing confirmation.

The page uses textContent and DOM node construction for device names and state;
it never interpolates them into HTML. JSON rendering escapes quotes, backslashes,
control characters, `<`, `>`, `&` and line separators. Device names are bounded,
validated UTF-8. `bonded: null` and `audio_sink: null` mean Unknown; a discovery
class alone is not proof of A2DP Sink support. Unknown metadata does not prevent
selection. Explicit Pair is allowed only for a selected, established unbonded
device; Connect may use the owner's protected lookup and fresh SSP workflow.

All responses, including errors/assets, carry no-store, nosniff, no-referrer,
frame denial and a CSP permitting only same-origin external scripts/styles and
same-origin fetch. Inline scripts/handlers, frames, forms, external resources
and base-URL replacement are forbidden. No CORS permission is emitted. Exact
Host/Origin checks require the owner to configure the same origin used by the
browser. This HTTP boundary does not provide TLS or protect tokens from a
hostile network; deployment must choose the trusted local access environment
and provision a strong secret without logging it. `Policy` Debug redacts it.

## Typed single-owner management

`management::Manager<N>` owns a fixed device table, one regular command slot,
one urgent command slot and the latest terminal completion. It owns no
Bluetooth address/key or persistence backend. The outer owner supplies
`DeviceId` mappings, safe names and actual metadata; it must not reuse IDs for
another device while stale references can exist. Approval u64 tokens must map
to the exact live device-manager approval token, selected peer and session.
An expired decision must never authorize a replacement pairing request.

`Manager::new(session, Snapshot)` requires a nonzero externally supplied session
ID that is not reused across service restarts. Session, revision and ticket
counters are checked rather than wrapped. `observe(snapshot, devices)` accepts
updates while tickets are owned; an identical observation does not advance the
generation. A changed snapshot invalidates older browser requests without
invalidating an already claimed command ticket. Device-table capacity,
duplicate/zero IDs, missing selected devices and malformed approvals are
rejected atomically.

The `Snapshot` contains legacy capabilities/state plus explicit `OwnerPhase`
and `cleanup_required`. In particular, Disconnecting and RequiresCleanup cannot
be truthfully represented as legacy Disconnected. The owner supplies the actual
phase and capability mask; the manager adds bounded queue, selection, bond and
current-approval checks. Defaults expose no actions. The UI never decides that
an operation is implemented because a button exists.

The owner calls `take_pending()` to claim a ticket exactly once, stores it while
its asynchronous work runs, and calls `finish(ticket, outcome, actual_snapshot)`
only on the corresponding terminal result. Read-only pending/urgent getters
retain the ticket and claimed flag until completion. Unclaimed, foreign,
already completed or modified tickets cannot complete work. Finish does not
optimistically label pairing, connection, cleanup or streaming successful.
The owner must bound every operation and report its actual result, including
cleanup errors. `CommandOutcome::Failed(code)` retains that exact local error.

Urgent commands preserve the primary ticket rather than replacing it:

- CancelScan while Scan is owned.
- Stop while Start is owned.
- Disconnect while Connect, Pair or Start is owned.
- Approve/Reject for the exact approval while Pair or Connect is owned.

The owner must cancel an unstarted primary locally rather than execute it after
its cancellation. A claimed primary finishes only after its actual operation
and required cleanup terminate; an urgent request does not automatically finish
it. Each ticket has its own terminal completion responsibility. The urgent
slot is bounded to one command, and all actions still require actual owner
availability. A completed approval decision does not complete the connection
or imply authenticated pairing success.

## Bounded socket pump

`pump::ConnectionPump` takes one **already accepted**, exclusively owned
`SocketIo`, caller request/response buffers, monotonic start time and one finite
whole-connection timeout. `SocketIo` supplies nonblocking read/write/close;
there are no guessed listener APIs, socket constants or native SDK calls here.
A reviewed platform adapter must establish descriptor ownership, prompt return,
exact byte-count/error semantics and consume-once close independently.

Each `step` issues at most one read or one write. Partial request reads accumulate
within the 1,024-byte ceiling. Partial writes retain the exact unsent suffix;
WouldBlock consumes nothing. No timeout is extended by progress, backpressure
or repeated calls. Clock regression, deadline equality, zero write, EOF before a
request, dirty lengths, I/O errors or insufficient response capacity terminate
the connection. The first terminal cause and any close error are retained.
Close is attempted exactly once, including failed construction or Drop. Owners
must use explicit completion/cancel and inspect outcomes; Drop cannot report a
cleanup failure. Request/response storage is cleared at terminal cleanup.

`service::handle` requires at least 1,024 output bytes before admitting any
command, enough for its bounded acknowledgment. Status/device/assets rendering
uses a count pass before writing so short-buffer rejection is atomic. Provision
larger response storage for static assets and the chosen device-table bound;
16 KiB covers the current assets and a table of up to 16 maximal labels. A pump
that lacks the requested response capacity closes with an explicit error.
Limit concurrent pumps to the owner's bounded resource budget; the supplied
pump itself owns exactly one connection and no listener backlog.

A browser disconnect or failed response write does **not** undo an already
queued/claimed command: the owner still holds its ticket and must finish it.
The browser refreshes actual status before another action rather than blindly
retrying a timed-out mutation. No request can select arbitrary commands, paths,
files, memory or bond key material.

## Verification and remaining activation

Run:

```sh
cargo test -p bt-control
cargo clippy -p bt-control --all-targets -- -D warnings
cargo check -p bt-control --target x86_64-unknown-none
node --check web/control/control.js
node web/control/control.test.cjs
```

The Rust suite retains v1 checks and adds generation/ownership/cancellation,
unknown metadata, current SSP decisions, all typed routes, redacted public
status, escaped protected names, header/body smuggling, atomic output rejection,
cross-version serialization, fragmented reads, partial writes, backpressure,
fixed deadlines, error/count validation and consume-once cleanup. Three Node
scenarios use a mock DOM/fetch to verify disabled operations, safe label
insertion, full-width IDs/tokens, generation matching, unlock/lock and failures.
They are browser-controller development tests, not real browser or PS5 results.

Implementation and assets are original GPL-3.0-or-later code; no upstream web
server/frontend implementation is copied or adapted. The assets are embedded
from `web/control/` at compile time and must be included in corresponding-source
bundles. This crate does not alter the platform transport/capture ABI or the
existing diagnostic build. The next activation requires a source-reviewed
native socket adapter/listener and outer owner integration, a current authorized
PS5 hardware window, bounded stop/cleanup evidence and controller-coexistence
checks. HTTP host tests do not establish any of these.
