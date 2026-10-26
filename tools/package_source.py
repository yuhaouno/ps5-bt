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

from build_ps5 import SDK_FILES, SDK_REVISION, USB_SDK_HEADERS, APPLICATION_SDK_HEADERS, APPLICATION_VARIANTS
from target_sbc import source_inventory as target_sbc_source_inventory

ROOT = Path(__file__).resolve().parent.parent
PREFIX = "ps5-bt-source/"
REQUIRED_SOURCES = {
    "platform/ps5/bootstrap/start.S", "platform/ps5/bootstrap/runtime.c",
    "platform/ps5/main.c", "platform/ps5/adapter_stub.c",
    "platform/ps5/bootstrap/minimal.ld", "platform/ps5/bootstrap/bootstrap.h",
    "platform/ps5/bootstrap/syscall_checked.S",
}
NATIVE_SOURCES = {"platform/ps5/diagnostics/native_query.c", "platform/ps5/diagnostics/native_query.h"}
NATIVE_L2CAP_SOURCES = {
    "platform/ps5/diagnostics/native_l2cap_probe.c",
    "platform/ps5/diagnostics/native_l2cap_probe.h",
}
USB_SOURCES = {
    "platform/ps5/diagnostics/usb_probe.c", "platform/ps5/diagnostics/usb_probe.h",
    "platform/ps5/diagnostics/usb_descriptors.c", "platform/ps5/diagnostics/usb_descriptors.h",
}
CAPTURE_SOURCES = {
    "platform/ps5/diagnostics/capture_probe.c", "platform/ps5/diagnostics/capture_probe.h",
    "platform/ps5/diagnostics/capture_observe.c", "platform/ps5/diagnostics/capture_observe.h",
}
SBC_SELFTEST_SOURCES = {
    "platform/codec/ps5_bt_sbc_selftest.c",
    "platform/codec/ps5_bt_sbc_selftest.h",
}
TARGET_SBC_SOURCE_FILES = frozenset(target_sbc_source_inventory())
SBC_RELINK_DIRECTORY = PurePosixPath("build/ps5/ps5-bt-sbc-selftest-relink")
SBC_CONTINUOUS_SOURCES = {"platform/ps5/diagnostics/continuous_clock.c"}
CAPTURE_STREAM_SOURCES = {"platform/ps5/diagnostics/continuous_clock.c",
                          "platform/ps5/capture/capture_stream.c",
                          "platform/ps5/capture/capture_stream.h"}
APPLICATION_SOURCES = {
    "platform/ps5/application/posix_imports.c", "platform/ps5/application/posix_imports.h",
    "platform/ps5/application/posix_bridge.h", "platform/ps5/application/posix_redirect.h",
    "platform/ps5/scheduler/scheduler_clock.c", "platform/ps5/scheduler/scheduler_clock.h",
    "platform/store/bond_file_store.c", "platform/store/bond_file_store.h",
    "platform/control/tcp_listener.c", "platform/control/tcp_listener.h",
    "platform/ps5/capture/capture_stream.c", "platform/ps5/capture/capture_stream.h",
}
APP_VARIANTS = APPLICATION_VARIANTS
CODEC_VARIANTS = ("sbc_selftest", "sbc_continuous", "application_generated")


def relink_directory(manifest):
    variant = selected_variant(manifest)
    if variant not in CODEC_VARIANTS:
        raise ValueError("relink materials require a codec variant")
    return PurePosixPath("build/ps5/" + VARIANTS[variant][0] + "-relink")

VARIANTS = {
    "application_namespace": ("ps5-bt-application-namespace", APPLICATION_SOURCES | {"platform/ps5/application/namespace_probe.c", "platform/ps5/application/namespace_probe.h"}, "--application-namespace"),
    "application_setup": ("ps5-bt-application-setup", APPLICATION_SOURCES | {"platform/ps5/application/setup_namespace.c", "platform/ps5/application/setup_namespace.h"}, "--application-setup"),
    "application_control": ("ps5-bt-application-control", APPLICATION_SOURCES, "--application-control"),
    "application_generated": ("ps5-bt-application-generated", APPLICATION_SOURCES, "--application-generated"),
    "application_observe": ("ps5-bt-application-observe", APPLICATION_SOURCES, "--application-observe"),
    "capture_stream": ("ps5-bt-capture-stream", CAPTURE_STREAM_SOURCES, "--capture-stream"),
    "native_l2cap_probe": ("ps5-bt-native-l2cap-probe", NATIVE_L2CAP_SOURCES, "--native-l2cap-probe"),
    "native_query": ("ps5-bt-native-query", NATIVE_SOURCES, "--native-query"),
    "usb_metadata": ("ps5-bt-usb-metadata", USB_SOURCES, "--usb-metadata"),
    "capture_preflight": ("ps5-bt-capture-preflight", CAPTURE_SOURCES, "--capture-preflight"),
    "capture_once": ("ps5-bt-capture-once", CAPTURE_SOURCES, "--capture-once"),
    "capture_observe": ("ps5-bt-capture-observe", CAPTURE_SOURCES, "--capture-observe"),
    "sbc_selftest": ("ps5-bt-sbc-selftest", SBC_SELFTEST_SOURCES, "--sbc-selftest"),
    "sbc_continuous": ("ps5-bt-sbc-continuous", SBC_CONTINUOUS_SOURCES, "--sbc-continuous"),
}
REQUIRED_FILES = REQUIRED_SOURCES | {
    "LICENSE", "Makefile", "Cargo.lock", "Cargo.toml", "rust-toolchain.toml",
    "tools/build_ps5.py", "tools/inspect_elf.py", "tools/package_source.py",
    "tools/usb_sdk_headers.json", "tools/application_sdk_headers.json",
    "docs/RUNTIME-REVIEW.md", "docs/TOOLCHAIN.md", "docs/SOURCE-DISTRIBUTION.md",
} | TARGET_SBC_SOURCE_FILES


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


