# PS5-BT — working title

For the next PS5 window on a new testing machine, start with the temporary
[operator handoff](docs/TEMP-PS5-TEST-HANDOFF.md) and
[decision matrix](docs/PS5-WINDOW-DECISION-MATRIX.md). The six-item plan has a
one-command local preparer and verified portable transfer; native tests still
require the actual console window. No usable Bluetooth connection backend or
headphone playback is claimed.

A Rust-first Bluetooth Audio stack for PlayStation 5 homebrew — built for low
latency, high quality, and portability.

The goal is game and system audio on ordinary Bluetooth headphones through the
console's built-in Bluetooth hardware, while DualSense stays wireless. PS5 is
the first-class initial platform; the Bluetooth/audio core should also work
with future Linux and FreeBSD backends.

**Status: diagnostic ELF bring-up. No playable audio or
verified PS5 support yet.** The workspace contains portable contracts and packet
parsers, Rust/C capture and codec candidates, and a separately selected
native application assembly with separate control, generated-SBC and raw-observation modes. The production Bluetooth adapter
explicitly returns `Unsupported`. It is an independent project, not a translation or fork of an
existing payload. The final public name is undecided; repository and package
collision checks must precede naming. All Cargo packages are `publish = false`.

The [2026-10-26 window](docs/HW4-EVIDENCE.md) verified a standalone Rust/target
SBC run: 750 generated blocks, exact bytes/digest and clean finish. The Rust
capture boundary completed one Empty read with clean stop/close. Application
startup remains blocked: generated mode failed Store directory acquisition
(errno 1), and setup's schema-2 report rejected the root write-protection
predicate (mask 4). Native metadata layout and namespace authority remain
unverified. Controller operation was reported normal before/during/after each
later run; no PCM-format, physical pacing or playback acceptance follows.
A separate [read-only namespace probe](docs/NAMESPACE-PROBE.md) now prepares the
root/data consistency observation. Its PS5 run failed at root-dot fstatat
(raw -1/errno 1) after successful root open/fstat, with clean close and normal
reported controller input. No data observation was reached; its separate log
is retained and store protection remains gated. The next independently selected
[open comparison](docs/OPEN-COMPARE.md) distinguishes relative and absolute
directory access while preserving that gate. Its retained 2026-10-27 run found
relative open EPERM and successful absolute open/fstat, with clean cleanup and
normal reported controller input. Both observed paths still failed the compiled
protection predicate; absolute access does not establish a safe store. A separate
[raw-openat route experiment](docs/RAW-OPENAT.md) compares that imported call with
the existing checked syscall bridge using the same fixed target and flags;
its pinned source mapping is a hypothesis, with PS5 acceptance still pending.

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
its immediate read returned Empty. A later 100 ms settled read returned 8,192
bytes with clean cleanup and user-confirmed normal wireless input. Native PCM
format/timing and audio playback remain unverified.
See [USB/capture evidence](docs/HW2-EVIDENCE.md) and
[the resumed hardware window](docs/HW3-EVIDENCE.md).
The earlier second-machine build reproduction remains in
[the wave report](docs/WAVE-1-REPORT.md).

## Compatibility and measurements

