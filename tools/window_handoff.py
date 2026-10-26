#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Offline portable window kit export, inspection and exclusive safe extraction."""
from __future__ import annotations

import argparse
import gzip
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile
import zlib

ROOT = Path(__file__).resolve().parents[1]
PREFIX = "ps5-bt-window/"
KIT = "build/ps5/next-window-kit-20261026"
MAX_FILES = 20_000
MAX_FILE_BYTES = 64 * 1024 * 1024
MAX_TOTAL_BYTES = 256 * 1024 * 1024
MAX_ARCHIVE_BYTES = 128 * 1024 * 1024
MAX_JSON_BYTES = 2 * 1024 * 1024
MAX_PLAN_TEXT_ITEMS = 32
MAX_PLAN_TEXT_BYTES = 512
CANDIDATES = {
    "usb-binding": "usb_metadata", "capture-settled": "capture_once",
    "sbc-selftest": "sbc_selftest", "capture-observe": "capture_observe",
    "sbc-continuous": "sbc_continuous", "capture-stream": "capture_stream",
    "application-setup": "application_setup",
    "application-namespace": "application_namespace",
    "application-open-compare": "application_open_compare",
    "application-control": "application_control",
    "application-generated": "application_generated",
    "application-observe": "application_observe",
}
VARIANTS = tuple(CANDIDATES.values()) + ("native_query", "native_l2cap_probe", "capture_preflight")
IDENTITY_VARIANTS = {"sbc_continuous", "capture_stream", "application_control",
                     "application_generated", "application_observe", "application_setup", "application_namespace", "application_open_compare"}
REQUIRED_PROJECT = {"LICENSE", "README.md", "Cargo.toml", "Cargo.lock", "rust-toolchain.toml",
                    "tools/window_handoff.py", "tools/console_window.py", "tools/mvp_test_kit.py",
                    "tools/diagnostic_result.py", "tools/startup_test_plan.json",
                    "docs/WINDOW-HANDOFF.md",
                    "docs/TOOLCHAIN.md", "docs/SOURCE-DISTRIBUTION.md"}


def sha(data):
    return hashlib.sha256(data).hexdigest()


def safe_name(name):
    if (not isinstance(name, str) or len(name) > 512 or not name
            or "\\" in name or not name.isascii()
            or any(ord(char) < 32 or ord(char) == 127 for char in name)):
        raise ValueError("unsafe member name")
    path = PurePosixPath(name)
    if path.is_absolute() or str(path) != name or any(part in ("", ".", "..", ".git") for part in path.parts):
        raise ValueError("unsafe member path")
    return name


def unique(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate JSON key")
        result[key] = value
    return result


def json_read(data):
    if len(data) > MAX_JSON_BYTES:
        raise ValueError("JSON size bound")
    def reject(_):
        raise ValueError("non-finite JSON value")
    value = json.loads(data, object_pairs_hook=unique, parse_constant=reject)
    if not isinstance(value, dict):
        raise ValueError("JSON object required")
    return value


def json_bytes(value):
    return (json.dumps(value, sort_keys=True, indent=2) + "\n").encode()


def bounded_file(path, maximum=MAX_FILE_BYTES):
    fd = os.open(path, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0))
    try:
        info = os.fstat(fd)
        if not stat.S_ISREG(info.st_mode) or info.st_size > maximum:
            raise ValueError("input type/size bound")
        with os.fdopen(fd, "rb", closefd=False) as source:
            data = source.read(maximum + 1)
        if len(data) != info.st_size or len(data) > maximum:
            raise ValueError("input changed or exceeded bound")
        return data
    finally:
        os.close(fd)


def tar_files(data):
    if len(data) > MAX_ARCHIVE_BYTES:
        raise ValueError("compressed archive size bound")
    files, total = {}, 0
    # Bound the entire decoded tar stream too, including extension/header bytes
    # that tarfile can consume before it yields a regular member.
    raw_limit = MAX_TOTAL_BYTES + MAX_FILES * 1024 + 10_240
    with gzip.GzipFile(fileobj=io.BytesIO(data), mode="rb") as compressed:
        raw = compressed.read(raw_limit + 1)
    if len(raw) > raw_limit:
        raise ValueError("decoded tar stream size bound")
    with tarfile.open(fileobj=io.BytesIO(raw), mode="r:") as archive:
        for member in archive:
            name = safe_name(member.name)
            if (not member.isfile() or member.mode not in (0o644, 0o755)
                    or name in files or len(files) >= MAX_FILES
                    or member.size < 0 or member.size > MAX_FILE_BYTES):
                raise ValueError("unsupported/duplicate archive member or bound")
            total += member.size
            if total > MAX_TOTAL_BYTES:
                raise ValueError("expanded archive size bound")
            source = archive.extractfile(member)
            if source is None:
                raise ValueError("missing archive member data")
            value = source.read(MAX_FILE_BYTES + 1)
            if len(value) != member.size:
                raise ValueError("truncated archive member")
            files[name] = (value, member.mode)
    # File/directory aliasing also fails before any extraction occurs.
    for name in files:
        if any(str(parent) in files for parent in PurePosixPath(name).parents if str(parent) != "."):
            raise ValueError("archive file/directory alias")
    return files


