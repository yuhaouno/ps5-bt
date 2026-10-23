# MVP orchestration state

Last updated: 2026-10-23. Lead-owned integration board; workers own only their
assigned paths. No PS5 hardware evidence exists. MVP is not operational.

## Current milestone and baseline

Next target: an inspectable PS5 ELF containing Rust, then HW-1 runtime evidence
and HW-2 actual controller information. Tone and captured audio follow those
hardware gates; portable work may continue independently.

Baseline commit: `7f4b9cb`. Existing history preserved; integration branch:
`feat/mvp-bringup`. Plain `make check` fails in this shell because cargo is not
on PATH. With the documented temporary Rust 1.90.0 environment it passes: four
parser tests, 15 C/Rust scenarios, Clippy/check/fmt, and a 398-object x86-64
freestanding archive. `make ps5-link` stops at the runtime-review gate. There
is no approved final-link/runtime or known working hardware path yet.

User test environment: PS5 Slim, system firmware 13.40, an available Classic
A2DP headset (model not yet recorded), and ability to load diagnostic ELF files
and return logs. This is a bring-up target, not verified compatibility. Pin the
latest verified loader/runtime releases before giving loading instructions.
Detect vendor/revision, HCI/LMP versions, features and buffer limits at runtime;
do not ask the user to infer them from the console model.

## Shared interfaces and integration rules

- Retain `bt_core::BluetoothTransport`: complete header-bearing command/ACL
  buffers without H4; one ordered, tagged receive stream into caller storage;
  bounded millisecond wait. Do not split receive into competing native readers.
- `bt_core::PcmSource` is the current AudioSource contract: declared format,
  interleaved f32 caller buffer, whole frame count, optional monotonic timestamp
  and discontinuity. Do not pass floats by value through the freestanding C ABI.
- `Error` categories and raw backend codes remain authoritative. Unknown is not
  success; no fake credits, silent queue loss, resets or unbounded retry loops.
- C ABI v1 and pointer/lifetime/token rules in ABI.md apply. A worker proposes
  changes centrally before editing another owner's contract.
- Lead owns Cargo.toml/lock, README, CHANGELOG, ROADMAP and this board. Workers
  do not stage, commit, reset, stash or modify others' paths. Lead reviews and
  commits each completed feature with tests/docs. Tests use the pinned Rust
  environment documented in BOOTSTRAP-REPORT.md until tooling is made durable.
- License review precedes source reuse. No exploits, privilege mutation, kernel
  patches, proprietary binaries/keys, codecs beyond SBC, GUI or other MVP extras.

## Task graph

| ID | State | Owner | Task / dependency / evidence |
| --- | --- | --- | --- |
| B0 | DONE | lead | inspect repository and repeat documented baseline |
| A1 | DONE | platform_research + lead review | exact SDK/loader pins, startup/syscall contract, metadata-only probe and capture boundaries, native routing blocker and per-file license review; see PS5_PLATFORM.md |
| B1 | READY | build_toolchain (running) | inspect SDK startup and tool availability; reproducible final link or exact missing dependency; owns build script/link recipe and toolchain/runtime review docs |
| D1a | DONE | hci_l2cap + lead review | safe HCI command/event/ACL and capability/completion types; malformed-input tests; no credit accounting or live transport |
| D1b | READY | hci_l2cap (running) | bounded L2CAP framing/reassembly in caller storage, tied to explicit handle/epoch; owns bt-l2cap |
| C1 | READY | platform_descriptors (running) | bounded configuration descriptor parser/hostile tests first; optional read-only probe after B1 resolver contract; no active endpoint takeover |
| C2 | BLOCKER | lead + platform worker | native HCI command/event/ACL ownership and enforceable credits; upstream read competition is not routing |
| H1 | HARDWARE | user + lead | HW-1 ELF load, Rust entry and clean return after reviewed startup |
| H2 | HARDWARE | user + lead | HW-2 descriptor/HCI data and DualSense before/during/after; requires C1/C2 |
| E1 | WAITING | connection worker | bounded Classic connection/pairing/key lifecycle and A2DP SDP discovery after packet interfaces |
| F1 | WAITING | AVDTP/A2DP worker | typed SBC negotiation/open/start/close and RTP packetization after D1/E1 contracts |
| G1 | WAITING | audio worker | independently review SBC implementation/license, bounded 48 kHz stereo source and encoder API; no import before review |
| T1 | WAITING | integration worker | hostile scripted transport/headset and PCM-to-SBC-to-RTP-to-ACL simulation once module contracts exist |
| H7 | HARDWARE | user + lead | 440 Hz SBC A2DP tone audible on real headset, after controller/connection/negotiation |
| H8 | HARDWARE | user + lead | actual PS5 PCM audible through the proven tone pipeline |
| I1 | WAITING | documentation worker | exact build/load/config/log workflow as actual artifacts and behavior become available |

Initial parallel wave A1/B1/D1a: A1 and D1a reviewed; B1 is pursuing a minimal
entry using only the pinned SDK syscall bridge and no stock CRT. Second wave:
C1 descriptor parsing and D1b L2CAP. SBC research can start after a slot frees.
Do not declare hardware completion from desktop simulation or an ELF link.

## Persistent blockers and next integration target

SDK pin: v0.45 / `a7685ce8e97ce1f8a4f38253a41d54241c9124f1`;
PlayPods and HearBridge research pins remain in upstream.lock.json. Default SDK
startup mutates credentials/syscall permissions; its suitability must be resolved
without implementing a bypass. Existing transport code competes with the native
stack for USB input. Console/loader/runtime/headset environment is unknown.

Last lead verification: 12 HCI tests, 15 ABI scenarios, fmt/check/Clippy and the
freestanding archive passed from an isolated staged snapshot. HCI framing was
integrated in `418071c`; no command dispatcher/credit ledger was added.

Verified tooling research pins: elfldr v0.27 at
`2a8da002e5292481ea964daa7342bbf4855f4c33`; websrv latest published v0.34 at
`4d35610914273f8f4fc37af2b1d5bfb069dfba5b` (newer default branch is distinct).
Neither researched loader provides a direct dlsym callback in the first argument:
it is a getpid syscall stub. Loader-supported relative relocations and BSS/return
requirements must be checked for the actual ELF.

Next integration target: review B1's minimal no-permission-write entry and ELF,
then C1 descriptor diagnostics and D1b bounded L2CAP. Continue desktop work while
awaiting actual hardware evidence.
