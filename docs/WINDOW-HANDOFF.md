# Portable PS5 test-window handoff

Status: offline exporter, inspector and safe extractor implemented with synthetic
Git/source/kit tests. Export and inspection perform no network operation, upload,
console launch, installation or log retrieval. Native results and the next console
window remain **Requires hardware** and need current execution authorization.

The transfer contains the selected validated kit, all corresponding source
bundles, and one ready-to-use project source tree at the same original committed
HEAD. It supports a nonempty selected plan of reviewed candidates, including the
separately selected `application-setup` artifact; it preserves the plan's order.
It does not infer acceptance from a successful build or transfer.

## Create the transfer on the development machine

Commit the reviewed project and build/package/assemble a matching clean-HEAD kit
first. The exporter refuses a dirty worktree, stale kit, modified artifact or
source mismatch. The temporary operator guide, if tracked at
`docs/TEMP-PS5-TEST-HANDOFF.md`, travels with the complete project tree. Keep all
transfer outputs under ignored `build/` or `dist/`; never commit the archive,
bootstrap copy, payloads, raw logs or private runtime state.

```sh
python3 tools/window_handoff.py export \
  --kit-dir build/ps5/next-window-kit-20261026 \
  --output build/ps5/ps5-bt-window-transfer.tar.gz
```

This creates three new mode-0600 files, refusing any existing file or symlink:

- `ps5-bt-window-transfer.tar.gz`: deterministic transfer archive.
- `ps5-bt-window-transfer.tar.gz.bootstrap.py`: the exact committed standalone
  standard-library-only `tools/window_handoff.py` for initial inspection/extraction.
- `ps5-bt-window-transfer.tar.gz.sha256`: the archive SHA-256 and filename.

Transfer all three privately to the next machine and retain the archive hash
through a trusted channel. A hash sidecar received beside a modified archive is
not an independent authentication mechanism. The exporter includes no ignored
files or saved console logs. It consumes only committed project blobs and the
kit's exact expected ELF/manifest/source/runbook/plan files; extra kit files,
symlinked inputs, unsupported tracked types or ignored tracked files fail.

Every kit artifact's bytes/size, manifest hash, clean source HEAD, selected variant,
loader flag and applicable embedded build identity must match. Slots are fixed to
`ps5-bt-<candidate-id>`, including the legacy `ps5-bt-usb-binding` and
`ps5-bt-capture-settled` aliases whose ELF stems remain usb-metadata and
capture-once. Each marker/stop list has 1–32 nonblank strings, each at most 512
UTF-8 bytes with no control characters. Export additionally requires RUNBOOK.md
to match the existing kit assembler's canonical rendering of that exact plan,
HEAD and entries. It reuses the trusted local renderer only during export;
received-source code is never executed by inspection or extraction.
Each intact source
bundle must contain its exact build manifest and a complete project tree with
bytes/modes equal to current committed Git blobs. Codec candidates retain the
source/license/replacement-link materials. No SDK binaries, external toolchains,
console credentials or `.git` history are obtained or added by this tool. Existing
corresponding-source bundles retain their source headers and any declared codec
replacement-link evidence unchanged.

## Inspect and unpack on the receiving machine

Only Python 3.11+ is needed for these steps. Git, Rust, LLVM and an SDK installation
are unnecessary for inspection, extraction, tool dry-runs or collecting an already
built candidate's saved results. Bootstrap inspection/extraction imports no
sibling project module. Run it with the trusted expected archive hash:

```sh
python3 ps5-bt-window-transfer.tar.gz.bootstrap.py inspect \
  ps5-bt-window-transfer.tar.gz --expect-sha256 <trusted-64-digit-sha256>
python3 ps5-bt-window-transfer.tar.gz.bootstrap.py extract \
  ps5-bt-window-transfer.tar.gz --expect-sha256 <trusted-64-digit-sha256> \
  --destination "$PWD/ps5-bt-window"
cd ps5-bt-window
python3 tools/console_window.py --preflight \
  --kit build/ps5/next-window-kit-20261026
```

