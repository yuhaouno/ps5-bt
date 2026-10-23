# First-assignment report

Completed locally on 2026-10-23. This report separates repository deliverables
from the major PS5 hardware milestone, which has not been reached.

## Discoveries

The active candidate toolchain is community Payload SDK v0.45, pinned to commit
`a7685ce8e97ce1f8a4f38253a41d54241c9124f1` and a verified release-archive hash.
HearBridge currently documents v0.43, so cross-version compatibility is unknown.
FGG-PlayPods and HearBridge sources were inspected at exact main-branch commits.
PlayPods main explicitly includes a resident-audio RFC with limited hardware
evidence; it is not equivalent to its release 1.0. Both references reveal shared
native HCI endpoint consumption. No amount of pending USB reads alone proves
safe coexistence. Their fixes and telemetry are useful research, not project
support evidence.

Capture uses Avcap2 with Ipmi/module resolution and private platform layouts.
HearBridge has Marvell/NXP and MediaTek profiles and descriptor-based endpoint
knowledge. SDK default startup and some capture fallback paths change credentials
or process syscall permissions. We have not adopted those behaviors. Details
and primary-source links: [RESEARCH.md](RESEARCH.md), [UPSTREAM.md](UPSTREAM.md).

## Decisions

- Five crates initially; no empty future protocol/codec crates. Rust owns
  protocol state and product policy; C keeps SDK/bootstrap integration, Avcap2
  loading/resolution/capture, USB ioctl/transfer lifetime, endpoint/controller
  selection, firmware quirks and cleanup. No platform implementation imported.
- Stable Rust 1.90.0, `no_std` freestanding x86-64 archive, PIC/small code model,
  panic abort; proposed final link through SDK C tools. No PS5 `std` ecosystem,
  nightly custom target, Rust loader, allocator or cross-language LTO required.
- Small versioned project-owned ABI: u32 session token, i32 status, byte buffers,
  explicit lengths, no upstream structs, no pointer retained after FFI return.
  Portable transport/clock/PCM traits; audio C ABI remains a measured-design task.
- Native traffic loss, unknown ownership and guessed ACL credits are blockers,
  not retry opportunities. Initial C adapter reports Unsupported.
- English canonical README plus eight short translated status guides. Public
  naming deferred until repository/package collision research is done.

## License and dependencies

Original bootstrap is GPL-3.0-or-later. SDK/HearBridge defaults are GPL-3.0-or-later;
PlayPods provides GPL v3 text without an established general later-version grant.
Its vendored libsbc is LGPL-2.1-or-later. Preserve file-specific grants, notices,
attribution, corresponding source and relinking obligations before reuse. The
older PS5SDK's per-file GPL v2 lineage remains a review issue if selected. No
codec licensing/patent clearance or qualification is claimed.

All Cargo dependencies are local workspace paths. The SDK is a candidate external
build/link dependency, not yet an approved runtime dependency. FGG-PlayPods,
HearBridge and libsbc are behavioral references only; there is no vendored or
adapted implementation, submodule or local upstream patch. The standard GPL
license text is included, not copied project implementation. See the dependency
lock and update procedure in [UPSTREAM.md](UPSTREAM.md).

## Repository tree

```text
README.md, ROADMAP.md, LICENSE, CHANGELOG.md
AGENTS.md, CONTRIBUTING.md, CODE_OF_CONDUCT.md, SECURITY.md
Cargo.toml, Cargo.lock, rust-toolchain.toml, Makefile, upstream.lock.json
.cargo/config.toml
.github/                     host CI and contribution templates
crates/
  bt-core/                   portable errors and transport/clock/PCM traits
  bt-hci/                    small, tested event/version diagnostic decoder
  ps5-sys/                   raw ABI declarations
  ps5-platform/              safe ownership and checked FFI buffers
  ps5-runtime/               C-callable Rust diagnostic staticlib
platform/ps5/
  include/ps5_bt.h            experimental contract, not Sony APIs
  adapter_stub.c              explicit Unsupported; no hardware access
  main.c, log.c, payload.mk   C entry/logging and gated link experiment
tests/
  fixtures/hci/               synthetic wire packet and provenance
  abi/                       host-only fake adapter and C contract checks
tools/                       actual C/Rust seam tests and archive inspection
docs/
  ARCHITECTURE.md, UPSTREAM.md, RESEARCH.md, BOOTSTRAP-REPORT.md
  ABI.md, BUILDING.md, PROTOCOL.md, TESTING.md
  HARDWARE-TEST.md, COMPATIBILITY.md
  i18n/                      zh-CN, zh-TW, ja, es, pt-BR, fr, de, ko
```

## First compilable deliverable and validation

