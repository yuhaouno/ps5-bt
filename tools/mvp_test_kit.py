#!/usr/bin/env python3
"""Assemble and explicitly deploy the bounded PS5 MVP diagnostic kit."""

import argparse
import ftplib
import hashlib
import json
import re
import shutil
import subprocess
import tarfile
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_PLAN = ROOT / "tools/mvp_test_plan.json"
DEFAULT_KIT = ROOT / "build/ps5/mvp-test-kit"
SAFE_ID = re.compile(r"[a-z0-9][a-z0-9-]{0,62}[a-z0-9]$")
HANDLED_ERRORS = (OSError, ValueError, subprocess.CalledProcessError, tarfile.TarError) + ftplib.all_errors


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(65536), b""):
            value.update(block)
    return value.hexdigest()


def project_path(root, value, prefix):
    path = Path(value)
    if path.is_absolute() or ".." in path.parts:
        raise ValueError(f"unsafe project path: {value}")
    resolved = (root / path).resolve()
    expected = (root / prefix).resolve()
    if not resolved.is_relative_to(expected):
        raise ValueError(f"path escapes {prefix}: {value}")
    return resolved


def load_plan(path=DEFAULT_PLAN, root=ROOT):
    plan = json.loads(path.read_text())
    if plan.get("schema") != 1:
        raise ValueError("unsupported test-plan schema")
    if plan.get("target_host") != "192.168.1.111" or plan.get("ftp_port") != 2121:
        raise ValueError("test plan must retain the reviewed .111 FTP target")
    if plan.get("remote_root") != "/data/pldmgr/payloads/local":
        raise ValueError("unexpected PayloadManager root")
    if plan.get("remote_log") != "/data/ps5-bt-hw1.log":
        raise ValueError("unexpected durable log path")
    candidates = plan.get("candidates")
    if not isinstance(candidates, list) or not candidates:
        raise ValueError("test plan has no candidates")
    identifiers = set()
    slots = set()
    orders = []
    for candidate in candidates:
        identifier = candidate.get("id", "")
        slot = candidate.get("slot", "")
        if not SAFE_ID.fullmatch(identifier) or not SAFE_ID.fullmatch(slot):
            raise ValueError("candidate id/slot is not a safe slug")
        if identifier in identifiers or slot in slots:
            raise ValueError("duplicate candidate id/slot")
        identifiers.add(identifier)
        slots.add(slot)
        orders.append(candidate.get("order"))
        if candidate.get("timeout_seconds") not in range(1, 61):
            raise ValueError("candidate timeout must be 1..60 seconds")
        filename = candidate.get("remote_filename", "")
        if Path(filename).name != filename or not filename.endswith(".elf"):
            raise ValueError("invalid remote ELF filename")
        for field, suffix in (
            ("artifact", ".elf"),
            ("build_manifest", ".manifest.json"),
            ("source_bundle", "-source.tar.gz"),
        ):
            value = candidate.get(field, "")
            if not value.endswith(suffix):
                raise ValueError(f"invalid {field}")
            project_path(root, value, "build/ps5")
        for field in ("expected_markers", "stop_conditions"):
            values = candidate.get(field)
            if not isinstance(values, list) or not values or not all(
                isinstance(value, str) and value for value in values
            ):
                raise ValueError(f"candidate {field} must be nonempty strings")
    if orders != list(range(1, len(candidates) + 1)):
        raise ValueError("candidate order must be contiguous and sorted")
    return plan


def git_state(root=ROOT):
    head = subprocess.run(
        ["git", "rev-parse", "HEAD"], cwd=root, check=True, text=True,
        capture_output=True
    ).stdout.strip()
    status = subprocess.run(
        ["git", "status", "--porcelain"], cwd=root, check=True, text=True,
        capture_output=True
    ).stdout.splitlines()
    return head, status


def validate_candidate(root, candidate, head):
    artifact = project_path(root, candidate["artifact"], "build/ps5")
    manifest_path = project_path(root, candidate["build_manifest"], "build/ps5")
    source = project_path(root, candidate["source_bundle"], "build/ps5")
    for path in (artifact, manifest_path, source):
        if not path.is_file():
            raise ValueError(f"missing kit input: {path.relative_to(root)}")
    manifest = json.loads(manifest_path.read_text())
    if manifest.get("git_head") != head or manifest.get("git_status") != []:
        raise ValueError(f"stale or dirty build manifest: {manifest_path.name}")
    if manifest.get("sha256") != digest(artifact) or manifest.get("size") != artifact.stat().st_size:
        raise ValueError(f"artifact identity mismatch: {artifact.name}")
    expected_variant = {
        "usb-binding": "usb_metadata",
        "capture-settled": "capture_once",
        "sbc-selftest": "sbc_selftest",
        "sbc-continuous": "sbc_continuous",
        "capture-observe": "capture_observe",
    }.get(candidate["id"])
    if expected_variant is None:
        raise ValueError("unreviewed candidate id")
    variants = ("native_query", "native_l2cap_probe", "usb_metadata", "capture_preflight", "capture_once",
                "sbc_selftest", "capture_observe", "sbc_continuous")
    if any(type(manifest.get(key, False)) is not bool for key in variants) or [
        key for key in variants if manifest.get(key, False)
    ] != [expected_variant]:
        raise ValueError(f"wrong diagnostic variant: {candidate['id']}")
    if not manifest.get("loader_kernel_resolver"):
        raise ValueError("candidate lacks reviewed loader kernel resolver")
    with tarfile.open(source, "r:gz") as archive:
        try:
            packaged_manifest = archive.extractfile("ps5-bt-source/build-manifest.json")
        except KeyError as error:
            raise ValueError("source bundle lacks its build manifest") from error
        if packaged_manifest is None or packaged_manifest.read() != manifest_path.read_bytes():
            raise ValueError("source bundle does not match the build manifest")
        if expected_variant in ("sbc_selftest", "sbc_continuous"):
            names = set(archive.getnames())
            if not {"ps5-bt-source/relink/relink.py",
                    "ps5-bt-source/relink/bundle-manifest.json",
                    "ps5-bt-source/project/third_party/sbc/upstream/COPYING.LIB"} <= names:
                raise ValueError("SBC candidate lacks source/license/relink materials")
    return artifact, manifest_path, source, manifest


