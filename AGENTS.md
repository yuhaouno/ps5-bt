# PS5-BT working rules

## Priority

- First reproduce a real headset connection and audible audio on the user's PS5; optimize or innovate afterward. Do not expand infrastructure while transport is blocked.
- Prefer reviewed HearBridge/other upstream C transport and connection code. Keep working C until hardware evidence justifies replacing it; Rust remains preferred for portable code, not a prerequisite for the first audible result.
- Prepare an isolated upstream reference separately from the product. Do not merge a whole upstream application or assume its reported hardware results transfer. Preparation does not authorize execution of excluded mechanisms below.
- Freeze directory probes, richer UI, generalized recovery and unrelated codecs unless a specific first-audio failure requires them.

## Efficient workflow

- Read this file and `docs/TEMP-PS5-TEST-HANDOFF.md` first. Read only documents/code relevant to the change; historical reports are evidence, not a mandatory rereading list.
- Inspect Git status and recent history; preserve unrelated work. One owner integrates, commits and deploys. Parallel agents are authorized only for independent useful work; no routine extra review agent.
- Make the smallest runnable change, then test the actual target. Do not split one hypothesis into many payloads or add diagnostics that cannot change the next decision.
- Run relevant existing checks once. Documentation-only edits need diff/link checks, not code builds. `make check-fast` is a cheap Rust smoke check; select the changed module's tests explicitly. Full `make check` is for broad integration, milestones or diagnosed regressions, not every edit.
- Add tests for meaningful new behavior, failures or resource bounds; avoid duplicate wrappers and exhaustive combinations without a concrete need. Keep existing suites available on demand; test count is not progress.
- Keep one concise current handoff. Update README for changed capabilities/build steps and changelog/roadmap for milestones; do not duplicate every run across many documents.
- Before a coherent Conventional Commit, inspect staged diff/status and relevant results. Commit intended changes; no build outputs, secrets or unrelated files. No push or release tag without direction. Report outcome, checks, remaining hardware work and short Git status.

## Evidence and execution bounds

- Pin upstream revision, selected files, licenses, attribution and local changes before reuse; retain required notices/corresponding source. Preserve historical hardware evidence and original diagnostic bounds.
- Existing ABI and recovery contracts in `docs/ARCHITECTURE.md` and relevant module docs still apply. Unknown APIs/layouts/offsets, PCM formats and measurements stay Unknown. Host tests/builds are not PS5 acceptance. Document unsafe lifetime/buffer/layout invariants; preserve errors.
- Keep production transport single-owner with known handles and real credits. Never invent allowance or refill it from elapsed time. Lost control traffic or unreconciled credits require observable failure/reconciliation; recovery is finite and cancellable.
- A separately selected source-backed experiment may investigate unresolved routing/ownership. State mechanism, known prerequisites, hypothesis, finite traffic/time budget, stop and exactly-once owned cleanup. Do not require its intended observation as a prerequisite.
- Competing USB readers, timer credit refill, radio resets, system-process/driver mutation, detach or dispatcher installation need an explicit separate scope decision before execution. Faster iteration alone is not that decision.
- Own-process credential/syscall permission changes may use only the installed loader's existing kernel access, as needed for Bluetooth/capture; review exact touched structures/offsets and narrow/restore where feasible. No new exploit chain, DRM bypass, Sony binaries/firmware/protected keys or game content.
- Keep bond keys and secrets out of Git/logs/LAN responses. Any enabled LAN mutation needs authentication and cross-site protection; unavailable operations stay unavailable.
- Console actions need a currently confirmed window and execution authority; do not reask during the same authorized session. Preserve artifact/source identity, verify uploads, collect immediately after each run, and observe wireless controller before/during/after. Stop on timeout, controller changes or uncertain cleanup; never repeat a negative run without a new hypothesis.
