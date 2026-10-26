# Current execution handoff — first audible audio

Updated 2026-10-27 after the user's priority correction. This is the current
execution queue; it supersedes earlier diagnostic sequences and pending-run
instructions. Read `AGENTS.md`, then only the sources needed for the next change.

## Next candidate: bounded queue drain

The user requested the shrink fix and preparation of the next test, confirming
devices remain available. The new `--queue-drain` build layers an original ABI-v1
policy and three-file adapter over the unchanged pressure-v1 artifact sources.
It waits for two free slots under a smaller target, at most 500 ms per episode;
retargeting/repeated requests cannot extend that deadline. Timeout or bad clock
applies the requested cap with the upstream drop behavior still observable.
No packet editing, trimming, credit changes or new radio/runtime mechanism is
introduced. Temporary retention can increase queue delay; hardware must measure
the tradeoff. Full policy scope is in the [module README](../experiments/queue-pressure/README.md#bounded-queue-shrinking-candidate).

Kit: `build/reference/queue-drain-candidate`; ELF
`hearbridge/dist/PS5-BT-HearBridge-drain-1.3.6.elf`, 727,184 bytes,
SHA-256 `ce20c0a8253c95470461bd2d9f27416ce770b14a09006bd8e1bf0f206ab3644e`.
Fresh-source target build and 71-import check passed. The actual upstream queue
admission/order/expiry regression and policy bounds pass strict warnings plus
ASan/UBSan, including from the freshly extracted candidate. Existing connection
switch and slow/fast channel-configuration checks pass (zero failures).
No hardware run yet.
Use the same 30-second capture-first and 180-second overall cooperative bounds,
preserve the completed pressure state and verify upload before any launch.
Observe whether a deferred shrink actually occurs, its elapsed time and cap,
sender/audible drops, backlog/bitpool and controller operation before/during/after.
Keep the missing disconnect-completion limitation; no superiority claim follows
from one clean run. Readiness means current audible PS5 content, explicit XM4
pairing mode and normal wireless controller input.

## Queue-pressure comparison result — 2026-10-27

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
media / 180-second cooperative overall bounds. The comparison is now complete:
one operator-reported dropout versus approximately two in the earlier baseline.
This is a short-run observation, not an established comparative improvement.

Candidate commit `f7768c1` is uploaded through PayloadManager 0.6.3 at
`192.168.1.111:8084`; FTP readback matches the hash above. The sole PS5-BT slot
is now `local/ps5-bt-hearbridge-pressure` (same ELF filename as above). The previous
PS5-BT reference slot was removed after verification; unrelated slots remain.
The completed capture state was renamed in place to
`/data/ps5-bt-hb-reference-capture-20261027`; its log hash matched the baseline,
and private bonds were not downloaded. Tone state remains in its earlier archive.
Preflight found no running reference/HearBridge process. Deployment evidence is
under `build/reference/queue-pressure-candidate/remote/`.

The initial launch was aborted before audio by an agent-side Python event helper
argument collision after selecting XM4. The stop path ran; no own ACL link was
established, USB cleanup calls were acknowledged and the process exited. The
operator confirmed normal controller input. Evidence is in `pressure-run/`;
remote state is preserved at `/data/ps5-bt-hb-reference-pressure-abort-20261027`.
The helper signature was corrected and call binding checked before the retry.

The operator authorized continuing and reconfirmed readiness. The corrected run
initially found no XM4; after the operator explicitly re-entered pairing mode,
one requested rescan found it. Tone was disabled before discovery/connection.
Immediate evidence is in `build/reference/queue-pressure-candidate/pressure-run-controlfix/`.

- The 30-second capture run ended automatically: 1,160 media packets, 11,253
  SBC frames, 1,405 successful reads, 13,132 Empty/over reads, **one sender drop**.
  The operator reported **one audible dropout, otherwise normal**, then explicitly
  confirmed the headset disconnected and the wireless controller remained normal.
- The original policy intervened three times: bitpool 45 → 41 → 37 → 33.
  The first two occurred before any sender drop; the third followed it. Queue
  drained afterward. Initial target remained 200 ms, but existing dynamic
  packet-count sizing produced nominal capacity durations up to 232 ms; neither
  end-to-end latency nor actual queue delay was measured.
- Last ACL counters: 1,217 submitted, 1,184 event credits, **28 timed fallbacks**,
  five outstanding in six-slot accounting, max queue nine and 17 stalls. These
  differ from the baseline's 44 fallbacks/23 stalls, but radio conditions and
  discovery differed; real shared allowance remains unproven.
- Capture Stop/Close/Terminate returned 0; USB cleanup calls were acknowledged,
  final status was stopped and the process was absent. HCI Disconnection Complete
  and AVDTP/L2CAP close responses remained unconfirmed, as in the baseline.
- Final log: 41,467 bytes, SHA-256
  `f72f71affad7c9f9d9ce70b174b534a0a0e6275df2e22489a6a0b78b570fcc04`.
  No bond files were collected. Current private state retains this completed run;
  preserve it before any later launch. No further launch is queued.

Next concrete hypothesis: the existing `tune_link` shrank the media cap from
10 to 8 immediately after rate reduction, with backlog already 8 and drops still
0. One drop appeared within the next 235 ms. Source inspection shows the setter
changes the cap without trimming; enqueue at capacity drops the oldest queued
whole media packet before admitting the new one.
Investigate bounded capacity reduction after drain, with an explicit delay bound;
do not claim this timing proves the drop's cause or silently increase buffering.

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
   The original two runs and the separately authorized optimization comparison
   above completed. Do not claim established ownership or safe coexistence.
2. The user heard corresponding game/menu audio with about two very brief
   dropouts; controller input was normal and the headset disconnected afterward.
   Sender accounting also reported two drops, but correlation is unproven.
   The new policy's first comparison is recorded above; follow the cap-reduction
   hypothesis next. Do not treat Empty reads as audio loss or timed event-credit
   estimates as actual controller capacity.
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
- Latest verified inventory has only `local/ps5-bt-hearbridge-pressure` among
  PS5-BT slots; unrelated entries are unchanged. All reference processes exited.
  Tone, baseline capture and aborted pressure state are preserved at the remote
  archive paths above. The completed pressure comparison remains at
  `/data/ps5-bt-hb-reference`. No bond file was downloaded. Do not blindly relaunch:
  fresh mkdir would fail; preserve evidence and confirm readiness for a later run.
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
