# Architecture

Status: accepted bootstrap decisions; hardware design remains experimental.
Implementation status at this design commit: the following contracts and builds
are planned for subsequent bootstrap commits.
Research snapshot: 2026-10-23. See [RESEARCH.md](RESEARCH.md) for evidence and
[UPSTREAM.md](UPSTREAM.md) for immutable source revisions.

## Product and boundary

Build an independent, Rust-first Bluetooth Audio stack with first-class PS5
support. The product goal is captured PS5 audio over the built-in controller to
ordinary Bluetooth Classic A2DP headphones while DualSense remains wireless.
No model, firmware, controller, headset, codec, or latency is verified here yet.

Homebrew capability and an ELF loader are prerequisites supplied by the user.
This project does not supply exploits, privilege escalation, kernel patches,
DRM/license bypasses, Sony binaries, firmware, keys, or game content. Existing
SDK/runtime behavior must be reviewed independently; using an SDK does not make
everything in its runtime acceptable. Stop at an insufficient-permissions error.

## Ownership

```text
C payload entry / existing loader integration
                    |
Rust runtime -> Rust audio engine (later)
                    |
Rust Bluetooth Audio: A2DP -> AVDTP/RTP -> SDP/L2CAP -> HCI
                    |
portable traits: BluetoothTransport, Clock, PcmSource
                    |
ps5-platform: safe ownership, checked buffers, error translation
                    |
ps5-sys: private-to-platform unsafe extern declarations
==================== C ABI ====================
platform/ps5: SDK startup integration, Avcap2 capture, USB/HCI,
             endpoint discovery, controller quirks, cleanup
                    |
             PS5 platform / hardware
```

Rust owns protocol state, framing, capability representation, connection and
reconnect policy, bounded audio queues, codec negotiation, RTP, measurements,
and eventually latency control. C owns platform adaptation. No upstream C
protocol structs or platform pointers enter portable code. Linux/FreeBSD backends
can implement the same traits without depending on any PS5 crate.

## Initial workspace

| Crate | Initial responsibility | Dependency direction |
| --- | --- | --- |
| `bt-core` | `no_std` errors, packet kinds, transport/clock/PCM contracts | no dependencies |
| `bt-hci` | `no_std` event framing and local-version diagnostic decoding | no platform dependency |
| `ps5-sys` | `no_std` declarations matching our own C header | no SDK struct bindings |
| `ps5-platform` | `no_std` safe C adapter wrapper, exclusive session ownership | `bt-core`, `ps5-sys` |
| `ps5-runtime` | diagnostic entry, C-callable Rust static library | platform wrapper and HCI |

All packages are unpublished, temporary identifiers, not reserved public names.
Keep L2CAP, SDP, AVDTP, A2DP, audio, and codecs as documented future modules; split
them when code and dependencies justify it. Do not create empty protocol crates.
The runtime uses the host standard library only for host linking/tests and is
`no_std` on the freestanding target. Portable crates remain `no_std` everywhere.
No async executor, allocator, codec, runtime dependency, GUI, or configuration
framework is needed for the bootstrap.

## ADR 001: C performs the final link

Use pinned stable Rust 1.90.0, `x86_64-unknown-none`, `staticlib`, panic abort,
PIC relocation, and the small code model. The prebuilt target avoids a nightly
custom target and avoids assuming PS5 is an ordinary FreeBSD installation.
Its C ABI supports integer/pointer calls with System V conventions; its default
soft-float behavior makes floating-point values by value inappropriate for the
boundary. PCM crosses later through buffers, never float-valued C arguments.
No `std`, OS calls, TLS, heap, unwinding, Rust dynamic library, or Rust linker
driver runs on PS5. No cross-language LTO; the SDK and rustc may use different
LLVM versions. Do not enable CPU-specific optimizations before measuring them.

```text
cargo build -p ps5-runtime --release --target x86_64-unknown-none
  -> libps5_runtime.a
SDK C compiler: own main.c + C adapter + archive
  -> SDK CRT/linker script/import stubs -> diagnostic ELF
  -> pre-existing user-supplied loader (outside project)
```

This is a build experiment, not an official Rust PS5 target. Validate archive
machine type, calling convention, imports, relocations, segment alignment, stack
alignment, panic termination, and return-to-loader on actual hardware. Use only
primitive/pointer FFI until the ABI is established. A custom target becomes an
option only when a concrete limitation is demonstrated.

**Current gate:** the SDK v0.45 CRT calls `__patch_init()` before `main`, which
changes credentials and syscall permissions. The repository provides a static
archive and C link recipe but does not build/deploy a stock-runtime PS5 ELF by
default. Resolve this with an audited existing runtime that fits the scope, or
an upstream-supported startup configuration. Do not implement a bypass or patch
chain here. See BUILDING.md (`BUILDING.md`; planned bootstrap artifact).

## ADR 002: explicit transport integrity and shared-controller ownership

USB endpoints are not independent Bluetooth controllers. More pending reads do
not constitute a multiplexer. Receiving a foreign command completion or ACL
packet is evidence of shared traffic, not permission to drop it. A file lock
only excludes cooperating payloads; it does not coordinate the system driver.

