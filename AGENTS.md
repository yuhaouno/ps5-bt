# Repository development rules

PS5-BT is a working title for an independent Rust-first Bluetooth Audio stack.
English is canonical. Read README.md and the applicable architecture, upstream,
ABI, build and testing documents before changing those areas.

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
- Update README.md when capability, platform/codec support, build/install steps
  or major architecture assumptions change. Maintain CHANGELOG.md and ROADMAP.md
  with meaningful milestones. Do not advertise unimplemented features.
- Bootstrap may use main. Later use short-lived branches where useful. Do not
  create release tags without demonstrated milestones or push without direction.
- Finish substantial work with intended changes committed. Report final
  `git status` and `git log --oneline --decorate -n 20`, checks passed/unavailable,
  remaining uncommitted files and hardware verification still required.

## Engineering and scope

- Rust owns portable protocols and audio policy. Keep valuable PS5-specific C
  adaptation behind the small documented ABI. Do not rewrite for language purity.
- No codecs, GUI or full stack expansion before their authorized milestone.
  Prove controller capabilities and native traffic ownership; do not guess HCI
  credits, hide transport loss with retries or claim mock tests as hardware tests.
- Elevate this payload's own process credentials and syscall-origin permissions
  only by using kernel read/write the installed jailbreak/loader environment
  already provides, and only as far as owning the Bluetooth transport and audio
  capture requires. Document the touched kernel structures and offsets as unsafe
  invariants, mark them Requires hardware until verified, and narrow or restore
  scope where feasible. An environment that does not already grant such access is
  a documented blocker, not a reason to loosen these bounds.
- Do not develop, bundle or invoke a new exploit or vulnerability chain to obtain
  that initial kernel access, and no DRM/license bypass, piracy, Sony binaries,
  firmware, keys or game content. Audit SDK/runtime behavior independently of its
  API.
- Before upstream reuse, inspect the license and record repository, exact
  revision, selected components, local changes and attribution. Keep integration
  separate from unrelated features. Preserve file-specific license obligations.
- Prefer safe Rust. Document unsafe invariants, enforce buffer/lifetime bounds,
  and preserve errors. Mark unverified claims Unknown, Untested or Requires
  hardware. Never invent APIs, offsets, compatibility or measurements.
