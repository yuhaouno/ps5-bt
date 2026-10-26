# Prepare a fresh testing-machine PS5 window

`tools/prepare_ps5_window.py` turns the reviewed plan into a complete local kit
with one build command. It validates the clean source commit and pinned tools,
builds each candidate, packages its matching source immediately, then assembles
and revalidates the final kit. It never connects to a console, deploys, launches
or performs native setup. All PS5 execution and acceptance remain separate.

## Prerequisites and read-only doctor

Use a fresh clone of the current committed branch, Git, Python 3.11+, a host C11
compiler, Rust 1.90.0 and LLVM 18.1.8. The LLVM directory must contain clang,
ld.lld, llvm-readobj, llvm-objdump, llvm-ar, llvm-nm and llvm-config. Existing
source/toolchain reviews remain in [BUILDING.md](BUILDING.md) and
[TOOLCHAIN.md](TOOLCHAIN.md).

If needed, install the exact Rust toolchain and target before preparation:

```sh
rustup toolchain install 1.90.0 --profile minimal --component rustfmt --component clippy
rustup target add x86_64-unknown-none --toolchain 1.90.0
```

Then inspect the preparation without building or provisioning anything:

```sh
python3 tools/prepare_ps5_window.py --doctor \
  --llvm-config /absolute/path/to/LLVM-18.1.8/bin/llvm-config
```

Omitting both `--doctor` and `--build` selects the same dry-run. It reads local
Git/plan/tool state and prints a JSON plan of exact commands. It creates no
output directory, compiles nothing, fetches no SDK source and performs no console
network operation. Rust validation uses installed `rustup run 1.90.0` without
`--install`, plus installed-target/component queries, so a missing toolchain
fails with remediation rather than being downloaded implicitly. LLVM resolution
and version checks reuse the existing builder/codec checks. A wrong/missing tool
or target is a blocked preparation, never a partial success claim.

The default plan is `tools/window_test_plan.json`. It includes the reviewed
next-window application/setup and standalone diagnostic profiles selected there.
For the broader existing diagnostic inventory, explicitly select:

```sh
python3 tools/prepare_ps5_window.py --doctor \
  --plan tools/mvp_test_plan.json \
  --llvm-config /absolute/path/to/LLVM-18.1.8/bin/llvm-config
```

Candidate flags/stems come from the authoritative `package_source.VARIANTS`
inventory. The preparer does not substitute a hand-maintained set of three app
profiles or guess a new payload flag. It validates plan IDs, artifact/source
paths, finite candidate count and the existing fixed-target kit plan contract.
A newly reviewed setup candidate can be selected only after its builder/package
profile exists. Preparation still does not execute that native setup operation.

For the admitted setup predicate follow-up, select only that candidate and keep
the old six-item kit intact:

```sh
python3 tools/prepare_ps5_window.py --build \
  --plan tools/setup_test_plan.json \
  --llvm-config /absolute/path/to/LLVM-18.1.8/bin/llvm-config \
  --output-dir build/ps5/setup-predicates-001
```

