#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Package corresponding source for an existing clean-commit diagnostic build, offline."""
import argparse
import gzip
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import subprocess
import tarfile
import tempfile

from build_ps5 import SDK_FILES, SDK_REVISION

ROOT = Path(__file__).resolve().parent.parent
PREFIX = "ps5-bt-source/"
REQUIRED_SOURCES = {
    "platform/ps5/bootstrap/start.S", "platform/ps5/bootstrap/runtime.c",
    "platform/ps5/main.c", "platform/ps5/adapter_stub.c",
    "platform/ps5/bootstrap/minimal.ld", "platform/ps5/bootstrap/bootstrap.h",
}
NATIVE_SOURCES = {"platform/ps5/diagnostics/native_query.c", "platform/ps5/diagnostics/native_query.h"}
REQUIRED_FILES = REQUIRED_SOURCES | {
    "LICENSE", "Makefile", "Cargo.lock", "Cargo.toml", "rust-toolchain.toml",
    "tools/build_ps5.py", "tools/inspect_elf.py", "tools/package_source.py",
    "docs/RUNTIME-REVIEW.md", "docs/TOOLCHAIN.md", "docs/SOURCE-DISTRIBUTION.md",
}


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def git(root, *args):
    return subprocess.run(["git", *args], cwd=root, check=True,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE).stdout


def clean_head(root):
    if git(root, "status", "--porcelain=v1", "--untracked-files=all",
           "--ignore-submodules=none"):
        raise ValueError("source packaging requires a clean committed worktree")
    return git(root, "rev-parse", "HEAD").decode().strip()


def tracked_files(root):
    if git(root, "ls-files", "-ci", "--exclude-standard", "-z"):
        raise ValueError("tracked files match ignore rules; review artifacts before packaging")
    files = {}
    for record in git(root, "ls-tree", "-rz", "--full-tree", "HEAD").split(b"\0"):
        if not record:
            continue
        info, raw_name = record.split(b"\t", 1)
        mode, kind, oid = info.split()
        name = raw_name.decode("utf-8")
        path = PurePosixPath(name)
        if path.is_absolute() or ".." in path.parts or kind != b"blob" or mode not in (b"100644", b"100755"):
            raise ValueError(f"unsupported tracked path/type: {name}")
        # Committed blobs prevent ignored/untracked files and working-file races entering the archive.
        files[name] = (git(root, "cat-file", "blob", oid.decode()),
                       0o755 if mode == b"100755" else 0o644)
    if not REQUIRED_FILES <= files.keys():
        raise ValueError("missing tracked build/license/source files: " +
                         ", ".join(sorted(REQUIRED_FILES - files.keys())))
    return files


def sdk_files(source):
    files = {}
    for name, expected in SDK_FILES.items():
        path = source / name
        data = path.read_bytes()
        if sha256(data) != expected:
            raise ValueError(f"SDK source hash mismatch: {name}")
        files[name] = data
    return files


def validate_manifest(root, head, files, manifest_path):
    manifest_bytes = manifest_path.read_bytes()
    manifest = json.loads(manifest_bytes)
    if manifest.get("git_head") != head or manifest.get("git_status") != []:
        raise ValueError("manifest must identify the current clean committed HEAD; rebuild first")
    if manifest.get("sdk_revision") != SDK_REVISION or manifest.get("sdk_sources") != SDK_FILES:
        raise ValueError("manifest SDK revision/source hashes differ from the selected build inputs")
    if manifest.get("runtime") != "minimal-syscall-v5":
        raise ValueError("unsupported runtime in manifest")
    if not str(manifest.get("rust", "")).startswith("rustc 1.90.0 "):
        raise ValueError("manifest does not identify pinned Rust 1.90.0")
    if "18.1.8" not in str(manifest.get("llvm", "")) or "18.1.8" not in str(manifest.get("lld", "")):
        raise ValueError("manifest does not identify pinned LLVM/LLD 18.1.8")
    native_query = manifest.get("native_query", False)
    if type(native_query) is not bool:
        raise ValueError("manifest native_query must be a boolean")
    stem = "ps5-bt-native-query" if native_query else "ps5-bt"
    if manifest_path.name != stem + ".manifest.json":
        raise ValueError("manifest filename differs from its selected build variant")
    sources = manifest.get("project_sources")
    required = REQUIRED_SOURCES | (NATIVE_SOURCES if native_query else set())
    if not isinstance(sources, dict) or not required <= sources.keys():
        raise ValueError("manifest lacks the linked project source inventory")
    for name, expected in sources.items():
        if name in files:
            actual = sha256(files[name][0])
        else:
            raise ValueError(f"manifest project source is not tracked: {name}")
        if actual != expected:
            raise ValueError(f"manifest source hash mismatch: {name}")
    if manifest.get("runtime_review_sha256") != sha256(files["docs/RUNTIME-REVIEW.md"][0]):
        raise ValueError("manifest runtime review hash mismatch")
    checks = {
        manifest_path.with_name(manifest_path.name.removesuffix(".manifest.json") + ".elf"): manifest.get("sha256"),
        root / "target/x86_64-unknown-none/release/libps5_runtime.a": manifest.get("rust_archive_sha256"),
    }
    for path, expected in checks.items():
        if sha256(path.read_bytes()) != expected:
            raise ValueError(f"manifest artifact hash mismatch: {path.name}")
    return manifest, manifest_bytes, checks


