# Current execution handoff — first audible audio

Updated 2026-10-27 after the user's priority correction. This is the current
execution queue; it supersedes earlier diagnostic sequences and pending-run
instructions. Read `AGENTS.md`, then only the sources needed for the next change.

## Active optimization round

The user has now authorized proceeding with the planned comparison and said
devices are available. This supersedes the earlier two-run permission checkpoint
below for the current optimization round, not for unrelated transport changes.
The [original queue-pressure policy](../experiments/queue-pressure/README.md) and
five-file upstream adapter are built as
`build/reference/queue-pressure-candidate/hearbridge/dist/PS5-BT-HearBridge-pressure-1.3.6.elf`.
SHA-256 `de6e2da952d52b933c5070a6bb25fd708a2f0bab03bf3a9ab7feffa3bdd26054`,
727,144 bytes. Six pure-policy groups (strict C11 and sanitizers), adapter checks,
unchanged upstream rate tests, fresh-source target link and 71-import check pass.
The original baseline remains intact. Use capture-first and the same 30-second
media / 180-second cooperative overall bounds. Preserve old private state and
confirm physical headset/content readiness before launch. No runtime result yet.

Candidate commit `f7768c1` is uploaded through PayloadManager 0.6.3 at
`192.168.1.111:8084`; FTP readback matches the hash above. The sole PS5-BT slot
is now `local/ps5-bt-hearbridge-pressure` (same ELF filename as above). The previous
PS5-BT reference slot was removed after verification; unrelated slots remain.
The completed capture state was renamed in place to
`/data/ps5-bt-hb-reference-capture-20261027`; its log hash matched the baseline,
and private bonds were not downloaded. Tone state remains in its earlier archive.
Preflight found no running reference/HearBridge process. Deployment evidence is
under `build/reference/queue-pressure-candidate/remote/`. Launch has **not** occurred:
the operator's current XM4 pairing-mode/content/controller readiness reply is
pending. Use this candidate's private `access.json` for capture-first control;
disable tone before scanning/selecting XM4, and collect immediately after launch.

## Objective and next decision

Fastest useful result: reproduce headset pairing/connection and audible sound
on the user's Slim / firmware 13.40 with XM4; measure and optimize afterward.
Prefer an isolated, pinned HearBridge reference before porting its working C
chain into Rust. A standalone reference result is not PS5-BT acceptance.
Do not require our current FileStore, management page or general recovery loop
for a reference experiment. Keep any necessary bond state private; do not weaken
the production store to bypass its current failure.

1. The pinned reference has produced both tone and captured game/menu audio: see the
   [hardware result](REFERENCE-FIRST-AUDIO.md) and
   [recipe and concrete run scope](../experiments/hearbridge-reference/README.md).
   It retains competing system USB reads, timed credit fallback, event-mask/SSP
   and conditional scan-setting writes, plus SDK stock own-process permissions.
   The user separately admitted two runs; both completed. A further run needs
   its own decision. Do not claim established ownership or safe coexistence.
2. The user heard corresponding game/menu audio with about two very brief
   dropouts; controller input was normal and the headset disconnected afterward.
   Sender accounting also reported two drops, but correlation is unproven.
   First inspect queue pressure before dropping: queue reached eight, then
   upstream reduced bitpool 41 → 37 → 22. Do not treat Empty reads as audio loss
   or timed event-credit estimates as actual controller capacity.
3. Preserve this working reference for comparison. Develop the smallest scoped
   change addressing short dropouts and integrate only the required C chain;
   keep working interfaces until measurements justify replacement. Do not
   rewrite for Rust purity or restart the directory-probe sequence.

The strict ownership path remains a research/development blocker if the reference
mechanisms are not admitted; no fast-completion promise is justified in that case.
Initial reference sound and production safety/stability are separate outcomes.

## Reference artifact and two runs — 2026-10-27

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
- Primary log: `/data/ps5-bt-hb-reference/hearbridge.log`; immediate tone evidence
  is in the kit's `remote/`, capture evidence in `capture-run/`. The latter
  reported 1,405 successful reads, 13,259 Empty/over reads, 1,163 media packets,
  two media drops and 44 timed credit fallbacks. Capture cleanup returned all 0;
  HCI disconnect completion was not observed. Full format/ownership/quality
  acceptance and comparative performance remain unverified.

## Current console and retained evidence

- Last confirmed console: `192.168.1.111`, FTP 2121, PayloadManager 0.6.3 on HTTP
  8084. That historical configuration is not a future session's authorization.
- `ps5-bt-application-raw-openat` was removed after the reference upload verified.
  Its diagnostic remains **on hold**. Do not ask the user to run it by default.
  Its immutable kit is `build/ps5/raw-openat-window-kit-e328d85`; no run is known.
- Latest verified inventory has only `local/ps5-bt-hearbridge-reference` among
  PS5-BT slots; unrelated entries are unchanged. Both authorized runs exited.
  First-run private state is preserved at `/data/ps5-bt-hb-reference-tone-20261027`;
  the second remains at `/data/ps5-bt-hb-reference`. No bond file was downloaded.
  Do not blindly relaunch: fresh mkdir would fail and both run permissions are consumed.
- Standalone SBC passed 750 blocks. Capture-stream returned Empty. Application
  startup failed store prerequisites. Directory comparisons found relative-open
  EPERM and absolute-open success; none established Bluetooth transport.
- Native L2CAP returned errno 47. There is no admitted production connection
  backend; the isolated reference tone does not establish native PCM format or
  real credit ownership, which remain unknown.
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