def runbook(plan, head, entries):
    lines = [
        "# PS5-BT bounded diagnostic runbook",
        "",
        f"Source HEAD: `{head}`",
        f"Target after explicit user confirmation: `{plan['target_host']}`",
        f"Durable log (overwritten every run): `{plan['remote_log']}`",
        "",
        "Do not deploy or launch until the user confirms the console window. The",
        "deployment helper defaults to dry-run and uploads only; launch remains",
        "manual in PayloadManager. Save the durable log before every next item.",
        "Confirm wireless controller input before, during and after each item.",
        "",
    ]
    for entry in entries:
        candidate = entry["candidate"]
        lines += [
            f"## {candidate['order']}. {candidate['id']}",
            "",
            f"Slot: `{candidate['slot']}`; file: `{candidate['remote_filename']}`",
            f"ELF SHA-256: `{entry['artifact_sha256']}`",
            f"External ceiling: {candidate['timeout_seconds']} seconds.",
            "",
            "Expected markers:",
            "",
            *[f"- `{marker}`" for marker in candidate["expected_markers"]],
            "",
            "Stop conditions:",
            "",
            *[f"- {condition}" for condition in candidate["stop_conditions"]],
            "",
        ]
    lines += [
        "No item owns HCI traffic. Binding metadata does not prove ownership, and",
        "an Empty capture read is not PCM success. Never reset the radio, install a",
        "loader, launch automatically or continue after uncertain cleanup.",
        "",
    ]
    return "\n".join(lines)


def assemble(plan_path, kit_dir):
    plan = load_plan(plan_path)
    head, status = git_state()
    if status:
        raise ValueError("kit assembly requires a clean committed worktree")
    if kit_dir.exists():
        raise ValueError(f"kit directory already exists: {kit_dir}")
    entries = []
    inputs = []
    for candidate in plan["candidates"]:
        artifact, manifest_path, source, manifest = validate_candidate(ROOT, candidate, head)
        entry = {
            "candidate": candidate,
            "artifact_sha256": digest(artifact),
            "artifact_size": artifact.stat().st_size,
            "build_manifest_sha256": digest(manifest_path),
            "source_bundle_sha256": digest(source),
            "runtime": manifest["runtime"],
        }
        entries.append(entry)
        inputs.append((candidate, artifact, manifest_path, source))
    kit_dir.mkdir(parents=True)
    try:
        for candidate, artifact, manifest_path, source in inputs:
            directory = kit_dir / candidate["id"]
            directory.mkdir()
            shutil.copyfile(artifact, directory / artifact.name)
            shutil.copyfile(manifest_path, directory / manifest_path.name)
            shutil.copyfile(source, directory / source.name)
        (kit_dir / "test-plan.json").write_text(json.dumps(plan, indent=2) + "\n")
        kit_manifest = {"schema": 1, "git_head": head, "entries": entries}
        (kit_dir / "kit-manifest.json").write_text(
            json.dumps(kit_manifest, indent=2) + "\n"
        )
        (kit_dir / "RUNBOOK.md").write_text(runbook(plan, head, entries))
    except Exception:
        shutil.rmtree(kit_dir)
        raise
    print(f"PASS assembled offline kit: {kit_dir}")
    for entry in entries:
        print(f"{entry['candidate']['id']} sha256={entry['artifact_sha256']}")


