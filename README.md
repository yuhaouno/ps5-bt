# PS5-BT — working title

A Rust-first Bluetooth Audio stack for PlayStation 5 homebrew — built for low
latency, high quality, and portability.

The goal is game and system audio on ordinary Bluetooth headphones through the
console's built-in Bluetooth hardware, while DualSense stays wireless. PS5 is
the first-class initial platform; the Bluetooth/audio core should also work
with future Linux and FreeBSD backends.

**Status: diagnostic ELF bring-up. No playable audio or
verified PS5 support yet.** The workspace contains portable contracts and packet
parsers, a Rust/C seam, and an adapter that explicitly returns
`Unsupported`. It is an independent project, not a translation or fork of an
existing payload. The final public name is undecided; repository and package
collision checks must precede naming. All Cargo packages are `publish = false`.

**HW-1 now executes on the recorded Slim / firmware 13.40 environment.**
The explicit `make ps5-hw1` build uses the existing loader's kernel-R/W
transport to resolve its own exports, records Rust entry and final
`2 (expected Unsupported)`, and sends start/finish notifications successfully
at the API level. The user also confirmed visible notifications and normal
wireless DualSense operation during the diagnostic. The optional native query executes but
returns -1; no controller capability or audio support follows from this.
See [hardware repair evidence](docs/HW1-REPAIR.md).
Own read-only USB enumeration has since confirmed `1286:2059` with HCI-class
interfaces 0 and 2. The capture diagnostic completed open/start/cleanup, but
its immediate read returned Empty; no PCM or audio playback is established.
See [USB/capture evidence](docs/HW2-EVIDENCE.md).
The earlier second-machine build reproduction remains in
[the wave report](docs/WAVE-1-REPORT.md).

## Compatibility and measurements

| Area | Status | Evidence |
| --- | --- | --- |
| Portable contracts, HCI command/ACL framing and capability event decoders | Implemented | host packet/malformed-input tests |
| L2CAP framing, bounded ACL fragmentation/reassembly and Classic channel signaling | Implemented on host | acceptance-confirmed outbound cursor plus scripted Connection/MTU Configuration/Disconnection and handle/epoch/bounds tests; no live transport/credits |
| SDP framing, bounded data elements and A2DP Sink discovery | Implemented on host | correlated transactions/epochs, bounded continuation and strict service-record semantics; no live query |
| AVDTP SBC initiator session | Implemented on host | bounded Discover/Get Capabilities/Set Configuration/Open/Start/Suspend/Close/Abort with conservative SBC selection; no signaling reassembly, live media channel, headset or PS5 execution |
| C ABI, safe wrappers and Rust runtime seam | Implemented | synthetic desktop boundary tests |
| USB descriptor parsing and metadata reports | Implemented | hostile host tests and sanitizers; no USB access |
| Opt-in read-only USB enumeration | Executed on one Slim/13.40 | own four-interface descriptor log; other node opens failed; transport remains Unsupported |
| Opt-in Avcap2 capture diagnostic | Lifecycle executed | open/start/stop/close/terminate 0; immediate read Empty; no acquired PCM |
| SBC parameter selection and frame sizing | Implemented | portable validation tests and host encoder boundary |
| Real PCM-to-SBC encoding | Host-only implementation | safe Rust owner, generic libsbc, 208 encoder/12 decoder vectors; not in PS5 ELF |
| SBC-over-RTP/A2DP packetization | Implemented on host | fixed negotiated MTU/frame boundaries, atomic backpressure, sequence/sample timestamps and wrap tests; no AVDTP/live stream |
| Media send orchestration | Implemented on host | binds complete SBC frames through RTP, L2CAP and acceptance-confirmed ACL fragmentation; no clock, HCI credits or live transport |
| LAN control/status contract | Implemented on host | bounded authenticated request parser, single-owner command queue and identity-free status JSON; no listener/page or PS5 integration |
| PCM staging, nominal sample timeline and bounded recovery | Host-only implementation | caller-owned stereo i16 queue; synthetic gaps/stalls; no resampler or live audio |
| Freestanding Rust archive | Implemented | x86-64 ELF objects inspected; no PS5 execution |
| PS5 diagnostic ELF with explicit loader kernel resolver | Verified on one environment | Slim/13.40: Rust entry, durable lifecycle and final 2; notifications returned success |
| Optional read-only native stack-version query | Executed | sysctlbyname resolved; query returned -1; version and controller remain Unknown |
| Live PS5 transport and capture | Unsupported | adapter explicitly returns Unsupported |
| PS5 Fat / Slim / Pro | Limited hardware evidence | one Slim/13.40 diagnostic run path; other models and Bluetooth remain Unknown |
| Console firmware | Limited hardware evidence | user-reported 13.40 diagnostic execution only |
| Marvell/NXP and MediaTek controllers | Experimental research targets | upstream source inspected; no live backend |
| Other controller families | Unknown | detection must fail explicitly |
| DualSense wireless coexistence | Unknown; required release gate | shared HCI ownership unresolved |
| SBC / high-quality SBC / SBC-XQ | Host SBC subset only; others planned | 48 kHz stereo/joint, bitpool 2..53; no live streaming or quality claim |
| AAC / additional codecs | Research only | technical and licensing review required |
| Headphones | Unknown | no compatible or incompatible models verified here |
| Latency, bitrate, CPU use, multi-hour stability | Unknown | no measurements yet |