def tar_bytes(files):
    if len(files) > MAX_FILES or sum(len(data) for data, _ in files.values()) > MAX_TOTAL_BYTES:
        raise ValueError("archive inventory bound")
    output = io.BytesIO()
    with gzip.GzipFile(filename="", mode="wb", fileobj=output, mtime=0) as compressed:
        with tarfile.open(fileobj=compressed, mode="w", format=tarfile.USTAR_FORMAT) as archive:
            for name, (data, mode) in sorted(files.items()):
                safe_name(name)
                if len(data) > MAX_FILE_BYTES or mode not in (0o644, 0o755):
                    raise ValueError("archive file bound/mode")
                info = tarfile.TarInfo(name)
                info.size, info.mode, info.mtime = len(data), mode, 0
                info.uid = info.gid = 0
                archive.addfile(info, io.BytesIO(data))
    value = output.getvalue()
    if len(value) > MAX_ARCHIVE_BYTES:
        raise ValueError("compressed archive size bound")
    return value


def git(root, *args):
    return subprocess.run(["git", *args], cwd=root, check=True,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE).stdout


def committed_project(root):
    if git(root, "status", "--porcelain=v1", "--untracked-files=all", "--ignore-submodules=none"):
        raise ValueError("handoff export requires clean committed HEAD")
    if git(root, "ls-files", "-ci", "--exclude-standard", "-z"):
        raise ValueError("tracked ignored files require review")
    head = git(root, "rev-parse", "HEAD").decode().strip()
    files = {}
    for record in git(root, "ls-tree", "-rz", "--full-tree", "HEAD").split(b"\0"):
        if not record:
            continue
        info, raw_name = record.split(b"\t", 1)
        mode, kind, oid = info.split()
        name = safe_name(raw_name.decode())
        if kind != b"blob" or mode not in (b"100644", b"100755"):
            raise ValueError("unsupported tracked source type")
        files[name] = (git(root, "cat-file", "blob", oid.decode()),
                       0o755 if mode == b"100755" else 0o644)
    if not REQUIRED_PROJECT <= files.keys():
        raise ValueError("missing tracked operator/source material")
    return head, files


def kit_inventory(kit_dir):
    # Reject even extra files: saved logs, credentials and arbitrary SDK outputs
    # are never admitted merely because they happen to be beside the kit.
    files = {}
    for directory, dirs, names in os.walk(kit_dir, followlinks=False):
        for name in dirs:
            if (Path(directory) / name).is_symlink():
                raise ValueError("symlinked kit directory")
        for name in names:
            path = Path(directory) / name
            relative = safe_name(path.relative_to(kit_dir).as_posix())
            if len(files) >= MAX_FILES:
                raise ValueError("kit inventory bound")
            files[relative] = (bounded_file(path), 0o644)
    return files


def validate_plan_text(candidate):
    for field in ("expected_markers", "stop_conditions"):
        values = candidate.get(field)
        if (not isinstance(values, list) or not 1 <= len(values) <= MAX_PLAN_TEXT_ITEMS
                or any(not isinstance(value, str) or not value.strip()
                       or len(value.encode("utf-8")) > MAX_PLAN_TEXT_BYTES
                       or any(ord(char) < 32 or ord(char) == 127 for char in value)
                       for value in values)):
            raise ValueError("candidate marker/stop text bound or type")


