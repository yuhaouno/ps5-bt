# Architecture

Status: accepted bootstrap decisions; hardware design remains experimental.
Research snapshot: 2026-10-23. See [RESEARCH.md](RESEARCH.md) for evidence and
[UPSTREAM.md](UPSTREAM.md) for immutable source revisions.

## Product and boundary

Build an independent, Rust-first Bluetooth Audio stack with first-class PS5
support. The product goal is captured PS5 audio over the built-in controller to
ordinary Bluetooth Classic A2DP headphones while DualSense remains wireless.
No model, firmware, controller, headset, codec, or latency is verified here yet.

Homebrew capability and an ELF loader are prerequisites supplied by the user.
Initial kernel access must be supplied by that environment. Reviewed changes
through its existing channel may adjust only this payload's necessary permissions,
as scoped in AGENTS.md. SDK/runtime behavior is audited independently. No new
exploit chain, DRM/license bypass, Sony binary, firmware, key or game content
is supplied.

## Ownership

```text
Original minimal C/assembly entry / existing loader integration
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
platform/ps5: minimal startup; planned Avcap2 capture, USB/HCI,
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
| `bt-hci` | `no_std` typed command/event/ACL framing and capability decoding | `bt-core`; no platform dependency |
| `bt-l2cap` | `no_std` basic framing and bounded handle/epoch ACL reassembly | `bt-core`, `bt-hci`; no channel negotiation |
| `bt-sdp` | `no_std` borrowed PDU/continuation framing and bounded data elements | no dependencies; no client/session |
| `ps5-sys` | `no_std` declarations matching our own C header | no SDK struct bindings |
| `ps5-platform` | `no_std` safe C adapter wrapper, exclusive session ownership | `bt-core`, `ps5-sys` |
| `ps5-runtime` | diagnostic entry, C-callable Rust static library | platform wrapper and HCI |

All packages are unpublished, temporary identifiers, not reserved public names.
Keep L2CAP signaling, SDP discovery, AVDTP, A2DP, audio, and codecs as future work; split
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
LLVM 18.1.8 clang: original entry/runtime + main.c + C stub + SDK syscall.c
  -> ld.lld + original minimal.ld + Rust archive -> diagnostic ELF
  -> pre-existing user-supplied loader (outside project)
```

This is a build experiment, not an official Rust PS5 target. Validate archive
machine type, calling convention, imports, relocations, segment alignment, stack
alignment, panic termination, and return-to-loader on actual hardware. Use only
primitive/pointer FFI until the ABI is established. A custom target becomes an
option only when a concrete limitation is demonstrated.

The reviewed build now produces a real x86-64 ET_DYN diagnostic ELF through
`make` (or `make ps5-link`), using LLVM 18.1.8 and Rust 1.90.0. Original
`platform/ps5/bootstrap/start.S`, `runtime.c` and `minimal.ld` supply the entry,
BSS initialization, byte-memory routines, logging/abort and 16 KiB RX/R/RW
segments. Exactly three unmodified, hash-verified SDK v0.45 external files are
selected: `crt/syscall.c`, `crt/syscall.h` and `crt/payload.h`. There is no SDK
libc, import library, proprietary binary or local SDK patch in the link.

The stock SDK CRT and patch.c remain excluded. Runtime v6 uses the loader's
callable first field without reading execute-only code. An explicit HW-1
variant additionally selects unmodified kernel.c/nid.c to initialize the
already-supplied kernel-R/W transport and resolve current-process exports.
It never changes credentials or syscall bounds. Its pipe/socket transport
state writes, kernel metadata assumptions and firmware table are documented
in [RUNTIME-REVIEW.md](RUNTIME-REVIEW.md).

**Current evidence:** the explicit variant initialized successfully, entered
Rust and returned final 2 with a complete durable log on the recorded
Slim/13.40 environment. Ordinary symbol lookup had failed. The native query
ran but returned -1. Bluetooth, audio, native traffic ownership and wireless
DualSense coexistence remain unverified. See [HW1-REPAIR.md](HW1-REPAIR.md).

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
reservation. Lost protocol events, malformed transport framing, unknown session
handles, raw receive-queue loss or unreconciled credits cause a visible transport
failure and require explicit reconciliation or session teardown. This does not
make ordinary PCM starvation or expired unsent audio a terminal HCI error.
Bounded, observable concealment/rebuffer is a media-layer policy; controlled
reconnect is cancellable and preserves ownership/epoch rules. Never drop a
partially transmitted PDU or restore credit from time. See AUDIO-RECOVERY.md.

## ADR 003: small, versioned C contract

The normative bootstrap interface is [ps5_bt.h](../platform/ps5/include/ps5_bt.h)
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

Transport remains single-owner, but capture, encode and transport may be separate
execution stages connected by bounded queues. These are synchronous contracts,
not a restriction to one blocking loop or a
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
Offline buffering, sample-clock scheduling and recovery simulations may proceed
before a live transport is available; that does not authorize active HCI or
claim audible playback. No codec is implemented or linked yet.
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
