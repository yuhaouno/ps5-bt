# Active hardware window and durable handoff

Updated: 2026-10-24. Read AGENTS.md and docs/MVP-STATE.md first.

## Current authority and priorities

The morning hardware window has ended. The user resumed offline development
after the usage reset. Do not connect to or deploy to PS5 without a renewed
hardware window. Parallel chats remain authorized for the scoped work below.
Target is **192.168.1.111**, never the historical .10 address.
Prioritize finishing diagnostics/build tests and the real Rust-to-SBC offline path.
No change was approved to the native HCI ownership/credit integrity gate.
Do not enable a competing-reader backend or timed credit refill.

## Confirmed access

- FTP: 2121, anonymous connection works. Use `curl --request LIST` for listings;
  this server rejects curl's `--list-only`/NLST with 502.
- ps5upload: 9120; its Web UI: 9180. These are NOT elfldr's raw ELF protocol.
- elfldr 9021 is not listening. Do not blindly send ELF bytes to 9120.
- PayloadManager local slots: `/data/pldmgr/payloads/local/`.
- Existing baseline slot: `ps5-bt-hw1/ps5-bt-hw1-v6.elf`, schema-2 meta.json.
- Each diagnostic replaces `/data/ps5-bt-hw1.log`; retrieve before the next run.
- SDK full source: `/tmp/ps5-bt-upstream.dpjS9B/sdk`, verified Git revision
  `a7685ce8e97ce1f8a4f38253a41d54241c9124f1`. Temporary path is not permanent.
- LLVM: `/opt/homebrew/opt/llvm@18/bin/llvm-config` (18.1.8).

## Parallel ownership (shared working directory; do not overwrite)

- Main chat: build integration, runtime, deployment, common docs, final commits.
- Capture chat `01a16bd9-053d-7013-b602-44d5be34bfc7`: exclusive
  `platform/ps5/diagnostics/capture_probe.{c,h}`,
  `tests/platform/capture_probe_test.c`, `docs/CAPTURE-PROBE.md`.
- Codec chat `01a16bd9-07f6-7b83-8038-6b4ddc7ac94d`: exclusive
  `third_party/sbc/`, `platform/codec/`, `tests/codec/`, `tools/test_sbc.py`,
  `docs/SBC-IMPLEMENTATION.md`, and `crates/bt-codec/`.
- USB subagent `/root/usb_probe`: exclusive `usb_probe.{c,h}` (optional core),
  its tests and docs/USB-PROBE.md.
- Workers do not stage/commit/deploy. Main must review all source/license/test
  evidence. No actual captured audio, addresses, pairing keys or proprietary
  modules enter source control.

## Resume procedure

1. Inspect status and the latest 20 commits; preserve concurrent changes.
2. Read this file and each worker's implementation/handoff document.
3. Inspect compact `wait_threads` snapshots for the two chat IDs above; do not
   create duplicates. User authorized messages to these tasks for coordination.
4. Use the local evidence under docs/evidence and build/ps5; do not assume the
   console is available. Rebuild diagnostics without deploying. A future live
   run needs a newly confirmed window and matching artifact/log provenance.
5. Review capture callbacks/module ABI before wiring any hardware operation.
6. Commit independently reviewable completed changes with tests/docs. Do not
   stage unfinished workers or package a dirty source tree as corresponding source.

## Latest completed baseline

HEAD before this wave: `1ca499f` (offline PCM queue and recovery), preceding
`d3f0a0b` (layered recovery policy), `7580087` (working diagnostic lifecycle).
44 Rust tests, four PCM simulations, `make check`, C ASan/UBSan and PS5 HW-1
build passed in the previous wave. No playable audio exists yet.

## Live results in this wave (read this first on resume)

- USB first run completed on .111. User replied "已运行，手柄无异常".
  Deployed ELF SHA-256:
  `f5abd0e7e8d119a5d67fda0b1dc82adc9d872d8fc5d23c41cf2d9958794b2ab9`.
  Raw log saved locally in ignored `build/ps5/hw2-usb-first.log`.
