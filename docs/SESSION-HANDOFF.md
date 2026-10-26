# Active hardware window and durable handoff

Updated: 2026-10-25. Read AGENTS.md and docs/MVP-STATE.md first.

## Current authority, 2026-10-26

The user currently has no PS5 environment; all hardware windows below are
historical and do not authorize a new connection, deployment or run. Continue
MVP implementation with PS5 validation pending. Functional, quality, latency,
coexistence and stability acceptance remains on PS5; do not require a Linux
Bluetooth backend. The current AGENTS.md governs project-wide scope over older
research-stage sequencing: scoped reviewed C protocol reuse, independent
experimental capture, local bond storage, minimal pairing/device controls and
fault-episode recovery are permitted. Isolated source-backed transport
experiments may measure unresolved facts under their documented admission
plan, without implying production support or a competing-reader/timer-credit
exception. This documentation change adds no live backend or hardware result.

## Latest: resumed LAN window and four completed candidates

This section supersedes the historical offline-only status below. The user
explicitly renewed hardware access from the PS5 LAN and authorized tests.
Target remains `.111`; FTP 2121 and the existing service at 9180 responded.
Only the four PS5-BT diagnostic slots were created/uploaded. Each uploaded ELF
was read back and SHA-256 verified; the user launched each manually and
confirmed completion with normal wireless controller behavior. Logs were saved
before the next launch. See HW3-EVIDENCE.md for exact identities and raw logs.

- USB binding, clean `942eb9d`: active configuration succeeds; driver names
  remain Unknown for all four interfaces, three other opens fail, cleanup
  succeeds, final 1 records incomplete inventory. HCI remains Unsupported.
- Settled capture, clean `942eb9d`: one read after 100 ms returns 8,192 bytes;
  all lifecycle/cleanup statuses 0, cleanup_uncertain=0, final 2.
- Generated-signal SBC, clean `34ea256`: two lifecycles/four 77-byte frames,
  prefix `9cfd206f` and digest `14e0ec79` exactly match host; codec/cleanup 0,
  final 2. No hardware encode duration was measured.
- Capture observation, clean `34ea256`: immediate Empty, one wait, then 8,192
  bytes; clock calls return 0, last-read-start elapsed 104,093 us; clean cleanup,
  final 2. Format and timestamp units remain unknown; no samples were retained.

New reviewable commits: `1745e97` media send orchestration, `b787f39` bounded
observation core, `6acedd9` deterministic codec self-test core, `f8a4bb8`
freestanding SBC headers/report, `a79bc77` first nonempty hardware evidence,
`3581c2e` replaceable target codec diagnostic, `9b9c8b4` binding gate update,
and `34ea256` independently selected bounded observation ELF.

The final trace-format fix preserves decimal read ordinals above 9; the earlier
one-digit formatter affected only later observation attempts, not the two-read
hardware result. Capture artifacts rebuilt after this fix have different
bytes and remain unexecuted until matched to a fresh log. Original tested
`942eb9d` and `34ea256` artifacts keep their recorded acceptance.

The complete local suite passed after the SBC integration; the observation
integration additionally passed strict/sanitized lifecycle/runner/main tests,
clock-regression cancellation, source-package self-tests and actual ELF links.
The workspace has 113 ordinary Rust tests plus eight feature-on codec checks,
208 encoder and 12 decoder vectors. Two independently rebuilt target codec
archives replacement-link to identical self-test ELF bytes. Extracting the
`34ea256` SBC corresponding-source package into a separate temporary project
also rebuilt the exact tested ELF. The package contains source/notices,
application objects, Rust/default codec archives and working replacement script.

Preserve original tested packages/manifests separately from final-HEAD rebuilds.
Rebuild all four candidates and packages after this documentation commit, then
assemble the ignored kit without editing manifests. Rebuilt artifacts do not
inherit hardware results unless the exact tested bytes match. Temporary workers
are complete; do not assume they or this hardware window remain active in a
future session without checking current state and user availability.

Remaining path: native capture format/timestamp evidence, measured encode
duration, Classic connection/pairing/key lifecycle, AVDTP reassembly/binding,
sample/credit pacing and actual LAN listener/page. The source-proven single-
owner HCI broker/ACL-credit contract is still the activation blocker. There is
no ready active-HCI/headphone candidate; do not invent a broker, competing USB
reader, reset or timed credit refill to bypass it.

## 2026-10-25 relay completion

The new lead completed and committed nine reviewable increments after
`ba93307`; the latest offline protocol wave adds:

- `6a3cfca`: bounded authenticated LAN request/status contract with one
  serialized command ticket; no socket listener, page or PS5 operation.
- `1a94fac`: acceptance-confirmed outbound L2CAP-to-HCI ACL fragmentation that
  retains a partially sent PDU until completion or explicit terminal/epoch
  cancellation; no capacity discovery or credit ownership.
