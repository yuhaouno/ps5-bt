# Building the bootstrap

The public project/package name is not selected. Crate names are local and
unpublished. Builds never contact or deploy to a console. The PS5 output is a
real diagnostic ELF linking C -> Rust -> an Unsupported adapter; the explicit HW-1 variant has executed on Slim/13.40, but it does not provide
working Bluetooth or HCI controller capability information. The separate USB
diagnostic below has read device descriptors on one console.

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
The no-delay lifecycle executed with an Empty read; the later settling change
is **Requires hardware**, not a successful PCM test. See HW2-EVIDENCE.md.
Capture adaptation is GPL-3.0-only; retain its provenance and GPL v3 terms.
Build and source-package commands never connect to a console.

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

### Bounded hardware-test kit

After the intended changes are committed and the worktree is clean, rebuild the
two next-window candidates and their corresponding source packages:

```sh
LLVM_CONFIG=/opt/homebrew/opt/llvm@18/bin/llvm-config make ps5-usb-metadata
python3 tools/package_source.py \
  --manifest build/ps5/ps5-bt-usb-metadata.manifest.json
LLVM_CONFIG=/opt/homebrew/opt/llvm@18/bin/llvm-config make ps5-capture-once
python3 tools/package_source.py \
  --manifest build/ps5/ps5-bt-capture-once.manifest.json
python3 tools/mvp_test_kit.py --assemble
```

The assembler rejects dirty/stale manifests, wrong diagnostic variants,
artifact hash/size mismatches, missing source bundles and unexpected target or
path values. It writes ignored `build/ps5/mvp-test-kit/` with both ELF files,
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
payload. Create/confirm the exact `ps5-bt-usb-binding` and
`ps5-bt-capture-settled` slots manually in PayloadManager first. A slot metadata
mismatch aborts before upload. Retrieve `/data/ps5-bt-hw1.log` before every
manual launch because the next run overwrites it.

These checks and the PS5 builder read Rust archives from Cargo's default
`target/` directory. An inherited non-default `CARGO_TARGET_DIR` puts the
archive somewhere else, and the current host check fails when
`target/<host>/debug/libps5_runtime.a` is absent. The documented environment
leaves `CARGO_TARGET_DIR` unset, and that default works.

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