- Own code read `/dev/ugen0.2`, VID:PID `1286:2059`, config 1, total bytes
  `0x0167`, four interfaces. HCI-class interface 0 uses 81/01/82/02;
  interface 2 ALSO declares HCI class and uses 84/04/85/05. SCO interfaces
  1 and 3 each have alternates 0..5. This is a topology candidate, NOT proof
  of independent controller state, native ownership or available credits.
- Other actual nodes `/dev/ugen0.1`, `/dev/ugen1.1`, `/dev/ugen2.1` returned
  normalized -1 from open. Directory and acquired device closed with 0.
  Inventory is honestly incomplete and main final status is 1, not a startup
  failure; full lifecycle returned. No permission changes were needed.
- Capture preflight (traced) executed successfully. All seven exports resolved,
  Ipmi/Avcap2 module load and secondary results were 0/success. ELF SHA-256:
  `8b200c3771c3ffb9b783a3fc3aeb3563f5ebaa928de494ba52d6fbdb26a58a3c`.
- Capture-once (without settling delay) executed successfully:
  `ced89c5c3c9dcedd0a5a453e4ae129386d5cd2bac97a4ca3f6ca805a02c711bb`.
  Initialize/Open/Start/Stop/Close/Terminate all returned 0; immediate read was
  Empty `0x81950002`, zero bytes. User said everything was normal. This proves
  lifecycle, not PCM acquisition, format or playable audio. Local raw log:
  `build/ps5/capture-once-first.log`.
- Second USB driver-binding artifact SHA-256
  `1cf0c2395ff219ebadc7584c01789c49ddfdc3f38f0b98798d4fdb22646f71fb`
  was deployed. Its launch/result was NOT confirmed before quota exhaustion.
- The optional 100 ms capture settling change was interrupted after updating
  capture_probe.h, then completed and host-tested after the usage reset. Main
  supplies a checked libkernel usleep callback. It has never been deployed or
  run on PS5. Do not call a successful no-delay run a settled-read pass.
- Hardware user said ~40 minutes remained around 03:01 UTC (11:01 Shanghai).
  At 03:12 UTC they said quota is nearly exhausted: finish hardware tests now;
  offline development resumes in the afternoon. Do not start Web work now.
- The morning diagnostic builds recorded dirty shared-checkout state honestly.
  Do not rewrite manifests to claim clean committed provenance. Source packages
  need a later clean rebuild; integration status follows below.

## Offline continuation after usage reset

Both parallel chats completed their scoped work and are idle/frozen. The USB
subagent is also finished. Diagnostic work is integrated as `44c902c`; the
independent codec change follows in a `feat(codec)` commit. Inspect Git for
the final HEAD rather than assuming this document's earlier baseline is current.

- Capture: fail closed on nonzero module secondary results, distinguish no
  export lookup from a successful zero status, aligned scratch, exact optional
  100 ms wait, and consume-once cleanup; strict/sanitized host tests pass.
- USB: active configuration/binding metadata and local-error distinction are
  host-tested and target-compiled, not hardware verified. No active transport.
- Codec: real `PcmQueue` -> safe Rust -> C -> generic libsbc works on the host.
  Eight Rust checks, 208 encoder vectors, 12 decodes, sanitizers and replacement
  archive linking pass. The example emits one 77-byte joint-stereo SBC frame.
  Feature-on cross builds intentionally fail; no codec enters the PS5 ELF.
- Review `docs/SBC-IMPLEMENTATION.md` for caller-storage/unsafe invariants,
  source hashes, patches and remaining target/relink/distribution gates.
- Earlier dirty hardware builds retain their original identities. Clean
  rebuilt diagnostics/source packages do not inherit those hardware results.

Next offline deliverable: bounded SBC/RTP packetization with negotiated MTU,
sequence/sample timestamps and scripted rejection/stall tests, followed by
AVDTP session/signaling and SDP discovery/connection. Keep the eventual minimal
LAN control page thin and honest about implemented states. Next hardware window
needs driver-binding metadata and one settled capture read first; nonempty PCM
format/timing and HCI routing/credit ownership are still separate blockers.