The backend must eventually prove ordered event delivery, preservation of native
traffic, command serialization, connection-handle ownership, and an enforceable
ACL budget. Unknown ownership prevents active HCI operations. The bootstrap
adapter consequently cannot open a live transport. Read-only descriptor
enumeration is the first platform experiment; even HCI read commands need
command/response routing and may consume native events.

Separate USB submission/completion from controller Number Of Completed Packets.
Do not invent HCI credits from elapsed time, count foreign handles as ours, or
reset the controller to recover a local failure. Shared capacity is not a
reservation. Packet loss, malformed transport framing, unknown session handles,
queue overflow, or unreconciled credits cause a visible terminal transport
error. Reconnect may follow an owned headset disconnection; it must not hide
transport integrity loss by blindly reopening the radio.

## ADR 003: small, versioned C contract

The normative bootstrap interface is ps5_bt.h (`../platform/ps5/include/ps5_bt.h`; planned bootstrap artifact)
and [ABI.md](ABI.md). These are project-defined functions, **not Sony/SDK APIs**.
Use an ABI version, fixed-width status codes, a nonzero opaque session token,
caller-owned byte buffers, explicit capacities and output lengths. No borrowed
pointer survives a call, no callbacks from background USB threads, no C struct
layout generated from an upstream header, and no Rust unwind crosses FFI.

One mutable session controls the transport, is neither `Send` nor `Sync`, and
supports explicit fallible shutdown. Drop only attempts cleanup. `send` success
means the complete frame was accepted and copied into bounded adapter storage;
it does not mean transmitted or credited. `receive` returns one complete event
or ACL packet, with packet type separate from data and without an H4 indicator.
It must distinguish timeout, disconnection, loss, and capacity errors. Oversized
frames are not silently truncated. Shutdown cancels and reaps transfers before
releasing their storage, then restores only proven, saved state it changed.

The C shim initially implements no hardware operations. It returns `Unsupported`
and clears output values; it must not fake a successful controller discovery.
Tests supply a separate, visibly synthetic adapter.

## ADR 004: portable contracts

`BluetoothTransport` sends complete command/ACL frames and receives into a
caller-owned buffer with a bounded wait. Command credits, protocol timers,
HCI ACL credits, and handle routing belong to Rust, subject to the platform's
proven ownership policy. `Clock` returns monotonic microseconds with no wall-clock
dependency. `PcmSource` declares its format and reads complete interleaved stereo
frames into a caller-owned float buffer; frame counts differ from sample counts.
Its timestamp and discontinuity semantics must be validated before capture is
implemented. Errors distinguish unavailable, busy, invalid, capacity, loss,
disconnected, I/O, timeout, and unknown backend codes.

These are synchronous contracts for the initial single-owner event loop, not a
promise that every platform API is nonblocking. Real adapter calls must honor
documented deadlines; the clock contract must be implemented before timed HCI
discovery is enabled. C audio open/read/close remain a **proposal** until capture
units, ownership, format, and statuses are tested. Do not advertise a frozen ABI.

## ADR 005: capability discovery precedes policy

Record USB identifiers and descriptors independently from console model. Unknown
vendors remain unknown and are not assigned a Marvell fallback. After transport
ownership is proven, collect Read Local Version Information, Supported Commands,
Supported Features, Buffer Size, and relevant extended features if supported.
Preserve raw numeric identifiers, status codes, validity, timestamps, and evidence
source. Unknown is not zero or false. Never infer capability from Fat/Slim/Pro
branding or Bluetooth version. Read results do not establish headset throughput.

## Audio and telemetry evolution

Later path: capture -> bounded PCM queue -> SBC -> RTP/AVDTP -> L2CAP -> HCI ACL.
Begin with conservative 48 kHz stereo SBC, first a generated tone, then capture.
No codec is implemented or selected as a dependency in this assignment.
Track capture discontinuities, queue occupancy/high-water marks, underruns,
overruns, encode time, submissions, USB completions, controller completions,
per-handle outstanding packets, pacing delay, connection epochs, and cleanup
errors. Structured internal records feed a simple log first. No web UI.

Measure complete capture-to-headphone latency using a repeatable external
measurement as well as per-stage software timings. Receiver buffering cannot be
deduced from HCI completion time. Game/Balanced/Hi-Fi and adaptation follow
reliable telemetry; bitrate changes must respect negotiated capabilities and
protocol reconfiguration rules. Do not adjust bitpool, buffers, or pacing to mask
lost events. Default reconnect is bounded, cancellable, epoch-aware, and logs
every attempt. Later pairing requires an explicit user decision and protected
key storage; keys/addresses never appear in public traces.

## First hardware experiment

See [HARDWARE-TEST.md](HARDWARE-TEST.md). First prove runtime start/return and
the C/Rust seam without opening Bluetooth. Next enumerate read-only USB metadata.
Only after routing/ownership is established, enable bounded local controller
queries. No reset, pairing, inquiry, event-mask write, scan write, ACL transmission,
audio capture, controller detach, credential mutation, or process takeover in
the first controller-information test. The major milestone is complete only
when a real PS5 logs actual controller information through the Rust/C boundary
and wireless DualSense still works during and after clean shutdown.
