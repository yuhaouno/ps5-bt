# Next PS5 window: readiness and remaining gates

The newer [temporary new-machine handoff](TEMP-PS5-TEST-HANDOFF.md) and
[six-item default plan](../tools/window_test_plan.json) supersede the three-item
preparation sequence below. Existing artifact records remain historical; use
the final freshly assembled kit's source identity and hashes. See
[WINDOW-PREPARER.md](WINDOW-PREPARER.md), [WINDOW-HANDOFF.md](WINDOW-HANDOFF.md) and
[APPLICATION-SETUP.md](APPLICATION-SETUP.md) for the added offline preparations.

Updated 2026-10-26. Planned environment confirmed by the user: PS5 Slim/13.40,
PayloadManager 0.6.3, WH-1000XM4. There is no currently confirmed online window.
This document prepares execution; it does not authorize deployment or launch.

Existing commits through `5acae45` were pushed to `origin/feat/mvp-bringup` and the
remote ref was read back. Subsequent offline implementation commits are local.
Rust continues to own audio policy, scheduling, protocols and application logic.

## What is ready to test

| Order | Candidate | Procedure | Passing evidence establishes |
| --- | --- | --- | --- |
| 1 | application-control | Native import/entropy preflight, protected store, loopback listener, Rust owner loop, finite stop and cleanup | This selected native startup path executed; no radio/audio support |
| 2 | application-generated | Same startup, actual clock pacing, persistent target SBC, 750 generated blocks to hash/discard | Correct bounded local encoding and scheduling, including a fresh post-discard completion observation |
| 3 | application-observe | Same startup plus raw capture, 100 ms first settle, 10 ms cadence, exclusive two-second interval, 256-call ceiling | Repeated raw capture procedure and its counters/cleanup; no PCM format, sample count or native-time-unit verification |

The generated profile expects 57,750 bytes, prefix `9cfd20cb`, FNV-1a
`7784d1fb`, no concealment/rebuffer/expiry and confirmed completion before its
exclusive two-second work deadline. Counts without that final observation are
incomplete timing evidence. Each audio quantum takes a fresh clock observation;
old pre-network/pre-encode timestamps cannot admit late work. The portable loop
uses those validated observations for its global deadline and subsequent Stop
budget, including failure cleanup.

Raw observation permits at most 190 scheduled polls under exact regular timing;
actual late scheduling can produce fewer, with no catch-up burst. Data, Empty,
Zero, Overrun and terminal outcomes remain distinct. An Overrun is an unknown
sample gap, not an inferred lost-frame count. A completed raw procedure, including
an Empty-only procedure, never supplies valid PCM to the encoder.

All application variants retain the independent global five-second, 20,000-turn,
10,000-wait ceiling and consume-once cleanup. Synchronous native calls cannot be
preempted by these budgets; use the separate 15-second external observation
ceiling and preserve an incomplete log if a call does not return.

The scoped native rebuild library has actual host FileStore/listener close/reopen
checks under one persistent recovery budget. It is not an automatic reconnect
installed in these loops, and there is no separate rebuild console payload.

## Prerequisites before these application runs

- Confirm the console is currently available and the intended run is authorized.
- Prepare the already-existing `/data/ps5-bt/control` directory with exact mode
  0700 and the payload's native owner identity. The payload validates it; FTP
  visibility does not prove its protection. No candidate creates or relaxes it.
- Prepare the three matching local PayloadManager slots. Use the exact names and
  filenames in the kit runbook. The helpers do not create slots or guess metadata.
- Upload the exact selected ELF files only after reviewing the deployment dry-run
  and receiving current deployment authorization. The upload helper never launches.
- Verify remote bytes and save each run's fixed diagnostic log before the next
  launch truncates it. Check wireless controller behavior before/during/after.

Missing/unsafe store paths or missing native exports are concrete startup errors,
not Bluetooth connection failures. Native export availability, entropy, filesystem
protection/durability, scheduling and cleanup still require this console evidence.