def source_readme(manifest, manifest_name):
    target = "ps5-native-query" if manifest.get("native_query", False) else "ps5-link"
    return f"""PS5-BT diagnostic corresponding source

Original project commit: {manifest['git_head']}
Associated ELF SHA-256: {manifest['sha256']}
SDK repository: https://github.com/ps5-payload-dev/sdk
SDK revision: {SDK_REVISION}
Runtime: minimal-syscall-v5; hardware entry remains unverified; adapter Unsupported.

Contents:
  project/                 Every tracked project file at the stated clean commit.
  sdk/{SDK_REVISION}/  Exactly three unmodified selected SDK source files.
  build-manifest.json      Original build provenance; no ELF or Rust archive included.

The project LICENSE supplies GPL v3 text; original project work is
GPL-3.0-or-later. Each SDK file retains John Törnblom's original copyright,
GPL-3.0-or-later grant and warranty notices. No SDK patches are applied.
See project/docs/UPSTREAM.md and project/docs/RUNTIME-REVIEW.md.

Rebuild after installing Git, Make, Python 3.11+, a host C11 compiler, rustup
with Rust 1.90.0 and x86_64-unknown-none, and LLVM 18.1.8 (clang, ld.lld,
llvm-readobj, llvm-objdump, llvm-config). No SDK download is needed. Install
those tools/targets before going offline; the workspace has no registry crates.

  cd project
  git init -q
  git add .
  git -c user.name='Source rebuild' -c user.email='rebuild@localhost' commit -qm 'Local source snapshot'
  PS5_SDK_SOURCE="$(pwd)/../sdk/{SDK_REVISION}" \\
    LLVM_CONFIG=/absolute/path/to/llvm-18.1.8/bin/llvm-config make {target}

The new local Git commit is only for the builder's provenance recording; it
will differ from the original commit recorded above. Keep build-manifest.json
outside project/ as the original build record. The rebuilt manifest is written
to project/build/ps5/{manifest_name}. Exact observed native tool hashes and
versions are in the original manifest and project/docs/TOOLCHAIN.md. Tools are
not bundled. Byte-identical cross-host ELF output is not promised.

This source package neither deploys anything nor establishes PS5, firmware,
loader, controller, Bluetooth or audio compatibility. Required installation
information and existing-loader prerequisites are documented in
project/docs/SOURCE-DISTRIBUTION.md and project/docs/HARDWARE-TEST.md.
""".encode()


