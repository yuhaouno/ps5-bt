#!/usr/bin/env python3
"""Assemble and explicitly deploy the bounded PS5 MVP diagnostic kit."""

import argparse
import ftplib
import hashlib
import io
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
LEGACY_LOG = "/data/ps5-bt-hw1.log"
NAMESPACE_LOG = "/data/ps5-bt-namespace.log"
OPEN_COMPARE_LOG = "/data/ps5-bt-open-compare.log"
RAW_OPENAT_LOG = "/data/ps5-bt-raw-openat.log"
READ_ONLY_LOGS = {"application-namespace": NAMESPACE_LOG, "application-open-compare": OPEN_COMPARE_LOG, "application-raw-openat": RAW_OPENAT_LOG}
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
    candidates = plan.get("candidates")
    if not isinstance(candidates, list) or not candidates or any(not isinstance(item, dict) for item in candidates):
        raise ValueError("test plan has no candidates")
    expected_log = READ_ONLY_LOGS.get(candidates[0].get("id"), LEGACY_LOG) if len(candidates) == 1 else LEGACY_LOG
    if plan.get("remote_log") != expected_log or (len(candidates) != 1 and any(item.get("id") in READ_ONLY_LOGS for item in candidates)):
        raise ValueError("unexpected durable log path or mixed read-only observation plan")
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
        "capture-stream": "capture_stream",
        "capture-observe": "capture_observe",
        "application-setup": "application_setup", "application-namespace": "application_namespace", "application-open-compare": "application_open_compare", "application-raw-openat": "application_raw_openat",
        "application-control": "application_control",
        "application-generated": "application_generated",
        "application-observe": "application_observe",
    }.get(candidate["id"])
    if expected_variant is None:
        raise ValueError("unreviewed candidate id")
    variants = ("native_query", "native_l2cap_probe", "usb_metadata", "capture_preflight", "capture_once",
                "sbc_selftest", "capture_observe", "sbc_continuous", "capture_stream", "application_control", "application_generated", "application_observe", "application_setup", "application_namespace", "application_open_compare", "application_raw_openat")
    if any(type(manifest.get(key, False)) is not bool for key in variants) or [
        key for key in variants if manifest.get(key, False)
    ] != [expected_variant]:
        raise ValueError(f"wrong diagnostic variant: {candidate['id']}")
    if not manifest.get("loader_kernel_resolver"):
        raise ValueError("candidate lacks reviewed loader kernel resolver")
    if expected_variant in ("sbc_continuous", "capture_stream", "application_control", "application_generated", "application_observe", "application_setup", "application_namespace", "application_open_compare", "application_raw_openat"):
        build_id = manifest.get("diagnostic_build_id")
        if (not isinstance(build_id, str) or not re.fullmatch(r"[0-9a-f]{64}", build_id)
                or manifest.get("diagnostic_candidate") != artifact.stem):
            raise ValueError("candidate lacks its embedded diagnostic identity")
    with tarfile.open(source, "r:gz") as archive:
        try:
            packaged_manifest = archive.extractfile("ps5-bt-source/build-manifest.json")
        except KeyError as error:
            raise ValueError("source bundle lacks its build manifest") from error
        if packaged_manifest is None or packaged_manifest.read() != manifest_path.read_bytes():
            raise ValueError("source bundle does not match the build manifest")
        if expected_variant in ("sbc_selftest", "sbc_continuous", "application_generated"):
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
    if plan["remote_log"] in READ_ONLY_LOGS.values():
        lines += [
            "This independently selected read-only namespace observation uses its own",
            "fixed log; other diagnostic variants do not overwrite it. Run it once,",
            "then collect when the operator returns. A repeat of this same artifact",
            "still overwrites its previous log. Predicate/match bits do not establish",
            "native ABI correctness or accept the protected store.",
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
            *([f"Embedded build identity: `{entry['diagnostic_build_id']}`", ""]
              if entry.get("diagnostic_build_id") else []),
            "Stop conditions:",
            "",
            *[f"- {condition}" for condition in candidate["stop_conditions"]],
            "",
        ]
    lines += [
        "Read-only namespace/open-comparison probes do not require or prepare the store.",
        "Application workloads require a private mode-0700 `/data/ps5-bt/control`",
        "owned by the payload identity. Separately selected application-setup",
        "only validates/creates fixed project directory components using native",
        "UID/protection checks; it never chmods/chowns existing paths. Building",
        "or assembling this kit does not execute setup or create console slots.",
        "Application workloads bind ephemeral loopback without revealing tokens.",
        "Application-generated and standalone SBC encode generated PCM to discard.",
        "Application-observe performs bounded repeated raw capture; capture-stream",
        "makes one immediate read. Neither verifies the native PCM format.",
        "Standalone SBC/capture are conditional isolation tests after a clean",
        "reviewed failure; do not continue after unknown cleanup or controller changes.",
        "Read docs/TEMP-PS5-TEST-HANDOFF.md and docs/PAYLOADMANAGER-SLOTS.md before use.",
        "No candidate supplies a LAN deployment or usable Bluetooth connection route.",
        "",
        "For identity-bearing candidates, save the local log and use",
        "`tools/diagnostic_result.py` with its matching manifest and explicit",
        "before/during/after controller observations. Never infer Unknown as normal.",
        "A missing matching finish/return remains an interrupted result.",
        "",
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
            "diagnostic_build_id": manifest.get("diagnostic_build_id"),
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
        namespace_plan = load_plan(ROOT / "tools/namespace_test_plan.json", root)
        assert namespace_plan["remote_log"] == NAMESPACE_LOG
        for log in (LEGACY_LOG, "/data/unreviewed.log"):
            path.write_text(json.dumps(namespace_plan | {"remote_log": log}))
            try:
                load_plan(path, root)
            except ValueError:
                pass
            else:
                raise AssertionError("unreviewed namespace log accepted")
        # Exercise selected-variant and embedded-identity checks for every item.
        open_compare_plan = load_plan(ROOT / "tools/open_compare_test_plan.json", root)
        assert open_compare_plan["remote_log"] == OPEN_COMPARE_LOG
        raw_openat_plan = load_plan(ROOT / "tools/raw_openat_test_plan.json", root)
        assert raw_openat_plan["remote_log"] == RAW_OPENAT_LOG
        for candidate in plan["candidates"] + namespace_plan["candidates"] + open_compare_plan["candidates"] + raw_openat_plan["candidates"]:
            artifact = root / candidate["artifact"]
            artifact.write_bytes(b"synthetic-elf")
            manifest_path = root / candidate["build_manifest"]
            source = root / candidate["source_bundle"]
            variant = {"usb-binding": "usb_metadata", "capture-settled": "capture_once",
                       "sbc-selftest": "sbc_selftest", "capture-observe": "capture_observe",
                       "sbc-continuous": "sbc_continuous", "capture-stream": "capture_stream",
                       "application-setup": "application_setup", "application-namespace": "application_namespace", "application-open-compare": "application_open_compare", "application-raw-openat": "application_raw_openat", "application-control": "application_control", "application-generated": "application_generated",
                       "application-observe": "application_observe"}[candidate["id"]]
            manifest = {"git_head": "test-head", "git_status": [], "sha256": digest(artifact),
                        "size": artifact.stat().st_size, "loader_kernel_resolver": True,
                        variant: True}
            if variant in ("sbc_continuous", "capture_stream", "application_control", "application_generated", "application_observe", "application_setup", "application_namespace", "application_open_compare", "application_raw_openat"):
                manifest.update(diagnostic_build_id="b" * 64, diagnostic_candidate=artifact.stem)
            def write_inputs():
                manifest_path.write_text(json.dumps(manifest))
                members = {"ps5-bt-source/build-manifest.json": manifest_path.read_bytes()}
                if variant in ("sbc_selftest", "sbc_continuous", "application_generated"):
                    members.update({name: b"synthetic" for name in (
                        "ps5-bt-source/relink/relink.py", "ps5-bt-source/relink/bundle-manifest.json",
                        "ps5-bt-source/project/third_party/sbc/upstream/COPYING.LIB")})
                with tarfile.open(source, "w:gz") as archive:
                    for name, data in members.items():
                        info = tarfile.TarInfo(name); info.size = len(data)
                        archive.addfile(info, io.BytesIO(data))
            def reject():
                write_inputs()
                try:
                    validate_candidate(root, candidate, "test-head")
                except ValueError:
                    return
                raise AssertionError("invalid candidate identity/selection accepted")
            write_inputs()
            validate_candidate(root, candidate, "test-head")
            manifest["native_query"] = True
            reject()
            del manifest["native_query"]
            manifest["loader_kernel_resolver"] = False
            reject()
            manifest["loader_kernel_resolver"] = True
            if "diagnostic_build_id" in manifest:
                manifest["diagnostic_build_id"] = "bad"
                reject()
                manifest["diagnostic_build_id"] = "b" * 64
                manifest["diagnostic_candidate"] = "other"
                reject()
        assert "private mode-0700" in runbook(plan, "test-head", [])
        assert "never chmods/chowns existing paths" in runbook(plan, "test-head", [])
        assert "conditional isolation tests" in runbook(plan, "test-head", [])
        sample = root / "sample"
        sample.write_bytes(b"bounded")
        assert digest(sample) == hashlib.sha256(b"bounded").hexdigest()
    print("PASS MVP kit plan bounds, target pin, default inventory and separate read-only observations, embedded identity, loader gate, path rejection and hashing")


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
