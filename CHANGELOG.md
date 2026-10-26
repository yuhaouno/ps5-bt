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

- Record one Slim/13.40 standalone Rust/target SBC pass: 750 generated blocks,
  57,750 bytes, exact digest and clean finish. Record the Rust capture boundary's
  one Empty read and clean stop/close. Retain immediate per-run logs and normal
  operator-reported controller observations; physical pacing, PCM and playback
  remain unverified. Remove each completed test slot; final PS5-BT slot count is
  zero and unrelated slots are unchanged.
- Record setup schema 2's root `PARENT_WRITE` rejection (mask 4), with clean
  cleanup. Keep protected-store activation gated and identify a separate read-only
  root/data consistency probe as the next proposal; no permission exception or
  namespace mutation follows from the result.

- Preserve the observed root-policy rejection in the setup-only PS5 rerun.
  Extend its report to ABI 2/log schema 2 with six numeric predicate-failure
  bits, zero-initialized stat buffers and legacy-log collection support.
  Add a single-item setup plan for immediate per-run collection. Native calls,
  protection and cleanup bounds are unchanged; the subsequent result is recorded
  above.

- Record the 2026-10-26 application window: six built/imported/hash-verified
  candidates, retained Store directory I/O failure before generated encoding,
  overwritten per-item outcomes and operator-reported normal controller input.
  Remove all eight remote PS5-BT slots at the user's request; preserve the local
  kit/evidence and select setup alone for the next observed run.

- Prepare the next console window for a new testing machine: a six-item default
  plan, installed-toolchain doctor, sequential build/source packaging, verified
  portable transfer with standalone Python extraction, and matching read-only
  result collection. Keep a temporary operator handoff and source-backed slot
  preparation/decision procedures. All native acceptance remains on PS5.

- Integrate separate generated-SBC and raw-capture workloads into the actual
  native application startup, protected store/listener, observed clock and Rust
  loop. Generated mode schedules 750 blocks, validates the pinned joint-stereo
  summary and discards output; observation retains unknown format/time and
  bounded raw-byte results. No Bluetooth traffic or playback is enabled.

- Add scoped concrete NativeFactory close/reopen on the same stable storage,
  preserving one recovery episode, local epochs, partial-failure cleanup and
  explicit live stable-success evidence. Unknown cleanup blocks reopening.
  This does not install automatic application reconnect or a rebuild payload.

- Extend the original source/kit inventory to nine candidates and add a three-item
  startup-only plan for a fresh console window. Add default-dry-run read-only
  slot/artifact preflight and bounded log/result collection; no automatic setup,
  upload, launch or permission changes. Native private-directory and controller
  observations remain explicit prerequisites/evidence.

- Add a separate finite control-only application ELF assembling the actual Rust
  resource factory, protected store, persistent listener, fallible clock/wait,
  Application/Runner and startup owner. Use an existing private 0700 directory,
  ephemeral loopback bind and fresh unlogged authorization; keep radio, capture
  and codec operations unavailable. Preserve partial-startup/store-lease cleanup,
  bounded Stop/drain and prior client-close uncertainty. Native PS5 imports,
  entropy, protection and scheduling still require hardware acceptance.

- Add a selected-export POSIX bridge and exact SDK-header inventory for the
  native application candidate. Resolve the full table before descriptor use,
  preserve native/cleanup failures and generate ephemeral HTTP authorization
  from one bounded nonblocking entropy-device lifecycle with no token logging.
  Native compatibility/entropy remain unverified; no syscall/permission fallback.

- Add a concrete fixed-storage Rust PS5 resource factory around existing reviewed
  clock, private store, listener, capture and persistent SBC owners. Preserve
  primary and reverse-cleanup failures, explicit store handoff and per-mode
  native-format gates. No transport, command allowance or ACL credits are created.

- Add a persistent bounded listener service that accepts successive clients,
  reuses HTTP state without self-reference, preserves admitted command ownership
  across reply failure, and returns to accept after ordinary client errors.
  Stop precedes accept/read; total budgets and uncertain cleanup remain sticky.
  Preserve the existing socket-owning ConnectionPump API and authentication.

- Correlate new Rust diagnostic logs with an embedded build identity and unique
  entered/final markers. Collect bounded local JSON outcomes with explicit
  controller observations, exact profile/cleanup checks, interrupted-run detection
  and atomic private host persistence. Extend matching source/kit validation;
  no hardware execution or PS5 power-loss durability is inferred.

- Add a separately selected, fallible native monotonic clock/finite-wait adapter
  with cached exports, raw diagnostics, cancellation, checked deadlines and sticky
  clock/native errors. Each caller step requests at most one 1 ms wait; actual
  timing and native loop/readiness still require hardware evidence.