def package(root, source, destination, manifest_path=None):
    destination.unlink(missing_ok=True)
    manifest_path = manifest_path or root / "build/ps5/ps5-bt.manifest.json"
    if not manifest_path.name.endswith(".manifest.json"):
        raise ValueError("manifest filename must end with .manifest.json")
    head = clean_head(root)
    files = tracked_files(root)
    sdk = sdk_files(source)
    manifest, manifest_bytes, artifacts = validate_manifest(root, head, files, manifest_path)
    entries = {PREFIX + "project/" + name: value for name, value in files.items()}
    entries.update({PREFIX + f"sdk/{SDK_REVISION}/" + name: (data, 0o644)
                    for name, data in sdk.items()})
    entries[PREFIX + "build-manifest.json"] = (manifest_bytes, 0o644)
    entries[PREFIX + "README.source.txt"] = (source_readme(manifest, manifest_path.name), 0o644)
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(dir=destination.parent, prefix=".source-", delete=False) as output:
            temporary = Path(output.name)
            with gzip.GzipFile(filename="", mode="wb", fileobj=output, mtime=0) as compressed:
                with tarfile.open(mode="w", fileobj=compressed, format=tarfile.USTAR_FORMAT) as archive:
                    for name, (data, mode) in sorted(entries.items()):
                        info = tarfile.TarInfo(name)
                        info.size, info.mode, info.mtime = len(data), mode, 0
                        info.uid = info.gid = 0
                        info.uname = info.gname = ""
                        archive.addfile(info, io.BytesIO(data))
        if clean_head(root) != head:
            raise ValueError("HEAD changed while packaging")
        for path, expected in artifacts.items():
            if sha256(path.read_bytes()) != expected:
                raise ValueError("build artifact changed while packaging")
        if manifest_path.read_bytes() != manifest_bytes:
            raise ValueError("manifest changed while packaging")
        os.replace(temporary, destination)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)
    return manifest


def bundle_destination(manifest_path):
    if not manifest_path.name.endswith(".manifest.json"):
        raise ValueError("manifest filename must end with .manifest.json")
    stem = manifest_path.name.removesuffix(".manifest.json")
    return manifest_path.with_name(stem + "-source.tar.gz")


