#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Offline real-Git/source/kit handoff checks; no console or network access."""
import contextlib
import gzip
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tools"))
import window_handoff as handoff

REPO = Path(__file__).resolve().parents[2]


def command(root, *args):
    return subprocess.run(args, cwd=root, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)


def fixture(root, identifiers=("application-control", "application-generated", "application-observe")):
    root.mkdir()
    command(root, "git", "init", "-q")
    command(root, "git", "config", "user.name", "Offline fixture")
    command(root, "git", "config", "user.email", "fixture@localhost")
    files = {name: (b"synthetic project source\n", 0o644) for name in handoff.REQUIRED_PROJECT}
    files[".gitignore"] = (b"/build/\n", 0o644)
    files["tools/executable.sh"] = (b"#!/bin/sh\nexit 0\n", 0o755)
    files["tools/mvp_test_plan.json"] = ((REPO / "tools/mvp_test_plan.json").read_bytes(), 0o644)
    for name in ("tools/window_handoff.py", "tools/console_window.py", "tools/mvp_test_kit.py", "tools/diagnostic_result.py"):
        files[name] = ((REPO / name).read_bytes(), 0o644)
    files["third_party/sbc/upstream/COPYING.LIB"] = (b"synthetic fixture license\n", 0o644)
    for name, (data, mode) in files.items():
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        path.chmod(mode)
    command(root, "git", "add", ".")
    command(root, "git", "commit", "-qm", "test: synthetic clean source")
    head = command(root, "git", "rev-parse", "HEAD").stdout.decode().strip()
    kit = root / "build/input-kit"
    kit.mkdir(parents=True)
    plan = {"schema": 1, "target_host": "192.168.1.111", "ftp_port": 2121,
            "remote_root": "/data/pldmgr/payloads/local", "remote_log": "/data/ps5-bt-hw1.log", "candidates": []}
    entries = []
    for order, identifier in enumerate(identifiers, 1):
        stem = "ps5-bt-" + handoff.CANDIDATES[identifier].replace("_", "-")
        candidate = {"id": identifier, "order": order, "artifact": "build/ps5/" + stem + ".elf",
                     "build_manifest": "build/ps5/" + stem + ".manifest.json",
                     "source_bundle": "build/ps5/" + stem + "-source.tar.gz",
                     "slot": "ps5-bt-" + identifier, "remote_filename": stem + ".elf", "timeout_seconds": 15,
                     "expected_markers": ["event=payload_return"], "stop_conditions": ["controller changed"]}
        artifact = b"synthetic-not-executable-target-ELF-" + identifier.encode()
        build = {"git_head": head, "git_status": [], "sha256": handoff.sha(artifact), "size": len(artifact),
                 "loader_kernel_resolver": True, handoff.CANDIDATES[identifier]: True,
                 "diagnostic_build_id": "b" * 64, "diagnostic_candidate": stem}
        build_data = handoff.json_bytes(build)
        source = {"ps5-bt-source/project/" + name: value for name, value in files.items()}
        source["ps5-bt-source/build-manifest.json"] = (build_data, 0o644)
        source["ps5-bt-source/README.source.txt"] = (b"synthetic rebuild instructions", 0o644)
        if identifier in ("application-generated", "sbc-selftest", "sbc-continuous"):
            for name in ("relink/relink.py", "relink/bundle-manifest.json"):
                source["ps5-bt-source/" + name] = (b"synthetic replacement-link fixture", 0o644)
        source_data = handoff.tar_bytes(source)
        directory = kit / identifier
        directory.mkdir()
        for name, data in ((stem + ".elf", artifact), (stem + ".manifest.json", build_data),
                           (stem + "-source.tar.gz", source_data)):
            (directory / name).write_bytes(data)
        entries.append({"candidate": candidate, "artifact_sha256": handoff.sha(artifact),
                        "artifact_size": len(artifact), "build_manifest_sha256": handoff.sha(build_data),
                        "source_bundle_sha256": handoff.sha(source_data), "diagnostic_build_id": "b" * 64})
        plan["candidates"].append(candidate)
    (kit / "test-plan.json").write_bytes(handoff.json_bytes(plan))
    (kit / "kit-manifest.json").write_bytes(handoff.json_bytes({"schema": 1, "git_head": head, "entries": entries}))
    from mvp_test_kit import runbook
    (kit / "RUNBOOK.md").write_text(runbook(plan, head, entries))
    return kit, head


class HandoffTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name).resolve()
        self.root = self.base / "project"
        self.kit, self.head = fixture(self.root)
        self.output = self.root / "build/transfer.tar.gz"

    def export(self):
        return handoff.export(self.root, self.kit, self.output)

    def test_deterministic_export_safe_extract_and_portable_python_dry_run(self):
        first = self.export()
        second_path = self.root / "build/again.tar.gz"
        second = handoff.export(self.root, self.kit, second_path)
        self.assertEqual(first, second)
        self.assertEqual(self.output.read_bytes(), second_path.read_bytes())
        self.assertEqual(first["git_head"], self.head)
        self.assertEqual(first["candidates"], 3)
        self.assertEqual(handoff.inspect(self.output, first["archive_sha256"]), first)
        bootstrap = Path(str(self.output) + ".bootstrap.py")
        self.assertEqual(bootstrap.read_bytes(), (self.root / "tools/window_handoff.py").read_bytes())
        # A standalone copied tool needs no sibling modules, Git, Rust or SDK.
        result = command(self.base, sys.executable, str(bootstrap), "inspect", str(self.output),
                         "--expect-sha256", first["archive_sha256"])
        self.assertEqual(json.loads(result.stdout), first)
        destination = self.base / "new-machine"
        self.assertEqual(handoff.extract(self.output, destination), first)
        self.assertFalse((destination / ".git").exists())
        self.assertEqual((destination / "tools/executable.sh").stat().st_mode & 0o777, 0o755)
        self.assertEqual((destination / "tools/window_handoff.py").read_bytes(), bootstrap.read_bytes())
        result = command(destination, sys.executable, "tools/console_window.py", "--preflight",
                         "--kit", handoff.KIT)
        self.assertIn(b'"mode": "dry-run"', result.stdout)
        self.assertNotIn(str(self.root).encode(), result.stdout)
        for suffix in ("", ".bootstrap.py", ".sha256"):
            self.assertEqual(Path(str(self.output) + suffix).stat().st_mode & 0o777, 0o600)

    def test_six_profile_plan_includes_distinct_setup_and_preserves_order(self):
        root = self.base / "six-project"
        identifiers = ("application-setup", "sbc-continuous", "capture-stream",
                       "application-control", "application-generated", "application-observe")
        kit, _ = fixture(root, identifiers)
        output = root / "build/six.tar.gz"
        report = handoff.export(root, kit, output)
        self.assertEqual(report["candidates"], 6)
        self.assertEqual(handoff.inspect(output), report)
        _, files = handoff.inspect_bytes(output.read_bytes())
        plan = handoff.json_read(files[handoff.KIT + "/test-plan.json"][0])
        self.assertEqual(tuple(item["id"] for item in plan["candidates"]), identifiers)
        manifest = handoff.json_read(files[handoff.KIT + "/application-setup/ps5-bt-application-setup.manifest.json"][0])
        self.assertIs(manifest["application_setup"], True)
        self.assertEqual(manifest["diagnostic_candidate"], "ps5-bt-application-setup")

    def mutate_candidate(self, field, value):
        plan_path, manifest_path = self.kit / "test-plan.json", self.kit / "kit-manifest.json"
        plan = handoff.json_read(plan_path.read_bytes())
        manifest = handoff.json_read(manifest_path.read_bytes())
        plan["candidates"][0][field] = value
        manifest["entries"][0]["candidate"][field] = value
        plan_path.write_bytes(handoff.json_bytes(plan))
        manifest_path.write_bytes(handoff.json_bytes(manifest))

    def test_wrong_slot_and_malformed_marker_stop_lists_are_refused(self):
        for field, value in (("slot", "unrelated-local-slot"),
                ("slot", "ps5-bt-application-generated"),
                *((field, value) for field in ("expected_markers", "stop_conditions")
                  for value in ([], None, "text", [None], [1], [""], ["   "],
                                ["line\ncommand"], ["x\x7f"], ["x" * 513],
                                ["x"] * 33))):
            with self.subTest(field=field, value=value):
                original_plan = (self.kit / "test-plan.json").read_bytes()
                original_manifest = (self.kit / "kit-manifest.json").read_bytes()
                self.mutate_candidate(field, value)
                # Validation fails independently of the canonical-runbook check.
                with self.assertRaises(ValueError):
                    handoff.validate_kit(handoff.kit_inventory(self.kit), self.head)
                with self.assertRaises(ValueError):
                    self.export()
                self.assertFalse(self.output.exists())
                (self.kit / "test-plan.json").write_bytes(original_plan)
                (self.kit / "kit-manifest.json").write_bytes(original_manifest)

    def test_legacy_slot_aliases_keep_distinct_artifact_stems(self):
        root = self.base / "legacy-project"
        kit, _ = fixture(root, ("usb-binding", "capture-settled"))
        output = root / "build/legacy.tar.gz"
        report = handoff.export(root, kit, output)
        self.assertEqual(report["candidates"], 2)
        self.assertEqual(handoff.inspect(output), report)
        plan = handoff.json_read((kit / "test-plan.json").read_bytes())
        self.assertEqual([(item["slot"], item["remote_filename"]) for item in plan["candidates"]],
            [("ps5-bt-usb-binding", "ps5-bt-usb-metadata.elf"),
             ("ps5-bt-capture-settled", "ps5-bt-capture-once.elf")])

    def test_modified_runbook_is_refused_before_publication(self):
        (self.kit / "RUNBOOK.md").write_text("changed operator commands\n")
        with self.assertRaises(ValueError):
            self.export()
        self.assertFalse(self.output.exists())

    def test_dirty_untracked_modified_and_stale_head_refused(self):
        dirty = self.root / "new-secret.log"
        dirty.write_text("must never enter transfer")
        with self.assertRaises(ValueError):
            self.export()
        dirty.unlink()
        tracked = self.root / "README.md"
        original = tracked.read_bytes()
        tracked.write_bytes(b"dirty change")
        with self.assertRaises(ValueError):
            self.export()
        tracked.write_bytes(original)
        path = self.kit / "kit-manifest.json"
        value = json.loads(path.read_text())
        value["git_head"] = "0" * 40
        path.write_bytes(handoff.json_bytes(value))
        with self.assertRaises(ValueError):
            self.export()
        self.assertFalse(self.output.exists())

    def test_no_overwrite_or_symlink_inputs_outputs_destination(self):
        self.export()
        for value in (self.output, self.base / "existing"):
            value.touch(exist_ok=True)
        with self.assertRaises(ValueError):
            self.export()
        with self.assertRaises(ValueError):
            handoff.extract(self.output, self.base / "existing")
        link = self.base / "archive-link"
        link.symlink_to(self.output)
        with self.assertRaises(OSError):
            handoff.inspect(link)
        directory_link = self.base / "directory-link"
        directory_link.symlink_to(self.root / "build", target_is_directory=True)
        with self.assertRaises(ValueError):
            handoff.extract(self.output, directory_link / "fresh")
        kit_link = self.base / "kit-link"
        kit_link.symlink_to(self.kit, target_is_directory=True)
        with self.assertRaises(ValueError):
            handoff.export(self.root, kit_link, self.root / "build/other.tar.gz")

    def test_extra_log_symlink_and_tampered_kit_artifact_fail(self):
        extra = self.kit / "secret.log"
        extra.write_bytes(b"must not be packaged")
        with self.assertRaises(ValueError):
            self.export()
        extra.unlink()
        extra.symlink_to(self.root / "README.md")
        with self.assertRaises(OSError):
            self.export()
        extra.unlink()
        artifact = next(self.kit.glob("*/*.elf"))
        artifact.write_bytes(b"changed")
        with self.assertRaises(ValueError):
            self.export()

    def rewrite_source(self, mutate):
        meta_path = self.kit / "kit-manifest.json"
        meta = json.loads(meta_path.read_text())
        entry = meta["entries"][0]
        path = self.kit / entry["candidate"]["id"] / Path(entry["candidate"]["source_bundle"]).name
        files = handoff.tar_files(path.read_bytes())
        mutate(files)
        path.write_bytes(handoff.tar_bytes(files))
        entry["source_bundle_sha256"] = handoff.sha(path.read_bytes())
        meta_path.write_bytes(handoff.json_bytes(meta))

    def test_nested_source_must_match_current_committed_source_and_build_manifest(self):
        self.rewrite_source(lambda files: files.__setitem__("ps5-bt-source/project/README.md", (b"changed committed source", 0o644)))
        with self.assertRaises(ValueError):
            self.export()

    def test_nested_source_manifest_mismatch_fails(self):
        self.rewrite_source(lambda files: files.__setitem__("ps5-bt-source/build-manifest.json", (b"{}", 0o644)))
        with self.assertRaises(ValueError):
            self.export()

    def test_source_executable_mode_must_match_committed_git_mode(self):
        self.rewrite_source(lambda files: files.__setitem__("ps5-bt-source/project/tools/executable.sh", (b"#!/bin/sh\nexit 0\n", 0o644)))
        with self.assertRaises(ValueError):
            self.export()

    def test_extraction_failure_cleans_staging_and_reserves_no_output(self):
        self.export()
        destination = self.base / "unpublished"
        original_open = Path.open
        def fail_extract(path, *args, **kwargs):
            if args and args[0] == "xb":
                raise OSError("synthetic disk failure")
            return original_open(path, *args, **kwargs)
        with patch.object(Path, "open", fail_extract):
            with self.assertRaises(OSError):
                handoff.extract(self.output, destination)
        self.assertFalse(destination.exists())
        self.assertEqual(list(self.base.glob(".ps5-bt-window-*")), [])

    def test_corrupt_archive_wrong_expected_hash_and_manifest_inventory_fail(self):
        self.export()
        with self.assertRaises(ValueError):
            handoff.inspect(self.output, "0" * 64)
        files = handoff.tar_files(self.output.read_bytes())
        files[handoff.PREFIX + "README.md"] = (b"modified after export", 0o644)
        self.output.write_bytes(handoff.tar_bytes(files))
        with self.assertRaises(ValueError):
            handoff.inspect(self.output)
        self.assertFalse((self.base / "fresh").exists())
        with self.assertRaises(ValueError):
            handoff.extract(self.output, self.base / "fresh")
        self.assertFalse((self.base / "fresh").exists())

    def test_archive_rejects_traversal_links_duplicates_special_modes_and_aliases(self):
        for name, kind, mode in (("../escape", tarfile.REGTYPE, 0o644),
                ("/absolute", tarfile.REGTYPE, 0o644), ("a\\b", tarfile.REGTYPE, 0o644),
                ("a/./b", tarfile.REGTYPE, 0o644), (".git/config", tarfile.REGTYPE, 0o644),
                ("a", tarfile.SYMTYPE, 0o644), ("a", tarfile.LNKTYPE, 0o644),
                ("a", tarfile.DIRTYPE, 0o755), ("a", tarfile.FIFOTYPE, 0o644),
                ("a", tarfile.REGTYPE, 0o4755)):
            with self.subTest(name=name, kind=kind):
                output = io.BytesIO()
                with tarfile.open(fileobj=output, mode="w:gz") as archive:
                    info = tarfile.TarInfo(name)
                    info.type, info.mode = kind, mode
                    archive.addfile(info)
                with self.assertRaises(ValueError):
                    handoff.tar_files(output.getvalue())
        output = io.BytesIO()
        with tarfile.open(fileobj=output, mode="w:gz") as archive:
            for _ in range(2):
                info = tarfile.TarInfo("duplicate")
                info.mode = 0o644
                archive.addfile(info)
        with self.assertRaises(ValueError):
            handoff.tar_files(output.getvalue())
        with self.assertRaises(ValueError):
            handoff.tar_files(handoff.tar_bytes({"a": (b"", 0o644), "a/b": (b"", 0o644)}))

    def test_size_and_inventory_bounds_are_enforced(self):
        with patch.object(handoff, "MAX_FILE_BYTES", 2):
            with self.assertRaises(ValueError):
                handoff.bounded_file(self.root / "README.md", 2)
            with self.assertRaises(ValueError):
                handoff.tar_bytes({"large": (b"123", 0o644)})
        encoded = handoff.tar_bytes({"a": (b"123", 0o644), "b": (b"123", 0o644)})
        with patch.object(handoff, "MAX_TOTAL_BYTES", 4):
            with self.assertRaises(ValueError):
                handoff.tar_files(encoded)
        with patch.object(handoff, "MAX_FILES", 1):
            with self.assertRaises(ValueError):
                handoff.tar_files(encoded)
        with self.assertRaises(ValueError):
            handoff.json_read(b'{"schema":1,"schema":1}')

    def test_partial_export_failure_removes_only_owned_outputs(self):
        original_open = os.open
        def fail_sidecar(path, *args, **kwargs):
            if str(path).endswith(".bootstrap.py"):
                raise OSError("synthetic publication failure")
            return original_open(path, *args, **kwargs)
        with patch.object(handoff.os, "open", side_effect=fail_sidecar):
            with self.assertRaises(OSError):
                self.export()
        for suffix in ("", ".bootstrap.py", ".sha256"):
            self.assertFalse(Path(str(self.output) + suffix).exists())

    def test_cli_failure_redacts_member_names_and_has_no_network(self):
        self.output.write_bytes(b"not a gzip archive; private-secret")
        error = io.StringIO()
        with contextlib.redirect_stderr(error):
            self.assertEqual(handoff.main(["inspect", str(self.output)]), 1)
        self.assertEqual(error.getvalue(), "FAIL: offline handoff validation or exclusive output failed\n")


if __name__ == "__main__":
    unittest.main()
