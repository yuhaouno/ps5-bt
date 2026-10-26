# Repository development rules

PS5-BT is a working title for an independent Rust-first Bluetooth Audio stack.
English is canonical. Read README.md and the applicable architecture, upstream,
ABI, build and testing documents before changing those areas.
The current rules here govern development scope. Dated research/handoff reports
and restrictions for a specific diagnostic describe that snapshot/artifact,
not permanent bans on other reviewed implementations. Update conflicting
normative documents together; preserve historical evidence and existing
diagnostic bounds. Documented implementation limits and build checks remain
real until the implementation and its checks are deliberately updated.

## Git discipline

- Inspect `git status` and `git log --oneline --decorate -n 20` before changes.
  Preserve existing history and unrelated user work; do not reinitialize Git.
- Commit each independently reviewable change with its directly related tests
  and documentation. Use meaningful Conventional Commits in English. Do not
  create empty commits, unrelated bundles, or unfinished-code progress commits.
- Before every commit, inspect the staged diff and status, format changed code,
  and run relevant checks/tests/builds. Validate the staged state when later
  untracked or unstaged files could mask a missing dependency. Check that no
  build output, secret, proprietary binary, firmware or unrelated file is staged.
  A coherent candidate may be committed with PS5 validation pending; record
  unavailable checks and never present that state as hardware acceptance.
- Update README.md when capability, platform/codec support, build/install steps
  or major architecture assumptions change. Maintain CHANGELOG.md and ROADMAP.md
  with meaningful milestones. Do not advertise unimplemented features.
- Bootstrap may use main. Later use short-lived branches where useful. Do not
  create release tags without demonstrated milestones or push without direction.
- Finish substantial work with intended changes committed. Report final
  `git status` and `git log --oneline --decorate -n 20`, checks passed/unavailable,
  remaining uncommitted files and hardware verification still required.

## Engineering and scope

- Rust remains the preferred owner of portable protocols and audio policy.
  Scoped, source/license-reviewed C protocol or connection modules may be used
  as transitional implementations behind a documented, versioned ABI. Record
  their ownership, bounds, retained behavior and replacement/retention rationale;
  do not import a whole application or assume its hardware results transfer.
  Keep valuable PS5-specific C adaptation. Do not rewrite for language purity.
- Offline audio buffering, sample-clock pacing, recovery simulations and codec
  boundary work may advance independently of hardware transport bring-up. Keep
  production HCI/audio activation gated on the relevant ownership and ABI
  evidence; separately selected experiments follow the admission rules below.
  The current MVP authorizes L2CAP channels, SDP discovery, connection/pairing,
  AVDTP/A2DP/RTP, capture adaptation, target SBC integration, deployment helpers
  and a thin LAN control/status page. Implement and test these independently;
  unresolved hardware evidence does not block offline implementation. Do not
  expand into additional codecs, a rich GUI or unrelated features.
  Do not guess HCI credits or claim simulation as hardware verification.
  Functional, audio-quality, latency, coexistence and stability acceptance
  remain on PS5. Keep development checks, but do not require a Linux Bluetooth
  backend or substitute a desktop demonstration for PS5 acceptance.
- Separate protocol integrity from media continuity. Lost control events,
  unknown handle ownership and unreconciled credits require explicit transport
  failure/reconciliation. Bounded PCM concealment, dropping expired unsent
  media before fragmentation and controlled rebuffer/reconnect may be used when
  their cause, bounds and outcome are observable. Preserve partially sent PDUs
  on a healthy connection. An explicit terminal transport failure or newer
  connection epoch may cancel the remainder with accepted-prefix accounting;
  cancellation never restores unknown credits. Never restore credits from
  elapsed time. Recovery is bounded and cancellable, not an invisible retry loop.
  Define retry/time budgets per fault episode, with observable waiting/exhausted
  states and a documented stable-success condition for resetting the budget.
  Do not reset budgets on every failed attempt or require a payload restart
  after every recoverable interruption. Stop/cancel must remain available.
- Keep transport single-owner; that does not prohibit separate capture/encode
  stages communicating through bounded queues. Fallible bounded initialization
  allocation is allowed; per-frame allocation is not. Bootstrap no-heap/no-SIMD
  flags are not permanent audio-engine rules: optimized modules need their own
  reviewed ABI, dependency and hardware-state validation.
  One payload owner alone does not establish coordination with the system
  Bluetooth owner. A native channel backend may leave HCI and credits under
  the system owner instead of forcing them through the raw-HCI ABI.