def self_test():
    # Synthetic bytes replace only SDK provenance constants; real Git, hash,
    # manifest, tar/gzip and filesystem paths are exercised without toolchain/network.
    original_sdk = SDK_FILES.copy()
    try:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "project"
            root.mkdir()
            git(root, "init", "-q")
            for name in REQUIRED_FILES | NATIVE_SOURCES:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("fixture source\n")
            (root / ".gitignore").write_text("/build/\n/target/\n/sdk/\n")
            git(root, "add", ".")
            git(root, "-c", "user.name=Fixture", "-c", "user.email=fixture@localhost",
                "commit", "-qm", "Fixture")
            source = root / "sdk"
            for name in SDK_FILES:
                path = source / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(("synthetic SDK " + name).encode())
                SDK_FILES[name] = sha256(path.read_bytes())
            files = tracked_files(root)
            elf = root / "build/ps5/ps5-bt.elf"
            elf.parent.mkdir(parents=True)
            elf.write_bytes(b"synthetic diagnostic ELF")
            rust = root / "target/x86_64-unknown-none/release/libps5_runtime.a"
            rust.parent.mkdir(parents=True)
            rust.write_bytes(b"synthetic Rust archive")
            manifest = {
                "git_head": clean_head(root), "git_status": [], "sha256": sha256(elf.read_bytes()),
                "sdk_revision": SDK_REVISION, "sdk_sources": SDK_FILES.copy(),
                "runtime": "minimal-syscall-v5", "rust": "rustc 1.90.0 (fixture)",
                "llvm": "clang 18.1.8", "lld": "LLD 18.1.8",
                "rust_archive_sha256": sha256(rust.read_bytes()),
                "project_sources": {name: sha256(files[name][0]) for name in REQUIRED_SOURCES},
                "runtime_review_sha256": sha256(files["docs/RUNTIME-REVIEW.md"][0]),
            }
            manifest_path = elf.with_suffix(".manifest.json")
            manifest_path.write_text(json.dumps(manifest))
            destination = elf.parent / "ps5-bt-source.tar.gz"
            package(root, source, destination)
            first = destination.read_bytes()
            package(root, source, destination)
            assert destination.read_bytes() == first, "nondeterministic archive"
            with tarfile.open(fileobj=io.BytesIO(first), mode="r:gz") as archive:
                members = archive.getmembers()
                expected = {PREFIX + "project/" + name for name in files}
                expected |= {PREFIX + f"sdk/{SDK_REVISION}/" + name for name in SDK_FILES}
                expected |= {PREFIX + "build-manifest.json", PREFIX + "README.source.txt"}
                assert {item.name for item in members} == expected
                assert all(item.mtime == item.uid == item.gid == 0 for item in members)
                for name, (data, _) in files.items():
                    assert archive.extractfile(PREFIX + "project/" + name).read() == data
                for name in SDK_FILES:
                    assert archive.extractfile(PREFIX + f"sdk/{SDK_REVISION}/" + name).read() == (source / name).read_bytes()
            query_elf = elf.with_name("ps5-bt-native-query.elf")
            query_elf.write_bytes(b"synthetic optional query ELF")
            query_sources = dict(manifest["project_sources"])
            query_sources.update({name: sha256(files[name][0]) for name in NATIVE_SOURCES})
            query_manifest = dict(manifest, native_query=True, sha256=sha256(query_elf.read_bytes()),
                                  project_sources=query_sources)
            query_manifest_path = query_elf.with_suffix(".manifest.json")
            query_manifest_path.write_text(json.dumps(query_manifest))
            query_destination = bundle_destination(query_manifest_path)
            assert query_destination.name == "ps5-bt-native-query-source.tar.gz"
            package(root, source, query_destination, query_manifest_path)
            with tarfile.open(query_destination) as archive:
                readme = archive.extractfile(PREFIX + "README.source.txt").read().decode()
                assert "make ps5-native-query" in readme
                assert "project/build/ps5/ps5-bt-native-query.manifest.json" in readme
                assert query_manifest["sha256"] in readme
                assert json.load(archive.extractfile(PREFIX + "build-manifest.json")) == query_manifest
            assert destination.read_bytes() == first, "variant replaced default bundle"

            def reject(label):
                destination.write_bytes(b"stale")
                try:
                    package(root, source, destination)
                except (ValueError, OSError):
                    assert not destination.exists(), label + " left stale output"
                else:
                    raise AssertionError(label + " accepted")

            dirty = root / "untracked"
            dirty.write_text("dirty")
            reject("untracked source")
            dirty.unlink()
            dirty = root / "LICENSE"
            dirty.write_text("modified")
            reject("modified source")
            git(root, "checkout", "--", "LICENSE")
            dirty.write_text("staged modification")
            git(root, "add", "LICENSE")
            reject("staged source")
            git(root, "reset", "-q", "HEAD", "--", "LICENSE")
            git(root, "checkout", "--", "LICENSE")
            for key, value in (("git_head", "0" * 40), ("git_status", [" M README.md"]),
                               ("sha256", "0" * 64), ("rust_archive_sha256", "0" * 64),
                               ("runtime_review_sha256", "0" * 64),
                               ("native_query", True), ("sdk_revision", "wrong"),
                               ("sdk_sources", {})):
                changed = dict(manifest, **{key: value})
                manifest_path.write_text(json.dumps(changed))
                reject(key + " mismatch")
            changed = dict(manifest, project_sources=dict(manifest["project_sources"], **{"platform/ps5/main.c": "0" * 64}))
            manifest_path.write_text(json.dumps(changed))
            reject("project source mismatch")
            manifest_path.write_text(json.dumps(manifest))
            (source / "crt/syscall.c").write_text("modified SDK")
            reject("SDK mismatch")
    finally:
        SDK_FILES.clear()
        SDK_FILES.update(original_sdk)
    print("PASS source bundle content/determinism/optional variant; dirty tree and manifest/source/ELF/archive/SDK mismatch rejection; no stale output (synthetic fixtures, no hardware)")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sdk-source", default=os.environ.get("PS5_SDK_SOURCE"),
                        help="already available SDK source tree; never downloads")
    parser.add_argument("--manifest", type=Path, default=ROOT / "build/ps5/ps5-bt.manifest.json",
                        help="build manifest; associated ELF and output source bundle use its stem")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    try:
        if args.self_test:
            self_test()
        else:
            manifest_path = args.manifest.resolve()
            destination = bundle_destination(manifest_path)
            source = Path(args.sdk_source).resolve() if args.sdk_source else ROOT / ".sdk" / SDK_REVISION
            manifest = package(ROOT, source, destination, manifest_path)
            print(f"PASS corresponding source: {destination}\nSHA-256 {sha256(destination.read_bytes())}\nELF SHA-256 {manifest['sha256']}\nHardware UNTESTED; no release or deployment claim.")
    except (OSError, ValueError, KeyError, TypeError, tarfile.TarError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Source packaging failed: {error}\n")
