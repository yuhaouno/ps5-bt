# Building the bootstrap

The public project/package name is not selected. Crate names are local and
unpublished. Builds never contact or deploy to a console.

## Desktop and freestanding archive

Requirements: Rust/rustup, C11 compiler (`cc`, or `HOST_CC`), Make, Python 3.11+.
Install rustup using [the official instructions](https://www.rust-lang.org/tools/install).
The checked-in `rust-toolchain.toml` pins **1.90.0**, rustfmt, Clippy and the
`x86_64-unknown-none` target. This is a deliberate bootstrap baseline, not a claim
that it is the newest release. Cargo downloads toolchain components when needed;
after setup the workspace has no registry dependencies. Do not commit local
toolchain files or change the pin to `stable` without reviewing the update.

```sh
make check
```

This runs fmt, cargo check, workspace tests, Clippy with warnings denied, actual C/Rust host
links and boundary tests, then builds/inspects the `no_std` archive:
`target/x86_64-unknown-none/release/libps5_runtime.a`.
Host binaries are in `build/host/`. The stub process intentionally exits 2 for
Unsupported; the harness recognizes that outcome as success for the stub test.
The synthetic adapter is test-only and never appears in `payload.mk`.

To rebuild on the desktop after local changes, use the same commands; Cargo.lock
contains only workspace packages. The host compiler is not pinned as a binary;
record its version when reporting a failure. Ubuntu and macOS host CI are
configured; a configuration is not proof those remote jobs have run.

## Candidate PS5 toolchain

The research baseline is community SDK v0.45, commit
`a7685ce8e97ce1f8a4f38253a41d54241c9124f1`.
Download the [versioned SDK archive](https://github.com/ps5-payload-dev/sdk/releases/download/v0.45/ps5-payload-sdk.zip)
to an external tools folder. Before extracting, compute SHA-256 and compare to:

```text
5fac64093d0e3e69f2584ce7935f24f703f622d9681f1495c85b9f00d1291a2c
```

Use `sha256sum` on Linux or `shasum -a 256` on macOS. Extract without replacing
an existing installation and set `PS5_PAYLOAD_SDK` to the extracted
`ps5-payload-sdk` directory. Never use a moving `latest` release in CI.
No proprietary console library files or firmware are required by our archive
build or supplied by this repository.

Upstream documents LLVM/Clang/lld 18 as a host baseline. On Debian/Ubuntu this
means `clang-18`, `lld-18`, `llvm-18`, Make and unzip; on macOS its documented
path uses Homebrew `llvm@18` and `LLVM_CONFIG` pointing to its `llvm-config`.
Follow the pinned SDK's own README/wrappers and record actual versions. Apple
Clang alone is not the documented toolchain. We have not validated a final SDK
link in this environment, and do not claim a reproducible PS5 ELF yet.

## Runtime scope gate

**Do not deploy the stock candidate runtime as an approved project payload.**
At the pinned revision, `crt/crt.c` calls `__patch_init()` before main and
`crt/patch.c` changes process credentials and syscall permissions via kernel
helpers. The project must not implement privilege changes as a prerequisite to
audio. We need a reviewed existing startup arrangement that fits the scope;
this assignment does not create a replacement loader/CRT or patch the SDK.

Once that question is resolved, save a runtime review including exact SDK/runtime
and loader revisions, startup behavior, existing permission prerequisites,
imports, local patches (if any), license/source plan and reason it fits scope.
The link recipe requires `PS5_RUNTIME_REVIEW` to name that real review file. The
file-existence gate is a workflow reminder, **not an automated security audit**.
Do not create an empty review to bypass it. If the reviewed arrangement requires
a different link path, update the recipe and provenance before using it.

The eventual link-only command is:

```sh
make ps5-link PS5_PAYLOAD_SDK=/absolute/path/to/ps5-payload-sdk \
  PS5_RUNTIME_REVIEW=/absolute/path/to/completed-runtime-review.md
```

It runs the SDK C compiler with our C main/log/stub and the Rust archive, writes
`build/ps5/bootstrap.elf` and a link map, and **does not deploy**. The output still
has no hardware backend. This recipe is proposed and has not passed an SDK link
or console test here. Current first compilable deliverables are the host seam
and freestanding Rust archive, not a working controller-information payload.

Before any console execution, inspect ELF type/machine/segments, entry symbol,
16 KiB load alignment, dynamic imports and relocations with the SDK tools; record
the map and hashes. Verify only intended SDK imports exist and no host runtime
or Linux/FreeBSD std/TLS dependency entered the ELF. Review panic `_Exit` behavior
and loader return on hardware. Follow [HARDWARE-TEST.md](HARDWARE-TEST.md).

## Reproducibility claim

The portable workspace/toolchain/target/flags and upstream research inputs are
pinned. `make check` is the repeatable host/bootstrap procedure. PS5 build-host
LLVM binaries, approved runtime and final hardware execution are not yet pinned
and demonstrated; therefore there is no byte-identical cross-host PS5 ELF claim.
Publish exact tool versions, source state, SDK archive hash, patches, build
commands, link map and artifact hashes when that milestone is reached.