This uses setup ABI 2/log schema 2 from the committed source, with the same
fixed-path native call/protection bounds. It does not prepare five other remote
slots or launch anything. Verify the resulting identity, deploy only setup,
and collect the manual run before selecting a next item. See
[APPLICATION-SETUP.md](APPLICATION-SETUP.md#admitted-schema-2-follow-up).

## One local build command

After the dry-run passes, explicitly build all selected materials:

```sh
python3 tools/prepare_ps5_window.py --build \
  --llvm-config /absolute/path/to/LLVM-18.1.8/bin/llvm-config \
  --output-dir build/ps5/window-testing-machine-001
```

This creates the fresh private preparation namespace with:

```text
build/ps5/window-testing-machine-001/
  preparation-started.json
  identity.json
  kit/
    kit-manifest.json
    test-plan.json
    RUNBOOK.md
    <candidate-id>/...
```

Use a fresh name for every preparation. If `--output-dir` is omitted, a new
random `build/ps5/window-<id>` namespace is selected and printed. To choose the
exact kit path instead, use `--kit-dir DIR`; the kit is assembled at DIR and its
identity records go beside it under `DIR-preparation/`. These options are mutually
exclusive. Existing directories/files and symlinks are refused. In-repository
outputs must be Git-ignored; an output directory cannot make a previously clean
source tree dirty and then invalidate source packaging.

Each candidate executes this strict sequence before moving to the next:

1. The existing `tools/build_ps5.py` builds its explicit independent variant
   with the loader-kernel-resolver setting and reviewed toolchain.
2. The existing `tools/package_source.py` immediately validates and packages that
   exact manifest, Rust archive, source/licenses and any required SBC replacement
   materials.
3. Only then may the next variant overwrite Cargo's shared Rust static archive.

The final command is the existing kit assembler with the selected plan and fresh
kit directory. The preparer calls the existing `load_kit` validation afterward,
requires the expected source HEAD and writes its artifact identity summary.
It does not duplicate native compilation, ELF inspection, source/license checks,
LGPL replacement rules or kit hashing in another build implementation.

Builds use the existing canonical ignored `build/ps5` and `target` locations;
the fresh namespace holds the selected kit and preparation identity. A local
exclusive lock at `build/ps5/.prepare-window.lock` serializes preparers because
the intermediate Rust archive is shared. Run no competing manual build against
those intermediates. The helper never breaks another lock. A stale lock after a
process crash must be inspected against its recorded PID/source HEAD before
manual removal; there is no automatic takeover or invisible retry.

Commands run in their own POSIX process group on the supported macOS/Linux
workflow. On timeout or keyboard interruption, the helper requests termination
of the owned group and bounds direct-process reaping. This cannot prove that no
descendant escaped the group or remains in a native operation. It therefore
**retains the preparation lock** after timeout, interruption, signal termination
or another unknown command completion, even when group termination appears to
succeed. The failure identity records `command_cleanup: "Unknown"` and
`lock_retained: true`; no next preparation may start. Inspect the recorded PID
and build descendants and reconcile shared `build/ps5` / `target` outputs before
manually removing that lock. Do not use another build as cleanup proof. Normal
completed nonzero commands still stop the sequence and release the owned lock.

## SDK selection and offline boundaries

Without `--sdk-source`, explicit `--build` lets the existing builder fetch only
missing immutable, SHA-verified SDK source/header pins into its ignored cache.
That source download permission is distinct from console access: no console
operation is performed. Dry-run does not touch that provisioning path.

To use already available matching SDK source, provide the same override to every
build/package operation:

```sh
python3 tools/prepare_ps5_window.py --build \
  --llvm-config /absolute/path/to/LLVM-18.1.8/bin/llvm-config \
  --sdk-source /absolute/path/to/pinned-sdk-source \
  --output-dir build/ps5/window-testing-machine-002
```

An explicit SDK override is verified locally against the union required by the
selected profiles and is never repaired by downloading files. The preparer
preserves the builder's original SDK/header notices and source-distribution
obligations. Generated SBC candidates still require their separate target codec
archive and working replacement/relink source materials. No Sony binary,
firmware, captured PCM or bond record is obtained by this preparation.

`LLVM_CONFIG` and `PS5_SDK_SOURCE` environment defaults are honored and recorded.
Arguments are passed as subprocess arrays, so spaces in tool/SDK/output paths do
not become shell syntax. The preparer makes a finite ordered invocation list,
with no retries. Each non-query command has a finite local timeout. Missing
prerequisites stop preparation; they do not expand the platform runtime scope.

## Identity, failures and handoff

The source must be clean and committed at entry and before every command. The
same HEAD and unchanged selected plan are required throughout. If another process
edits/commits source or replaces the plan, preparation stops before packaging or
assembling mismatched materials. A failed build/package command stops the list
immediately. A successful prefix is recorded as failed preparation, without an
assembled-success claim or automatic resume.

`preparation-started.json` is an immutable initial record. `identity.json` records
ready or failed status, original Git HEAD, plan hash, exact commands/completed
prefix, tool identity and validated kit entries when complete. Both use private
atomic no-clobber JSON writing with file/directory fsync. A write/fsync failure is
not confirmed durability even if a private file has become visible. The command
exits 0 only for a passing dry-run or a fully validated ready kit; blocked/failed
preparation exits 1. Treat a failed namespace as evidence to inspect, not as a kit
to deploy, and use a fresh namespace after resolving the failure.

Share the complete validated kit, its identity summary, current source commit
and runbook with the testing machine/operator. Source packages already accompany
each ELF, including relevant notices and replacement materials. No binary output
is staged or committed by this helper. It never runs Git add/commit/push, FTP,
SSH, deployment helpers or a produced payload. A local link/source/kit result
is not PS5 functionality, audio quality, coexistence or hardware acceptance.
The actual window, hardware facts, existing slot metadata, fixed log collection
and explicit controller observations still belong to the separate console
runbook and [CONSOLE-WINDOW.md](CONSOLE-WINDOW.md).

Run local regression checks with:

```sh
python3 tests/tools/test_prepare_ps5_window.py
```

Tests use temporary real Git repositories, real plan/profile and kit validation,
local fake-version tool executables and mocked build/package subprocess work.
They cover sequential immediate packaging, source/plan changes, failure stops,
namespace/lock safety, path arguments, dynamic profiles and pinned-tool rejection.
A bounded real synthetic parent/grandchild test verifies group termination after
timeout and refusal of a racing retry; interruption/unknown-completion cases
verify retained-lock evidence. No target mass build, SDK download or console
connection is performed by them.
