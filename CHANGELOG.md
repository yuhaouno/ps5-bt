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
