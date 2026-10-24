# Bounded LAN control contract

Status: host-tested protocol/state foundation; no listener, rendered page or PS5
integration exists yet.

`bt-control` is a safe `no_std` boundary between a future small LAN listener and
the single Bluetooth owner. It does not open a socket, execute a command, read a
file, expose memory or store a credential. The caller supplies every request and
response buffer and owns the listener deadline, connection count and token
storage.

## Surface

Exactly four authenticated HTTP/1.1 routes are recognized:

| Method and path | Result |
| --- | --- |
| `GET /v1/status` | read the closed, identity-free status model |
| `POST /v1/connect` | enqueue one connect command |
| `POST /v1/start` | enqueue one start command for the caller's configured source |
| `POST /v1/stop` | enqueue one stop command |

Requests are limited to 1024 bytes, 24 headers and 256 bytes per header line.
They must use CRLF, an exact configured `Host`, a nonempty exact bearer token,
no body and no transfer coding. Duplicate security-relevant headers, query
strings, folded lines and pipelining are rejected. State-changing requests also
need an exact configured `Origin` and `X-PS5-BT-Control: 1`. These checks are a
minimum browser cross-site/unauthorized-use boundary; the future listener must
bind only to the intended interface, generate and protect a high-entropy token,
limit concurrent connections and deadlines, and return `Connection: close`.
Do not put the token in a URL, log, status body or committed configuration.

This is intentionally not a general web server. It has no arbitrary path,
filesystem mapping, upload, shell, memory inspection, CORS wildcard or dynamic
header passthrough. TLS is not implemented. Use only a trusted local network;
remote access needs a separately reviewed secure channel rather than exposing
this listener to the Internet.

## Single-owner command flow

`Control::submit` validates capability and lifecycle state, changes the visible
state to `connecting`, `starting` or `stopping`, and records one ticket. It never
performs Bluetooth work. While that ticket is pending, another command and a
backend status replacement both return `Busy`. The existing transport owner
observes the ticket and calls `finish` exactly once with the same ID and command.
A stale completion cannot clear or replace current work.

Failed commands preserve a numeric error and return to the last state that is
actually known: failed connect is disconnected, failed start is stopped, and a
failed or cancelled stop remains streaming. Higher layers must reconcile a
transport-integrity failure or tear the session down; this control model creates
no retry loop and grants no authority to reset the controller.

The status JSON contains only closed capability/state strings, a pending command
name, an optional signed status and a monotonic revision. It deliberately omits
Bluetooth addresses, pairing keys, bearer tokens, PCM, paths and logs. A short
output buffer leaves the buffer and state unchanged.

## Capability honesty

`Unsupported`, `Unavailable` and `Ready` remain distinct. In the current PS5
runtime the live transport and playback capabilities must remain `Unsupported`
or `Unavailable`; host tests cannot set them to a truthful hardware-ready state.
`Start` requires ready transport, codec and end-to-end playback plus a connected
link. Capture is reported independently because a generated tone may eventually
be a valid configured source before captured PCM is proven.

The next increment is a bounded socket adapter and static page that only reflect
this model. It must serialize all commands through the established owner and
retain the request/authentication limits above. This document does not claim a
reachable page, connection, pairing, audio stream or hardware verification.