`make check` passed locally: formatting, cargo check, Clippy warnings denied, four Rust HCI
tests including exhaustive framing-length combinations, direct C stub contract
checks, 15 C/Rust scenario runs, and the optimized freestanding archive build.
The archive inspector found 398 little-endian ELF64 x86-64 relocatable objects.
An additional symbol inspection found exactly the eight intended `ps5_bt_*` /
`ps5_platform_*` unresolved C functions and no other external symbols. Local
Markdown links were checked. Remote CI has been configured, not run here.

Local environment: aarch64 macOS, Rust 1.90.0, Apple Clang 21.0.0
(`clang-2100.3.34.2`), Python 3.12.15. Rust was installed for this validation in
`/tmp/ps5-bt-research-20261023/{rustup,cargo}` with no shell-profile changes.
To rerun in this same session without a normal rustup installation:

```sh
env RUSTUP_HOME=/tmp/ps5-bt-research-20261023/rustup \
  CARGO_HOME=/tmp/ps5-bt-research-20261023/cargo \
  PATH=/tmp/ps5-bt-research-20261023/cargo/bin:$PATH make check
```

That temporary directory is not a permanent project dependency; use the normal
setup in [BUILDING.md](BUILDING.md) on other machines or after temporary cleanup.

Archive: `target/x86_64-unknown-none/release/libps5_runtime.a`, 8,547,202 bytes,
SHA-256 `faebf2ec8218cbf9ff83f22453d79735ccaec817284783adb22f4789cc8beb92`.
This is a local build observation, not a byte-identical cross-host promise.
No SDK final link, PS5 ELF execution, sanitizer campaign, fuzz campaign, codec,
playback, latency measurement or console compatibility was validated.

## Next major milestone and blockers

M1 remains: **Rust participates in a PS5 ELF, communicates through a minimal
platform/C boundary, and obtains/logs real controller information while normal
wireless DualSense operation is preserved.**

1. Resolve/audit the existing startup/runtime arrangement against scope. Default
   SDK startup cannot be silently assumed acceptable; no replacement exploit or
   privilege mutation will be added. SDK LLVM/lld final link is untested here.
2. Run H0 start/return/seam, then H1 read-only descriptor discovery on the actual
   console. Built-in device nodes, VID/PID/revision and interface ownership are
   unknown here. Do not dispatch by console branding.
3. Establish an actual mechanism preserving native HCI commands/events/ACL before
   H2 read commands. Shared controller buffer capacity is not a private budget.
   If no scope-compatible routing mechanism exists, stop and document it.
4. Run bounded, credit-aware capability queries, record valid numeric information,
   errors and cleanup, and verify DualSense before/during/after and on reconnect.

Capture permissions/format/timestamps, Fat/Slim/Pro behavior, firmware coverage,
headset interoperability, throughput, latency, Wi-Fi coexistence, reconnect and
long-run stability remain Unknown. [HARDWARE-TEST.md](HARDWARE-TEST.md) defines
the first experiments; [ROADMAP.md](../ROADMAP.md) defines later evidence gates.
No final project name, remote repository, public release or release tag has been
created. The bootstrap is now recorded in atomic Conventional Commits on main.

## Git discipline follow-up

The repository already existed on main with no commits when the Git discipline
assignment began; it was preserved without reinitialization. Existing bootstrap
files were committed in dependency order as separately reviewable project,
research, architecture, roadmap, portable workspace, HCI, platform ABI, runtime,
build, CI, contribution-policy and translation changes, followed by this report.
README status was updated alongside implementation/build changes. No empty
commits, history rewriting, release tags or pushes were used.

Each staged snapshot was exported independently of the working tree to validate
links, whitespace, JSON, license text and intended source/fixture paths. No
compiled binary, SDK archive or firmware is committed; the only binary fixture
is the documented 14-byte synthetic HCI event. Rust-bearing snapshots passed
formatting, cargo check, unit/doc tests and Clippy before commit. The platform
snapshot also passed its host C stub contract; runtime snapshots passed all
15 C/Rust seam scenarios and the freestanding archive build/inspection. New C
sources passed C11 syntax checks with warnings denied. The build snapshot passed
`make check`, and its unreviewed PS5 link was confirmed blocked without generating
an ELF. CI/issue YAML and local Markdown links were checked; remote CI and PS5
execution remain untested.

The required workflow is in [AGENTS.md](../AGENTS.md) and
[CONTRIBUTING.md](../CONTRIBUTING.md#git-and-commit-discipline). Use
`git status` and `git log --oneline --decorate -n 20` for the current repository
state and history. Generated `build/` and `target/` outputs remain ignored.
Real-controller information, approved runtime startup and wireless DualSense
coexistence remain hardware/runtime gates; committing the bootstrap does not
complete M1.
