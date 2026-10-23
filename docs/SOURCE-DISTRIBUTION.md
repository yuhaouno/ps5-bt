# Diagnostic corresponding-source package

A diagnostic ELF handed to another person must be accompanied by the actual
corresponding source and retained license notices. A GitHub link alone does not
supply that package. This packaging workflow is for the reviewed
`minimal-syscall-v1` diagnostic, whose adapter returns Unsupported; it establishes
no PS5, firmware, loader, controller or audio compatibility and is not a release.

After the intended project changes are committed, rebuild the ELF from the clean
commit using the pinned toolchain described in [TOOLCHAIN.md](TOOLCHAIN.md).
Keep the resulting ELF, manifest and Rust archive locally. Then run:

```sh
python3 tools/package_source.py
```

The offline command writes `build/ps5/ps5-bt-source.tar.gz`. It uses the three
already available SDK sources in `.sdk/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/`,
or the same `PS5_SDK_SOURCE` directory used for the build. It never fetches files.
For the separately built optional query artifact, select its manifest explicitly:

```sh
python3 tools/package_source.py --manifest build/ps5/ps5-bt-native-query.manifest.json
```

This writes `build/ps5/ps5-bt-native-query-source.tar.gz`, binds the package to
`ps5-bt-native-query.elf`, and supplies the `make ps5-native-query` rebuild target.
The default command remains bound to `ps5-bt.elf`. Run
`python3 tools/package_source.py --self-test` for synthetic packaging tests.

Packaging requires a clean Git worktree, including no untracked project files,
and the same clean Git HEAD recorded in the build manifest. It verifies the ELF
and Rust archive SHA-256, linked project source hashes, runtime review hash, SDK
revision and all three selected SDK source hashes against the builder's pinned
inventory. A dirty or stale build fails; the previous source package is removed
before validation, so a failure cannot leave a stale package as apparent output.
Commit changes and rebuild instead of editing a manifest to bypass these checks.

The archive contains:

- `ps5-bt-source/project/`: every tracked file from the clean project commit,
  including Rust/C/assembly sources, headers, tests, build/inspection/packaging
  scripts, toolchain configuration, lock files, documentation and GPL text in
  `LICENSE`. Tracked file modes are retained; ignored and untracked files are
  excluded. Tracked files matching ignore rules, symlinks and submodules fail
  explicitly rather than silently including artifacts or losing source.
- `ps5-bt-source/sdk/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/`: exactly
  `crt/syscall.c`, `crt/syscall.h` and `crt/payload.h`, with their original bytes
  and John Törnblom's copyright, GPL-3.0-or-later and warranty notices intact.
  No local SDK patches are applied. See [UPSTREAM.md](UPSTREAM.md).
- `ps5-bt-source/build-manifest.json`: the original ELF build record, including
  its SHA-256 and the original clean project commit. The ELF, Rust archive,
  tools, SDK binaries, console files and Git history are not bundled.
- `ps5-bt-source/README.source.txt`: concrete rebuild commands using the bundled
  selected SDK tree through `PS5_SDK_SOURCE`, Rust 1.90.0 and LLVM 18.1.8.

The extracted project has no `.git` directory. Its README explicitly creates a
new local Git snapshot because the builder records Git provenance. That local
commit differs from the original build commit; the included original manifest
remains the record for the associated ELF. A rebuilt ELF receives its own
manifest. Install the documented external tools and Rust target before an offline
rebuild. The project has no third-party Cargo registry dependencies. Tools are
not included. A second macOS arm64 machine reproduced the baseline ELF hash
recorded in [WAVE-1-REPORT.md](WAVE-1-REPORT.md); byte-identical output on other
hosts is not claimed.

Tar member ordering, ownership, timestamps and gzip timestamp/name are normalized;
repeating packaging for identical committed source, SDK bytes and manifest must
produce the same package bytes. This is package determinism, not proof of runtime
correctness or cross-host compiler reproducibility. Review staged project content
before committing: packaging includes every tracked file and cannot decide
whether a newly committed file contains a secret or inappropriate material.

Distribute the source package together with the matching ELF and its original
manifest; identify both hashes. Retain the package and exact build provenance
for any subsequent distribution obligations. The root GPL text does not replace
file-specific terms. This is a technical source-delivery mechanism, not a final
legal determination of every possible distribution arrangement.

The build requires no Sony binary, firmware, key, exploit, permission patch,
SDK stock CRT, kernel-memory helper or bundled loader. Existing permission and
compatible-loader prerequisites, actual loader behavior and PS5 Slim firmware
13.40 remain external and hardware-untested; see [RUNTIME-REVIEW.md](RUNTIME-REVIEW.md)
and [HARDWARE-TEST.md](HARDWARE-TEST.md). There is no project installation bypass
or console deployment procedure. If a distribution arrangement requires further
installation information, resolve that obligation for its actual environment
before distribution; the package does not claim an exemption or supply unknown
firmware-specific facts.
