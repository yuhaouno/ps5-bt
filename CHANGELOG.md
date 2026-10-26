# Changelog

## HW-1 repair, 2026-10-24

- Execute the diagnostic on Slim/13.40: existing-loader kernel initialization,
  Rust entry, durable lifecycle, notification API success and final Unsupported
  status 2; native query executes but returns -1.
- Remove faulting executable-byte inspection; capture syscall carry errors with
  six-argument/stack/MAP_FAILED regression tests; tolerate missing stderr when
  durable logging works. Add explicit existing-loader own-process symbol lookup,
  without credential setters, stock CRT or patch.c. Earlier startup assumptions
  below are superseded by docs/HW1-REPAIR.md.


## Unreleased

- Add a bounded selected-peer Classic link/security candidate with exact HCI
  command acceptance, cancellation-race cleanup, stored-key lookup and explicit
  SSP approval. Require authentication, encryption and queried minimum key size
  before Ready; preserve failed acknowledgement routing fields and reject stale
  events. PIN/passkey/OOB pairing, inquiry and bond persistence remain outside
  this subset. No active PS5 backend or headset pairing is claimed.

- Add caller-storage AVDTP receive signaling reassembly and a packet-oriented
  initiator entry that shares existing semantic validation. Bound packet counts,
  SDU lengths and a fixed Start deadline; isolate channel/epoch/transaction
  generations and quarantine cancelled/expired assemblies. Preserve the legacy
  single-message API. This is development evidence, not PS5 interoperability.

- Add a caller-driven fault-episode recovery controller with bounded attempts,
  monotonic deadlines, capped backoff and stable-success budget reset. Correlate
  cleanup/reconciliation and attempt tickets, retain uncertainty after timeout
  or stop, and reject stale results. No automatic native reopen, credit refill
  or PS5 recovery result is introduced.

- Align project-wide development policy with PS5-only functional acceptance:
  allow coherent unverified candidates and scoped transitional C protocol reuse,
  separate source-backed transport experiment admission from production proof,
  and retain credit, native-traffic, lifetime and existing-loader scope bounds.
  Clarify fault-episode recovery, terminal partial-PDU cancellation, protected
  local bond-key persistence and minimal LAN pairing/device-management scope.
  Reconcile architecture, platform, security, contribution, build, testing and
  handoff guidance without changing runtime code or claiming new hardware results.

- Add an allocation-free ACL packet-reservation ledger with exact accepted and
  completed counts, strict epoch/handle/event validation and sticky terminal
  uncertainty. Integrate it with the retained media PDU sender and real host
  SBC example; Busy and credit exhaustion preserve fragment bytes. No timer
  refill, guessed controller budget or PS5 ownership claim is introduced.

- Execute the native L2CAP availability probe: UDP control creation/close
  succeeded, but Bluetooth creation returned -1 / errno 47 (`EAFNOSUPPORT`).
  Preserve the complete log and user-confirmed normal wireless input. Close
  this socket branch for the current process/environment; no permission change
  or headset connection follows from the result.

- Add a separately selected native L2CAP availability experiment: resolve the
  exact socket/close/errno ABI, create/close an IPv4 control socket, then one
  unconnected L2CAP socket. Preserve immediate errno, phase markers and cleanup
  uncertainty. No bind, connection, controller I/O or privilege change occurs;
  successful creation does not establish a usable Bluetooth route.

- Verify the target SBC self-test on PS5: two clean lifecycles and four frames
  match the host digest exactly. Verify bounded capture observation with one
  Empty, one wait, then 8,192 bytes using a successful monotonic-clock path;
  cleanup and user-observed wireless input remain normal. Preserve raw logs
  and exact artifact/source-package hashes in HW3-EVIDENCE.md.

- Add the independently selected capture observation ELF with a checked
  monotonic-clock adapter, immediate first read and explicit 100 ms waits only
  after Empty/Overrun. Stop at first data/error, 20 reads or a two-second
  deadline and preserve exactly-once cleanup, durable counts and raw errors.

- Link the generated-signal SBC self-test into an independently selected PS5
  ELF with durable numeric status. Build a generic static-only target archive
  using compiler resource headers, verify the exact external symbol set, and
  export application objects/source/notices plus a tested replacement/relink
  script. The baseline transport remains Unsupported.

- Record the renewed PS5 LAN window: read-only USB binding queries returned
  Unknown names with incomplete inventory; the single 100 ms settled capture
  read returned 8,192 bytes and clean cleanup. The user confirmed normal
  wireless input. PCM format/timing and Bluetooth playback remain unverified.