| Area | Status | Evidence |
| --- | --- | --- |
| Portable contracts, HCI command/ACL framing and capability event decoders | Implemented | host packet/malformed-input tests |
| Owned ACL credit accounting | Implemented on host | exact acceptance/completion accounting with explicit reservation, sticky loss/uncertain-send failure and no timer refill; no hardware reservation established |
| L2CAP framing, bounded ACL fragmentation/reassembly and Classic channel signaling | Implemented on host | acceptance-confirmed outbound cursor plus scripted Connection/MTU Configuration/Disconnection and handle/epoch/bounds tests; no live transport/credits |
| Channel/SDP scheduling | Implemented on host | acceptance-gated SDP discovery/cleanup, signaling/media channels, bounded cancellation and fresh-CID media reopen; external routing, ACL fragmentation and credits required |
| SDP framing, bounded data elements and A2DP Sink discovery | Implemented on host | correlated transactions/epochs, bounded continuation and strict service-record semantics; no live query |
| Classic device discovery | Portable candidate | bounded Inquiry/Cancel, stable opaque device IDs, validated names and finite cleanup; PS5 command routing remains unavailable |
| Selected-device pairing/bond manager | Implemented on host | protected-store contract, versioned bond records, SSP approval, Ready-gated persistence, forget revocation and clock-failure cleanup; no PS5 store, inquiry or live pairing |
| Protected bond file store | POSIX candidate | versioned Rust/C owner, private-directory validation and atomic durable records; PS5 imports, protection and reboot behavior require hardware |
| Selected-peer Classic connection/security | Implemented on host | acceptance-correlated connection/cancel, stored-key or approved SSP, authentication/encryption/key-size gate; no inquiry, bond persistence, live transport or PS5 pairing result |
| AVDTP SBC initiator session | Implemented on host | conservative SBC negotiation/lifecycle plus bounded receive signaling reassembly with epoch/CID/transaction isolation; no live media channel, headset or PS5 execution |
| C ABI, safe wrappers and Rust runtime seam | Implemented | synthetic desktop boundary tests |
| USB descriptor parsing and metadata reports | Implemented | hostile host tests and sanitizers; no USB access |
| Opt-in read-only USB enumeration | Executed on one Slim/13.40 | own four-interface descriptor log; other node opens failed; transport remains Unsupported |
| Opt-in Avcap2 capture diagnostic | Nonempty data observed on one Slim/13.40 | 100 ms settled read returned 8,192 bytes; all lifecycle/cleanup statuses 0; native PCM format/timing unknown |
| SBC parameter selection and frame sizing | Implemented | portable validation tests and host encoder boundary |
| Real PCM-to-SBC encoding | Bounded target run passed on one Slim/13.40 | persistent Rust/C owner encoded 750 generated blocks with exact bytes/digest and clean finish; synthetic nominal clock, discard sink; no physical pacing or playback proof |
| Generated-signal SBC self-test | Executed on one Slim/13.40 environment | two fresh init/finish cycles, four 77-byte frames and pinned digest match host; clean cleanup; no measured encode duration or Bluetooth output |
| Bounded capture observation | Executed on one Slim/13.40 environment | immediate Empty then 8,192 bytes after one wait; checked clock call and clean cleanup; native PCM format/timestamp still unknown |
| SBC-over-RTP/A2DP packetization | Implemented on host | fixed negotiated MTU/frame boundaries, atomic backpressure, sequence/sample timestamps and wrap tests; no AVDTP/live stream |
| Media send orchestration | Implemented on host | SBC/RTP/L2CAP/ACL submission now uses an external credit owner; Busy/credit stops retain the PDU and terminal errors report cancellation counts; no clock or live transport |
| PS5 application startup/workloads | Target candidates; generated startup failed on one Slim/13.40 | imports/entropy succeeded; Store directory I/O failed before encoding with reported clean cleanup; other current profiles Unknown; no radio/playback |
| PS5 resource factory | Rust candidate | concrete fixed-storage clock/store/listener/capture/SBC constructors, staged acquisition and reverse cleanup; native imports/behavior require validation, unknown format and radio authority remain gated |
| Bounded application loop and native resource rebuild | Host-tested candidates | typed bounded lanes and global stop; scoped actual store/listener close/reopen on the same storage preserves fault tickets and local resource epochs; automatic application reconnect is not installed |
| Management/ACL runner | Portable candidate | discovery-to-device/SSP bridge, actual capability gates, retained input and shared-credit control/media ordering; no production PS5 acquisition or ownership authority |
| Single-owner application driver | Implemented on host | device security, SDP/channel scheduling, explicit start/stop/restart, continuous audio handoff and finite media/recovery budgets; typed loop/listener candidates, no activated PS5 backend |
| A2DP source session binding | Implemented on host | Classic Ready/context gate, automatic bounded SBC negotiation, explicit sample-gap timestamps, media-channel binding, partial-PDU-aware stop and clean restart; no capture/encoder/pacing, LAN owner loop or PS5 activation |
| Native clock/finite wait | Experimental adapter | separately versioned, fallible cached clock and one at-most-1ms wait request per step; host/native fixture and target-object checks only, native latency remains unverified |
| Native control listener/service | Target-linked candidate | bounded sequential clients and fixed HTTP buffers; selected-export PS5 link plus native startup assembly, with runtime imports/permissions still Requires hardware |
| LAN management service/page | Portable service and control-only startup candidate | bounded v1/v2 authenticated service and actual-state page; separate ephemeral loopback candidate keeps all radio/capture/codec operations unavailable |
| Experimental Rust capture entry | One-read lifecycle executed on one Slim/13.40 | separate versioned Avcap2 ABI and consume-once owner; immediate Empty, zero bytes, clean stop/close; native PCM/time and continuous behavior remain Unknown |
| Continuous PCM-to-SBC scheduling | Implemented on host | sample-clock block pacing, retained encoded offers, bounded concealment/rebuffer/expiry and persistent real host codec adapter; declared normalized input and independent target encoder/capture candidates, no playable PS5 integration |
| PCM staging, nominal sample timeline and bounded recovery | Host-only implementation | caller-owned stereo i16 queue; synthetic gaps/stalls; no resampler or live audio |
| Fault-episode recovery controller | Implemented on host | finite attempt/time budgets, capped backoff, cleanup/reconciliation tickets and stable-success reset; factory-based reopening candidate; no native hardware recovery claim |
| Correlated diagnostic result collection | Offline tool | embedded source/build identity for new Rust candidates, bounded numeric-only local JSON, explicit controller observations and host fsync; PS5 persistence remains Unknown |
| Freestanding Rust archive | Implemented | x86-64 ELF objects inspected; no PS5 execution |
| PS5 diagnostic ELF with explicit loader kernel resolver | Verified on one environment | Slim/13.40: Rust entry, durable lifecycle and final 2; notifications returned success |
| Optional read-only native stack-version query | Executed | sysctlbyname resolved; query returned -1; version and controller remain Unknown |
| Unconnected native L2CAP socket probe | Executed; negative on one Slim/13.40 | UDP control created/closed; L2CAP creation -1 / errno 47; normal wireless input; no usable native channel established |
| Production PS5 Bluetooth/captured-audio playback | Unsupported | transport remains Unsupported; experimental capture byte reads do not establish PCM format or playback |
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
make ps5-native-l2cap-probe # create/close two unconnected sockets; no radio traffic requested
make ps5-sbc-selftest # generated-signal codec test; no Bluetooth/capture operation
make ps5-capture-observe # first data/deadline/error, at most 20 reads/two seconds
make ps5-application-control # finite native startup/loopback; no radio
make ps5-application-generated # same startup + generated PCM/Rust SBC discard
make ps5-application-observe # same startup + bounded raw capture; format Unknown
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
    -> Rust audio scheduling (host-tested candidate)
    -> Rust A2DP / AVDTP / RTP (bounded packetization and initiator session host-tested)
    -> Rust SDP / L2CAP framing (discovery and initiator signaling host-tested)
    -> Rust HCI representation and state
    -> portable platform traits
    -> ps5-platform safe wrappers -> ps5-sys -> small C ABI
    -> C SDK / capture / USB HCI / controller quirks -> PS5