def validate_kit(files, expected_head=None, expected_project=None):
    plan = json_read(files["test-plan.json"][0])
    manifest = json_read(files["kit-manifest.json"][0])
    head = manifest.get("git_head")
    if (type(manifest.get("schema")) is not int or manifest.get("schema") != 1 or not isinstance(head, str)
            or not re.fullmatch(r"[0-9a-f]{40}", head)
            or expected_head is not None and head != expected_head
            or type(plan.get("schema")) is not int or plan.get("schema") != 1
            or (plan.get("target_host"), plan.get("ftp_port"), plan.get("remote_root"))
                != ("192.168.1.111", 2121, "/data/pldmgr/payloads/local")):
        raise ValueError("kit provenance/target mismatch")
    candidates, entries = plan.get("candidates"), manifest.get("entries")
    if (not isinstance(candidates, list) or not isinstance(entries, list)
            or not 1 <= len(candidates) <= len(CANDIDATES) or len(entries) != len(candidates)
            or any(not isinstance(item, dict) for item in candidates + entries)
            or len({item.get("id") for item in candidates}) != len(candidates)
            or any(item.get("id") not in CANDIDATES for item in candidates)):
        raise ValueError("reviewed nonempty unique candidate selection required")
    read_only_logs = {"application-namespace": "/data/ps5-bt-namespace.log",
                      "application-open-compare": "/data/ps5-bt-open-compare.log"}
    expected_log = read_only_logs.get(candidates[0]["id"], "/data/ps5-bt-hw1.log") if len(candidates) == 1 else "/data/ps5-bt-hw1.log"
    if plan.get("remote_log") != expected_log or (len(candidates) != 1 and any(item["id"] in read_only_logs for item in candidates)):
        raise ValueError("unreviewed log path or mixed read-only observation plan")
    expected = {"test-plan.json", "kit-manifest.json", "RUNBOOK.md"}
    project = expected_project
    slots = set()
    for order, (candidate, entry) in enumerate(zip(candidates, entries, strict=True), 1):
        identifier = candidate["id"]
        validate_plan_text(candidate)
        stem = Path(candidate.get("artifact", "")).stem
        if (candidate != entry.get("candidate") or candidate.get("order") != order
                or stem != "ps5-bt-" + CANDIDATES[identifier].replace("_", "-")
                or candidate.get("remote_filename") != stem + ".elf"
                or candidate.get("slot") != "ps5-bt-" + identifier
                or candidate["slot"] in slots or type(candidate.get("timeout_seconds")) is not int
                or not 1 <= candidate["timeout_seconds"] <= 60):
            raise ValueError("candidate plan mismatch")
        slots.add(candidate["slot"])
        paths = []
        for key, suffix, hash_key in (("artifact", ".elf", "artifact_sha256"),
                ("build_manifest", ".manifest.json", "build_manifest_sha256"),
                ("source_bundle", "-source.tar.gz", "source_bundle_sha256")):
            path = safe_name(candidate[key])
            if path != "build/ps5/" + stem + suffix:
                raise ValueError("candidate input path mismatch")
            relative = identifier + "/" + stem + suffix
            expected.add(relative)
            data = files[relative][0]
            if sha(data) != entry.get(hash_key):
                raise ValueError("kit file hash mismatch")
            paths.append(data)
        artifact, build_data, source_data = paths
        build = json_read(build_data)
        if (build.get("git_head") != head or build.get("git_status") != []
                or build.get("sha256") != sha(artifact) or type(build.get("size")) is not int
                or build["size"] != len(artifact) or type(entry.get("artifact_size")) is not int
                or entry.get("artifact_size") != len(artifact)
                or build.get("loader_kernel_resolver") is not True
                or any(type(build.get(key, False)) is not bool for key in VARIANTS)
                or [key for key in VARIANTS if build.get(key, False)] != [CANDIDATES[identifier]]):
            raise ValueError("candidate build identity mismatch")
        if CANDIDATES[identifier] in IDENTITY_VARIANTS:
            if (not isinstance(build.get("diagnostic_build_id"), str)
                    or not re.fullmatch(r"[0-9a-f]{64}", build["diagnostic_build_id"])
                    or build.get("diagnostic_candidate") != stem
                    or entry.get("diagnostic_build_id") != build["diagnostic_build_id"]):
                raise ValueError("embedded diagnostic identity mismatch")
        source = tar_files(source_data)
        if source.get("ps5-bt-source/build-manifest.json", (None,))[0] != build_data:
            raise ValueError("source/build manifest mismatch")
        source_project = {name[len("ps5-bt-source/project/"):]: value for name, value in source.items()
                          if name.startswith("ps5-bt-source/project/")}
        if not REQUIRED_PROJECT <= source_project.keys():
            raise ValueError("source bundle missing operator/project files")
        if project is None:
            project = source_project
        if source_project != project:
            raise ValueError("source bundle does not match committed project")
        if CANDIDATES[identifier] in ("sbc_selftest", "sbc_continuous", "application_generated"):
            if not {"ps5-bt-source/relink/relink.py", "ps5-bt-source/relink/bundle-manifest.json",
                    "ps5-bt-source/project/third_party/sbc/upstream/COPYING.LIB"} <= source.keys():
                raise ValueError("codec source/relink/license materials missing")
    if files.keys() != expected:
        raise ValueError("extra or missing kit file")
    return head, project