- Remove host SDK headers from the static-only SBC target build through a
  narrowly scoped freestanding-header patch; retain upstream algorithms and
  verify all reference vectors. Add an atomic bounded ASCII self-test report
  with raw stage/cleanup status and deterministic frame prefix/digest.

- Add a deterministic allocation-free SBC generated-signal self-test core. It
  performs two independent init/finish cycles, encodes four fixed PCM frames
  and verifies byte identity plus a pinned prefix/digest without retaining or
  reporting media. Host and freestanding target compilation pass; PS5 runtime
  linkage and execution remain separate gates.

- Add a caller-driven bounded capture observation state machine with one
  nonblocking read per step, explicit deadline/call/capacity limits, classified
  Empty/Data/Overrun/error outcomes and an exactly-once cleanup obligation. It
  does not yet select a PS5 clock/wait adapter or change capture-once behavior.

- Add a `no_std` bounded media send cursor that binds already-encoded SBC frames
  through RTP/A2DP, L2CAP and acceptance-confirmed ACL fragmentation. Failed
  sends reprepare byte-identical frames; partially accepted PDUs survive until
  completion, typed terminal failure or a strictly newer epoch. It owns no
  clock, controller credits or transport.

- Add an allocation-free bounded AVDTP initiator for Discover, Get
  Capabilities, Set Configuration, Open, Start, Suspend, Close and Abort, with
  conservative SBC selection, epoch-bound transactions and explicit state
  rollback. General Reject and reserved SEID/header bits are strict;
  fragmented signaling and live media transport remain unsupported.

- Add a bounded A2DP Audio Sink SDP discovery client with exact request
  attributes, per-request transactions, connection epochs, whole-exchange
  deadline, finite continuation history and atomic final service validation.
  Enforce UInt16 PSM/version/feature wire types; no live query is claimed.

- Add allocation-free outbound L2CAP-to-HCI ACL fragmentation with an explicit
  acceptance cursor, byte-identical preparation retry, PB=0/PB=1 framing,
  handle/epoch checks and durable accounting for terminal or epoch cancellation.
  It does not discover capacity, own controller credits or activate transport.

- Add a no_std bounded LAN control/status contract: strict authenticated HTTP
  routing for status/connect/start/stop, exact same-origin checks for mutations,
  one pending command owned by the Bluetooth loop, stale-completion rejection
  and atomic identity-free status JSON. No listener, page or PS5 operation is
  claimed.

- Complete the 2026-10-25 relay with durable commit/check/worker state, final
  offline kit rebuild instructions and the remaining protocol, target-codec,
  capture, control and HCI ownership gates. No console window or playback result
  is inferred from the date.

- Add a manifest-driven next-window kit for the binding and settled-capture
  diagnostics. Assembly requires clean-HEAD artifact/source identity; deployment
  is dry-run by default, explicit upload validates existing PayloadManager slots,
  and the helper never launches a payload or deletes the per-run log.

- Re-audit native HCI ownership after the executed `-1` query and own dual-HCI
  USB descriptors. Keep active transport Unsupported: FreeBSD-shaped SDK socket
  headers do not establish a PS5 broker, ACL-send route or bounded ownership.
  Select the read-only binding diagnostic as the next executable evidence step
  and record a strict one-command/credit acceptance experiment for any future
  source-proven dispatcher.

- Add allocation-free complete-frame SBC-over-RTP/A2DP packetization with
  negotiated MTU enforcement, explicit marker state, atomic backpressure and
  sequence/sample-timestamp wrap tests. Extend the real host codec proof through
  90-byte RTP, 94-byte L2CAP and 98-byte synthetic ACL framing without claiming
  a live AVDTP session or playback.

- Add allocation-free Classic L2CAP Connection, MTU Configuration and
  Disconnection signaling plus an epoch-bound initiator channel machine. Cover
  exact scripted-peer bytes, malformed commands, short-output atomicity,
  rejection, timeout/cancel, stale identifiers/CIDs and peer/local disconnect;
  live transport remains gated on HCI ownership and credits.

- Authorize parallel conservative-SBC MVP protocol/platform implementation and
  a thin LAN control page before live transport succeeds. Scope bootstrap-only
  allocation/SIMD rules correctly, permit explicitly experimental audio adapters
  before ABI freeze, correct stale hardware instructions, and record the next
  console-window acceptance sequence without promising unverified playback.

- Connect bounded PCM staging to a safe no_std Rust SBC owner and a separately
  rebuildable generic libsbc 2.2 archive on the host. Retain pristine LGPL source,
  exact hashes/notices and generic/caller-storage patches. Pass 208 reference
  encoder vectors, 12 decoder checks, sanitizers, replacement-archive linking
  and eight Rust checks. The real example emits 77 SBC bytes from 128 stereo
  PCM frames. PS5 ABI, live streaming and combined-ELF distribution remain gated.

