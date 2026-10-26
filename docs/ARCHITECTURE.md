# Architecture

Status: accepted bootstrap decisions; development policy aligned 2026-10-26;
hardware design remains experimental.
Research snapshot: 2026-10-23. See [RESEARCH.md](RESEARCH.md) for evidence and
[UPSTREAM.md](UPSTREAM.md) for immutable source revisions.

## Product and boundary

Build an independent, Rust-first Bluetooth Audio stack with first-class PS5
support. The product goal is captured PS5 audio over the built-in controller to
ordinary Bluetooth Classic A2DP headphones while DualSense remains wireless.
One Slim/13.40 environment has executed diagnostics, not audio. Headset playback,
live codec behavior and latency remain unverified; see HW2-EVIDENCE.md.

Homebrew capability and an ELF loader are prerequisites supplied by the user.
Initial kernel access must be supplied by that environment. Reviewed changes
through its existing channel may adjust only this payload's necessary permissions,
as scoped in AGENTS.md. SDK/runtime behavior is audited independently. No new
exploit chain, DRM/license bypass, Sony binary, firmware, key or game content
is supplied.

## Ownership

An experimental native L2CAP channel path is also under investigation
([availability probe](NATIVE-L2CAP-PROBE.md)). If hardware proves it usable,
the native stack would retain HCI ownership/credits while Rust retains
SDP/AVDTP/RTP and audio policy. This would need a separate reviewed channel
boundary; the current raw-HCI ABI remains Unsupported. Socket allocation alone
does not establish this architecture or a controller route.

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

Rust is the preferred owner of protocol state, framing, capability representation,
connection and reconnect policy, bounded audio queues, codec negotiation, RTP,
measurements and eventually latency control. Reviewed, scoped C protocol/connection modules
may be used as transitional implementations under AGENTS.md, with explicit
state ownership, versioned ABI, provenance and retention/replacement rationale.
This permits selective reuse, not an unreviewed whole-application import or a
claim that upstream hardware acceptance transfers here. C owns platform
adaptation. No upstream C protocol structs or platform pointers enter portable
code. Linux/FreeBSD backends can implement the same traits without depending
on any PS5 crate; they are not a prerequisite for PS5 development or acceptance.

## Initial workspace

| Crate | Initial responsibility | Dependency direction |
| --- | --- | --- |
| `bt-runner` | `no_std` discovery/device/management bridge and noninterleaving connection ACL pumps | application, discovery, control and framing; no ownership discovery or native waits |
| `bt-discovery` | `no_std` bounded Classic Inquiry/Cancel and stable device observations | core/HCI/link types; no native transport or automatic pairing |
| `bt-store` | protected fixed-record filesystem owner through opaque versioned C ABI | device/link; optional native POSIX or separately linked external adapter |
| `bt-listener` | one nonblocking listener/client owner through opaque versioned C ABI | control socket trait; optional native POSIX or separately linked external adapter |
| `bt-app` | `no_std` single-owner selected-connection application driver, channel/source/audio handoff and finite stop/retry budgets | device/channel/source/audio and existing framing crates; no native loop/ownership proof or listener |
| `bt-audio` | `no_std` normalized PCM block pacing, persistent encoder boundary and ticketed complete-SBC handoff | `bt-core`, optional existing host `bt-codec`; no capture ABI or transport |
| `bt-channel` | `no_std` SDP/channel scheduling, acceptance-gated publication and bounded cleanup/reopen | L2CAP/SDP/core/HCI; no native transport/credits |
| `bt-device` | `no_std` selected-peer approval, protected bond-record/store lifecycle and forget revocation | link/core/HCI; no platform filesystem or inquiry |
| `bt-a2dp` | `no_std` bounded complete-frame SBC-over-RTP packetization | `bt-core`; no session or transport dependency |
| `bt-avdtp` | `no_std` bounded initiator signaling and conservative SBC capability/session state | `bt-core`; bounded receive reassembly; no transport owner or media binding |
| `bt-core` | `no_std` contracts, SBC parameters, PCM staging and nominal sample timeline | no dependencies |
| `bt-codec` | `no_std` caller-storage encoder owner; real generic libsbc behind host-only feature | `bt-core`; C static archive only with `host-libsbc` |
| `bt-control` | `no_std` authenticated v1/v2 management, static page service and bounded socket pump | no dependencies; no listener or transport owner |
| `bt-hci` | `no_std` typed command/event/ACL framing, capability decoding and externally owned ACL reservation accounting | `bt-core`; no platform dependency or ownership discovery |
| `bt-l2cap` | `no_std` basic framing, bounded handle/epoch ACL fragmentation/reassembly and initiator-side Classic Basic-mode signaling | `bt-core`, `bt-hci`; no live transport, credit ledger or responder allocator |
| `bt-link` | `no_std` selected-peer Classic connection/security lifecycle, exact HCI command acceptance, stored-key/SSP decisions and authenticated encrypted Ready gate | `bt-core`, `bt-hci`; no transport ownership, inquiry or persistent key store |
| `bt-media-session` | `no_std` SBC-frame to RTP/L2CAP/ACL send cursor plus one-attempt submission through an external credit owner | `bt-a2dp`, `bt-core`, `bt-hci`, `bt-l2cap`; no clock, encoder, reservation discovery or transport owner |
| `bt-sdp` | `no_std` borrowed PDU/data elements and bounded A2DP Sink discovery with atomic final validation | no dependencies; no L2CAP or live session |
| `bt-session` | `no_std` source negotiation, Classic Ready/context gate, media-channel binding and bounded partial-PDU-aware stop | `bt-link`, `bt-avdtp`, `bt-sdp`, `bt-media-session` and framing types; no transport owner, capture/encode or pacing loop |
| `ps5-sys` | `no_std` declarations matching our own C header | no SDK struct bindings |
| `ps5-platform` | `no_std` safe C adapter wrapper, exclusive session ownership | `bt-core`, `ps5-sys` |
| `ps5-runtime` | diagnostic entry, C-callable Rust static library | platform wrapper and HCI |