def start_here(head):
    return f"""# Portable PS5-BT test-window handoff

Original source HEAD: `{head}`. No console was contacted by this export.
Run from this extracted directory using Python 3.11+ (standard library only):

    python3 tools/window_handoff.py inspect /path/to/transfer.tar.gz
    python3 tools/console_window.py --preflight --kit {KIT}

Read docs/TEMP-PS5-TEST-HANDOFF.md, docs/WINDOW-HANDOFF.md and the kit RUNBOOK.md.
The preflight command is dry-run. Execution needs a newly confirmed console window;
manual launch remains in PayloadManager, and controller checks remain mandatory.
No old Mac absolute path is needed to inspect or collect matching saved results.
Historical build manifests may retain old tool paths as provenance only.

The project tree has no .git. To rebuild, prefer cloning the repository at the
original HEAD, or follow a kit source bundle's README.source.txt to create a NEW
local snapshot and use its pinned SDK SOURCE (never SDK binaries). A new snapshot
has different provenance; keep all original kit manifests unchanged. Rebuild
requires Git, Python 3.11+, Rust1.90.0/x86_64-unknown-none, host C11/Make/patch/ar/nm
and LLVM18.1.8; see docs/TOOLCHAIN.md. Those external tools are not bundled.
No payload, saved log, SDK binary, secret or .git history is taken from elsewhere.
Hashes identify local files; they do not attest execution, trust, or PS5 support.
""".encode()


def export(root, kit_dir, output):
    root, kit_dir, output = Path(root), Path(kit_dir), Path(output)
    head, committed = committed_project(root)
    if kit_dir.is_symlink() or not kit_dir.is_dir():
        raise ValueError("kit must be a real directory")
    kit_files = kit_inventory(kit_dir)
    _, project = validate_kit(kit_files, head, committed)
    # Export runs within the trusted clean project. Reuse its canonical renderer
    # rather than copying runbook policy into this portable inspector. Inspect
    # and extract never import sibling modules or execute transferred sources.
    from mvp_test_kit import runbook
    plan = json_read(kit_files["test-plan.json"][0])
    entries = json_read(kit_files["kit-manifest.json"][0])["entries"]
    if kit_files["RUNBOOK.md"] != (runbook(plan, head, entries).encode(), 0o644):
        raise ValueError("kit runbook differs from canonical plan renderer")
    files = dict(project)
    for name, value in kit_files.items():
        files[KIT + "/" + name] = value
    if {"HANDOFF.json", "START-HERE.md"} & files.keys():
        raise ValueError("reserved handoff path")
    files["START-HERE.md"] = (start_here(head), 0o644)
    manifest = {"schema": 1, "git_head": head, "kit_path": KIT,
                "files": {name: {"sha256": sha(data), "size": len(data), "mode": mode}
                          for name, (data, mode) in sorted(files.items())}}
    files["HANDOFF.json"] = (json_bytes(manifest), 0o644)
    archive = tar_bytes({PREFIX + name: value for name, value in files.items()})
    if committed_project(root) != (head, committed):
        raise ValueError("project changed during handoff export")
    # Validate what will be transferred, including its nested intact kit.
    inspect_bytes(archive)
    targets = {output: archive, Path(str(output) + ".bootstrap.py"): project["tools/window_handoff.py"][0],
               Path(str(output) + ".sha256"): (sha(archive) + "  " + output.name + "\n").encode()}
    if any(path.exists() or path.is_symlink() for path in targets):
        raise ValueError("handoff output already exists")
    created = []
    try:
        for path, data in targets.items():
            fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0), 0o600)
            created.append(path)
            with os.fdopen(fd, "wb") as stream:
                stream.write(data)
                stream.flush()
                os.fsync(stream.fileno())
    except BaseException:
        for path in created:
            path.unlink(missing_ok=True)
        raise
    return {"schema": 1, "git_head": head, "archive_sha256": sha(archive),
            "archive_size": len(archive), "candidates": len(json_read(kit_files["kit-manifest.json"][0])["entries"]),
            "files": len(files)}


