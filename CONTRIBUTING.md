# Contributing

Read the [mission/architecture](docs/ARCHITECTURE.md), [roadmap](ROADMAP.md),
[scope](SECURITY.md), and [upstream rules](docs/UPSTREAM.md) first. This is an
independent project. Prioritize correctness, PS5 stability, maintainability,
upstream compatibility and measurement before latency, quality, speed, Rust
purity or feature count. Keep valuable platform C behind the ABI.

Use English for source/comments/API docs, project docs, commit messages and
templates. Translations are welcome and must link to the authoritative English
README with an out-of-date notice. Initial translations should be short status guides.
Do not claim untested compatibility or add speculative abstractions/codecs.

Run `make check` before a PR. Describe the concrete problem, resulting behavior,
tests and evidence limits. Include parser/state tests for externally supplied
data and platform fault tests when adding C code. Unsafe blocks need a local
safety explanation; a comment does not substitute for a valid invariant.
Document any new ABI, dependency/license, state ownership, resource bounds and
hardware assumptions before implementation. Keep fixes small and reviewable.

For upstream adaptation, record commit, original license, selected files, credit,
relationship, local patches and sync procedure. Preserve original notices and
history. A C/Rust split does not change distribution obligations. Never commit
Sony binaries, secrets, raw pairing keys, copyrighted game/audio content or
exploit/privilege-changing code. Do not hide missing permissions with a workaround.

Approachable contributions include documentation source checks, meaningful
malformed-packet fixtures, translation review and sanitized hardware reports
following the gated procedure. Hardware is not required to improve the portable
core. No CLA, sign-off requirement or contact address has been invented for this
bootstrap; governance can be established openly with maintainers later.

## Git and commit discipline

Inspect `git status` and `git log --oneline --decorate -n 20` before changing
files. Preserve existing history and unrelated user work. Do not reinitialize
an existing repository. The initial bootstrap may use main; use short-lived
feature branches when they help review, without creating branches for ceremony.

One independently reviewable change belongs in one meaningful Conventional
Commit. Keep implementation, its tests and directly related docs together;
separate unrelated features, refactors, build changes, upstream integrations,
translations and documentation milestones. Do not create empty commits or use
vague messages such as "update", "changes", "wip" or "fix things". Write commit
messages in English and describe the actual result. Do not mention automated
coding tools unless the change is specifically about those tools.

Before each commit:

1. Inspect the intended diff and staged paths. Stage explicit files; exclude
   unrelated changes, build products, credentials and prohibited content.
2. Run formatting, static checks, relevant tests and supported builds. For Rust
   changes use `cargo fmt --all -- --check`, `cargo check --workspace --all-targets
   --locked`, `cargo test --workspace --locked` and
   `cargo clippy --workspace --all-targets --locked -- -D warnings`, or `make check`.
3. If later unstaged/untracked files could conceal a missing dependency, export
   the index to an isolated directory and validate that staged snapshot. Keep
   intermediate workspace states buildable and tests with the feature they verify.
4. Run `git diff --cached --check`, inspect `git diff --cached` and `git status`,
   and commit only a coherent working state. Document unavailable PS5 checks;
   never imply that host tests verified hardware.

Update README.md when user-visible capabilities, platform/codec support,
build/install procedures or major architecture assumptions change. Maintain
CHANGELOG.md and ROADMAP.md alongside meaningful features and milestones.
Documentation is committed work. Do not tag a release before its milestone has
been demonstrated, and do not publish a premature v1.0.0.

At the end of a substantial session, commit intended completed work and report
`git status` plus `git log --oneline --decorate -n 20`. Summarize commits,
checks passed, checks unavailable, remaining uncommitted files and why, and
hardware verification still required. A finished feature includes its relevant
tests, documentation and clean commit. Do not silently commit another person's
changes or fabricate successful builds, compatibility or latency measurements.
