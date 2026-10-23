# PS5-BT — working title

A Rust-first Bluetooth Audio stack for PlayStation 5 homebrew — built for low
latency, high quality, and portability.

The goal is game and system audio on ordinary Bluetooth headphones through the
console's built-in Bluetooth hardware, while DualSense stays wireless. PS5 is
the first-class initial platform; the Bluetooth/audio core should also work
with future Linux and FreeBSD backends.

**Status: Phase 0 research and repository bootstrap. No playable audio or
verified PS5 support yet.** The skeleton contains portable contracts, a small HCI
diagnostic parser, a Rust/C seam, and an adapter that explicitly returns
`Unsupported`. It is an independent project, not a translation or fork of an
existing payload. The final public name is undecided; repository and package
collision checks must precede naming. All Cargo packages are `publish = false`.

## Compatibility and measurements

| Area | Status | Evidence |
| --- | --- | --- |
| Portable contracts, HCI command/ACL framing and capability event decoders | Implemented | host packet/malformed-input tests |
| L2CAP basic framing and bounded ACL reassembly | Implemented | synthetic handle/epoch/bounds tests; no channel negotiation |
| C ABI, safe wrappers and Rust runtime seam | Implemented | synthetic desktop boundary tests |
| USB descriptor parsing and metadata reports | Implemented | hostile host tests and sanitizers; no USB access |
| Freestanding Rust archive | Implemented | x86-64 ELF objects inspected; no PS5 execution |
| Live PS5 transport and capture | Unsupported | adapter explicitly returns Unsupported |
| PS5 Fat / Slim / Pro | Unknown | no project hardware runs |
| Console firmware | Unknown | no version verified |
| Marvell/NXP and MediaTek controllers | Experimental research targets | upstream source inspected; no live backend |
| Other controller families | Unknown | detection must fail explicitly |
| DualSense wireless coexistence | Unknown; required release gate | shared HCI ownership unresolved |
| SBC / high-quality SBC / SBC-XQ | Planned | no encoder or streaming implementation |
| AAC / additional codecs | Research only | technical and licensing review required |
| Headphones | Unknown | no compatible or incompatible models verified here |
| Latency, bitrate, CPU use, multi-hour stability | Unknown | no measurements yet |

Evidence labels are **Verified**, **Reported**, **Experimental**, **Unsupported**,
and **Unknown**. Upstream results do not confer our Verified status. See
[compatibility policy](docs/COMPATIBILITY.md).

## Development

Install Rust using the pinned [toolchain](rust-toolchain.toml), a C11 host compiler,
Python 3.11+ and Make. Then:

```sh
make check          # formatting, Rust check/tests/lints, C/Rust seam tests, cross archive
make host-smoke     # C main -> Rust -> unsupported C adapter, on the desktop
make rust-archive   # no_std x86-64 ELF archive; no SDK or console needed
```

See [build instructions](docs/BUILDING.md), [testing](docs/TESTING.md), the
[research report](docs/RESEARCH.md), and the [bootstrap report](docs/BOOTSTRAP-REPORT.md).
No third-party Rust packages are required.
Host checks and target compilation do not establish PS5 compatibility.
The first hardware bring-up target is PS5 Slim on firmware 13.40; support is
unverified. Controller vendor, revision and capabilities will be detected rather
than inferred from that model. See the [live MVP task board](docs/MVP-STATE.md)
and [portable framing contract](docs/HCI-L2CAP.md).

There is no installable payload or end-user pairing procedure yet. The first
major milestone is Rust in a PS5 ELF logging actual Bluetooth controller
information through a reviewed C adapter. The SDK's default startup and native
HCI ownership need resolution before deployment. The C link recipe is an
explicitly gated experiment; it does not deploy to a console.

## Architecture

```text
Application / payload
    -> Rust audio engine (planned)
    -> Rust A2DP / AVDTP / RTP / SDP / L2CAP (planned)
    -> Rust HCI representation and state
    -> portable platform traits
    -> ps5-platform safe wrappers -> ps5-sys -> small C ABI
    -> C SDK / capture / USB HCI / controller quirks -> PS5
```

Rust owns the architecture. Proven platform adaptation may remain C. See
[architecture decisions](docs/ARCHITECTURE.md) and [ABI contract](docs/ABI.md).
The workspace contains `bt-core`, `bt-hci`, `bt-l2cap`, `ps5-sys`, `ps5-platform`,
and `ps5-runtime`. L2CAP signaling and further protocol crates remain future work.

## Roadmap and contribution

[Milestones](ROADMAP.md): research/bootstrap -> Rust/C hardware diagnostics ->
transport and capture -> conservative SBC tone -> captured audio -> stability
and coexistence -> latency/quality -> additional codecs -> adaptive control.
No adaptation before reliable telemetry. No codec support promises before
hardware demonstration. Bluetooth version numbers do not predict audio quality.

Read [CONTRIBUTING.md](CONTRIBUTING.md), [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md),
[SECURITY.md](SECURITY.md), and [CHANGELOG.md](CHANGELOG.md). English is canonical
for source, API docs, issues, commits, and project documentation.
Use [Conventional Commits and staged-state checks](CONTRIBUTING.md#git-and-commit-discipline)
for each independently reviewable change; see [repository development rules](AGENTS.md).

## Upstream and license

FGG-PlayPods, HearBridge PS5, the PS5 Payload SDK, PS5SDK, and BlueZ/libsbc are
important research references. Credit their authors and contributors; preserve
their history and file-specific licenses when reusing code. No upstream
Bluetooth/audio implementation has been imported. [UPSTREAM.md](docs/UPSTREAM.md)
records exact revisions, licenses, behavior provenance, credits, and update rules.

Original bootstrap work is **GPL-3.0-or-later**, see [LICENSE](LICENSE).
Third-party code retains its own terms. FFI is an architectural boundary, not
an exemption from license obligations. Codec licensing/patents and distribution
requirements must be reviewed before adoption.

This is not a jailbreak project. A homebrew-capable console and existing ELF
loader are prerequisites. We do not implement or distribute exploits, privilege
escalation, DRM/license bypasses, Sony binaries, firmware, keys, or game content.
If an operation needs permissions absent from the existing environment, report
the blocker. There is no affiliation with or endorsement by Sony.

## Translations

Initial translations are concise status guides, not full duplicated manuals.
English remains authoritative if a translation is outdated.

[简体中文](docs/i18n/README.zh-CN.md) · [繁體中文](docs/i18n/README.zh-TW.md) ·
[日本語](docs/i18n/README.ja.md) · [Español](docs/i18n/README.es.md) ·
[Português brasileiro](docs/i18n/README.pt-BR.md) · [Français](docs/i18n/README.fr.md) ·
[Deutsch](docs/i18n/README.de.md) · [한국어](docs/i18n/README.ko.md)