- `875d99d`: bounded A2DP Audio Sink SDP discovery with exact UInt16 service
  semantics, epoch/deadline correlation and finite continuation history.
- `e55f2e3`: bounded AVDTP initiator session for conservative SBC discovery,
  configuration and lifecycle transitions; fragmented signaling, live media
  binding and hardware use remain absent.

The preceding relay increments were:

- `0e32102`: allocation-free Classic L2CAP Connection, MTU Configuration and
  Disconnection signaling with an epoch-bound initiator channel and six
  scripted-peer tests.
- `0c2ecf5`: `bt-a2dp` complete-frame SBC-over-RTP packetization plus a real
  host path from 128 stereo PCM frames through 77 SBC bytes, 90 RTP/A2DP bytes,
  94 L2CAP bytes and 98 synthetic ACL bytes.
- `4ef1d00`: post-HW2 HCI ownership audit. The executed native query returned
  -1 and FreeBSD-shaped SDK socket headers still do not prove a PS5 broker,
  ACL-send route, bounded dispatcher or credits. Active transport remains
  Unsupported; the binding diagnostic is the next executable evidence step.
- `40b6127`: clean-manifest hardware-test kit assembly and guarded FTP upload.
  The plan pins `.111`, binding before settled capture, 15-second external
  ceilings and log/controller stop rules. It is dry-run by default and never
  launches a payload.

Last green checks at `e55f2e3`: `make check`, `make check-c-sanitize`,
`make sbc-codec-test`, `make check-build-tools`, the `bt-a2dp` freestanding
check, the `bt-avdtp` freestanding check, and Git diff/format checks. The
workspace suite has 104 ordinary Rust
tests; the host codec feature adds eight Rust checks plus 208 encoder vectors,
12 decoder checks, sanitizers, forbidden-import inspection and replacement
archive relinking. These are host/build results, not PS5 playback evidence.

The binding and settled-capture candidates, manifests and corresponding-source
packages were rebuilt from the clean kit commit and assembled under ignored
`build/ps5/mvp-test-kit/`. Rebuild them once more if HEAD changes after this
handoff update; never edit a manifest. The kit's `kit-manifest.json` is the
authoritative final hash list. Run `python3 tools/mvp_test_kit.py --deploy` for
a network-free dry run. Only after the user confirms the console window and the
two exact PayloadManager slots exist with matching schema-2 metadata may the
lead consider `--deploy --execute`; it uploads only and still does not launch.

The temporary collaboration workers from these protocol turns are no longer
live. Their changes were reviewed and corrected by the integration owner before
the commits above; the final AVDTP audit reported no blocking issue. Do not
assume `/root/acl_fragmenter`, `/root/avdtp_session`, `/root/hci_ownership`,
`/root/l2cap_signaling`, `/root/rtp_packetizer` or `/root/sdp_discovery` can be
resumed. The two older local worker chat IDs below remain historical references.

Remaining offline critical path: Classic baseband connection, explicit pairing
and key storage; AVDTP signaling reassembly and live media-channel binding;
transport pacing; reviewed target libsbc archive/self-test and its relink
distribution materials; experimental capture format/timestamp adapter; and the
bounded LAN listener/static page wired to actual owner state.
HCI activation still requires a source-proven single-owner dispatcher and the
one-command/credit acceptance experiment in NATIVE-HCI-RESEARCH.md. No raw USB
reader, timed credit refill or guessed socket broker is an alternative.

## Historical offline authority (superseded by renewed window above)

The morning hardware window ended, after which offline development resumed.
The later explicit LAN message renewed access and produced the results above.
Parallel work remains authorized for the scoped MVP.
Target is **192.168.1.111**, never the historical .10 address.
Prioritize finishing diagnostics/build tests and the real Rust-to-SBC offline path.
That baseline is complete at `62e3c39`. The user has now requested a new lead
chat to continue implementation and prepare the 2026-10-25 PS5 window. Read
MVP-NEXT-RUN.md for the authoritative relay plan and worker continuation roster.
Future sessions must check actual user availability instead of treating this
historical plan as a continuously open hardware window.
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
That codec commit is `62e3c39`. Clean-source diagnostic/source-package builds
passed there; USB/capture-once bundles independently reproduced identical ELF
bytes. The rule/relay documentation update comes after that build, so rebuild
manifests from the new clean HEAD before packaging/deployment.

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

Next offline deliverable: bind the completed SDP, L2CAP, AVDTP and RTP pieces
through an explicit paced media orchestration path, while separately advancing
baseband/pairing, target SBC and capture-format evidence. Keep the eventual
minimal LAN page thin and honest about implemented states. The next hardware
window still needs driver-binding metadata and one settled capture read first;
nonempty PCM format/timing and HCI routing/credit ownership remain separate
blockers.