- Assemble actual Rust HTTP, management, owned input, audio and ACL lanes with
  per-turn budgets, global cancellation and observable reverse-order cleanup.
  Connect declared capture through real persistent SBC under Busy/credit/partial
  drain tests. Rebuild capture/encoder resources with newer epochs and the same
  fault episode; hand accepted retry tickets to the rebuilt application without
  spending twice. Native acquisition/readiness and PS5 acceptance remain pending.

- Add a separate experimental capture ABI and Rust owner with bounded reads,
  consume-once cleanup, raw-status preservation and sticky native cleanup
  uncertainty. Normalize explicitly declared i16/f32 inputs, preserve independent
  time/sample domains and retain encoder-block remainders. Build an independent
  one-read Rust/C target diagnostic; native format/time remain Unknown.

- Add independent target SBC linking and a persistent Rust `BlockEncoder`, with
  per-object ELF/import checks and external replaceable codec archives. Build a
  separate bounded 750-block generated-PCM Rust scheduler diagnostic with clock
  fault handling, exact bitstream checks and matching source/relink materials;
  PS5 execution remains pending and the short C self-test stays available.

- Record the discovery/management integration milestone and add one optional
  host candidate-check target covering native/external adapters, fault harnesses
  and page state. PS5 hardware gates and the diagnostic remain unchanged.

- Add a separately selectable nonblocking POSIX control listener with an opaque
  versioned ABI, one borrowed client, scoped SIGPIPE protection and consume-once
  cleanup. Exercise real loopback HTTP and idle-client timeout; target native
  compatibility and PS5 activation remain unverified.

- Join discovery, protected bond metadata, device/SSP management and application
  progression in a bounded Rust runner. Serialize control/media PDUs against one
  real external credit ledger, retain rejected input, enforce aggregate deadlines
  and revoke stale management state immediately on clock failure. No PS5 owner
  or credit authority is manufactured.

- Add the minimal LAN device/pairing page, authenticated v2 service and bounded
  nonblocking connection pump. Preserve v1, serialize urgent/primary tickets,
  reject stale generations and expose only actual capabilities and outcomes.
  Static assets are embedded; native PS5 activation remains pending.

- Implement a protected local bond file store behind a versioned opaque C ABI:
  private directory/record validation, exclusive ownership, atomic replacement,
  fsync boundaries, durable deletion and explicit uncertain outcomes. Separate
  native host builds from target-linked adapters; PS5 behavior remains unverified.

- Add bounded Classic Inquiry/Cancel device discovery with stable scan identities,
  validated EIR names, finite result/event limits and explicit cancellation races.
  Selection requires confirmed quiescence; no PS5 radio access is enabled.

- Join selected-device security, SDP/channel scheduling and SBC source/audio
  handoff in a caller-driven single Rust application owner. Gate traffic on
  security/binding loss, require explicit Start, support clean Stop/Start, retain
  total media/disconnect budgets and connect live retry/stable-success policy.
  Platform ownership, protected filesystem, native capture and PS5 acceptance
  remain pending; the diagnostic backend is unchanged.

- Add bounded continuous normalized stereo PCM-to-SBC scheduling, complete-block
  ticketed handoff, sample-clock pacing, retained backpressure and observable
  expiry/concealment/rebuffer. Exercise the real host codec across 750 blocks and
  2,250 completion-accounted synthetic ACL fragments; PS5 audio remains unverified.

- Schedule bounded SDP discovery/cleanup and separate AVDTP signaling/media
  channels under one Rust owner, retaining whole-PDU offers until acceptance.
  Preserve cleanup responsibility on late callbacks, bound peer-close replies
  and permit media reopen only with fresh local CIDs/transaction ranges.

- Add selected-device pairing/bond management over the owned Classic lifecycle:
  versioned protected records, Ready-gated saves, explicit SSP tokens, failed-save
  retry and forget revocation. Clock failure immediately revokes authority while
  preserving cleanup errors. No platform filesystem or inquiry is implemented.

- Preserve explicit PCM sample gaps through RTP wrapping and atomic media
  queuing; support clean stream restart after actual media-channel cleanup with
  a fresh local CID, retained transaction identities and unchanged credit ownership.

- Bind the selected-peer Classic Ready boundary, AVDTP SBC negotiation, explicit
  signaling/media channels and existing credit-aware media sender in a bounded
  source session. Require full signaling acceptance and Start acknowledgement;
  drain healthy partial PDUs before Suspend/Close, retain total stop deadlines,
  and preserve terminal uncertainty for the recovery controller. No live PS5
  transport, capture, encoder loop or audio/pacing claim is introduced.

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

### Offline fixed namespace preparation, 2026-10-26

- Add a separate source-backed application-setup candidate for the project's
  fixed private store namespace, with native UID/mode/inode checks, bounded
  creation/sync, raw reports and exactly-once cleanup. Existing permissions are
  never changed; unknown cleanup blocks reuse. Add strict/sanitized fault and
  actual entry orchestration checks, matching source packaging and result
  collection. Native behavior remains Requires hardware.