## Local kit and commands

After the final documentation commit, the clean source revision is rebuilt into:

- `build/ps5/next-window-kit-20261026`: the three application items above.
- `build/ps5/window-inventory-kit-20261026`: all nine independently selectable
  candidates, including existing standalone fallbacks and their source packages.
- Baseline ELF/source also remain under `build/ps5/`.

Use the kit manifests for exact source identity and hashes. Both generated-codec
application and standalone codec packages include replacement/relink materials.
Historical kits retain their original identities; never relabel their manifests.

Offline review, with zero console connections or output writes:

```sh
python3 tools/console_window.py \
  --kit build/ps5/next-window-kit-20261026 --preflight
python3 tools/mvp_test_kit.py \
  --kit-dir build/ps5/next-window-kit-20261026 --deploy
```

The second command is a deployment **dry-run**. Explicit execution is a separate
step after the current console window and deployment scope are confirmed.

During an authorized window, after uploading, read-only verification is:

```sh
python3 tools/console_window.py \
  --kit build/ps5/next-window-kit-20261026 --preflight --execute --verify-hash
```

Launch one item manually in PayloadManager, observe it, then collect before
launching another. First inspect the collection request offline:

```sh
python3 tools/console_window.py \
  --kit build/ps5/next-window-kit-20261026 \
  --collect --candidate application-generated
```

See CONSOLE-WINDOW.md for explicit read-only execution and the actual observed
controller/run flags. Unknown remains Unknown. The helper neither witnesses a
run nor proves log freshness merely from a matching build ID. It retains partial,
mismatched or invalid logs privately without overwriting prior evidence.

## Interpret results before continuing

| Result | Next action |
| --- | --- |
| Missing export or clean private-store rejection | Save the exact import/resource stage, raw return and cleanup; repair that prerequisite before another application run |
| Correct generated frame summary but missing final time observation | Keep incomplete; do not call pacing verified |
| Clean generated workload with exact summary | Advance to raw observation; this is not Bluetooth transmission |
| Repeated nonempty raw capture | Retain byte/status/cadence evidence; native PCM format/time units remain Unknown |
| Clean application setup failure | Independently selected continuous-SBC or one-read Rust capture may still isolate unaffected wiring; review the failure before selecting a fallback |
| Cleanup uncertainty, missing final lifecycle or changed wireless input | Stop the test sequence and reconcile; do not use another launch as cleanup proof |

The existing standalone continuous-SBC and one-read Rust-capture candidates do not
need the new application store/listener preflight. Their separate source packages
and limits are in the full inventory. They are diagnostic fallbacks, not permission
to continue after uncertain cleanup. The already-negative native L2CAP socket test
is not on this default test list and should not be repeated without new evidence.

## What even three passing runs do not solve

There is currently **no usable admitted PS5 Bluetooth connection backend and no
ready active-HCI artifact**. The tested `socket(36,5,135)` branch returned
`EAFNOSUPPORT`/errno 47 on this environment. This is a negative result for that
specific call path, not proof that all possible PS5 Bluetooth services are absent.
A different source-backed native channel/broker ABI or a transport ownership
mechanism is still needed. A system-managed channel could retain HCI credits in
the system; a raw-HCI backend needs actual owned allowances. Neither is obtained
from elapsed time, descriptor topology or successful local encoding.

Native capture format is also unverified. Raw-byte observation cannot set
`HardwareVerified`, infer frame layout or enable NativeAudio. An inspected format
contract and corresponding console verification are still needed. No current
candidate automatically closes that format gate.

After those facts exist, discovery, pairing, SDP, AVDTP, SBC/RTP and delivery must
be connected to the actual backend and tested with the headset. The portable
implementations and offline fixtures reduce that work but do not replace it.
Therefore this kit supports prompt, reproducible testing; it cannot guarantee
that all three tests passing leaves only a small adjustment before headphones
connect or play. Further backend/format implementation may be required.