All packages are unpublished, temporary identifiers, not reserved public names.
The first bounded L2CAP channel-signaling machine, AVDTP initiator session,
SBC-over-RTP packetizer and RTP/L2CAP/ACL media send cursor are host-tested;
SDP discovery is also host-tested.
AVDTP receive signaling reassembly and a bounded selected-peer Classic link
security candidate are host-tested. `bt-session` now binds Classic Ready and
explicit channels to negotiation/media submission on the host. `bt-app` now
owns selected-device security and protocol channel/SDP scheduling, and drives the
normalized PCM scheduler into Source media submission. Protected store semantics
are implemented; a concrete PS5 store, native capture/target encoder
integration and live PS5 owner loop remain work. Portable inquiry, an optional
POSIX store/listener, static LAN page and management/ACL runner are implemented
separately; see APPLICATION-RUNNER.md. No connection backend is active.
See [the application contract](APPLICATION-OWNER.md). Split
them when code and dependencies justify it. Do not create empty protocol crates.
The runtime uses the host standard library only for host linking/tests and is
`no_std` on the freestanding target. Portable crates remain `no_std` everywhere.
No async executor, allocator, GUI framework, or configuration framework is needed
for the minimal bootstrap. This does not prohibit bounded engine initialization
or the thin LAN page. Its portable request/state contract is implemented without
a native PS5 listener; the static-page service and optional host POSIX listener
are independently selectable candidates, not activated runtime capabilities.
The optional offline codec is separate from the diagnostic runtime:
`host-libsbc` requires TARGET == HOST and fails before invoking a host C compiler
for a cross target. Its C-private state lives in caller-owned aligned storage,
queried through integer/pointer ABI calls and borrowed exclusively by Rust.
See SBC-IMPLEMENTATION.md for invariants, patches and target/distribution gates.

## ADR 001: C performs the final link

Use pinned stable Rust 1.90.0, `x86_64-unknown-none`, `staticlib`, panic abort,
PIC relocation, and the small code model. The prebuilt target avoids a nightly
custom target and avoids assuming PS5 is an ordinary FreeBSD installation.
Its C ABI supports integer/pointer calls with System V conventions; its default
soft-float behavior makes floating-point values by value inappropriate for the
boundary. PCM crosses later through buffers, never float-valued C arguments.
The minimal Rust diagnostic has no `std`, direct OS calls, TLS, heap, unwinding,
Rust dynamic library or Rust linker driver. These bootstrap properties do not
ban bounded fallible initialization allocation in a separately reviewed audio
module. Keep unwinding out of FFI. No cross-language LTO; the SDK and rustc may
use different LLVM versions. Keep generic/no-SIMD as the baseline; optimized
modules require reviewed ABI/state handling and measured hardware acceptance.

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

Production activation requires ordered event delivery, preservation of native
traffic, command serialization, connection-handle ownership, and an enforceable
ACL budget. The bootstrap adapter remains Unsupported. A separately selected,
source-backed diagnostic may investigate an unresolved ownership/routing fact
under the experimental admission rules in [AGENTS.md](../AGENTS.md); its intended
measurement need not already be proven. It must document the actual mechanism,
API/layout, permitted traffic, known prerequisites, unresolved risks, finite
budgets, stop/cleanup and wireless-controller checks before execution. Even HCI
read commands consume command credit and can consume native events. This
exception does not authorize competing readers, guessed credits, resets or
system-driver changes; an otherwise excluded mechanism needs its own scope
decision. No new active transport candidate is established by this policy.