def inspect_bytes(data, expected_sha256=None):
    archive_hash, archive_size = sha(data), len(data)
    if expected_sha256 is not None and (not re.fullmatch(r"[0-9a-f]{64}", expected_sha256)
                                        or archive_hash != expected_sha256):
        raise ValueError("transfer archive SHA-256 mismatch")
    packed = tar_files(data)
    if any(not name.startswith(PREFIX) for name in packed):
        raise ValueError("unexpected archive root")
    files = {name[len(PREFIX):]: value for name, value in packed.items()}
    manifest = json_read(files["HANDOFF.json"][0])
    if (type(manifest.get("schema")) is not int or manifest.get("schema") != 1 or manifest.get("kit_path") != KIT
            or type(manifest.get("files")) is not dict or not isinstance(manifest.get("git_head"), str)
            or not re.fullmatch(r"[0-9a-f]{40}", manifest["git_head"])):
        raise ValueError("invalid handoff manifest")
    expected = manifest["files"]
    if files.keys() != expected.keys() | {"HANDOFF.json"}:
        raise ValueError("handoff inventory mismatch")
    for name, descriptor in expected.items():
        safe_name(name)
        data, mode = files[name]
        if (not isinstance(descriptor, dict) or type(descriptor.get("size")) is not int
                or type(descriptor.get("mode")) is not int
                or descriptor != {"sha256": sha(data), "size": len(data), "mode": mode}):
            raise ValueError("handoff file identity mismatch")
    kit_files = {name[len(KIT) + 1:]: value for name, value in files.items() if name.startswith(KIT + "/")}
    project = {name: value for name, value in files.items()
               if not name.startswith(KIT + "/") and name not in ("HANDOFF.json", "START-HERE.md")}
    head, _ = validate_kit(kit_files, manifest.get("git_head"), project)
    if files["START-HERE.md"] != (start_here(head), 0o644):
        raise ValueError("bootstrap instructions mismatch")
    return {"schema": 1, "git_head": head, "archive_sha256": archive_hash,
            "archive_size": archive_size, "candidates": len(json_read(kit_files["kit-manifest.json"][0])["entries"]),
            "files": len(files)}, files


def inspect(path, expected_sha256=None):
    report, _ = inspect_bytes(bounded_file(path, MAX_ARCHIVE_BYTES), expected_sha256)
    return report


def extract(path, destination, expected_sha256=None):
    report, files = inspect_bytes(bounded_file(path, MAX_ARCHIVE_BYTES), expected_sha256)
    destination = Path(destination).absolute()
    if destination.exists() or destination.is_symlink():
        raise ValueError("extraction destination already exists")
    # Refuse symlinked ancestor components before creating a private staging
    # directory. No untrusted tarfile extraction API or existing output tree used.
    if not destination.parent.is_dir() or any(parent.is_symlink() for parent in (destination.parent, *destination.parent.parents)):
        raise ValueError("extraction parent missing or symlinked")
    staging = Path(tempfile.mkdtemp(prefix=".ps5-bt-window-", dir=destination.parent))
    try:
        for name, (data, mode) in sorted(files.items()):
            target = staging / name
            target.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
            with target.open("xb") as stream:
                stream.write(data)
            target.chmod(mode)
        # mkdir reserves the final name exclusively; moving children into this
        # owned empty directory avoids rename replacing a concurrently made dir.
        destination.mkdir(mode=0o700)
        try:
            for child in staging.iterdir():
                child.rename(destination / child.name)
        except BaseException:
            shutil.rmtree(destination)
            raise
    finally:
        shutil.rmtree(staging)
    return report


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    command = commands.add_parser("export")
    command.add_argument("--kit-dir", type=Path, required=True)
    command.add_argument("--output", type=Path, required=True)
    for action in ("inspect", "extract"):
        command = commands.add_parser(action)
        command.add_argument("archive", type=Path)
        command.add_argument("--expect-sha256")
        if action == "extract":
            command.add_argument("--destination", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        if args.command == "export":
            report = export(ROOT, args.kit_dir, args.output)
        elif args.command == "inspect":
            report = inspect(args.archive, args.expect_sha256)
        else:
            report = extract(args.archive, args.destination, args.expect_sha256)
        print(json.dumps(report, sort_keys=True))
        return 0
    except (OSError, EOFError, ValueError, KeyError, TypeError, RecursionError,
            subprocess.CalledProcessError, tarfile.TarError, zlib.error):
        # Do not echo arbitrary archive members, source content or local paths.
        print("FAIL: offline handoff validation or exclusive output failed", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