```

Rust owns the architecture. Proven platform adaptation may remain C. Reviewed,
scoped C protocol or connection modules may serve as transitional implementations
behind a versioned ABI; preserve provenance and explicit state/resource ownership.
This policy does not import an upstream stack or transfer its hardware claims. See
[architecture decisions](docs/ARCHITECTURE.md) and [ABI contract](docs/ABI.md).
The workspace contains `bt-a2dp`, `bt-app`, `bt-audio`, `bt-avdtp`, `bt-channel`,
`bt-control`, `bt-core`, `bt-device`, `bt-discovery`, `bt-store`, `bt-runner`,
`bt-listener`, `bt-capture`, `ps5-app`,
`bt-codec`, `bt-hci`, `bt-l2cap`, `bt-link`, `bt-media-session`, `bt-sdp`,
`bt-session`, `ps5-sys`,
`ps5-platform`, and `ps5-runtime`. The first initiator-side L2CAP Basic-mode channel machine, SDP
Audio Sink discovery client, bounded AVDTP initiator session and SBC-over-RTP
packetizer are host-tested. The media send cursor binds encoded frames through
RTP/L2CAP/ACL with explicit acceptance and cancellation accounting. AVDTP
receive signaling reassembly and sample-clock scheduling are host-tested; hardware
credit authority and every live channel remain absent.
The new source-session binding joins Classic Ready, SBC negotiation, separate
media-channel binding and the existing credit-aware sender on the host; it has
no PS5 activation. See [the application binding contract](docs/SOURCE-SESSION.md).
The bounded LAN management service and static page are implemented; see
[the control contract](docs/LAN-MANAGEMENT.md). An [optional native listener](docs/CONTROL-LISTENER.md) is implemented separately.
The [portable application driver](docs/APPLICATION-OWNER.md) owns selected-device
security, channel/SDP progression, source negotiation, normalized PCM scheduling
and bounded stop/recovery. Rust capture and target SBC seams are implemented as
independent candidates; actual native format, traffic ownership and headset
acceptance remain unresolved.
The separate [control-only application](docs/APPLICATION-STARTUP.md) assembles
`ps5-app`, the real protected store/listener/clock, Application/Runner and a finite
owner loop. It binds an ephemeral loopback port and uses a fresh unlogged bearer
secret. It requires an existing private `/data/ps5-bt/control` directory with mode
0700; it creates no directory. No radio, capture or codec operation is enabled.
The separate generated mode uses that same startup and observed clock to process
750 generated 128-frame blocks through Rust scheduling and the real target SBC
encoder, then hashes/discards them. The raw-observation mode uses the same owner
loop for at most 256 calls within an exclusive two-second budget, with a 100 ms
settle and 10 ms interval (up to 190 polls under exact regular scheduling). Neither sends
Bluetooth or infers native PCM. See [generated workload](docs/APPLICATION-GENERATED.md)
and [raw observation](docs/APPLICATION-OBSERVE.md).
PS5 imports, entropy, filesystem protection and scheduling remain Requires
hardware. The default diagnostic and existing capture/SBC bounds stay unchanged.
[Scoped native rebuilding](docs/NATIVE-RESOURCE-REBUILD.md) now tests actual local
resource close/reopen on the same storage while retaining one recovery episode.
It is not installed as automatic reconnect in the existing main loop and adds no
rebuild hardware artifact.
The [portable management/ACL runner](docs/APPLICATION-RUNNER.md) supplies bounded
loop steps and explicit retained-packet/ticket ownership for a platform scheduler.
See [next-session work and hardware run order](docs/MVP-NEXT-RUN.md).

There is still no usable PS5 Bluetooth connection backend. The recorded native
L2CAP probe returned errno 47, and no safe active-HCI test artifact is ready.
These local startup/workload candidates cannot connect the reported XM4 headset
or establish playback/coexistence. The full kit inventory has nine candidates;
`tools/startup_test_plan.json` selects only the three application candidates for
a newly authorized window. Assembly and read-only preflight default to no network
execution; see [kit and collection workflow](docs/BUILDING.md).

For the optional discovery, management, native store/listener and page development
checks, run `make application-candidates-test` on a Unix host with Node.js.
It uses synthetic records and loopback sockets and never contacts a PS5.

## Roadmap and contribution

[Milestones](ROADMAP.md): research/bootstrap -> Rust/C hardware diagnostics ->
transport and capture -> conservative SBC tone -> captured audio -> stability
and coexistence -> latency/quality -> additional codecs -> adaptive control.
No adaptation before reliable telemetry. No codec support promises before
hardware demonstration. Bluetooth version numbers do not predict audio quality.

Development need not wait for console access: connection/pairing and protected
bond storage, channel/session binding, continuous SBC scheduling, recovery,
experimental capture adapters, diagnostics and minimal LAN device management
may be implemented with PS5 validation explicitly pending. Functional, quality,
latency, coexistence and stability acceptance remains on PS5; a Linux Bluetooth
backend is not a prerequisite. Separate source-backed transport experiments
follow [the admission rules](AGENTS.md#active-mvp-execution); they need not
pre-prove their intended measurements, but do not implicitly authorize competing
USB readers, timed credit refill or system-driver changes. No new live backend
or user-facing capability is added by these policy clarifications.

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
The optional codec imports selected BlueZ libsbc 2.2 source under
LGPL-2.1-or-later with three recorded patches. The separately selected
short self-test and continuous Rust diagnostic link that replaceable archive
into PS5 ELFs; production codec execution remains unverified. The corresponding-source package includes application
objects and a working codec replacement/relink script. See
[target codec workflow](docs/SBC-TARGET.md) and
[vendored notices](third_party/sbc/VENDORED.md).

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
No new exploit chain, DRM/license bypass, Sony binary, firmware, protected
Sony/DRM key or game content is implemented or distributed. Normal local
Bluetooth bond-key storage is allowed for pairing/reconnect/forget-device,
but keys must never enter Git, logs or the LAN page. System-driver/process
mutation is outside the current own-process permission scope. See
[repository rules](AGENTS.md).
There is no affiliation with or endorsement by Sony.

## Translations

Initial translations are concise status guides, not full duplicated manuals.
English remains authoritative if a translation is outdated.

[简体中文](docs/i18n/README.zh-CN.md) · [繁體中文](docs/i18n/README.zh-TW.md) ·
[日本語](docs/i18n/README.ja.md) · [Español](docs/i18n/README.es.md) ·
[Português brasileiro](docs/i18n/README.pt-BR.md) · [Français](docs/i18n/README.fr.md) ·
[Deutsch](docs/i18n/README.de.md) · [한국어](docs/i18n/README.ko.md)

The separately selected [application-setup candidate](docs/APPLICATION-SETUP.md)
can prepare the fixed private store namespace before native application tests.
It validates native ownership/protected parents, creates only missing project
components and consumes owned descriptors once; it never changes existing
permissions. `make application-setup-test` exercises strict/sanitized host faults;
`make ps5-application-setup` builds its independent target candidate. Native
permissions, export availability and durability still require PS5 acceptance.