Separate USB submission/completion from controller Number Of Completed Packets.
Do not invent HCI credits from elapsed time, count foreign handles as ours, or
reset the controller to recover a local failure. Shared capacity is not a
reservation. Lost protocol events, malformed transport framing, unknown session
handles, raw receive-queue loss or unreconciled credits cause a visible transport
failure and require explicit reconciliation or session teardown. This does not
make ordinary PCM starvation or expired unsent audio a terminal HCI error.
Bounded, observable concealment/rebuffer is a media-layer policy; controlled
reconnect is cancellable and preserves ownership/epoch rules. Preserve a
partially transmitted PDU while the connection is healthy. A typed terminal
transport failure or newer epoch may cancel its remainder with exact
accepted-prefix accounting and invalidation of the old session; this does not
refund uncertain outstanding credits. Never restore credit from time.
See AUDIO-RECOVERY.md.

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
caller-owned buffer with a bounded wait. In a raw-HCI backend, its reviewed owner
(preferably Rust) manages command credits, protocol timers, HCI ACL credits and handle routing against
the platform's proven ownership policy. A native channel backend may retain
HCI ownership and accounting in the system under a separate channel ABI.
`Clock` returns monotonic microseconds with no wall-clock
dependency. `PcmSource` declares its format and reads complete interleaved stereo
frames into a caller-owned float buffer; frame counts differ from sample counts.
Its timestamp and discontinuity semantics must be validated before production
capture activation or ABI freeze. A source-reviewed experimental capture
adapter may be implemented first, preserving unknown format/timing fields and
keeping native byte counts separate from verified PCM frames. Errors
distinguish unavailable, busy, invalid, capacity, loss,
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
claim audible playback. One host-only `bt-codec` example feeds a real generic SBC
encoder from this queue; a second wraps the frame in RTP/A2DP, L2CAP and one
synthetic HCI ACL packet. The independent target SBC self-test now links and
has executed the generic codec on PS5; this is not live audio integration.
`bt_core::media` now provides caller-owned stereo i16 staging, absolute nominal
sample deadlines and bounded, observable gap/expiry handling. It is not capture,
f32 conversion, resampling, an encoder, an HCI queue or a live scheduling loop.
See [implemented media contract](AUDIO-RECOVERY.md#implemented-offline-foundation).
Track capture discontinuities, queue occupancy/high-water marks, underruns,
overruns, encode time, submissions, USB completions, controller completions,
per-handle outstanding packets, pacing delay, connection epochs, and cleanup
errors. Structured internal records feed a simple log first. A thin LAN page
may expose the same actual state plus bounded connect/start/stop commands;
it is not a second transport owner or proof that playback exists. Rich UI and
additional codecs remain outside this MVP. See MVP-NEXT-RUN.md.

Measure complete capture-to-headphone latency using a repeatable external
measurement as well as per-stage software timings. Receiver buffering cannot be
deduced from HCI completion time. Game/Balanced/Hi-Fi and adaptation follow
reliable telemetry; bitrate changes must respect negotiated capabilities and
protocol reconfiguration rules. Do not adjust bitpool, buffers, or pacing to mask
lost events. Default reconnect is bounded, cancellable, epoch-aware, and logs
every attempt. Budgets apply to a fault episode, with explicit waiting/exhausted
states, stop/cancel and a documented stable-success reset condition; failed
attempts do not reset the budget. Recovery does not inherently require a payload
restart. Pairing requires an explicit user decision and protected local bond-key
storage; keys/addresses never appear in public traces. Device selection,
pairing approval, disconnect and forget-device are minimal LAN controls, not a
rich GUI. A versioned authenticated management surface may expose only the
device identity needed for these operations; bond keys remain private.

## First hardware experiment

See [HARDWARE-TEST.md](HARDWARE-TEST.md). First prove runtime start/return and
the C/Rust seam without opening Bluetooth. Next enumerate read-only USB metadata.
Only after routing/ownership is established, enable bounded local controller
queries. No reset, pairing, inquiry, event-mask write, scan write, ACL transmission,
audio capture, controller detach, credential mutation, or process takeover in
the first controller-information test. The major milestone is complete only
when a real PS5 logs actual controller information through the Rust/C boundary
and wireless DualSense still works during and after clean shutdown.
