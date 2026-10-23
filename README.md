# PS5-BT — working title

A Rust-first Bluetooth Audio stack for PlayStation 5 homebrew, designed for low
latency, audio quality and portability. The goal is ordinary Bluetooth headphones
through the built-in radio while DualSense stays wireless.

**Status: Phase 0 repository bootstrap.** Project policy, research, architecture
and platform boundaries are documented. The portable Rust workspace and platform
contracts and a minimal HCI diagnostic decoder are implemented. The full
Bluetooth/audio stack remains planned.

| Area | Status |
| --- | --- |
| Project mission, scope and Git discipline | Implemented |
| Portable no_std transport, clock and PCM contracts | Implemented |
| HCI event framing and local-version diagnostic decoder | Implemented; synthetic tests only |
| Rust Bluetooth/audio stack and thin C platform adapter | Planned |
| PS5 ELF build, console models/firmware, controller families and headphones | Unknown / untested |
| SBC, other codecs, captured audio, latency and reconnect | Planned; no support claimed |
| Installable payload and pairing workflow | Unsupported |

Architecture: application -> portable Rust audio/protocol core -> platform
traits -> safe Rust PS5 wrappers -> narrow C ABI -> SDK/capture/USB/controller
adaptation -> hardware. Valuable PS5-specific C may remain for compatibility
and upstream synchronization. This is not a goal to rewrite all platform code.

A homebrew-capable console and an existing ELF loader are prerequisites. This
project does not implement jailbreaks, privilege escalation, DRM/license bypasses,
piracy, Sony binaries, firmware, keys or game content. Missing permissions must
be reported as blockers.

The [roadmap](ROADMAP.md) is research -> Rust/C controller diagnostics -> transport/capture ->
SBC test tone -> PS5 PCM -> stability -> latency/quality -> later codecs and
adaptation. No working audio or PS5 compatibility is claimed.

The [pinned Rust toolchain](rust-toolchain.toml) builds the portable `bt-core`
crate without registry dependencies. Run `cargo fmt --all -- --check`,
`cargo check --workspace --all-targets --locked`, `cargo test --workspace --locked`,
and `cargo clippy --workspace --all-targets --locked -- -D warnings`. These
contracts are not a live hardware implementation.

FGG-PlayPods, HearBridge PS5 and community Payload SDKs are research references;
this independent project is not their fork or Rust translation. No upstream
implementation is imported. Reuse requires license review, exact provenance and
attribution. Original contributions use [GPL-3.0-or-later](LICENSE); third-party
material keeps its own terms.

Contributions should be small, tested, documented and committed in reviewable
units; see [repository rules](AGENTS.md). English is canonical. Translated status
guides will be linked when introduced. The public name remains undecided pending
repository/package collision checks. No release tag is justified yet.

Research: [technical findings](docs/RESEARCH.md), [upstream revisions and
license inventory](docs/UPSTREAM.md). The SDK is a candidate build dependency;
upstream Bluetooth/audio implementations are behavioral references only.

Design: [architecture](docs/ARCHITECTURE.md), [proposed ABI](docs/ABI.md),
[hardware test gates](docs/HARDWARE-TEST.md), [compatibility evidence](docs/COMPATIBILITY.md),
and [security/scope policy](SECURITY.md). Shared HCI ownership and SDK startup
review must be resolved before live controller queries.

Changes are recorded in [CHANGELOG.md](CHANGELOG.md); milestones require
evidence rather than planned release dates.

The HCI decoder has synthetic packet fixtures, truncation/error tests and
exhaustive event-length checks. See the [protocol implementation plan](docs/PROTOCOL.md).
It is not a live HCI dispatcher or evidence of controller support.