Evidence labels are **Verified**, **Reported**, **Experimental**, **Unsupported**,
and **Unknown**. Upstream results do not confer our Verified status. See
[compatibility policy](docs/COMPATIBILITY.md).

## Development

Install Rust using the pinned [toolchain](rust-toolchain.toml), a C11 host compiler,
Python 3.11+, Make and LLVM 18.1.8. Set `LLVM_CONFIG` to that installation's
`llvm-config`; see [exact setup and provenance](docs/TOOLCHAIN.md). Then:

```sh
make                # build/ps5/ps5-bt.elf: diagnostic only, hardware untested
make ps5-hw1        # explicit existing-loader kernel resolver; verified diagnostic path
make check          # formatting, Rust check/tests/lints, C/Rust seam tests, cross archive
make host-smoke     # C main -> Rust -> unsupported C adapter, on the desktop
make sbc-codec-test # real offline PCM -> SBC -> RTP/L2CAP/ACL; no PS5/audio output
make rust-archive   # no_std x86-64 ELF archive; no SDK or console needed
make ps5-native-query # separate opt-in query ELF; run only after baseline entry test
python3 tools/mvp_test_kit.py # dry-run next-window plan; no network/deployment
```

See [build instructions](docs/BUILDING.md), [testing](docs/TESTING.md), the
[research report](docs/RESEARCH.md), and the [bootstrap report](docs/BOOTSTRAP-REPORT.md).
No third-party Rust packages are required. The optional host codec uses the
vendored, hash-verified BlueZ libsbc selection; `make check` exercises it with
the host C compiler, `patch`, `ar`, `nm` and ASan/UBSan. See
[codec evidence and limits](docs/SBC-IMPLEMENTATION.md).
Host checks and target compilation do not establish PS5 compatibility.
Offline audio continuity work may progress independently of live transport:
see [layered recovery policy](docs/AUDIO-RECOVERY.md). Raw HCI integrity/credits
remain strict; bounded PCM concealment/rebuffer is a separate policy.
The first hardware bring-up target is PS5 Slim on firmware 13.40; support is
unverified. Controller vendor, revision and capabilities will be detected rather
than inferred from that model. See the [live MVP task board](docs/MVP-STATE.md)
and [portable framing contract](docs/HCI-L2CAP.md).