- Add separate bounded USB metadata and capture preflight/one-read diagnostics,
  current-process module export lookup, durable phase edges, host fault tests
  and matching source-package variants. Pin the 34-header USB SDK dependency
  closure with retained notices and standalone provisioning.
- Record own Slim/13.40 USB enumeration, including HCI-class interfaces 0 and 2,
  and capture lifecycle success with one Empty read. Preserve three complete
  numeric logs and exact tested artifact hashes. The delayed-read and binding
  extension remain hardware-unverified; no live HCI or audible audio is claimed.

- Implement allocation-free PCM staging with absolute sample deadlines, atomic
  capacity/stale rejection, counted bounded silence, explicit rebuffer and expired
  unencoded-frame discard. Add ten media tests and four deterministic host-only
  recovery scenarios to `make check`; no codec, HCI transport or audio is enabled.

- Clarify layered media recovery: bounded PCM concealment, expired unsent-media
  discard and controlled rebuffer do not excuse HCI integrity loss or invent
  credits. Allow independent offline audio work and bounded initialization
  allocation. Record user-confirmed diagnostic notifications/wireless input
  and the subsequent 192.168.1.111 target; no new hardware test is claimed.

- Record the first PS5 Slim / firmware 13.40 load attempt: Payload Manager v0.6.3
  transferred the complete ELF repeatedly, while the payload produced no start
  notification or persistent log. Repair the over-constrained startup guard by
  accepting both contracts implemented by the selected SDK: a direct
  `sceKernelDlsym` resolver verified by identity, or the pinned `+0xa` syscall
  instruction without constraining libkernel's return/error path. The repaired
  hardware result remains pending.

- Add visible start/finish notifications, an explicit final status and a
  per-run persistent `/data/ps5-bt-hw1.log` to the minimal HW-1 diagnostic,
  with synthetic startup, channel-failure and request-layout tests. Hardware
  behavior remains unverified until the new ELF runs on the target console.

- Record a second macOS 27.0.1 arm64 reproduction of the HW-1 baseline ELF from
  a clean clone of `feat/mvp-bringup`. The documented toolchain produced the
  published SHA-256 with no repository changes. Physical validation and HW-2
  remain undone.

- Close the first software development wave at the HW-1 diagnostic artifact;
  independently rebuild it offline from packaged source with identical ELF
  bytes. Physical validation is pending; HW-2 and working audio are unverified.

- Add allocation-free SDP PDU/continuation framing and bounded typed data-element
  parsing; full service discovery and connection policy remain future work.

- Package matching committed project/SDK corresponding source and license
  notices offline, with clean-build/hash gates and deterministic archive tests.

- Add a separate opt-in diagnostic ELF for one read-only native stack-version
  query, raw failure reporting and main-hook tests; transport remains Unsupported.

- Produce an inspected PS5 diagnostic ELF through `make`, using an original
  minimal startup/linker script and three hash-verified SDK syscall source files;
  exclude stock permission-changing CRT and keep hardware support unverified.

- Add validated, codec-independent SBC parameter selections and frame-size
  calculations; audit libsbc licensing/runtime needs without importing it.

- Add bounded USB descriptor parsing and deterministic metadata reports with
  host regression tests and C sanitizers; no live USB operation is enabled.

- Add L2CAP basic framing and allocation-free ACL reassembly with explicit
  connection epochs, caller-owned storage and rejection-state invariants.

- Add allocation-free typed HCI command/ACL framing, controller capability and
  completion event decoding, and malformed-input/maximum-length regressions.
- Begin the dependency-aware MVP bring-up program with PS5 Slim firmware 13.40
  as an unverified hardware test target.

- Establish the independent project mission, English README and Git discipline.
- Record architecture, scope boundaries, pinned upstream research and licensing.
- Define evidence-based development milestones and hardware test gates.
- Add a pinned Rust workspace and portable no_std transport/clock/PCM contracts.
- Add HCI event framing/local-version decoding with synthetic malformed-packet tests.
- Define the small C ABI, safe Rust session/buffer wrappers and unsupported C adapter.
- Add host C conformance checks for stub outputs and unsupported operations.
- Add the C-callable Rust diagnostic bootstrap and 15 host C/Rust seam scenarios.
- Build/inspect the freestanding Rust archive with PIC and small code model.
- Add repeatable Make targets and an explicitly gated, untested PS5 SDK link recipe.
- Document atomic Conventional Commits, staged-state validation and session Git reports.
- Add contribution, conduct and issue/PR reporting guidance.
- Add eight concise translated status guides with English as the authority.
- No PS5 hardware, audio, headset or latency compatibility is claimed.
