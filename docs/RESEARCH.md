# Phase 0 technical research

Date: 2026-10-23 (Asia/Shanghai). Primary repository snapshots were read locally
from shallow clones outside this project. Full pins, license evidence and source
permalinks are in [UPSTREAM.md](UPSTREAM.md). This is the initial research snapshot. Current build integration is documented
in [TOOLCHAIN.md](TOOLCHAIN.md) and [RUNTIME-REVIEW.md](RUNTIME-REVIEW.md):
three unchanged SDK syscall files are now external link inputs. Findings below
describe source, not independently reproduced hardware facts.

## SDK ecosystem

`ps5-payload-dev/sdk` is the initial candidate: an active community C/C++ payload
SDK with dynamic imports, CRT, FreeBSD-derived headers, compiler/linker wrappers,
samples, and ELF deployment tooling. The latest release observed was v0.45;
the downloaded archive matched the recorded digest. Its compiler wrapper targets
`x86_64-sie-ps5`; its linker wrapper supplies a PIE, SDK linker script, GNU hash
style, and `0x4000` maximum page size. LLVM/Clang/lld are external tools, not
provided by rustc. `prospero.mk` supplies paths and build variables, not a Rust
target. The selected Rust archive must be linked by that C toolchain.

The SDK documents Linux/Fedora/macOS hosts. That is build-host support, not
proof that our ELF works on any console. HearBridge names SDK v0.43. PS5SDK is
an older CMake/Ninja path with a different payload entry convention; it was last
pushed in 2024 according to repository metadata, versus October 2026 for the
selected SDK. It is not interchangeable with the selected CRT. Pacbrew provides
an ecosystem of packages; none is necessary for the diagnostic bootstrap.
`ps5-payload-dev/elfldr` and `websrv` are active existing loader infrastructure;
loading is an external prerequisite, not functionality to implement here.

**Scope discovery:** SDK `crt/crt.c` invokes `__patch_init()` before main;
`crt/patch.c` writes kernel-backed process credential attributes and syscall
permissions. It must not be silently included as a harmless startup assumption.
No startup patch, replacement kernel primitive, or privilege workaround will be
implemented. The final-link/deployment step remains gated on a scope-compatible
runtime review. A Rust archive and host ABI smoke test can proceed independently.

## FGG-PlayPods

Inspected main at `d726e1c32568048aae66f3218c0540adc357e408`. It contains
capture, USB HCI, Bluetooth/signaling, SDP, A2DP, logging/session locking, and
vendored libsbc. Latest release observed is 1.0, distinct from main. The README
marks current resident-audio work as an RFC and cautions against inheriting old
usage/compatibility claims. The included hardware summary records one limited
reconnect experiment on firmware 12.40; it explicitly does not establish long
gameplay, repeated startup, native restoration, or the normalized RFC binary.

`capture.c` loads Ipmi before Avcap2, resolves symbols through SDK helpers, keeps
a private OpenAudio parameter block, and handles empty/overrun statuses. Its
source requests a Remote Play mix and raw stereo floats at 48 kHz. Actual format,
timing, permissions, and firmware behavior remain hardware questions here.
`hci.c` uses a fixed ugen node and endpoint constants, USB FS transfer slots and
USB control requests. Those constants are observations for that implementation,
not a hardware-independent PS5 interface. Comments explicitly warn that multiple
readers do not preserve delivery to the native owner. Bluetooth code distinguishes
foreign command replies and documents assumed timed buffer reuse as unproven.
These limitations are architectural inputs, not algorithms to translate.

## HearBridge PS5

Inspected main at `cd5b7fce1b8226d8850cdd306437550098c6a62b`; latest release
observed v1.3.6. This is a larger C application with platform capture, USB HCI,
descriptor parsing, A2DP/AVDTP, reconnect, diagnostics, rate control and UI.
The UI and optional profiles are not initial dependencies for this project.