Replace the hash placeholder; it is not a literal shell argument. Use an existing
real parent directory and a destination that does not yet exist. Symlinked parent
components are rejected; on systems where `/tmp` or `/var` is a symlink, choose
its real directory path. Extraction validates the whole archive and nested kit
before writing, stages privately, reserves a fresh final directory exclusively,
and preserves source executable bits. Existing destination files/directories are
never merged or overwritten. This is local extraction, not a console action or
a power-loss durability guarantee.

The archive has one `ps5-bt-window/` prefix. Extraction places its contents directly
inside the requested destination, so `tools/`, `docs/`, `Cargo.toml` and `build/`
are siblings. There is one expanded project tree, taken from a verified source
bundle; every original source bundle remains intact beside its matching ELF.
`HANDOFF.json` records source HEAD, relative kit path and each payload member's
SHA-256, size and mode. `START-HERE.md` gives the initial commands. Relative project
layout makes tool imports and candidate paths independent of the old machine.
Historical build manifests can retain old absolute compiler/tool paths as
provenance; no receiver command needs to reuse those paths.

The final command above is an offline preflight by default. Follow
`docs/TEMP-PS5-TEST-HANDOFF.md`, `docs/CONSOLE-WINDOW.md` and the kit's `RUNBOOK.md`
for actual sequence, finite bounds, new-window confirmation and controller checks.
The setup artifact has its own limited namespace contract; inclusion in the kit
is not permission to run it. Save and correlate each run before another launch
can replace the console log. Unknown PCM format, timestamp units, Bluetooth
ownership/credits, audio quality and persistence remain Unknown.

## Rebuild and project provenance

The expanded tree deliberately contains no `.git` directory. Inspection and
collection run directly; rebuilding requires external tools from
[TOOLCHAIN.md](TOOLCHAIN.md): Git, Python 3.11+, pinned Rust 1.90.0 with
`x86_64-unknown-none`, a host C11 compiler, Make and the documented codec tools,
and LLVM 18.1.8. These tools are not bundled or installed automatically.

For continued development or a build with the original source identity, clone
the actual project repository and check out the recorded original HEAD. The
matching kit source bundle includes `README.source.txt` and the reviewed pinned
SDK **source** subset for offline reconstruction. Use its concrete instructions
with locally installed external tools. A source-only local Git snapshot is a new
commit with new provenance; never manufacture the original `.git` state, replace
the kit's original manifests, or describe a local reconstructed commit as the
original build. Rebuilt artifacts get their own manifests and fresh test kit.
See [SOURCE-DISTRIBUTION.md](SOURCE-DISTRIBUTION.md) for notices and codec relinking.

## Bounds and checks

Archive inputs are regular files, at most 128 MiB compressed; individual members
are at most 64 MiB, with at most 20,000 files and 256 MiB expanded payload. The
entire decoded tar stream is also bounded, including header overhead. JSON input
is capped at 2 MiB and duplicate keys/non-finite values fail. Archive members must
be normalized relative ASCII paths and regular files with modes 0644/0755. Links,
special files, absolute/escaping names, duplicate members, file/directory aliases,
`.git` components and mismatched inventories fail before extraction. Manifests
bind local inputs; they neither authenticate PS5 execution nor make arbitrary
untrusted transferred Python source safe to execute.

```sh
python3 tests/tools/test_window_handoff.py
python3 -m py_compile tools/window_handoff.py tests/tools/test_window_handoff.py
```

Fixtures use real temporary Git repositories, synthetic ELF/source bundles and the
actual extracted Python preflight. They check deterministic export, three/six-item
plans, setup's distinct identity, standalone bootstrap use, preserved executable
mode, clean/stale/mutated source and kit rejection, exact current/legacy slots,
malformed/oversized marker/stop lists, canonical runbook, strict inventory, no overwrite,
link/traversal/type/size rejection, failed-publication cleanup and redacted errors.
They perform no network or PS5 operation. A real clean-HEAD export must still be
run against the final rebuilt kit after the relevant source commits.
