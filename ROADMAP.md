# Milestone roadmap

Milestones are evidence gates, not dates or promises. A compiling archive, a
linked ELF, execution on a PS5, and stable audio are different achievements.
The current dependency graph, worker ownership and blockers are tracked in
[MVP-STATE.md](docs/MVP-STATE.md).

Current wave: software-side HW-1 artifact complete; physical PS5 access is
available again. **Next action: HW-1 physical validation** of the instrumented
diagnostic, following [HARDWARE-TEST.md](docs/HARDWARE-TEST.md). Do not start
HW-2 or infer hardware success from desktop tests. Slim / firmware 13.40 remains
the unverified first target. See [wave evidence](docs/WAVE-1-REPORT.md).

| Milestone | Deliverable | Exit evidence | Current state |
| --- | --- | --- | --- |
| M0 — research/bootstrap | source/license inventory, architecture, bounded ABI, desktop tests, freestanding Rust archive | reproducible host checks and artifact inspection; explicit unknowns | bootstrap implemented; validation below |
| M1 — Rust on PS5 / controller information | audited C startup, Rust entry, minimal metadata backend, bounded HCI queries only after routing gate | real console log with build hashes, USB IDs, HCI/LMP version, actual commands/features/buffers where accessible; clean return and DualSense before/during/after | diagnostic ELF linked; ownership and console execution unverified |
| M2 — platform transport and capture | safe command/event/ACL transport, vendor detection, capture open/read/close | fault/cleanup tests, no hidden loss, proven native traffic routing, capture format/timing report | planned |
| M3a — minimal A2DP tone | Rust L2CAP/SDP/AVDTP/A2DP/RTP, explicit pairing/key storage, conservative SBC | reproducible 48 kHz stereo tone on named headset/controller/firmware, correct credits and negotiated limits | planned |
| M3b — captured audio | PS5 PCM through bounded queues to the demonstrated sink | game/menu/silence tests, capture discontinuity and queue telemetry, clean stop | planned |
| M4 — stability | bounded reconnect, starvation/overflow handling, diagnostics | staged 1 h then 8 h playback per declared target; no crashes or monotonic memory growth; no sustained underruns; headset power cycles, pad reconnects, Wi-Fi/RF load, cleanup logs | planned |
| M5 — latency/quality | measured SBC HQ/XQ experiments, pacing/buffer tuning, Game/Balanced/Hi-Fi profiles | reproducible full-path latency distributions and encoder/queue metrics; headset interoperability and regression comparison | planned |
| M6 — codecs | consider AAC and other codecs separately | implementation license/patent review, throughput/CPU/latency data and hardware demonstration before any support claim | research only |
| M7 — adaptive engine | telemetry-driven buffer/bitrate/pacing control | deterministic replay, fixed-policy baseline, stability bounds/hysteresis, controlled loss/coexistence trials | deferred |

M4 targets are proposed acceptance criteria, not measured results. Define the
underrun and memory thresholds with a baseline before starting each soak run;
publish raw counts and exceptions. A failed native-controller restoration or
unreconciled credit test blocks promotion regardless of how good audio sounds.
Fat, Slim and Pro become supported individually through evidence, not together
through a model-name conditional.

First approachable work: review source permalinks and license annotations, add
malformed-event fixtures with provenance, review translation status guides, or
record read-only hardware descriptors using the documented test procedure once
the runtime is reviewed. Do not open artificial issues for each empty crate.

Initial host validation: four Rust parser tests, 15 C/Rust seam scenarios, C stub
contract checks, fmt/check/Clippy and the freestanding archive build/inspection.
Current local checks: 34 Rust tests, descriptor/native-query C tests with ASan/UBSan, 15 ABI
scenarios, fmt/check/Clippy and archive inspection pass. The minimal PS5 ELF
links and passes structural/failure-path checks; no console execution is verified.