def sdk_files(source, usb_metadata=False, application_control=False):
    files = {}
    selected = SDK_FILES | (USB_SDK_HEADERS if usb_metadata else {}) | (APPLICATION_SDK_HEADERS if application_control else {})
    for name, expected in selected.items():
        relative = PurePosixPath(name)
        if relative.is_absolute() or ".." in relative.parts or str(relative) != name or "\\" in name:
            raise ValueError(f"invalid pinned SDK source path: {name}")
        path = source / name
        data = path.read_bytes()
        if sha256(data) != expected:
            raise ValueError(f"SDK source hash mismatch: {name}")
        files[name] = data
    return files


def selected_variant(manifest):
    active = []
    for key in VARIANTS:
        value = manifest.get(key, False)
        if type(value) is not bool:
            raise ValueError(f"manifest {key} must be a boolean")
        if value:
            active.append(key)
    if len(active) > 1:
        raise ValueError("manifest diagnostic variants are mutually exclusive")
    return active[0] if active else None


def valid_sha256(value):
    return (isinstance(value, str) and len(value) == 64 and
            all(character in "0123456789abcdef" for character in value))


def safe_relative(name, prefix=None):
    if not isinstance(name, str) or "\\" in name:
        raise ValueError("manifest path must be a POSIX string")
    path = PurePosixPath(name)
    if (not path.parts or str(path) == "." or path.is_absolute() or
            ".." in path.parts or str(path) != name):
        raise ValueError(f"invalid manifest path: {name}")
    if prefix is not None and not path.is_relative_to(prefix):
        raise ValueError(f"manifest path escapes {prefix}: {name}")
    return path


def validate_codec_sources(files, manifest):
    sources = manifest.get("codec_sources")
    if not isinstance(sources, dict) or sources.keys() != TARGET_SBC_SOURCE_FILES:
        raise ValueError("codec source inventory differs from target_sbc.source_inventory")
    for name, expected in sources.items():
        safe_relative(name)
        if not valid_sha256(expected) or name not in files:
            raise ValueError(f"invalid or untracked codec source: {name}")
        if sha256(files[name][0]) != expected:
            raise ValueError(f"codec source hash mismatch: {name}")
    return sources


