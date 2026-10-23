# MVP orchestration state

Last updated: 2026-10-24. Lead-owned integration board; workers own only their
assigned paths. No PS5 hardware evidence exists. MVP is not operational.

## Current milestone and baseline

Current software wave is complete at the HW-1 artifact. A real inspected PS5
ELF containing Rust and corresponding-source package are available. **The exact
next action is HW-1 physical validation.** Physical access is available again;
the instrumented candidate adds start/finish notifications and a persistent
per-run log, but no result is claimed until it is executed. No HW-2 work starts
in this wave. A second
macOS 27.0.1 arm64 machine cloned `origin/feat/mvp-bringup` at `89bdce0` and
reproduced the baseline ELF hash with the documented toolchain. No repository
change was required. Tone, capture and all controller operations remain
unverified. See WAVE-1-REPORT.md.

Baseline commit: `7f4b9cb`. Existing history preserved; integration branch:
`feat/mvp-bringup`. Plain `make check` fails in this shell because cargo is not
on PATH. With the documented temporary Rust 1.90.0 environment it passed: four
parser tests, 15 C/Rust scenarios, Clippy/check/fmt, and a 398-object x86-64
freestanding archive. `make ps5-link` stops at the runtime-review gate. There
was no final-link/runtime or known working hardware path at baseline. The
reviewed minimal startup now links a genuine diagnostic ELF; execution is untested.

Intended user test environment: PS5 Slim, system firmware 13.40, and a Classic
A2DP headset (model not yet recorded). The target is now reachable for the HW-1
diagnostic; no console execution result has been recorded yet. This is a
bring-up target, not verified compatibility. Pin the
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
| B1 | DONE | build_toolchain + independent/lead review | real inspected diagnostic ELF, original minimal entry/linker, three hash-verified SDK syscall inputs; no stock permission-changing CRT |
| D1a | DONE | hci_l2cap + lead review | safe HCI command/event/ACL and capability/completion types; malformed-input tests; no credit accounting or live transport |
| D1b | DONE | hci_l2cap + lead review | bounded L2CAP framing/reassembly in caller storage, tied to explicit handle/epoch; no signaling/channel establishment |
| A2 | DONE | native_hci_research + lead review | source-backed single read-only native stack-version query; no PS5 broker found in bounded inventory; see NATIVE-HCI-RESEARCH.md |
| C1a | DONE | platform_descriptors + lead review | bounded descriptor parser/report, hostile host tests and ASan/UBSan; no live USB access |
| C1b | WAITING | platform worker | actual read-only descriptor probe after verified runtime operations/SDK headers; no endpoint takeover |
| C2 | BLOCKER | lead + platform worker | native HCI command/event/ACL ownership and enforceable credits; upstream read competition is not routing |
| C3 | DONE | native_presence_probe + lead review | separate opt-in ELF, one read-only stack-version query, strict/sanitized/main-hook tests; PS5 availability Unknown |
| H1 | HARDWARE | user + lead | HW-1 ELF load, Rust entry and clean return after reviewed startup |
| H2 | HARDWARE | user + lead | HW-2 descriptor/HCI data and DualSense before/during/after; requires C1/C2 |
| E1a | DONE | sdp_framing + independent/lead review | bounded SDP PDU/continuation/data-element foundation, nine parser tests; no discovery client |
| E1b | WAITING | connection worker | bounded Classic connection/pairing/key lifecycle and A2DP SDP discovery after packet interfaces |
| F1 | WAITING | AVDTP/A2DP worker | typed SBC negotiation/open/start/close and RTP packetization after D1/E1 contracts |
| G1a | DONE | sbc_audit + independent/lead review | validated shared SBC selections and LGPL/runtime audit; no encoder imported |
| G1b | WAITING | codec worker | proven libsbc encoder behind narrow wrapper, after bounded initialization storage and relinking plan; per-frame allocation prohibited |
| T1 | WAITING | integration worker | hostile scripted transport/headset and PCM-to-SBC-to-RTP-to-ACL simulation once module contracts exist |
| H7 | HARDWARE | user + lead | 440 Hz SBC A2DP tone audible on real headset, after controller/connection/negotiation |
| H8 | HARDWARE | user + lead | actual PS5 PCM audible through the proven tone pipeline |
| I1a | DONE | build_docs_integration + lead | pinned build/source-package instructions and exact baseline loading/log checklist |
| I1b | WAITING | documentation worker | pairing/configuration/audio workflow only when those capabilities exist |

Completed waves: upstream/runtime research and real minimal ELF; portable HCI,
L2CAP, SBC selections and SDP framing; host-only descriptor diagnostics; separate
native-query ELF. Each was reviewed and tested before atomic integration.
Do not declare hardware completion from desktop simulation or an ELF link.

## Persistent blockers and next integration target

SDK pin: v0.45 / `a7685ce8e97ce1f8a4f38253a41d54241c9124f1`;
PlayPods and HearBridge research pins remain in upstream.lock.json. Default SDK
startup mutates credentials/syscall permissions and is excluded. The reviewed
minimal startup requires existing loader permissions and a checked stub ABI.
Existing upstream transport competes with the native stack for USB input. The
test target is Slim/13.40; actual loader, controller and headset identity remain
unknown until runtime evidence is returned.

Last lead verification: 34 HCI/L2CAP/SBC-model/SDP tests, 15 ABI scenarios,
descriptor/native-query/main-hook regressions with ASan/UBSan, fmt/check/Clippy and the
freestanding archive passed from an isolated staged snapshot. HCI framing was
integrated in `418071c`; no command dispatcher/credit ledger was added.

Verified tooling research pins: elfldr v0.27 at
`2a8da002e5292481ea964daa7342bbf4855f4c33`; websrv latest published v0.34 at
`4d35610914273f8f4fc37af2b1d5bfb069dfba5b` (newer default branch is distinct).
Neither researched loader provides a direct dlsym callback in the first argument:
it is a getpid syscall stub. Loader-supported relative relocations and BSS/return
requirements must be checked for the actual ELF.

No active protocol implementation assignments remain. All completed workers
were reviewed and integrated; the clean-source rebuild review passed. The
optional query ELF remains unexecuted. Controller transport is blocked on
ownership evidence. Next action is only HW-1 instrumented entry/log/return and
DualSense validation. Do not start HW-2 or speculative work to fill the wait.