`btchip.h` identifies Marvell/NXP by USB VID `0x1286` and MediaTek by `0x0e8d`;
it mentions `0x3603`/`0x3605` MediaTek PIDs. Descriptor code describes pairing
bulk OUT with bulk IN endpoint numbering on MediaTek. The HCI command layer has
MediaTek-specific scan pause/restoration and pacing behavior. Retain this
knowledge in a C backend when it can be adopted with provenance and tests;
do not use console model names as the dispatch rule. Unknown vendors must not
silently enter its default profile in our design.

The USB source states that the system stack reads the same endpoints. Its many
pending reads and timed completion refill are evidence of a workaround under
competing consumers, not proof of correct multiplexing or controller credit
ownership. Its receive ring can overwrite older entries. Our transport must
surface loss rather than let a fresh-looking queue conceal it.

Avcap2 code first tries existing credentials, but also has fallback credential
changes through `creds.c`. Those fallbacks are excluded. It contains format and
channel heuristics and restart policy that need isolated capture tests; they
must not become portable Rust assumptions. Dynamic imports and cleanup belong
in C; typed protocol state and policy belong in Rust.

## Minimum build experiment

Use the stable Rust prebuilt freestanding target with no OS runtime, override
its default kernel code model to small and enable PIC, then let the SDK perform
the final C link. Do not link FreeBSD `std`, rely on Linux syscalls, or create a
custom Rust PS5 target before it is needed. Restrict FFI to fixed-width integers,
pointers and byte buffers. Panic terminates through a C function, never unwinds.
Host tests validate contracts; target archive inspection validates compilation;
only a console validates execution. See [BUILDING.md](BUILDING.md) for commands
and [ARCHITECTURE.md](ARCHITECTURE.md) for the crate/dependency decisions.

Primary Rust references: [freestanding target](https://doc.rust-lang.org/rustc/platform-support/x86_64-unknown-none.html),
[static library linkage](https://doc.rust-lang.org/reference/linkage.html#r-link.staticlib).
Primary Bluetooth references: [Core 6.0](https://www.bluetooth.com/specifications/specs/core-specification-6-0/),
[A2DP 1.4](https://www.bluetooth.com/specifications/specs/advanced-audio-distribution-profile-1-4/),
[specification catalog](https://www.bluetooth.com/specifications/specs/).
These are protocol research references, not a claim of qualification or support
for a particular Core version. Before each protocol implementation, record the
exact specification revision, section, applicable errata and synthetic fixtures.

## Unknowns and experiments

| Unknown | Smallest useful experiment / exit evidence |
| --- | --- |
| Runtime that fits scope on the user's existing loader | audit startup and privileges; log start and clean return without changing credentials |
| Rust/C ABI and relocation acceptance | archive inspection, link-map/import checks, integer/pointer ABI smoke test on PS5 |
| Built-in controller node, vendor/product, interfaces and revision | read-only descriptor report, no inferred model mapping |
| Event/ACL ownership and native command routing | establish platform-supported routing or coordination before opening USB FS reads; show native traffic preserved |
| Shared command and ACL budget | measured per-handle submissions/completions plus a real enforceable budget; time is not credit |
| Capture access without new privilege changes | module/resolve/open/read/close report using existing environment permissions only |
| Capture byte/frame/sample units and clock | timed bounded reads with status/format metadata; controlled menu/game audio and silence checks |
| Controller firmware and command/features support | bounded version/commands/features/buffer queries after routing gate |
| Fat/Slim/Pro, firmware, Wi-Fi and DualSense behavior | per-device test matrix with build hashes and before/during/after controller checks |
| Throughput, headset latency, reconnect robustness | later SBC/soak/interoperability phases; no numeric claim now |

The first assignment completes a research/bootstrap foundation, not the major
hardware milestone. Missing transport ownership or permissions is a documented
blocker; it is not permission to build a jailbreak or to hide failures with
retries. The next milestone is real controller metadata through a reviewed
C/Rust payload and verified normal DualSense operation.
