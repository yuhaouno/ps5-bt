# Building the bootstrap

The public project/package name is not selected. Crate names are local and
unpublished. Builds never contact or deploy to a console. The PS5 output is a
real diagnostic ELF linking C -> Rust -> an Unsupported Bluetooth adapter, plus
separately selected capture, codec and native application workload candidates. The
explicit HW-1 variant has executed on Slim/13.40, but it does not provide working
Bluetooth or HCI controller capability information. The separate USB
diagnostic below has read device descriptors on one console.

These build/import constraints describe the selected diagnostic artifacts.
They do not prohibit independently developed source-reviewed experimental,
optimized or transitional C modules under [AGENTS.md](../AGENTS.md). Such modules
need explicit build variants and reviewed ABI/dependency/state contracts;
preserve the current baseline and its inspection checks. Candidates may be
committed with PS5 validation pending; building never authorizes deployment.

## Minimal PS5 ELF

Install Rust/rustup, a C11 host compiler (`cc`, or `HOST_CC`), Make, Python 3.11+
and LLVM **18.1.8** containing clang, ld.lld, llvm-readobj and llvm-objdump.
The checked-in [rust-toolchain.toml](../rust-toolchain.toml) pins **Rust 1.90.0**,
rustfmt, Clippy and `x86_64-unknown-none`. Install rustup using
[the official instructions](https://www.rust-lang.org/tools/install). These are
deliberate build baselines, not claims about the newest releases. Cargo downloads
toolchain components when needed; the workspace has no registry dependencies.
Do not change the pin to `stable` without reviewing the update.

Set `LLVM_CONFIG` to the LLVM 18.1.8 `llvm-config`. The documented macOS/Homebrew
path is `/opt/homebrew/opt/llvm@18/bin/llvm-config`; Linux installations may use
another path. Apple Clang alone does not supply this final-link baseline.
After setup, the single build entry is:

```sh
make
```

`make ps5-link` selects the same build explicitly. `LLVM_CONFIG` and optional
`PS5_SDK_SOURCE=/absolute/path/to/sdk` can be environment or Make variables.
See [TOOLCHAIN.md](TOOLCHAIN.md) for the exact observed host environment,
flags, synthetic tests and native tool provenance.

[tools/build_ps5.py](../tools/build_ps5.py) enforces the Rust/LLVM versions and
the pinned SDK inventory from v0.45, revision
`a7685ce8e97ce1f8a4f38253a41d54241c9124f1`: `crt/syscall.c`, `crt/syscall.h`
and `crt/payload.h` are the default link inputs. Seven additional files are
provisioned for the explicit loader-kernel-resolver variant below. The builder
verifies each file's SHA-256, including when using an
external checkout. Without an override it fetches missing files from immutable
revision URLs into ignored `.sdk/`. The release SDK binary archive is neither
required nor linked. No SDK installation, SDK libc/import library, proprietary
console binary or firmware is required by this build.

The builder runs synthetic host startup tests, builds the Rust static archive,
compiles the project C/assembly and selected syscall source, and links using the
original project [minimal.ld](../platform/ps5/bootstrap/minimal.ld). It writes
these ignored outputs under `build/ps5/`:

* `ps5-bt.elf`: x86-64 ET_DYN diagnostic ELF.
* `ps5-bt.map`, `ps5-bt.inspect.txt`, `ps5-bt.disasm.txt`: link map, LLVM
  inspection and disassembly.
* `ps5-bt.manifest.json`: artifact, source, archive and tool hashes, exact
  versions, SDK revision, Git HEAD/dirty state and structural inspection.

Inspection rejects unsupported relocations, imports, TLS, constructors,
forbidden stock-runtime symbols and invalid segment alignment/permissions.
Output has 16 KiB-aligned RX/R/RW load segments and retains the section headers
needed by the researched loaders' relocation pass. These checks establish a
reviewable build artifact, not PS5 loader or firmware compatibility.

## Runtime scope and execution gate

The fixed [RUNTIME-REVIEW.md](RUNTIME-REVIEW.md) documents runtime
`minimal-syscall-v6`, selected source behavior and the inspected loader contract.
The project-owned entry preserves the caller's stack/frame, clears BSS,
initializes the selected syscall bridge, opens a fresh diagnostic log at
`/data/ps5-bt-hw1.log`, and resolves existing libkernel `write`, `_exit` and the
best-effort notification entry point before invoking C main -> Rust -> the
Unsupported C adapter. Start and finish notifications plus an explicit final
status distinguish a completed run from a payload that never entered Rust. The
log file is truncated at the start of each run and duplicates the synchronous
diagnostic stream. Startup accepts the selected SDK's callback/getpid-veneer contracts without
reading executable code bytes. It does not search alternate offsets. Unsupported arguments, resolution or
permissions fail explicitly. No optional native diagnostic query is wired into
this ELF.

`make ps5-native-query` explicitly builds a separate
`build/ps5/ps5-bt-native-query.elf` with the single read-only diagnostic in
[NATIVE-QUERY.md](NATIVE-QUERY.md). It never replaces the baseline artifact;
run it only after the baseline entry/log/return test succeeds.

### Existing-loader HW-1 variant

The separately selected `make ps5-native-l2cap-probe` uses the established
existing-loader resolver and writes `build/ps5/ps5-bt-native-l2cap-probe.elf`.
It creates/closes one IPv4 UDP control socket and, only after clean control
cleanup, one unconnected L2CAP socket. See [the exact experiment and limits](NATIVE-L2CAP-PROBE.md).
It does not implement the transport ABI or connect to a headset.

```sh
LLVM_CONFIG=/opt/homebrew/opt/llvm@18/bin/llvm-config make ps5-hw1
# Optional separate query artifact:
LLVM_CONFIG=/opt/homebrew/opt/llvm@18/bin/llvm-config python3 tools/build_ps5.py --native-query --loader-kernel-resolver
```

The explicit flag selects fixed SDK kernel.c/nid.c and their headers for
`__kernel_init` and current-process export lookup through the existing loader
pipe/socket transport. All ten provisioned SDK files are hash-verified;
the default link uses only the original three, while this variant adds the
two C translation units and their headers. No patch.c, stock CRT or credential
setter is linked. Missing loader kernel transport returns 128. Runtime v6
normalizes syscall error flags and preserves mmap's MAP_FAILED convention.
The manifest records `loader_kernel_resolver` and all inputs.

The default minimal build remains available for environments where ordinary
lookup already works. The recorded environment instead required the explicit
resolver: ordinary write lookup failed, then kernel initialization and the
diagnostic succeeded with final 2. Notification API success, durable logging
and Rust entry were observed; the user confirmed wireless input and human-visible
display. Panic termination remains Untested. See
[HW1-REPAIR.md](HW1-REPAIR.md) and [RUNTIME-REVIEW.md](RUNTIME-REVIEW.md).

## Desktop checks and freestanding archive

### Independent USB and capture diagnostics

These never enable the Bluetooth transport and keep the baseline artifact:

```sh
make ps5-usb-metadata
make ps5-capture-preflight
make ps5-capture-once
make ps5-sbc-selftest
make ps5-capture-observe
```

Each target selects a separate `build/ps5/ps5-bt-<variant>.elf` and manifest.
Set LLVM_CONFIG as above. USB additionally provisions 34 hash-pinned SDK headers
listed in `tools/usb_sdk_headers.json`; no SDK libc or import library is linked.
The upstream stdint.h symlink is expanded into identical notice-bearing target
bytes in the standalone subset. A full pinned SDK source tree may be supplied
through PS5_SDK_SOURCE. The manifest checks the exact header dependency closure.

Capture diagnostics require the explicit existing-loader kernel resolver.
Preflight loads Ipmi then Avcap2 and resolves exports only. Capture-once opens
one session, optionally waits once for 100 ms through libkernel usleep, issues
one nonblocking read, then cleans up. No audio data is stored or logged.
The no-delay lifecycle executed with an Empty read; the later settled read
returned 8,192 bytes with clean cleanup. Native PCM format/timing remain
unverified. See HW2-EVIDENCE.md and HW3-EVIDENCE.md.
Capture adaptation is GPL-3.0-only; retain its provenance and GPL v3 terms.
Build and source-package commands never connect to a console.

Capture-observe is separately selected: it reads immediately, then waits 100 ms
only between Empty/Overrun results. It stops at first data/error, 20 reads or
the exclusive two-second monotonic deadline, followed by the same cleanup.
`clock_gettime` lookup/conversion errors fail closed. Its clock ABI follows the
pinned SDK's `time.h`, `sys/_timespec.h` and `x86/_types.h`: CLOCK_MONOTONIC=4,
32-bit clock ID and two signed 64-bit timespec fields. The adapter checks native
status, negative/invalid fields and microsecond overflow. Its runtime clock
availability and behavior must be tested independently; see CAPTURE-OBSERVE.md.

The SBC self-test links the separately built generic BlueZ libsbc archive with
caller storage and no allocator/SIMD/host SDK headers. Its numeric report must
show result 0, stage 6, two cycles, four 77-byte frames, prefix `0x9cfd206f` and
digest `0x14e0ec79`; final payload 2 still describes the Unsupported Bluetooth
adapter. The builder additionally requires LLVM 18.1.8 `llvm-ar` and `llvm-nm`,
exports `build/ps5/ps5-bt-sbc-selftest-relink/`, and actually relinks its supplied
application objects against the codec archive. Package its manifest with
`tools/package_source.py` before sharing or deploying the combined ELF; see
[SBC-TARGET.md](SBC-TARGET.md) for replacing the archive.

`make usb-probe-test` and `make capture-probe-test` exercise injected host
callbacks; both are included in `make check` and `make check-c-sanitize`.

### Complete local checks

```sh
make check
```

This explicitly selects desktop formatting, Cargo checks/tests/Clippy, actual
C/Rust host links and boundary tests, descriptor tests, then builds/inspects
`target/x86_64-unknown-none/release/libps5_runtime.a`. It does not need the
LLVM final-link tools or fetch SDK sources. Host binaries are in `build/host/`.
The codec subcheck additionally needs `patch`, `ar`, `nm` and a C compiler with
ASan/UBSan. It verifies and patches the vendored generic libsbc sources only in
temporary directories. Its optional target-object probe is not a final PS5
codec link; missing target headers are reported separately.
The stub process intentionally exits 2 for Unsupported; the harness recognizes
that as the expected stub outcome. Synthetic adapters remain test-only.

`make host-smoke` runs the desktop seam; `make rust-archive` selects the
freestanding archive alone. Cargo.lock contains only workspace packages. The
host C compiler is not pinned as a binary; record its version on failures.
Ubuntu and macOS host CI are configured; configuration is not proof remote
jobs ran. Desktop tests do not execute the real syscall bridge or loader and
must not be reported as hardware tests.

`make media-recovery-test` runs the deterministic host-only PCM example, also
included in `make check`. It exercises normal capture, bounded short-gap silence,
long-gap rebuffer and explicit expired-PCM discard after a sender stall. It does
not play sound, encode, connect to hardware or estimate HCI credits.

`make sbc-codec-test` separately selects the real host-only codec acceptance
suite, also included in `make check`: 208 encoder reference vectors, 12 decoder
checks, C sanitizer/bounds tests, static-archive replacement linking, safe Rust
owner tests/lints and a real `PcmQueue` -> SBC example. The latter produces a
77-byte joint-stereo bitpool-32 frame from 128 stereo frames; it does not play
sound. `host-libsbc` intentionally refuses cross-compilation before invoking
the host C compiler. Feature-off `bt-codec` remains portable `no_std`.
See SBC-IMPLEMENTATION.md for exact commands and remaining PS5 ABI/link gates.

`make check-build-tools` runs the builder/source-package/kit self-tests and the
Python diagnostic-result, console-window, window-preparer and portable-handoff
suites. The two new suites may also run directly:

```sh
python3 tests/tools/test_prepare_ps5_window.py
python3 tests/tools/test_window_handoff.py
```

These use synthetic local fixtures, not SDK downloads or console execution.
On the recorded ARM macOS host, the unchanged first check,
`tools/test_syscall_checked.py`, cannot execute its x86 host binary (errno 86,
Bad CPU type). Consequently the full Make target stops there; run the remaining
independent commands explicitly and report that syscall check as unavailable.
See [TESTING.md](TESTING.md); a passing Python suite does not make the full target
or hardware checks pass.

### Bounded hardware-test kit

After the intended changes are committed and the worktree is clean, rebuild the
ten independently selected full-inventory candidates and their corresponding
source packages.
The six-item default window can instead be prepared with one command below; the
full inventory remains an explicit choice:

```sh
LLVM_CONFIG=/opt/homebrew/opt/llvm@18/bin/llvm-config make ps5-usb-metadata
python3 tools/package_source.py \
  --manifest build/ps5/ps5-bt-usb-metadata.manifest.json
LLVM_CONFIG=/opt/homebrew/opt/llvm@18/bin/llvm-config make ps5-capture-once
python3 tools/package_source.py \
  --manifest build/ps5/ps5-bt-capture-once.manifest.json
LLVM_CONFIG=/opt/homebrew/opt/llvm@18/bin/llvm-config make ps5-sbc-selftest
python3 tools/package_source.py \
  --manifest build/ps5/ps5-bt-sbc-selftest.manifest.json
LLVM_CONFIG=/opt/homebrew/opt/llvm@18/bin/llvm-config make ps5-capture-observe
python3 tools/package_source.py \
  --manifest build/ps5/ps5-bt-capture-observe.manifest.json
LLVM_CONFIG=/opt/homebrew/opt/llvm@18/bin/llvm-config make ps5-sbc-continuous
python3 tools/package_source.py \
  --manifest build/ps5/ps5-bt-sbc-continuous.manifest.json
LLVM_CONFIG=/opt/homebrew/opt/llvm@18/bin/llvm-config make ps5-capture-stream
python3 tools/package_source.py \
  --manifest build/ps5/ps5-bt-capture-stream.manifest.json
LLVM_CONFIG=/opt/homebrew/opt/llvm@18/bin/llvm-config make ps5-application-control
python3 tools/package_source.py \
  --manifest build/ps5/ps5-bt-application-control.manifest.json
LLVM_CONFIG=/opt/homebrew/opt/llvm@18/bin/llvm-config make ps5-application-generated
python3 tools/package_source.py \
  --manifest build/ps5/ps5-bt-application-generated.manifest.json
LLVM_CONFIG=/opt/homebrew/opt/llvm@18/bin/llvm-config make ps5-application-observe
python3 tools/package_source.py \
  --manifest build/ps5/ps5-bt-application-observe.manifest.json
LLVM_CONFIG=/opt/homebrew/opt/llvm@18/bin/llvm-config make ps5-application-setup
python3 tools/package_source.py \
  --manifest build/ps5/ps5-bt-application-setup.manifest.json
python3 tools/mvp_test_kit.py --plan tools/mvp_test_plan.json --assemble
```

The assembler rejects dirty/stale manifests, wrong diagnostic variants,
artifact hash/size mismatches, missing source bundles and unexpected target or
path values. It writes ignored `build/ps5/mvp-test-kit/` with all selected ELF files,
their exact build manifests/source bundles, a hash manifest and an ordered
runbook. It performs no network operation.

Deployment is a separate explicit action and defaults to dry-run:

```sh
python3 tools/mvp_test_kit.py --deploy
# Only after the user confirms the hardware window and reviews the dry-run:
python3 tools/mvp_test_kit.py --deploy --execute
```

The execute mode connects only to `192.168.1.111:2121`, validates each existing
PayloadManager local slot's bounded `meta.json`, uploads to a unique temporary
name, checks remote size, then renames to the exact slot filename. It never
creates a slot, launches a payload, deletes a durable log or touches another
payload. Create/confirm every selected plan's exact existing slot manually in
PayloadManager first, including `ps5-bt-usb-binding` and `ps5-bt-capture-settled`
when using the full inventory. The native setup candidate creates no
PayloadManager slot. A slot metadata mismatch aborts before upload. Retrieve `/data/ps5-bt-hw1.log` before every
manual launch because the next run overwrites it.

The PS5 builder explicitly selects the project `target/` directory with
`--target-dir`, independent of an inherited `CARGO_TARGET_DIR`. Some unchanged
host checks still expect `target/<host>/debug/libps5_runtime.a`; for those host
checks, leave `CARGO_TARGET_DIR` unset to avoid a missing-archive failure.

## Reproducibility and distribution

The source/target/flags, Rust and LLVM versions, and selected SDK file hashes
are pinned. Consult the manifest from the final source state for artifact
hashes. A second macOS 27.0.1 arm64 machine reproduced the `89bdce0` baseline
ELF hash with this documented toolchain; see [WAVE-1-REPORT.md](WAVE-1-REPORT.md).
Byte-identical results on other hosts or OS versions are not claimed, and no
hardware execution is claimed. Publish source state, actual tool versions/hashes,
command, map and artifact hashes with any future release.

All three selected SDK files retain John Törnblom's copyright and
GPL-3.0-or-later notices without modification. No SDK patches are applied.
Redistributing a linked ELF requires the selected corresponding source,
notices, GPL text and project/build sources; GitHub links alone are not a
complete source distribution. There is no release distribution yet. See
[UPSTREAM.md](UPSTREAM.md) for attribution, exact selection and update rules.

## Rust continuous SBC candidate

`make ps5-sbc-continuous` (with LLVM_CONFIG as above) builds a separate
`build/ps5/ps5-bt-sbc-continuous.elf`. It builds/inspects the generic target
archive first, selects `ps5-runtime/continuous-sbc` with independent target codec
linking, and exports matching replacement materials. The old short C self-test
and baseline remain independently selectable. See [SBC-CONTINUOUS.md](SBC-CONTINUOUS.md)
for exact budgets, expected reports and hardware admission.

After a clean commit and rebuild, package this candidate with:

```sh
python3 tools/package_source.py --manifest build/ps5/ps5-bt-sbc-continuous.manifest.json
```

The six-item default window and ten-item full-inventory plans both include this
independent candidate; build/package all selected manifests and corresponding
sources before assembly. No new slot is created by the uploader. A new hardware
window and execution authorization remain required.

## Experimental Rust capture entry

`make capture-stream-test` runs portable/actual Rust-to-C fixture checks, strict
lints, freestanding external-ABI checks and the C ASan/UBSan lifecycle harness.
`make ps5-capture-stream` builds the separate one-read
`build/ps5/ps5-bt-capture-stream.elf`. The explicit loader resolver supplies the
same reviewed module lookup as the older capture diagnostic. Format/time remain
unknown; raw bytes and native metadata are never logged or encoded.
Package the clean build with `tools/package_source.py --manifest
build/ps5/ps5-bt-capture-stream.manifest.json`. The six-item default window and
ten-item full inventory include this candidate; build/package every item
selected by the plan before assembly. This creates no new hardware window or
deployment authority. See CAPTURE-STREAM.md for exact admission.

## Native clock/wait adapter candidate

`make scheduler-clock-test` runs the actual Rust/C fixture, native error/bounds
checks, C ASan/UBSan and external freestanding compilation. Production selection
is `ps5-platform/external-clock-wait`; its final platform link must explicitly
include `platform/ps5/scheduler/scheduler_clock.c`. It supplies no native transport
or readiness backend. The application-control candidate below independently
selects it for finite startup/HTTP driving; the adapter adds no standalone clock
diagnostic. See PLATFORM-CLOCK.md.

## Correlated local experiment results

The Rust capture, continuous SBC, three application workloads and separate
application-setup candidate embed an input-build identity
matching `diagnostic_build_id` / `diagnostic_candidate` in their manifest. Their
unique begin/finish markers precede the runtime's existing termination report.
After an authorized hardware run and local log collection,
`tools/diagnostic_result.py` writes private bounded JSON with exact correlation,
cleanup and abnormal-run classification. All three controller observations must
be entered explicitly; Unknown remains Unknown. This collector never connects
to a console. See DIAGNOSTIC-RESULTS.md; `python3 tests/tools/test_diagnostic_result.py`
checks parser and persistence failure paths.


## Fixed native namespace setup candidate

The separate setup artifact prepares only permitted missing components of
`/data/ps5-bt/control`, validates native owner/mode and reports bounded cleanup.
Existing paths are never chmodded/chowned, and root plus `/data` must already
exist. It reads no entropy and starts no listener, capture, encoder or radio.
Its matching application import/header closure remains source-reviewed; native
exports, permissions and persistence require hardware evidence.

```sh
make application-setup-test
LLVM_CONFIG=/absolute/path/to/LLVM-18.1.8/bin/llvm-config make ps5-application-setup
# Equivalent explicit build selection:
python3 tools/build_ps5.py --application-setup --loader-kernel-resolver \
  --llvm-config /absolute/path/to/LLVM-18.1.8/bin/llvm-config
# After a clean committed rebuild, package before building another variant:
python3 tools/package_source.py \
  --manifest build/ps5/ps5-bt-application-setup.manifest.json
```

The host test target uses the actual setup/main code with fault stubs and
ASan/UBSan; it never changes the host's `/data`. The artifact is
`build/ps5/ps5-bt-application-setup.elf`, with its matching manifest and original
43 SDK headers/notices in the source bundle. Selection is mutually exclusive
with every other diagnostic flag. Building or adding setup to a kit never runs
it or removes the subsequent workload's own private-directory validation.
Follow [APPLICATION-SETUP.md](APPLICATION-SETUP.md) for its finite operation and
external observation budgets, cancellation, persistent created components and
sticky cleanup uncertainty. A current authorized window is required for launch.

## Control-only native application candidate

Build the independently selected candidate with the existing loader resolver:

```sh
make ps5-application-control
# Equivalent explicit builder selection:
python3 tools/build_ps5.py --application-control --loader-kernel-resolver
```

Use the same pinned Rust/LLVM setup and optional `PS5_SDK_SOURCE` as other
variants. This writes `build/ps5/ps5-bt-application-control.elf` plus its matching
manifest/map/inspection files. Selection is mutually exclusive with the other
diagnostic modes. The builder hashes the Rust/C sources, independent candidate
identity and exact `tools/application_sdk_headers.json` inventory, compiles the
43-header target dependency closure, and binds existing store/listener calls to
reviewed selected exports. Missing imports fail before resource acquisition.
No target libc/import library or new syscall-number fallback is linked.

The candidate assembles the concrete factory, protected store, persistent HTTP
listener, fallible clock/wait, Application/Runner and finite Rust owner loop.
It performs no Bluetooth, capture or codec operation. The chosen listener is an
ephemeral port on `127.0.0.1`; this is not a LAN control deployment. Fresh native
entropy supplies the bearer token; token/port/address/path are never logged.
The caller must already have a private `/data/ps5-bt/control` directory with exact
mode 0700 and appropriate native protections. A separately selected
[application-setup candidate](APPLICATION-SETUP.md) may establish its permitted
missing components and verify the native owner/mode before this workload. The
control workload does not create or relax that directory, and a missing/unsafe
path fails with observable cleanup.
PS5 import availability, entropy, filesystem protection/durability, scheduling
and wireless-controller behavior remain Requires hardware.

Default budgets are 5,000,000 observed microseconds, 20,000 turns, 10,000 wait
calls, 64 accepted clients and 100,000 us per HTTP request/normal protocol stop.
Wait requests are at most 1000 us; kernel calls may overrun their requested bounds.
Stop remains observable across import/entropy/startup/HTTP and cleanup. Primary
and prior client-close uncertainty survive later successful resource closes.
Existing baseline/capture/SBC artifacts retain their original limits. See
APPLICATION-STARTUP.md and PS5-POSIX-IMPORTS.md for precise contracts.

After committing intended changes, rebuild from the clean commit and package:

```sh
python3 tools/package_source.py --manifest build/ps5/ps5-bt-application-control.manifest.json
```

The package includes the exact original target headers/notices and preserves
`--application-control --loader-kernel-resolver` in its rebuild command. This
candidate does not execute the SBC archive or need codec replacement materials.
It is included in the six-item default window and ten-item full inventory;
assembly requires every selected matching clean-commit build/source bundle. No
final kit identity exists until assembly succeeds. The uploader does not create
the application-control console slot. Building and packaging never deploy or authorize a hardware run.
A new current authorized window and explicit controller observations are required
before execution.


## Integrated generated and observation application candidates

Build each mode separately; the builder flags are mutually exclusive:

```sh
make ps5-application-generated
make ps5-application-observe
# Equivalent explicit selections:
python3 tools/build_ps5.py --application-generated --loader-kernel-resolver
python3 tools/build_ps5.py --application-observe --loader-kernel-resolver
```

Both acquire the actual protected store, persistent loopback listener and reviewed
clock, then run bounded Rust work inside the platform owner loop. The same
private 0700 directory `/data/ps5-bt/control`, matching native owner and import/
entropy prerequisites apply. Builds, source packaging and preflight never create
the native directory. Only a separately selected, authorized application-setup
execution may create its fixed missing components with mode 0700 and verify
ownership/protection; it never chmods/chowns an existing path. Setup is not
automatically run by another workload, and preparation alone proves no native
permissions.

Generated mode uses observed clock pacing for 750 blocks/96,000 generated stereo
frames across two seconds of nominal sample positions. Real persistent target
SBC output is hashed/discarded: the joint profile expects 57,750 bytes, prefix
`9cfd20cb` and FNV-1a `7784d1fb`. It sends no media. Its source bundle includes
matching codec replacement/relink material and preserves
`--application-generated --loader-kernel-resolver`.

Observation mode starts the reviewed capture owner, retains format/native time
Unknown and continues bounded raw reads after Data/Empty/Overrun. Its 100 ms settle
and 10 ms interval permit up to 190 scheduled polls before an exclusive two-second deadline;
256 is a hard upper bound. Late scheduling makes fewer reads, never catch-up
bursts. No samples, PCM format or lost-sample count are inferred and raw bytes are
scrubbed. Existing one-read and first-data diagnostics keep their original bounds.
See APPLICATION-GENERATED.md and APPLICATION-OBSERVE.md for outcome details.

## Default six-item window, portable handoff and read-only collection

The default `tools/prepare_ps5_window.py` plan is `tools/window_test_plan.json`:
application-setup, application-control, application-generated,
application-observe, sbc-continuous and capture-stream, in that order. The full
`tools/mvp_test_plan.json` inventory now contains ten independent candidates.
The earlier `tools/startup_test_plan.json` remains a three-workload subset
(control, generated, observe); it omits setup and requires an independently
established safe private namespace.

From a clean committed clone with the installed pinned tools, inspect then
prepare the default six candidates locally:

```sh
python3 tools/prepare_ps5_window.py --doctor \
  --llvm-config /absolute/path/to/LLVM-18.1.8/bin/llvm-config
python3 tools/prepare_ps5_window.py --build \
  --llvm-config /absolute/path/to/LLVM-18.1.8/bin/llvm-config \
  --kit-dir build/ps5/next-window-kit-20261026
```

Doctor/default dry-run creates no output and performs no builds, SDK downloads
or console operations. Explicit `--build` invokes the existing builders and
packages each candidate immediately before the next build overwrites Cargo's
shared Rust archive. It uses a fresh namespace, validates clean HEAD and the
unchanged plan throughout, then assembles/revalidates the kit and writes an
atomic identity summary. Only explicit build may provision missing pinned SDK
source through the existing builder; `--sdk-source DIR` selects a locally
verified source override. For the full inventory, pass
`--plan tools/mvp_test_plan.json`; every selected item must build/package.
See [WINDOW-PREPARER.md](WINDOW-PREPARER.md) for installed-tool checks, lock,
failure handling, output namespaces and exact options.

For already built and immediately packaged matching candidates, assembly alone
is also available. The following preserves the historical three-workload subset:

```sh
python3 tools/mvp_test_kit.py --plan tools/startup_test_plan.json \
  --kit-dir build/ps5/startup-subset-kit --assemble
python3 tools/console_window.py --kit build/ps5/startup-subset-kit --preflight
```

The preflight command above defaults to dry-run. With a separately authorized
console window, adding `--execute` performs read-only FTP slot metadata and
artifact-size checks; optional `--verify-hash` reads back bounded ELF contents.
Neither preflight nor the local preparer creates a PayloadManager slot, native
directory or permission, deploys, or launches. Every selected slot must already
exist with matching metadata. The three-workload subset needs its three
`ps5-bt-application-*` slots; the default six-item plan additionally needs
`ps5-bt-application-setup`, `ps5-bt-sbc-continuous` and `ps5-bt-capture-stream`.
The separate application-setup artifact is the only selected candidate allowed
to create its fixed missing native namespace components; building or preflight
does not execute it. Private 0700 protection and the expected native owner
remain Unknown until the native payload verifies them. Planned environment
is the reported Slim/13.40, PayloadManager 0.6.3 and XM4; this is no compatibility
or present execution authorization.

For a different testing machine, export the validated clean-HEAD kit and its
complete matching committed project tree/source bundles:

```sh
python3 tools/window_handoff.py export \
  --kit-dir build/ps5/next-window-kit-20261026 \
  --output build/ps5/ps5-bt-window-transfer.tar.gz
```

The exporter creates the archive, standalone bootstrap script and SHA-256
sidecar without console access. The receiver needs only Python 3.11+ for trusted
hash inspection, safe extraction and already-built kit preflight/collection;
rebuilding still needs the external pinned tools and original Git provenance.
It never manufactures Git history or installs a toolchain. See
[WINDOW-HANDOFF.md](WINDOW-HANDOFF.md) for exact receiving commands and bounds,
and [TEMP-PS5-TEST-HANDOFF.md](TEMP-PS5-TEST-HANDOFF.md) plus the kit runbook for
operator sequence. Preparation/export success is no native setup or PS5 test
acceptance. Preserve the transfer hash through a trusted channel.

After a separately authorized manual launch, collect the fixed diagnostic log:

```sh
python3 tools/console_window.py --kit build/ps5/next-window-kit-20261026 \
  --collect --candidate application-generated
```

Collection also defaults to dry-run. Adding `--execute` fetches only the bounded
fixed log, saves a unique private raw log and correlated JSON under
`build/ps5/console-window` (or explicit `--output-dir`). Supply actual
`--controller-before`, `--controller-during`, `--controller-after` observations
(`Unknown`, `normal` or `changed`) and `--run-observation` (`Unknown` or `observed`).
Defaults remain Unknown; reading a log does not prove a human observed this run
or that the controller remained normal. Preserve each log before another manual
launch overwrites it. Root's final handoff records actual assembled kit identity;
an intended path alone is not a completed artifact or deployment.

There is still no usable Bluetooth connection backend or ready safe active-HCI
artifact. The recorded native L2CAP errno 47 result remains a blocker. None of
these local setup/startup/audio diagnostics connects the XM4 or validates
audible playback. Scoped native resource rebuilding is host-tested library work,
not automatic reconnect in these application ELFs and not an additional console artifact.