def load_kit(kit_dir):
    plan = load_plan(kit_dir / "test-plan.json", ROOT)
    manifest = json.loads((kit_dir / "kit-manifest.json").read_text())
    if manifest.get("schema") != 1 or len(manifest.get("entries", [])) != len(plan["candidates"]):
        raise ValueError("invalid kit manifest")
    for candidate, entry in zip(plan["candidates"], manifest["entries"], strict=True):
        if entry.get("candidate") != candidate:
            raise ValueError("kit plan/manifest candidate mismatch")
        directory = kit_dir / candidate["id"]
        artifact = directory / Path(candidate["artifact"]).name
        build_manifest = directory / Path(candidate["build_manifest"]).name
        source_bundle = directory / Path(candidate["source_bundle"]).name
        if not artifact.is_file() or digest(artifact) != entry.get("artifact_sha256"):
            raise ValueError("kit artifact hash mismatch")
        if not build_manifest.is_file() or digest(build_manifest) != entry.get("build_manifest_sha256"):
            raise ValueError("kit build-manifest hash mismatch")
        if not source_bundle.is_file() or digest(source_bundle) != entry.get("source_bundle_sha256"):
            raise ValueError("kit source-bundle hash mismatch")
    return plan, manifest


def retrieve_meta(ftp, limit=16384):
    data = bytearray()
    def append(block):
        if len(data) + len(block) > limit:
            raise ValueError("remote meta.json exceeds limit")
        data.extend(block)
    ftp.retrbinary("RETR meta.json", append)
    return json.loads(data)


def deploy(kit_dir, execute):
    plan, manifest = load_kit(kit_dir)
    print("DEPLOY MODE:", "EXPLICIT EXECUTE" if execute else "DRY-RUN")
    for candidate, entry in zip(plan["candidates"], manifest["entries"], strict=True):
        remote = f"{plan['remote_root']}/{candidate['slot']}/{candidate['remote_filename']}"
        print(f"would upload {candidate['id']} sha256={entry['artifact_sha256']} -> {remote}")
    print("uploads only; no payload is launched and no durable log is deleted")
    if not execute:
        return
    with ftplib.FTP() as ftp:
        ftp.connect(plan["target_host"], plan["ftp_port"], timeout=10)
        ftp.login("anonymous", "ps5-bt@localhost")
        for candidate, entry in zip(plan["candidates"], manifest["entries"], strict=True):
            ftp.cwd(f"{plan['remote_root']}/{candidate['slot']}")
            meta = retrieve_meta(ftp)
            if meta.get("schema") != 2 or meta.get("kind") != "local" or meta.get("filename") != candidate["remote_filename"]:
                raise ValueError(f"slot metadata mismatch: {candidate['slot']}")
            local = kit_dir / candidate["id"] / candidate["remote_filename"]
            temporary = f".ps5-bt-{entry['artifact_sha256'][:12]}.upload"
            try:
                with local.open("rb") as source:
                    ftp.storbinary(f"STOR {temporary}", source)
                if ftp.size(temporary) != local.stat().st_size:
                    raise ValueError("remote upload size mismatch")
                ftp.rename(temporary, candidate["remote_filename"])
            except Exception:
                try:
                    ftp.delete(temporary)
                except ftplib.all_errors:
                    pass
                raise
            ftp.cwd("/")
            print(f"uploaded {candidate['id']}; launch remains manual")


def self_test():
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        (root / "build/ps5").mkdir(parents=True)
        plan = json.loads(DEFAULT_PLAN.read_text())
        path = root / "plan.json"
        path.write_text(json.dumps(plan))
        assert load_plan(path, root) == plan
        bad = dict(plan)
        bad["target_host"] = "192.168.1.10"
        path.write_text(json.dumps(bad))
        try:
            load_plan(path, root)
        except ValueError:
            pass
        else:
            raise AssertionError("historical target accepted")
        bad = json.loads(DEFAULT_PLAN.read_text())
        bad["candidates"][0]["artifact"] = "../outside.elf"
        path.write_text(json.dumps(bad))
        try:
            load_plan(path, root)
        except ValueError:
            pass
        else:
            raise AssertionError("path escape accepted")
        sample = root / "sample"
        sample.write_bytes(b"bounded")
        assert digest(sample) == hashlib.sha256(b"bounded").hexdigest()
    print("PASS MVP kit plan bounds, target pin, path rejection and hashing")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", type=Path, default=DEFAULT_PLAN)
    parser.add_argument("--kit-dir", type=Path, default=DEFAULT_KIT)
    actions = parser.add_mutually_exclusive_group()
    actions.add_argument("--assemble", action="store_true")
    actions.add_argument("--deploy", action="store_true")
    actions.add_argument("--self-test", action="store_true")
    parser.add_argument("--execute", action="store_true", help="perform FTP upload; never launches")
    args = parser.parse_args()
    if args.execute and not args.deploy:
        parser.error("--execute requires --deploy")
    try:
        if args.self_test:
            self_test()
        elif args.assemble:
            assemble(args.plan.resolve(), args.kit_dir.resolve())
        elif args.deploy:
            deploy(args.kit_dir.resolve(), args.execute)
        else:
            plan = load_plan(args.plan.resolve())
            print("DRY-RUN plan; no files, network, deployment or launch")
            for candidate in plan["candidates"]:
                print(f"{candidate['order']}. {candidate['id']} -> {candidate['slot']}")
    except HANDLED_ERRORS as error:
        parser.exit(1, f"MVP test kit failed: {error}\n")


if __name__ == "__main__":
    main()