def validate_relink_files(root, files, manifest, codec_sources):
    directory_relative = relink_directory(manifest)
    stem = VARIANTS[selected_variant(manifest)][0]
    recorded = manifest.get("relink_files")
    if not isinstance(recorded, dict) or not recorded:
        raise ValueError("missing relink file inventory")
    directory = root.joinpath(*directory_relative.parts)
    if not directory.is_dir() or directory.is_symlink():
        raise ValueError("missing or invalid SBC relink directory")

    actual_names = set()
    checks = {}
    entries = {}
    for path in directory.rglob("*"):
        if path.is_symlink():
            raise ValueError(f"relink bundle contains a symlink: {path}")
        if path.is_file():
            actual_names.add(path.relative_to(root).as_posix())
        elif not path.is_dir():
            raise ValueError(f"relink bundle contains a special file: {path}")
    if recorded.keys() != actual_names:
        raise ValueError("relink manifest differs from the complete bundle contents")

    for name, expected in recorded.items():
        relative = safe_relative(name, directory_relative)
        if not valid_sha256(expected):
            raise ValueError(f"invalid relink SHA-256: {name}")
        path = root.joinpath(*relative.parts)
        data = path.read_bytes()
        if sha256(data) != expected:
            raise ValueError(f"relink file hash mismatch: {name}")
        checks[path] = expected
        bundle_name = relative.relative_to(directory_relative).as_posix()
        entries[PREFIX + "relink/" + bundle_name] = (
            data, 0o755 if path.stat().st_mode & 0o111 else 0o644)

    bundle_path = directory / "bundle-manifest.json"
    try:
        bundle = json.loads(bundle_path.read_text())
    except (OSError, json.JSONDecodeError) as error:
        raise ValueError("invalid SBC relink bundle manifest") from error
    if (bundle.get("schema") != 1 or bundle.get("target") != "x86_64-sie-ps5" or
            bundle.get("output_name") != stem + ".elf"):
        raise ValueError("unexpected SBC relink bundle identity")

    app_objects = bundle.get("app_objects")
    if (not isinstance(app_objects, list) or not app_objects or
            len(app_objects) != len(set(app_objects))):
        raise ValueError("invalid ordered application object list")
    for name in app_objects:
        path = safe_relative(name)
        if path.parts[0] != "objects" or path.suffix != ".o":
            raise ValueError(f"invalid application object path: {name}")

    rust_archive = bundle.get("rust_archive")
    rust_path = safe_relative(rust_archive)
    if rust_path.parts[0] != "archives" or rust_path.suffix != ".a":
        raise ValueError("invalid Rust archive path in relink bundle")
    if bundle.get("linker_script") != "linker/minimal.ld":
        raise ValueError("unexpected relink linker script")
    if bundle.get("default_codec") != "codec/libps5_bt_sbc.a":
        raise ValueError("unexpected default replacement codec path")

    codec_sha256 = manifest.get("codec_archive_sha256")
    if (not valid_sha256(codec_sha256) or
            bundle.get("default_codec_sha256") != codec_sha256):
        raise ValueError("codec archive hash differs across build/relink manifests")

    immutable = bundle.get("immutable_sha256")
    expected_immutable = {
        *app_objects,
        rust_archive,
        "linker/minimal.ld",
        "relink.py",
        "support/inspect_elf.py",
        *("source/" + name for name in TARGET_SBC_SOURCE_FILES),
    }
    if not isinstance(immutable, dict) or immutable.keys() != expected_immutable:
        raise ValueError("relink immutable inventory is incomplete or unexpected")
    for name, expected in immutable.items():
        safe_relative(name)
        recorded_name = (directory_relative / name).as_posix()
        if not valid_sha256(expected) or recorded.get(recorded_name) != expected:
            raise ValueError(f"relink immutable hash mismatch: {name}")
    for name, expected in codec_sources.items():
        if immutable.get("source/" + name) != expected:
            raise ValueError(f"relink source copy differs from committed codec source: {name}")
    if immutable.get("linker/minimal.ld") != sha256(
            files["platform/ps5/bootstrap/minimal.ld"][0]):
        raise ValueError("relink linker script differs from committed project source")
    if immutable.get("support/inspect_elf.py") != sha256(
            files["tools/inspect_elf.py"][0]):
        raise ValueError("relink inspection helper differs from committed project source")

    artifacts = {
        f"artifacts/{stem}.elf",
        f"artifacts/{stem}.map",
        f"artifacts/{stem}.structure.json",
        f"artifacts/{stem}.inspect.txt",
        f"artifacts/{stem}.disasm.txt",
        f"artifacts/{stem}.relink-result.json",
    }
    expected_files = {
        ".ps5-bt-target-sbc-owned.json",
        "bundle-manifest.json",
        "codec/libps5_bt_sbc.a",
        *expected_immutable,
        *artifacts,
    }
    actual_bundle_files = {
        PurePosixPath(name).relative_to(directory_relative).as_posix()
        for name in actual_names
    }
    if actual_bundle_files != expected_files:
        raise ValueError("unexpected or missing file in SBC relink bundle")
    try:
        ownership = json.loads(
            (directory / ".ps5-bt-target-sbc-owned.json").read_text())
    except (OSError, json.JSONDecodeError) as error:
        raise ValueError("invalid SBC relink ownership marker") from error
    if ownership != {"kind": "relink", "schema": 1}:
        raise ValueError("unexpected SBC relink ownership marker")
    if not (directory / "relink.py").stat().st_mode & 0o111:
        raise ValueError("SBC relink script is not executable")

    def recorded_hash(name):
        return recorded[(directory_relative / name).as_posix()]

    if recorded_hash("codec/libps5_bt_sbc.a") != codec_sha256:
        raise ValueError("default relink codec does not match built codec archive")
    if recorded_hash(rust_archive) != manifest.get("rust_archive_sha256"):
        raise ValueError("relink Rust archive does not match linked runtime archive")
    if recorded_hash(f"artifacts/{stem}.elf") != manifest.get("sha256"):
        raise ValueError("baseline relink ELF does not match the distributed ELF")
    try:
        result = json.loads((directory / f"artifacts/{stem}.relink-result.json").read_text())
        structure = json.loads((directory / f"artifacts/{stem}.structure.json").read_text())
    except (OSError, json.JSONDecodeError) as error:
        raise ValueError("invalid baseline relink result or structure") from error
    expected_result_hashes = {
        "elf_sha256": recorded_hash(f"artifacts/{stem}.elf"),
        "codec_sha256": codec_sha256,
        "map_sha256": recorded_hash(f"artifacts/{stem}.map"),
        "structure_sha256": recorded_hash(f"artifacts/{stem}.structure.json"),
        "inspection_sha256": recorded_hash(f"artifacts/{stem}.inspect.txt"),
        "disassembly_sha256": recorded_hash(f"artifacts/{stem}.disasm.txt"),
    }
    if (not isinstance(result, dict) or result.get("schema") != 1 or
            not isinstance(structure, dict) or
            any(result.get(name) != expected
                for name, expected in expected_result_hashes.items()) or
            any("18.1.8" not in str(result.get(name, ""))
                for name in ("lld", "llvm_readobj", "llvm_objdump"))):
        raise ValueError("baseline relink result does not match its artifacts")
    return checks, entries