The HW-1 ELF sends start/finish notifications and mirrors diagnostics to
`/data/ps5-bt-hw1.log`, replaced per run. The tested path enters Rust and
returns Unsupported. The default minimal build retains no kernel helper;
the explicit HW-1 variant borrows the installed loader's kernel-R/W transport
for initialization and own-process export resolution. It changes no process
credentials or syscall bounds and includes no stock CRT or patch.c.
See [runtime scope](docs/RUNTIME-REVIEW.md) and
[build instructions](docs/BUILDING.md). Source bundles reproduce the selected
variant and preserve SDK notices.

## Architecture

```text
Application / payload
    -> Rust audio engine (planned)
    -> Rust A2DP / AVDTP / RTP (bounded packetization and initiator session host-tested)
    -> Rust SDP / L2CAP framing (discovery and initiator signaling host-tested)
    -> Rust HCI representation and state
    -> portable platform traits
    -> ps5-platform safe wrappers -> ps5-sys -> small C ABI
    -> C SDK / capture / USB HCI / controller quirks -> PS5
```

Rust owns the architecture. Proven platform adaptation may remain C. See
[architecture decisions](docs/ARCHITECTURE.md) and [ABI contract](docs/ABI.md).
The workspace contains `bt-a2dp`, `bt-avdtp`, `bt-control`, `bt-core`,
`bt-codec`, `bt-hci`, `bt-l2cap`, `bt-media-session`, `bt-sdp`, `ps5-sys`,
`ps5-platform`, and `ps5-runtime`. The first initiator-side L2CAP Basic-mode channel machine, SDP
Audio Sink discovery client, bounded AVDTP initiator session and SBC-over-RTP
packetizer are host-tested. The media send cursor binds encoded frames through
RTP/L2CAP/ACL with explicit acceptance and cancellation accounting. AVDTP
signaling reassembly, clock/credit pacing and every live channel remain absent.
The bounded LAN request/state contract is host-tested, but has no socket listener
or page; the remaining protocol and LAN integration work is not a current capability.
See [next-session work and hardware run order](docs/MVP-NEXT-RUN.md).

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
Bluetooth protocol stack has been imported; the capture diagnostic adaptation
has separate provenance. [UPSTREAM.md](docs/UPSTREAM.md)
records exact revisions, licenses, behavior provenance, credits, and update rules.
The optional host codec imports selected BlueZ libsbc 2.2 source under
LGPL-2.1-or-later with two recorded patches. It is not linked into PS5 builds;
target ABI validation and the codec-enabled ELF relinking/distribution workflow
remain gates. See [vendored notices](third_party/sbc/VENDORED.md).

Original bootstrap work is **GPL-3.0-or-later**, see [LICENSE](LICENSE).
The capture diagnostic adaptation is conservatively GPL-3.0-only; its combined
diagnostic uses GPL v3. See [capture provenance](docs/CAPTURE-PROBE.md).
Third-party code retains its own terms. FFI is an architectural boundary, not
an exemption from license obligations. Codec licensing/patents and distribution
requirements must be reviewed before adoption.

A homebrew-capable console and existing ELF loader are prerequisites.
Initial kernel access must come from that environment. Project rules permit
reviewed, necessary changes to this payload's own permissions through the
existing channel; the current diagnostic does not need those changes.
No new exploit chain, DRM/license bypass, Sony binary, firmware, key or game
content is implemented or distributed. See [repository rules](AGENTS.md).
There is no affiliation with or endorsement by Sony.

## Translations

Initial translations are concise status guides, not full duplicated manuals.
English remains authoritative if a translation is outdated.

[简体中文](docs/i18n/README.zh-CN.md) · [繁體中文](docs/i18n/README.zh-TW.md) ·
[日本語](docs/i18n/README.ja.md) · [Español](docs/i18n/README.es.md) ·
[Português brasileiro](docs/i18n/README.pt-BR.md) · [Français](docs/i18n/README.fr.md) ·
[Deutsch](docs/i18n/README.de.md) · [한국어](docs/i18n/README.ko.md)
