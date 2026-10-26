# Current execution handoff — first audible audio

Updated 2026-10-27 after the user's priority correction. This is the current
execution queue; it supersedes earlier diagnostic sequences and pending-run
instructions. Read `AGENTS.md`, then only the sources needed for the next change.

## Objective and next decision

Fastest useful result: reproduce headset pairing/connection and audible sound
on the user's Slim / firmware 13.40 with XM4; measure and optimize afterward.
Prefer an isolated, pinned HearBridge reference before porting its working C
chain into Rust. A standalone reference result is not PS5-BT acceptance.
Do not require our current FileStore, management page or general recovery loop
for a reference experiment. Keep any necessary bond state private; do not weaken
the production store to bypass its current failure.

1. The pinned reference is built: see the
   [recipe and concrete run scope](../experiments/hearbridge-reference/README.md).
   It retains competing system USB reads, timed credit fallback, event-mask/SSP
   and conditional scan-setting writes, plus SDK stock own-process permissions.
   These details require an explicit execution-scope decision. Do not silently
   deploy/run either the stock baseline or candidate, or claim safe coexistence.
2. If admitted in a current hardware window, select/pair XM4 and hear the
   default generated tone, which now works independently of capture. Within the
   same short media window, turning tone off attempts game/system capture once.
   Record the first failed stage or audible result, disconnect/stop and controller
   behavior; collect immediately. Do not restart the directory probes.
3. After reference success, integrate the minimum required C chain into PS5-BT,
   retaining working interfaces until an actual playback comparison justifies
   Rust substitution or an optimization. Assess latency/stability afterward.

The strict ownership path remains a research/development blocker if the reference
mechanisms are not admitted; no fast-completion promise is justified in that case.
Initial reference sound and production safety/stability are separate outcomes.

## Prepared reference — 2026-10-27, not uploaded or executed

- HearBridge pin `cd5b7fce1b8226d8850cdd306437550098c6a62b`; SDK v0.43 pin
  `d9c9519116944a7f1c22d262012c53b80b3520b7`; LLVM 18.1.8, built from source.
- Kit: `build/reference/first-audio-candidate`; `manifest.json` records exact
  patch/source/ELF hashes. ELF: `hearbridge/dist/PS5-BT-HearBridge-reference-1.3.6.elf`,
  726,992 bytes, SHA-256 `21edbfce50f77292dad90fa245d37b56119975db83cc881dceef0d80df32d9c9`.
- Fresh-source patch application, target link/import check (71 allowed imports),
  host control tests including six gateway checks, and stop/deadline/clock tests
  passed. Existing SDK warnings and unused disabled helper warnings remain;
  no full Rust suite was run because the product Rust code was not changed.
- Independent process/state; fresh `/data/ps5-bt-hb-reference` mkdir 0700 must
  succeed. Existing state is preserved and stops startup. Private `access.json`
  supplies UI login; never upload it or collect bond files with logs.
- Cooperative ceiling 180 s after stop initialization; immutable media ceiling
  30 s. Stop on failure; no outer reconnect. Blocking native calls/joins can
  overrun. Cleanup uncertainty is reported, not called a clean exit.
- Primary log: `/data/ps5-bt-hb-reference/hearbridge.log`; also retain diag.txt
  and status.txt if present. No headset result, capture-format acceptance or
  comparative performance improvement is established.

## Current console and retained evidence

- Last confirmed console: `192.168.1.111`, FTP 2121, PayloadManager 0.6.3 on HTTP
  8084. That historical configuration is not a future session's authorization.
- `ps5-bt-application-raw-openat` is uploaded, hash-verified and **on hold, not
  queued for manual execution**. Do not ask the user to run it by default.
  Its immutable kit is `build/ps5/raw-openat-window-kit-e328d85`; no run is known.
- Last verified inventory has only that PS5-BT slot; unrelated entries unchanged.
  No remote mutation or launch was performed by this workflow-priority update.
- Standalone SBC passed 750 blocks. Capture-stream returned Empty. Application
  startup failed store prerequisites. Directory comparisons found relative-open
  EPERM and absolute-open success; none established Bluetooth transport.
- Native L2CAP returned errno 47. There is no admitted usable connection backend
  or demonstrated XM4 playback; native PCM format and real credits remain unknown.
- Exact runs/hashes: [HW4](HW4-EVIDENCE.md), [HW3](HW3-EVIDENCE.md),
  [raw-openat upload](RAW-OPENAT.md), [transport review](NATIVE-HCI-RESEARCH.md),
  [upstream pins/licenses](UPSTREAM.md). Read these only for the relevant question.

## Iteration and validation

Use the smallest changed-module check and one target build before hardware.
Preserve compile/ABI, resource cleanup, artifact hash and honest log checks.
Do not regenerate portable kits, run every suite, or synchronize historical
reports for prose edits. Existing tests stay available on demand.
Full `make check` has a recorded ARM-host x86-fixture limitation; never call it a
pass. No simulator, successful upload or upstream claim substitutes for PS5 audio.