def validate_manifest(root, head, files, manifest_path):
    manifest_bytes = manifest_path.read_bytes()
    manifest = json.loads(manifest_bytes)
    if manifest.get("git_head") != head or manifest.get("git_status") != []:
        raise ValueError("manifest must identify the current clean committed HEAD; rebuild first")
    if manifest.get("sdk_revision") != SDK_REVISION or manifest.get("sdk_sources") != SDK_FILES:
        raise ValueError("manifest SDK revision/source hashes differ from the selected build inputs")
    if manifest.get("runtime") != "minimal-syscall-v6":
        raise ValueError("unsupported runtime in manifest")
    if not str(manifest.get("rust", "")).startswith("rustc 1.90.0 "):
        raise ValueError("manifest does not identify pinned Rust 1.90.0")
    if "18.1.8" not in str(manifest.get("llvm", "")) or "18.1.8" not in str(manifest.get("lld", "")):
        raise ValueError("manifest does not identify pinned LLVM/LLD 18.1.8")
    variant = selected_variant(manifest)
    if type(manifest.get("loader_kernel_resolver", False)) is not bool:
        raise ValueError("manifest loader_kernel_resolver must be a boolean")
    if variant in ("capture_preflight", "capture_once", "capture_observe", "capture_stream", *APP_VARIANTS) and not manifest.get("loader_kernel_resolver", False):
        raise ValueError("capture diagnostic requires the existing-loader kernel resolver")
    expected_headers = APPLICATION_SDK_HEADERS if variant in APP_VARIANTS else USB_SDK_HEADERS if variant == "usb_metadata" else {}
    if manifest.get("sdk_headers", {}) != expected_headers:
        raise ValueError("manifest SDK header inventory differs from the selected pinned variant")
    stem = VARIANTS[variant][0] if variant else "ps5-bt"
    if manifest_path.name != stem + ".manifest.json":
        raise ValueError("manifest filename differs from its selected build variant")
    if variant in ("sbc_continuous", "capture_stream", *APP_VARIANTS):
        if not valid_sha256(manifest.get("diagnostic_build_id")) or manifest.get("diagnostic_candidate") != stem:
            raise ValueError("missing or invalid embedded diagnostic identity")
    elif "diagnostic_build_id" in manifest or "diagnostic_candidate" in manifest:
        raise ValueError("unexpected embedded diagnostic identity for legacy variant")
    sources = manifest.get("project_sources")
    required = REQUIRED_SOURCES | (VARIANTS[variant][1] if variant else set())
    if not isinstance(sources, dict) or required != sources.keys():
        raise ValueError("manifest differs from the selected linked project source inventory")
    for name, expected in sources.items():
        if name in files:
            actual = sha256(files[name][0])
        else:
            raise ValueError(f"manifest project source is not tracked: {name}")
        if actual != expected:
            raise ValueError(f"manifest source hash mismatch: {name}")
    relink_entries = {}
    if variant in CODEC_VARIANTS:
        codec_sources = validate_codec_sources(files, manifest)
        relink_checks, relink_entries = validate_relink_files(
            root, files, manifest, codec_sources)
    else:
        for name in ("codec_sources", "codec_archive_sha256", "relink_files"):
            if name in manifest:
                raise ValueError(f"non-codec manifest contains {name}")
        relink_checks = {}
    if manifest.get("runtime_review_sha256") != sha256(files["docs/RUNTIME-REVIEW.md"][0]):
        raise ValueError("manifest runtime review hash mismatch")
    checks = {
        manifest_path.with_name(manifest_path.name.removesuffix(".manifest.json") + ".elf"): manifest.get("sha256"),
        root / "target/x86_64-unknown-none/release/libps5_runtime.a": manifest.get("rust_archive_sha256"),
    }
    for path, expected in checks.items():
        if sha256(path.read_bytes()) != expected:
            raise ValueError(f"manifest artifact hash mismatch: {path.name}")
    checks.update(relink_checks)
    return manifest, manifest_bytes, checks, relink_entries


