# Building the bootstrap

The public project/package name is not selected. Crate names are local and
unpublished. Builds never contact or deploy to a console. The PS5 output is a
real diagnostic ELF linking C -> Rust -> an Unsupported adapter; it has never
run on PS5 and does not provide working Bluetooth or controller information.

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
exactly three unmodified external files from SDK v0.45, revision
`a7685ce8e97ce1f8a4f38253a41d54241c9124f1`: `crt/syscall.c`, `crt/syscall.h`
and `crt/payload.h`. It verifies each file's SHA-256, including when using an
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
`minimal-syscall-v1`, selected source behavior and the inspected loader contract.
The project-owned entry preserves the caller's stack/frame, clears BSS,
initializes the selected syscall bridge, resolves existing libkernel `write`
and `_exit`, then invokes C main -> Rust -> the Unsupported C adapter. Startup
validates the loader-supplied getpid stub prefix before using the upstream
adapter. Unsupported arguments, resolution or permissions fail visibly. No
optional native diagnostic query is wired into this ELF.

`make ps5-native-query` explicitly builds a separate
`build/ps5/ps5-bt-native-query.elf` with the single read-only diagnostic in
[NATIVE-QUERY.md](NATIVE-QUERY.md). It never replaces the baseline artifact;
run it only after the baseline entry/log/return test succeeds.

The SDK's default `crt/crt.c` calls `__patch_init()` and its `crt/patch.c` changes
credentials and syscall permissions; neither is a build input. No kernel-memory
helper, stock CRT, SDK linker script or local SDK patch enters the link. The
minimal startup performs no credential or permission mutation, module loading,
controller takeover, exploit or new loader implementation.

A compatible already-installed loader and sufficient existing permissions are
external prerequisites. Their actual behavior, syscall stub bytes, entry/return
ABI, stderr routing and panic termination require hardware verification. PS5
Slim / firmware 13.40 remains **Unknown**, as do controller information,
DualSense coexistence and audio. This artifact is an experimental
hardware test candidate. Execute only in an already-capable, compatible loader
environment after establishing those prerequisites and the actual runtime ABI,
following [HARDWARE-TEST.md](HARDWARE-TEST.md).

## Desktop checks and freestanding archive

```sh
make check
```

This explicitly selects desktop formatting, Cargo checks/tests/Clippy, actual
C/Rust host links and boundary tests, descriptor tests, then builds/inspects
`target/x86_64-unknown-none/release/libps5_runtime.a`. It does not need the
LLVM final-link tools or fetch SDK sources. Host binaries are in `build/host/`.
The stub process intentionally exits 2 for Unsupported; the harness recognizes
that as the expected stub outcome. Synthetic adapters remain test-only.

`make host-smoke` runs the desktop seam; `make rust-archive` selects the
freestanding archive alone. Cargo.lock contains only workspace packages. The
host C compiler is not pinned as a binary; record its version on failures.
Ubuntu and macOS host CI are configured; configuration is not proof remote
jobs ran. Desktop tests do not execute the real syscall bridge or loader and
must not be reported as hardware tests.

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
