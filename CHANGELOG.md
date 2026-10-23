# Changelog

## Unreleased

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