- Elevate this payload's own process credentials and syscall-origin permissions
  only by using kernel read/write the installed jailbreak/loader environment
  already provides, and only as far as owning the Bluetooth transport and audio
  capture requires. Document the touched kernel structures and offsets as unsafe
  invariants, mark them Requires hardware until verified, and narrow or restore
  scope where feasible. An environment that does not already grant such access is
  a documented blocker, not a reason to loosen these bounds.
  This does not authorize system-process/driver mutation, driver detach or
  dispatcher installation. A concrete proposal requiring those changes needs
  a separate scope decision; do not expand permissions speculatively.
- Do not develop, bundle or invoke a new exploit or vulnerability chain to obtain
  that initial kernel access, and no DRM/license bypass, piracy, Sony binaries,
  firmware, protected Sony/DRM keys or game content. Normal Bluetooth pairing
  may create and securely persist this payload's local bond keys for reconnect
  and forget-device operations. Never commit, distribute or expose bond keys
  through logs or the LAN page. Audit SDK/runtime behavior independently of its API.
- Before upstream reuse, inspect the license and record repository, exact
  revision, selected components, local changes and attribution. Keep integration
  separate from unrelated features. Preserve file-specific license obligations.
- Prefer safe Rust. Document unsafe invariants, enforce buffer/lifetime bounds,
  and preserve errors. Mark unverified claims Unknown, Untested or Requires
  hardware. Never invent APIs, offsets, compatibility or measurements.

## Active MVP execution

- Milestones gate activation, claims and acceptance, not all implementation.
  Experimental versioned adapters may be implemented behind explicit features
  after source/layout/lifetime review; do not freeze unknown audio units or
  replace the established diagnostic with an untested production backend.
- Admit source-backed transport experiments separately from production support.
  A candidate may measure routing, ownership or capacity that it does not yet
  prove; do not require the experiment's intended result as its prerequisite.
  Before execution, document the concrete mechanism and inspected API/layout,
  hypothesis, known prerequisites, unresolved facts, permitted commands/traffic,
  finite operation/time budgets, stop conditions and cleanup. Unknown fields
  remain unknown; ACL submission still needs an established owned allowance.
  Select a separate diagnostic feature/artifact, preserve the baseline, record
  all observed foreign/lost traffic and credit uncertainty, and check wireless
  controller operation before/during/after. A run must have a currently confirmed
  hardware window and execution authorization; an old window is not authority.
  This is not blanket approval for competing USB readers, timed credit refill,
  guessed APIs/offsets, radio resets or system-driver changes. A proposal relying
  on an otherwise excluded mechanism needs its own explicit scope decision.
- Prepare independently selectable, bounded hardware tests before the next
  console window. Every test needs a timeout/stop condition, exactly-once owned
  cleanup, artifact identity, durable outcome and controller-coexistence check.
  A bounded Empty-read observation is not an invisible retry or a PCM success.
- The minimal LAN page may be developed now. It must expose actual capability,
  discovery/device selection, pairing approval, connect/disconnect, start/stop,
  forget-device state and errors; these are MVP operations, not a rich GUI.
  Version and extend the existing command/request contract as needed; its
  current four routes are not a permanent scope limit. Unavailable operations
  stay unavailable. Protected local device management may expose the minimum
  device identity needed for selection; public status/traces remain redacted.
  Bound requests and resources, serialize commands through the single owner,
  validate state-changing requests against cross-site/unauthorized use, and
  expose neither keys nor arbitrary memory/file/command execution.
- The user authorized parallel agent work for this MVP. Use disjoint file
  ownership and one integration/commit/deployment owner; reuse known workers
  when useful, update the durable handoff before quota exhaustion, and do not
  assume an old hardware window or an idle worker is still active.
- Prioritize a usable conservative-SBC path and executable test candidates over
  repeated research-only reports. A real ownership/permission/ABI blocker must
  have concrete evidence and the next bounded experiment or missing prerequisite.
  Keep experimental admission distinct from production invariants; do not
  promise playback or improved continuity without a real PS5 headset test.
