# Minimal PS5 ELF toolchain

The baseline artifact is a diagnostic C -> Rust -> Unsupported-adapter ELF.
The explicit existing-loader HW-1 variant has executed on one Slim/13.40
environment; see HW1-REPAIR.md and HW3-EVIDENCE.md for the tested artifacts and
limits. A compatible already-installed loader and its existing kernel-R/W
channel are prerequisites for that variant. See [runtime review](RUNTIME-REVIEW.md).

Compiler flags, no-SIMD/no-allocator properties and import checks below describe
the current diagnostic build, not permanent restrictions on every audio module.
Source-reviewed experimental/optimized adapters need separately selected build
inputs, ABI/state review and PS5 acceptance under [AGENTS.md](../AGENTS.md).
Do not weaken existing diagnostic inspection to admit an unrelated backend.

## One build command after setup

Install the repository's Rust 1.90.0 toolchain and target, Python 3.11+, a host
C11 compiler, Make, and LLVM **18.1.8** containing clang, ld.lld, llvm-readobj and
llvm-objdump. Apple Clang is not the final-link baseline. On the SDK's documented
macOS/Homebrew path, `brew install llvm@18` supplies the LLVM 18.1.8 tools; set
`LLVM_CONFIG=/opt/homebrew/opt/llvm@18/bin/llvm-config`. On Linux, point
`LLVM_CONFIG` at an LLVM 18.1.8 installation with those tools. The builder checks
the exact version rather than selecting moving `latest`.

```sh
make
```

`LLVM_CONFIG` may also be passed as a Make variable. The builder obtains only
three source files from the fixed SDK v0.45 revision into ignored `.sdk/`,
verifies their SHA-256 values, runs synthetic host startup tests, builds the
pinned Rust archive, compiles C/assembly and performs the final link. It never
deploys or contacts a console. There are no SDK libc/import-library or Sony
binary dependencies. An existing external SDK source checkout may be used
offline with `PS5_SDK_SOURCE=/absolute/path/to/sdk`; exactly the same three
source hashes must match. A release SDK archive alone does not contain the
selected source files and is unnecessary for this minimal build.

Outputs under ignored `build/ps5/`:

* `ps5-bt.elf`: x86-64 ET_DYN diagnostic artifact.
* `ps5-bt.map`, `ps5-bt.inspect.txt`, `ps5-bt.disasm.txt`: link map, complete LLVM
  inspection, and disassembly for independent review.
* `ps5-bt.manifest.json`: ELF/linked source/archive/tool hashes, exact Rust/LLVM
  versions, SDK revision, Git HEAD/dirty status and structural inspection.

The build uses `-target x86_64-sie-ps5` for C, freestanding PIC, no red zone,
general registers only, no builtins/stack protector and separate code/data
sections. Rust remains `x86_64-unknown-none`, PIC/small, panic abort, no LTO.
The original minimal linker script uses 16 KiB RX/R/RW loads and retains section
headers/symbols required for inspection and the audited loader's RELA pass.
Only debug sections are stripped. No custom Rust target or OS standard library
is introduced. The ABI remains integer/pointer-only; floating values by value
must not cross this boundary.

```sh
python3 tools/build_ps5.py --self-test
python3 tools/inspect_elf.py build/ps5/ps5-bt.elf --self-test
```

The first command exercises missing/modified source and missing/wrong toolchain
failures. Every full build tests synthetic startup argument/layout rejection,
resolver errors, null outputs, fixed handle fallback, complete/short/failed
logging and byte-memory routines with the host C compiler. The stub-byte helper
tests every shorter available length and a mutation of every checked byte.
This substitutes the syscall adapter and an empty BSS interval; it does **not**
execute the SDK syscall bridge, real loader, BSS clear, panic exit or console.
The ELF self-test rejects corrupt/truncated headers, wrong machine/type, TLS,
bad load alignment/permissions, imports, DT_NEEDED and unsupported relocations.
Production inspection also rejects TLS sections, constructors, absent Rust/
bootstrap symbols and forbidden stock-runtime symbols.

