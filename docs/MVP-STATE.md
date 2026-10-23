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
| A1 | READY | platform_research (running) | verify pinned/current upstream and latest loader releases, startup/permissions, native HCI routing, C adaptation and license options; docs/UPSTREAM.md and docs/PS5_PLATFORM.md only |
| B1 | READY | build_toolchain (running) | inspect SDK startup and tool availability; reproducible final link or exact missing dependency; owns build script/link recipe and toolchain/runtime review docs |
| D1a | DONE | hci_l2cap + lead review | safe HCI command/event/ACL and capability/completion types; malformed-input tests; no credit accounting or live transport |
| D1b | READY | hci_l2cap (running) | bounded L2CAP framing/reassembly in caller storage, tied to explicit handle/epoch; owns bt-l2cap |
| C1 | WAITING | platform adapter worker | read-only USB descriptor probe and capability/backend selection after A1/B1 decisions; no active endpoint takeover |
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

Initial parallel wave: A1, B1, D1. Platform implementation waits for actual
runtime/ownership findings. SBC research can start after a worker slot frees.
Do not declare hardware completion from desktop simulation or an ELF link.

## Persistent blockers and next integration target

SDK pin: v0.45 / `a7685ce8e97ce1f8a4f38253a41d54241c9124f1`;
PlayPods and HearBridge research pins remain in upstream.lock.json. Default SDK
startup mutates credentials/syscall permissions; its suitability must be resolved
without implementing a bypass. Existing transport code competes with the native
stack for USB input. Console/loader/runtime/headset environment is unknown.

Next integration target: review A1/B1 recommendations, produce the closest
scope-compatible ELF build, and integrate D1's desktop-tested framing as its own
atomic feature. Continue desktop work while awaiting actual hardware evidence.
