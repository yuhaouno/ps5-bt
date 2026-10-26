# TEMP: next PS5 test window — start here on the new machine

Prepared 2026-10-26. Keep this temporary handoff until the next window has a
reviewed outcome. English is canonical. Read `AGENTS.md` before changing code.

Window update, 2026-10-26: read [HW4-EVIDENCE.md](HW4-EVIDENCE.md) first.

Latest pending entry: **`ps5-bt-application-namespace`**, uploaded and fully
hash-verified from clean `e219c8d`; one PS5-BT slot, unrelated slots unchanged.
The kit is `build/ps5/namespace-window-kit-e219c8d`, and its separate log is
`/data/ps5-bt-namespace.log`. It has not been agent-launched. The operator will
launch it once on returning and report controller behavior; do not wait for an
intermediate reply or upload blocked profiles. See the exact identities and
[uploaded checkpoint](NAMESPACE-PROBE.md#uploaded-manual-run-checkpoint-2026-10-26).

The clean `20a5d66` six-item kit was built, uploaded and fully read back. The
retained generated-application run failed Store directory acquisition (errno 1).
Subsequent one-at-a-time setup runs rejected root protection; the clean `a3c2fc1`
schema-2 build reported only `PARENT_WRITE` (mask 4), with clean close. Native
metadata layout and the underlying authority remain unresolved.
The original-kit standalone SBC fallback passed 750 blocks with matching bytes
and digest. The Rust capture fallback completed one Empty read and clean cleanup;
no PCM format or playback follows. Each of these later runs was immediately
collected, with normal controller behavior reported before/during/after.
Control/observe's original outcomes remain overwritten/Unknown and were not
relaunched after setup rejection. Final remote inventory has zero PS5-BT slots;
unrelated slots and local kits/evidence were preserved at that checkpoint.
The next [read-only root/data consistency candidate](NAMESPACE-PROBE.md) is now
implemented. The operator authorized preparation and upload while away, with
manual launch on return. Use `tools/namespace_test_plan.json`, its own matching
kit and dedicated `/data/ps5-bt-namespace.log`; do not wait for an intermediate
reply to finish preparation. Run that entry only once, observe controller input,
then retain its log. Do not repeat setup or the application sequence without
resolving its prerequisite. The final task report records actual upload/hash
status; a source checkout alone does not establish remote installation.

The selected environment is PS5 Slim / firmware 13.40, PayloadManager 0.6.3,
Sony WH-1000XM4. Reconfirm the console and the user's current test window before
any console connection, upload or launch. Preparing this document and kit does
not execute a console test. The project remains Rust-first.

## What is ready, and what the test can establish

The six-item `tools/window_test_plan.json` selects independently built artifacts:

| Order | Candidate | What to learn | Internal bound |
| --- | --- | --- | --- |
| 1 | `application-setup` | Can this payload validate/create its fixed private store directory? | At most 32 counted operations plus four consume-once closes; no retries |
| 2 | `application-control` | Do reviewed imports, entropy, clock, FileStore, loopback listener and Rust loop work together? | 5 observed seconds, 20,000 turns, 10,000 waits, 64 clients |
| 3 | `application-generated` | Can the actual application schedule persistent target SBC from generated PCM? | 750 blocks / 96,000 stereo frames, exclusive 2-second work deadline |
| 4 | `application-observe` | Can the actual application repeatedly own/read/clean up native capture? | 100 ms settle, 10 ms cadence, exclusive 2 seconds, hard 256 calls; at most 190 regularly scheduled polls |
| 5 | `sbc-continuous` | Isolate codec/Rust wiring if application startup failed cleanly | 750 blocks, 2 observed seconds, synthetic nominal sample clock |
| 6 | `capture-stream` | Isolate the immediate single-read Rust/native boundary | One NOWAIT read, 2 observed seconds |

Items 5–6 are conditional isolation tests, not mandatory repeats after successful
application tests. Use a **15-second external observation ceiling for every
launch**. Native synchronous calls may exceed their requested bounds; there is
no claim that a timer can interrupt a blocked kernel call. Stop the sequence on
controller changes, timeout, missing lifecycle or uncertain cleanup.

There is no admitted usable Bluetooth connection backend or ready safe active-HCI
artifact. The prior native L2CAP socket returned errno 47. No candidate here pairs
or connects the XM4. Capture format and native timestamp units remain Unknown;
nonempty raw data cannot enable `NativeAudio`. Passing these tests narrows the
remaining work but does not establish headphone playback. The earlier historical
USB/capture/short-SBC evidence is in `docs/HW2-EVIDENCE.md` and
`docs/HW3-EVIDENCE.md`; do not repeat negative socket/protocol scans without a new
source-backed mechanism.

## Fastest route: transfer the prepared package

Use a macOS or Linux testing machine with Python 3.11+. The verified portable
package supplies the executable candidates, matching source/license/relink
bundles, project tools, plans and this document. Rust, Git, LLVM and an installed
SDK are unnecessary for running the already-built kit or collecting results.
Windows native permissions/collection have not been validated.

Transfer these **three ignored local output files** from the development machine:

```text
build/ps5/ps5-bt-window-transfer.tar.gz
build/ps5/ps5-bt-window-transfer.tar.gz.bootstrap.py
build/ps5/ps5-bt-window-transfer.tar.gz.sha256
```

The final response supplies the actual archive SHA-256. Retain that hash through
a trusted channel; a sidecar alone does not authenticate modified Python code.
On the receiving machine, from a directory containing the three files:

```sh
python3 ps5-bt-window-transfer.tar.gz.bootstrap.py inspect \
  ps5-bt-window-transfer.tar.gz --expect-sha256 TRUSTED_HASH
python3 ps5-bt-window-transfer.tar.gz.bootstrap.py extract \
  ps5-bt-window-transfer.tar.gz --expect-sha256 TRUSTED_HASH \
  --destination "$PWD/ps5-bt-window"
cd ps5-bt-window
python3 tools/console_window.py --preflight \
  --kit build/ps5/next-window-kit-20261026
```

Replace `TRUSTED_HASH` with the 64-digit hash, not the literal placeholder. Use a
new destination under a real existing directory, with no symlinked ancestors.
Extraction rejects traversal, links, altered inventory and overwrites. Read
`START-HERE.md`, `HANDOFF.json`, and the kit's `RUNBOOK.md` after extraction. The
last command above is an offline dry-run. The extracted source has **no `.git`**;
never manufacture original Git provenance or modify the supplied manifests.
See `docs/WINDOW-HANDOFF.md` for full transfer bounds.

Git does not contain the ignored ELF/SDK/kit outputs. A source clone alone does
not transfer these three files. If they cannot be transferred, use the rebuild
route below before starting the console window.

## Alternative: fresh clone and one local build

Use Git, Python 3.11+, Make, a host C11 compiler, `patch`, `ar`, `nm`, pinned Rust
1.90.0 and **LLVM 18.1.8**. The LLVM directory needs clang, ld.lld, llvm-readobj,
llvm-objdump, llvm-ar, llvm-nm and llvm-config. Follow `docs/TOOLCHAIN.md` for the
reviewed installation/source requirements; LLVM version 18 alone is insufficient.
Do not copy an old Mac's `/private/tmp` tool path as a new-machine prerequisite.

```sh
git clone --branch feat/mvp-bringup https://github.com/yuhaouno/ps5-bt.git
cd ps5-bt
git status --short
git log --oneline --decorate -n 20
rustup toolchain install 1.90.0 --profile minimal --component rustfmt --component clippy
rustup target add x86_64-unknown-none --toolchain 1.90.0
export LLVM_CONFIG=/absolute/path/to/LLVM-18.1.8/bin/llvm-config
python3 tools/prepare_ps5_window.py --doctor --llvm-config "$LLVM_CONFIG"
python3 tools/prepare_ps5_window.py --build --llvm-config "$LLVM_CONFIG" \
  --kit-dir build/ps5/next-window-kit-20261026
```

Use the final pushed commit (or the original HEAD recorded in the handed-over
kit); verify it against the final response. Do not mix revisions. The doctor is
read-only and does not install tools. Explicit `--build` may fetch only reviewed,
hash-pinned public SDK **source** when not already cached. It performs no console
operation. Existing output paths are refused; use a fresh kit name if necessary
and pass that same name in all later commands. An inherited nondefault Cargo
target directory is not needed; the builder explicitly selects the reviewed
project target directory.

The preparer builds each artifact and **immediately packages its exact matching
source** before another feature overwrites the shared Rust archive. It then
assembles/revalidates the six-item kit and writes an adjacent preparation identity
record. All must report success; a partial build is not a runnable complete kit.
Do not run another builder concurrently. A timeout/interruption can retain the
preparation lock and report command cleanup Unknown. Preserve that identity and
inspect the recorded process/work before removing only that owned lock; never
retry over possibly running compiler children. See WINDOW-PREPARER.md.
No SDK/proprietary binary/toolchain is
bundled. See `docs/WINDOW-PREPARER.md` and `docs/SOURCE-DISTRIBUTION.md`.

## Console setup and upload — only in the newly confirmed window

The reviewed helpers are fixed to `192.168.1.111:2121`, anonymous FTP and
`/data/pldmgr/payloads/local`. Reconfirm this is the intended console. Ports
9120/9180 are ps5upload services, not raw elfldr; do not send raw ELF bytes to
9120. There is no requirement to install another loader or exploit.

Prepare/confirm these PayloadManager **local** slots with the source-reviewed
UI or finite terminal recipe in [PAYLOADMANAGER-SLOTS.md](PAYLOADMANAGER-SLOTS.md).
Use the actual installed manager dashboard URL and verify version 0.6.3 first.
The pinned source uses port 8084; neither it nor historical 9180 is assumed live.
The manager uploads the exact ELF and creates its full metadata using `mode=new`;
it refuses an existing slot. Inspect conflicts before choosing an exact replacement.
Do not use Keep both. The FTP uploader below only handles existing slots:

| Slot | Exact filename |
| --- | --- |
| `ps5-bt-application-setup` | `ps5-bt-application-setup.elf` |
| `ps5-bt-application-control` | `ps5-bt-application-control.elf` |
| `ps5-bt-application-generated` | `ps5-bt-application-generated.elf` |
| `ps5-bt-application-observe` | `ps5-bt-application-observe.elf` |
| `ps5-bt-sbc-continuous` | `ps5-bt-sbc-continuous.elf` |
| `ps5-bt-capture-stream` | `ps5-bt-capture-stream.elf` |

Each slot's bounded `meta.json` must declare `schema: 2`, `kind: "local"`, and
its exact `filename`; this lists checked fields, not a complete installation
schema. A missing/mismatched slot is a prerequisite failure before upload.

If all six were imported through the manager UI/API, skip the FTP replacement
and go directly to read-back hash preflight. For already prepared exact slots,
the optional existing-slot FTP replacement workflow is:

```sh
# Offline upload plan; inspect the six intended slots and hashes.
python3 tools/mvp_test_kit.py --kit-dir build/ps5/next-window-kit-20261026 --deploy
# In the current authorized window only: upload; this does not launch.
python3 tools/mvp_test_kit.py --kit-dir build/ps5/next-window-kit-20261026 --deploy --execute
# Read back exact remote ELF hashes before manual launches.
python3 tools/console_window.py --kit build/ps5/next-window-kit-20261026 \
  --preflight --execute --verify-hash
```

SIZE alone is insufficient; read-back hash must match. A transfer failure is not
a native-code failure. None of these commands chmods directories, changes driver
state or launches a payload.

Launch **only `application-setup` first** in PayloadManager. It uses the actual
payload UID and no-follow held descriptors to validate the fixed parents and
create only missing `/data/ps5-bt` and `/data/ps5-bt/control` at 0700. It never
chmods/chowns an existing path or changes another payload. Existing protected
parents may be root-owned; the final control directory and newly created
components must be exact 0700 and payload-owned. A restrictive inherited umask,
unsupported `mkdirat`, native permission/sync failure or unsafe existing path is
reported, not repaired by guessing. Created directories remain on failure.
Do not treat FTP-visible paths or host `mkdir` as proof of native protection.
See `docs/APPLICATION-SETUP.md` before handling a failure.

## Run, observe and collect one item at a time

1. Check wireless controller input before launch; record what was actually seen.
2. Manually launch the selected exact slot in PayloadManager; observe controller
   input during it. Start the 15-second external observation ceiling immediately.
3. Confirm final return; check controller input afterward. If cleanup/return is
   uncertain, stop and preserve the log. Do not repeat automatically.
4. Collect **before launching anything else**: every payload replaces the same
   `/data/ps5-bt-hw1.log`. Never infer freshness from its timestamp or identity alone.

Example for the first item, after a genuinely observed run with normal controller
input at all three stages:

```sh
python3 tools/console_window.py --kit build/ps5/next-window-kit-20261026 \
  --collect --candidate application-setup
python3 tools/console_window.py --kit build/ps5/next-window-kit-20261026 \
  --collect --candidate application-setup --execute \
  --run-observation observed \
  --controller-before normal --controller-during normal --controller-after normal
```

The first command is dry-run; the second reads only the fixed bounded log and
writes unique private raw-log/result files in ignored `build/ps5/console-window`.
Use truthful `changed`/`Unknown` observations when applicable. Replace only the
candidate name for the next launch. Keep every report path and hash. Collect an
abnormal/incomplete log too; do not overwrite it with an unobserved rerun.
If the helper fails, retain its redacted error and collect the fixed log through
the existing supported read-only FTP workflow; do not substitute arbitrary paths.
No bearer, bond key, raw capture sample or private log belongs in Git.

Proceed from setup success to control, generated and observe. Expected outer
`payload_return status=2` means the unchanged Bluetooth adapter is Unsupported;
the matching inner report determines whether this particular experiment passed.
A setup result needs stage 6, all four verified/opened/closed bits, zero errors
and both import phases closed. Generated application success needs 750 discarded
blocks, 57,750 bytes, prefix `9cfd20cb`, digest `7784d1fb`, exact summary matches
and fresh completion before its deadline. Raw observation may validly finish
with Empty-only reads; that is lifecycle evidence, not captured PCM success.
Never promote format, native units, lost-sample counts or Bluetooth ownership
from these reports.

## Decide the next direction from the evidence

Read `docs/PS5-WINDOW-DECISION-MATRIX.md` for exact import indices and failure
branches. Fix a concrete failed stage before proposing another experiment.

- Setup/import/entropy/store/listener/clock failure: retain full primary and
  cleanup reports. If cleanup is certain and the controller is normal, the
  independently selected codec/capture fallback can isolate the application
  prerequisite; it does not repair it.
- Generated application passes: target continuous SBC and actual application
  pacing are demonstrated for this configuration. Continue with capture evidence
  and transport bring-up; no need to retest the old four-frame codec self-test.
- Capture produces data: preserve numeric status/length/cadence. Source/layout
  review and a separately admitted format experiment still must establish sample
  type, layout, channels and rate before the Rust normalizer/encoder can consume it.
- All local tests pass: transport ownership/native channel availability remains
  the connection blocker. Prepare the next finite source-backed transport
  experiment; do not create guessed credits, competing readers or radio resets.
- A later transport experiment proves an owned route and actual allowance: connect
  the existing Rust discovery/pairing/SDP/AVDTP path and test a generated SBC tone
  first. Only after negotiated media and audible tone, verified capture format
  can enable captured game/menu audio. Exact delivery time depends on this result;
  there is no justified hour/day promise before a usable route is demonstrated.
- If the route needs system-driver mutation, a dispatcher or competing reads,
  document the concrete mechanism and bring that scope decision to the user.
  Do not silently expand scope to make the test appear successful.

## Handoff to the next agent or chat

Start in this repository or the extracted source root. Suggested prompt:

> Read AGENTS.md, docs/TEMP-PS5-TEST-HANDOFF.md,
> docs/PS5-WINDOW-DECISION-MATRIX.md, docs/APPLICATION-SETUP.md and the kit RUNBOOK.
> Verify the kit's original HEAD and hashes. The confirmed environment is Slim /
> 13.40, PayloadManager 0.6.3, WH-1000XM4. Ask only for the current execution window
> if it has not been confirmed in this new session. Run the six-item plan one
> candidate at a time, with conditional fallbacks, manual launches and truthful
> controller observations. Collect before the next launch; stop on uncertain
> cleanup. Keep Rust mainline, unknown format/transport/allowance gates and no
> deployment outside current authority. Use actual reports to choose the next
> bounded experiment. Do not promise pairing/playback from local-only success.

The final development-turn response records the actual pushed HEAD, completed
checks, final local package location and archive SHA-256. Generated manifests
record their source HEAD; this tracked document deliberately avoids a self-hash
or a fabricated future build identity. Local checks cannot validate PS5 imports,
permissions, durable storage, capture format, timing, headset behavior or controller
coexistence. Earlier full `make check` remains unavailable on ARM macOS at the
unchanged x86 syscall execution fixture (errno 86); do not call it a pass.