def source_readme(manifest, manifest_name):
    kernel_flag = " --loader-kernel-resolver" if manifest.get("loader_kernel_resolver", False) else ""
    variant = selected_variant(manifest)
    variant_flag = " " + VARIANTS[variant][2] if variant else ""
    header_notice = (f"The USB diagnostic additionally includes {len(USB_SDK_HEADERS)} original pinned SDK headers.\n"
                     "Their file-specific BSD and other notices are retained verbatim; the project's\n"
                     "GPL text does not replace those individual terms. No headers are patched.\n"
                     if variant == "usb_metadata" else "")
    if variant in APP_VARIANTS:
        header_notice = (f"The application diagnostic additionally includes {len(APPLICATION_SDK_HEADERS)} original pinned SDK headers.\n"
                         "Their file-specific BSD and other notices are retained verbatim, including advertising clauses.\n"
                         "No SDK header is patched; standalone errno/fcntl/stdint aliases retain target notice-bearing bytes.\n")
    relink_contents = (
        "  relink/                  Application objects, Rust archive, default codec archive,\n"
        "                           linker script, inspection helper and replacement-link script.\n"
        if variant in CODEC_VARIANTS else "")
    relink_notice = (
        """
This SBC diagnostic combines GPL application/runtime material with the selected
BlueZ libsbc 2.2 library under LGPL-2.1-or-later. The complete selected library
source, notices and local patches are present in project/third_party/sbc and are
also copied under relink/source/ with exact hashes. relink/ contains the linked
application objects and a working replacement-link procedure; it intentionally
does not treat the default codec archive as immutable.

To relink with a compatible rebuilt or modified library, keep the immutable
files unchanged and run from relink/:

  python3 relink.py --codec /absolute/path/to/rebuilt/libps5_bt_sbc.a \\
    --output /absolute/path/to/relinked.elf \\
    --lld /absolute/path/to/LLVM-18.1.8/bin/ld.lld \\
    --readobj /absolute/path/to/LLVM-18.1.8/bin/llvm-readobj \\
    --objdump /absolute/path/to/LLVM-18.1.8/bin/llvm-objdump

The script checks the immutable object/source/linker hashes, records the actual
replacement codec hash and emits inspection/disassembly/result files beside the
chosen output. Relinking is a build result, not PS5 execution or codec quality
evidence. See project/docs/SBC-IMPLEMENTATION.md.
""" if variant in CODEC_VARIANTS else "")
    return f"""PS5-BT diagnostic corresponding source

Original project commit: {manifest['git_head']}
Associated ELF SHA-256: {manifest['sha256']}
SDK repository: https://github.com/ps5-payload-dev/sdk
SDK revision: {SDK_REVISION}
Runtime: minimal-syscall-v6; see project/docs/HW1-REPAIR.md for hardware evidence.

Contents:
  project/                 Every tracked project file at the stated clean commit.
  sdk/{SDK_REVISION}/  Hash-verified SDK inputs with their original notices.
{relink_contents}  build-manifest.json      Original build provenance; the associated standalone ELF remains separate.

The project LICENSE supplies GPL v3 text; original bootstrap work is
GPL-3.0-or-later. capture_probe.c and its capture_stream.c derivative are
conservatively GPL-3.0-only; combined capture/application diagnostics use GPL v3.
See project/docs/CAPTURE-PROBE.md and CAPTURE-STREAM.md for provenance.
Selected SDK CRT sources retain John Törnblom's original
copyright, GPL-3.0-or-later grant and warranty notices. No SDK patches are applied.
{header_notice}See project/docs/UPSTREAM.md and project/docs/RUNTIME-REVIEW.md.
{relink_notice}

Rebuild after installing Git, Make, Python 3.11+, a host C11 compiler, rustup
with Rust 1.90.0 and x86_64-unknown-none, and LLVM 18.1.8 (clang, ld.lld,
llvm-readobj, llvm-objdump, llvm-config). No SDK download is needed. Install
those tools/targets before going offline; the workspace has no registry crates.

  cd project
  git init -q
  git add .
  git -c user.name='Source rebuild' -c user.email='rebuild@localhost' commit -qm 'Local source snapshot'
  PS5_SDK_SOURCE="$(pwd)/../sdk/{SDK_REVISION}" \\
    LLVM_CONFIG=/absolute/path/to/llvm-18.1.8/bin/llvm-config python3 tools/build_ps5.py{kernel_flag}{variant_flag}

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
    manifest, manifest_bytes, artifacts, relink_entries = validate_manifest(
        root, head, files, manifest_path)
    sdk = sdk_files(source, manifest.get("usb_metadata", False), selected_variant(manifest) in APP_VARIANTS)
    entries = {PREFIX + "project/" + name: value for name, value in files.items()}
    entries.update({PREFIX + f"sdk/{SDK_REVISION}/" + name: (data, 0o644)
                    for name, data in sdk.items()})
    entries.update(relink_entries)
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
    original_headers = USB_SDK_HEADERS.copy()
    original_application_headers = APPLICATION_SDK_HEADERS.copy()
    try:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "project"
            root.mkdir()
            git(root, "init", "-q")
            fixture_files = REQUIRED_FILES | set().union(*(value[1] for value in VARIANTS.values()))
            for name in fixture_files:
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
            for name in USB_SDK_HEADERS:
                path = source / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(("synthetic retained BSD SDK header " + name).encode())
                USB_SDK_HEADERS[name] = sha256(path.read_bytes())
            for name in APPLICATION_SDK_HEADERS:
                path = source / name
                path.parent.mkdir(parents=True, exist_ok=True)
                if not path.exists():
                    path.write_bytes(("synthetic retained application BSD SDK header " + name).encode())
                APPLICATION_SDK_HEADERS[name] = sha256(path.read_bytes())
            files = tracked_files(root)
            elf = root / "build/ps5/ps5-bt.elf"
            elf.parent.mkdir(parents=True)
            elf.write_bytes(b"synthetic diagnostic ELF")
            rust = root / "target/x86_64-unknown-none/release/libps5_runtime.a"
            rust.parent.mkdir(parents=True)
            rust.write_bytes(b"synthetic Rust archive")

            def synthetic_relink_bundle(variant_elf):
                stem = variant_elf.stem
                directory_relative = PurePosixPath("build/ps5/" + stem + "-relink")
                directory = root.joinpath(*directory_relative.parts)
                directory.mkdir(parents=True)
                payloads = {
                    ".ps5-bt-target-sbc-owned.json": b'{"kind":"relink","schema":1}\n',
                    "relink.py": b"#!/usr/bin/env python3\n",
                    "objects/00-main.o": b"synthetic application object",
                    "archives/libps5_runtime.a": rust.read_bytes(),
                    "codec/libps5_bt_sbc.a": b"synthetic replaceable codec archive",
                    "linker/minimal.ld": files["platform/ps5/bootstrap/minimal.ld"][0],
                    "support/inspect_elf.py": files["tools/inspect_elf.py"][0],
                    f"artifacts/{stem}.elf": variant_elf.read_bytes(),
                    f"artifacts/{stem}.map": b"synthetic link map",
                    f"artifacts/{stem}.structure.json": b"{}\n",
                    f"artifacts/{stem}.inspect.txt": b"synthetic inspection",
                    f"artifacts/{stem}.disasm.txt": b"synthetic disassembly",
                }
                for name in TARGET_SBC_SOURCE_FILES:
                    payloads["source/" + name] = files[name][0]
                immutable_names = {
                    "objects/00-main.o",
                    "archives/libps5_runtime.a",
                    "linker/minimal.ld",
                    "relink.py",
                    "support/inspect_elf.py",
                    *("source/" + name for name in TARGET_SBC_SOURCE_FILES),
                }
                immutable = {name: sha256(payloads[name]) for name in immutable_names}
                result = {
                    "schema": 1,
                    "elf": f"synthetic/{stem}.elf",
                    "elf_sha256": sha256(
                        payloads[f"artifacts/{stem}.elf"]),
                    "codec": "synthetic/libps5_bt_sbc.a",
                    "codec_sha256": sha256(
                        payloads["codec/libps5_bt_sbc.a"]),
                    "map_sha256": sha256(
                        payloads[f"artifacts/{stem}.map"]),
                    "structure_sha256": sha256(
                        payloads[f"artifacts/{stem}.structure.json"]),
                    "inspection_sha256": sha256(
                        payloads[f"artifacts/{stem}.inspect.txt"]),
                    "disassembly_sha256": sha256(
                        payloads[f"artifacts/{stem}.disasm.txt"]),
                    "lld": "LLD 18.1.8",
                    "llvm_readobj": "LLVM 18.1.8",
                    "llvm_objdump": "LLVM 18.1.8",
                }
                payloads[f"artifacts/{stem}.relink-result.json"] = (
                    json.dumps(result, indent=2, sort_keys=True) + "\n").encode()
                bundle = {
                    "schema": 1,
                    "target": "x86_64-sie-ps5",
                    "output_name": f"{stem}.elf",
                    "app_objects": ["objects/00-main.o"],
                    "rust_archive": "archives/libps5_runtime.a",
                    "linker_script": "linker/minimal.ld",
                    "default_codec": "codec/libps5_bt_sbc.a",
                    "default_codec_sha256": sha256(payloads["codec/libps5_bt_sbc.a"]),
                    "immutable_sha256": immutable,
                }
                payloads["bundle-manifest.json"] = (
                    json.dumps(bundle, indent=2, sort_keys=True) + "\n").encode()
                for name, data in payloads.items():
                    path = directory / name
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_bytes(data)
                    if name == "relink.py":
                        path.chmod(0o755)
                relink_files = {
                    path.relative_to(root).as_posix(): sha256(path.read_bytes())
                    for path in directory.rglob("*") if path.is_file()
                }
                return (bundle["default_codec_sha256"], relink_files,
                        {name: sha256(files[name][0]) for name in TARGET_SBC_SOURCE_FILES})

            manifest = {
                "git_head": clean_head(root), "git_status": [], "sha256": sha256(elf.read_bytes()),
                "sdk_revision": SDK_REVISION, "sdk_sources": SDK_FILES.copy(),
                "runtime": "minimal-syscall-v6", "rust": "rustc 1.90.0 (fixture)",
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
            for key, (stem, extra_sources, flag) in VARIANTS.items():
                directory_relative = PurePosixPath("build/ps5/" + stem + "-relink")
                variant_elf = elf.with_name(stem + ".elf")
                variant_elf.write_bytes(("synthetic optional ELF " + key).encode())
                variant_sources = dict(manifest["project_sources"])
                variant_sources.update({name: sha256(files[name][0]) for name in extra_sources})
                variant_manifest = dict(manifest, **{key: True}, loader_kernel_resolver=True,
                                        sha256=sha256(variant_elf.read_bytes()),
                                        project_sources=variant_sources)
                if key in ("sbc_continuous", "capture_stream", *APP_VARIANTS):
                    variant_manifest["diagnostic_build_id"] = "a" * 64
                    variant_manifest["diagnostic_candidate"] = stem
                if key == "usb_metadata":
                    variant_manifest["sdk_headers"] = USB_SDK_HEADERS.copy()
                elif key in APP_VARIANTS:
                    variant_manifest["sdk_headers"] = APPLICATION_SDK_HEADERS.copy()
                if key in CODEC_VARIANTS:
                    codec_sha256, relink_files, codec_sources = synthetic_relink_bundle(
                        variant_elf)
                    variant_manifest.update(
                        codec_archive_sha256=codec_sha256,
                        codec_sources=codec_sources,
                        relink_files=relink_files,
                    )
                variant_path = variant_elf.with_suffix(".manifest.json")
                variant_path.write_text(json.dumps(variant_manifest))
                variant_destination = bundle_destination(variant_path)
                assert variant_destination.name == stem + "-source.tar.gz"
                package(root, source, variant_destination, variant_path)
                first_variant = variant_destination.read_bytes()
                package(root, source, variant_destination, variant_path)
                assert variant_destination.read_bytes() == first_variant
                with tarfile.open(variant_destination) as archive:
                    readme = archive.extractfile(PREFIX + "README.source.txt").read().decode()
                    assert "python3 tools/build_ps5.py --loader-kernel-resolver " + flag in readme
                    assert "project/build/ps5/" + stem + ".manifest.json" in readme
                    assert variant_manifest["sha256"] in readme
                    assert json.load(archive.extractfile(PREFIX + "build-manifest.json")) == variant_manifest
                    expected_sdk = SDK_FILES | (USB_SDK_HEADERS if key == "usb_metadata" else {}) | (APPLICATION_SDK_HEADERS if key in APP_VARIANTS else {})
                    sdk_prefix = PREFIX + f"sdk/{SDK_REVISION}/"
                    assert {item.name.removeprefix(sdk_prefix) for item in archive.getmembers()
                            if item.name.startswith(sdk_prefix)} == expected_sdk.keys()
                    for name in expected_sdk:
                        assert archive.extractfile(sdk_prefix + name).read() == (source / name).read_bytes()
                    if key in ("usb_metadata", *APP_VARIANTS):
                        assert "file-specific BSD and other notices" in readme
                    if key in CODEC_VARIANTS:
                        assert "--codec /absolute/path/to/rebuilt/libps5_bt_sbc.a" in readme
                        relink_prefix = PREFIX + "relink/"
                        assert {
                            item.name.removeprefix(relink_prefix)
                            for item in archive.getmembers()
                            if item.name.startswith(relink_prefix)
                        } == {
                            PurePosixPath(name).relative_to(
                                directory_relative).as_posix()
                            for name in relink_files
                        }
                    if key not in ("usb_metadata", *APP_VARIANTS):
                        assert "original pinned SDK headers" not in readme
                assert destination.read_bytes() == first, "variant replaced default bundle"

                def reject_variant(changed, label):
                    variant_path.write_text(json.dumps(changed))
                    variant_destination.write_bytes(b"stale variant")
                    try:
                        package(root, source, variant_destination, variant_path)
                    except (ValueError, OSError):
                        assert not variant_destination.exists(), label + " left stale output"
                    else:
                        raise AssertionError(label + " accepted")

                if key in ("sbc_continuous", "capture_stream", *APP_VARIANTS):
                    for bad_identity in (None, "invalid", "A" * 64):
                        reject_variant(dict(variant_manifest, diagnostic_build_id=bad_identity), "invalid embedded build identity")
                    reject_variant(dict(variant_manifest, diagnostic_candidate="ps5-bt-other"), "wrong embedded candidate")
                missing_sources = dict(variant_sources)
                missing_sources.pop(sorted(extra_sources)[0])
                reject_variant(dict(variant_manifest, project_sources=missing_sources), "missing variant source")
                reject_variant(dict(variant_manifest, **{key: 1}), "non-boolean variant")
                other_key = next(item for item in VARIANTS if item != key)
                reject_variant(dict(variant_manifest, **{other_key: True}), "mixed variants")
                escaped_sources = dict(variant_sources, **{"../outside-secret": "0" * 64})
                reject_variant(dict(variant_manifest, project_sources=escaped_sources), "project path escape")
                if key in ("capture_preflight", "capture_once", "capture_observe", "capture_stream", *APP_VARIANTS):
                    reject_variant(dict(variant_manifest, loader_kernel_resolver=False), "capture without resolver")
                if key in ("usb_metadata", *APP_VARIANTS):
                    selected_headers = USB_SDK_HEADERS if key == "usb_metadata" else APPLICATION_SDK_HEADERS
                    for invalid_headers in ({}, {"../outside-secret": "0" * 64},
                                            {"/private/tmp/outside-secret": "0" * 64}):
                        reject_variant(dict(variant_manifest, sdk_headers=invalid_headers), "SDK header inventory/path")
                    header_name = next(iter(selected_headers))
                    missing_headers = selected_headers.copy()
                    missing_headers.pop(header_name)
                    reject_variant(dict(variant_manifest, sdk_headers=missing_headers), "missing header pin")
                    changed_headers = dict(selected_headers, **{header_name: "0" * 64})
                    reject_variant(dict(variant_manifest, sdk_headers=changed_headers), "changed header pin")
                    header_path = source / header_name
                    original = header_path.read_bytes()
                    header_path.write_bytes(b"modified SDK header")
                    reject_variant(variant_manifest, "modified header bytes")
                    header_path.unlink()
                    reject_variant(variant_manifest, "missing header bytes")
                    header_path.write_bytes(original)
                if key == "application_generated":
                    for field in ("codec_sources", "codec_archive_sha256", "relink_files"):
                        missing_material = dict(variant_manifest)
                        missing_material.pop(field)
                        reject_variant(missing_material, "application-generated missing " + field)
                    missing_material = dict(variant_manifest)
                    missing_material.pop("sdk_headers")
                    reject_variant(missing_material, "application-generated missing SDK headers")
                if key in CODEC_VARIANTS:
                    missing_codec_sources = dict(codec_sources)
                    missing_codec_sources.pop(next(iter(missing_codec_sources)))
                    reject_variant(dict(variant_manifest,
                                        codec_sources=missing_codec_sources),
                                   "missing codec source")
                    changed_codec_sources = dict(codec_sources)
                    codec_name = next(iter(changed_codec_sources))
                    changed_codec_sources[codec_name] = "0" * 64
                    reject_variant(dict(variant_manifest,
                                        codec_sources=changed_codec_sources),
                                   "changed codec source hash")
                    escaped_relink = dict(relink_files,
                                          **{"../outside-object": "0" * 64})
                    reject_variant(dict(variant_manifest,
                                        relink_files=escaped_relink),
                                   "relink path escape")
                    reject_variant(dict(variant_manifest,
                                        codec_archive_sha256="0" * 64),
                                   "replacement codec hash mismatch")
                    relink_script = root.joinpath(
                        *directory_relative.parts, "relink.py")
                    original_relink_script = relink_script.read_bytes()
                    relink_script.write_bytes(b"modified relink script")
                    reject_variant(variant_manifest, "modified relink file")
                    relink_script.write_bytes(original_relink_script)
                    relink_script.chmod(0o755)
                variant_path.write_text(json.dumps(variant_manifest))
                package(root, source, variant_destination, variant_path)

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
                               ("sdk_sources", {}), ("sdk_headers", USB_SDK_HEADERS)):
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
        USB_SDK_HEADERS.clear()
        USB_SDK_HEADERS.update(original_headers)
        APPLICATION_SDK_HEADERS.clear()
        APPLICATION_SDK_HEADERS.update(original_application_headers)
    print("PASS source bundle content/determinism/default/query/USB/capture/SBC/application-control/generated/observe/setup/namespace variants; pinned original SDK headers/notices and codec relink inputs; dirty tree, invalid variant, path escape and manifest/source/ELF/archive/relink/SDK mismatch rejection; no stale output (synthetic fixtures, no hardware)")


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