## Build observed on 2026-10-23

Host: macOS Darwin 27.0.0 arm64; Python 3.12.15; Apple Clang 21.0.0 for synthetic
host tests. Rust `1.90.0 (1159e78c4 2025-09-14)` and target are installed outside
the shell's default PATH under `/tmp/ps5-bt-research-20261023`. The successful
environment is reproducible within this session:

```sh
env RUSTUP_HOME=/tmp/ps5-bt-research-20261023/rustup \
  CARGO_HOME=/tmp/ps5-bt-research-20261023/cargo \
  PATH=/tmp/ps5-bt-research-20261023/cargo/bin:$PATH \
  LLVM_CONFIG=/tmp/ps5-bt-research-20261023/llvm18/llvm@18/18.1.8/bin/llvm-config \
  make ps5-link
```

LLVM is Homebrew clang/lld **18.1.8**, arm64_golden_gate bottle rebuild 3,
downloaded using `brew fetch --force-bottle llvm@18` with an external
`HOMEBREW_CACHE`, extracted under `/tmp/ps5-bt-research-20261023/llvm18`, without
changing the user's Homebrew installation or shell configuration. Bottle SHA-256:
`8695d745db18541547d954d19ac8ede8a2032c3e712ba4470a1937d1c9c6fba1`.
The bottle's `@@HOMEBREW_PREFIX@@/opt/zstd/lib/libzstd.1.dylib` install-name was
relocated **only in the external libLLVM.dylib and lld executable** to
`/opt/homebrew/opt/zstd/lib/libzstd.1.dylib`, then both were ad-hoc re-signed.
Host zstd is Homebrew **1.5.7_1**. This changes native build-tool binaries, not
target code or SDK source. An ordinary Homebrew installation handles bottle
relocation itself and needs none of these manual steps. Exact selected native
tool hashes are saved in each manifest.

The SDK release archive was independently verified as SHA-256
`5fac64093d0e3e69f2584ce7935f24f703f622d9681f1495c85b9f00d1291a2c`, but its
default runtime was rejected and it is not a link input. The source checkout
override and immutable automatic-fetch path both produced genuine inspected
ELFs. The SDK's linker script was also tried; missing unwind/initialized-data
sections caused unaligned PT_LOAD starts, so the final build uses the original
project script described in RUNTIME-REVIEW.md.

Artifact hashes depend on the committed source state. Consult the manifest from
the final rebuild, rather than copying a development hash into release claims.
Repeated identical-input builds are compared locally. Host tests and ELF
structural validity establish neither firmware compatibility nor usable
controller/audio support.

## Clean-machine reproduction, 2026-10-23

A second macOS arm64 development machine cloned `origin/feat/mvp-bringup` at
`89bdce0` and used the documented setup: Rust 1.90.0 from
[rust-toolchain.toml](../rust-toolchain.toml), `brew install llvm@18`, and
`LLVM_CONFIG=/opt/homebrew/opt/llvm@18/bin/llvm-config`. The builder fetched
the three pinned SDK sources. The host was macOS 27.0.1, Darwin arm64, with
Python 3.14.7 and LLVM 18.1.8. No repository changes were required.

`make check`, `make check-c-sanitize`, the baseline `make`, ELF inspection, and
source packaging passed. The ELF SHA-256 matched
`a4a42640ec604e69aec535f5ddb015e8eafd1ebe6efe8a7386de7a7e2cb01cb0`. A second
build was byte-identical. Repeated source packaging of the same clean inputs
was byte-identical. This is one additional macOS arm64 result, not a Linux
result, another OS version, or PS5 execution. Details are in
[WAVE-1-REPORT.md](WAVE-1-REPORT.md).

An inherited non-default `CARGO_TARGET_DIR` breaks the host check, which
expects Cargo's default `target/` directory. The documented environment does
not set that variable, and that default works. Byte identity on other hosts
is not claimed.
