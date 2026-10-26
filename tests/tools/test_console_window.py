#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Real kit/collector APIs around fake FTP only; never connects to a console."""
import contextlib
import ftplib
import hashlib
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tools"))
sys.path.insert(0, str(Path(__file__).resolve().parent))
import console_window as window
from test_diagnostic_result import application_log, application_mode_log, setup_log, namespace_log, log, manifest, BUILD

SECRET = "server-reply-secret-never-print"


def digest(data):
    return hashlib.sha256(data).hexdigest()


SIX_CANDIDATES = tuple(item for item in window.CANDIDATES if item != "application-namespace")
LEGACY_CANDIDATES = ("application-control", "application-generated", "application-observe")


def make_kit(root, identifiers=LEGACY_CANDIDATES):
    root.mkdir()
    plan = {"schema": 1, "target_host": window.TARGET, "ftp_port": window.PORT,
            "remote_root": window.REMOTE_ROOT, "remote_log": window.REMOTE_LOG, "candidates": []}
    if tuple(identifiers) == ("application-namespace",):
        plan["remote_log"] = window.mvp_test_kit.NAMESPACE_LOG
    entries, remote = [], {}
    for order, identifier in enumerate(identifiers, 1):
        stem = "ps5-bt-" + identifier
        candidate = {"id": identifier, "order": order, "artifact": f"build/ps5/{stem}.elf",
                     "build_manifest": f"build/ps5/{stem}.manifest.json", "source_bundle": f"build/ps5/{stem}-source.tar.gz",
                     "slot": stem, "remote_filename": stem + ".elf", "timeout_seconds": 15,
                     "expected_markers": ["event=payload_return"], "stop_conditions": ["controller changes"]}
        plan["candidates"].append(candidate)
        directory = root / identifier
        directory.mkdir()
        artifact = ("synthetic-own-ELF-" + identifier).encode()
        build = manifest(identifier)
        build.update(sha256=digest(artifact), size=len(artifact))
        build_bytes = json.dumps(build).encode()
        source = b"synthetic-source-bundle"
        for name, data in [(stem + ".elf", artifact), (stem + ".manifest.json", build_bytes), (stem + "-source.tar.gz", source)]:
            (directory / name).write_bytes(data)
        entries.append({"candidate": candidate, "artifact_sha256": digest(artifact), "artifact_size": len(artifact),
                        "build_manifest_sha256": digest(build_bytes), "source_bundle_sha256": digest(source)})
        prefix = window.REMOTE_ROOT + "/" + stem
        remote[prefix + "/meta.json"] = json.dumps({"schema": 2, "kind": "local", "filename": stem + ".elf", "ignored_secret": SECRET}).encode()
        remote[prefix + "/" + stem + ".elf"] = artifact
    (root / "test-plan.json").write_text(json.dumps(plan))
    (root / "kit-manifest.json").write_text(json.dumps({"schema": 1, "git_head": "c" * 40, "entries": entries}))
    remote[plan["remote_log"]] = (namespace_log() if tuple(identifiers) == ("application-namespace",) else application_log()).encode()
    return remote


class FakeFTP:
    def __init__(self, remote):
        self.remote = dict(remote)
        self.directory = "/"
        self.calls = []
        self.closes = 0
        self.fail_connect = False
        self.fail_quit = False
        self.fail_close = False
        self.size_override = None
        self.interrupt_at = None
        self.on_retrieve = None

    def connect(self, host, port, timeout):
        self.calls.append(("connect", host, port, timeout))
        assert (host, port, timeout) == (window.TARGET, window.PORT, window.SOCKET_TIMEOUT)
        if self.fail_connect:
            raise OSError(SECRET)

    def login(self, username, password):
        self.calls.append(("login", username, password))
        assert (username, password) == ("anonymous", "ps5-bt@localhost")

    def cwd(self, path):
        self.calls.append(("cwd", path))
        assert path in [window.REMOTE_ROOT + "/ps5-bt-" + value for value in window.CANDIDATES]
        self.directory = path

    def voidcmd(self, command):
        self.calls.append(("voidcmd", command))
        assert command == "TYPE I"

    def size(self, filename):
        self.calls.append(("size", filename))
        if self.size_override is not None:
            return self.size_override
        return len(self.remote[self.directory + "/" + filename])

    def retrbinary(self, command, callback, blocksize):
        self.calls.append(("retrbinary", command, blocksize))
        assert command.startswith("RETR ")
        name = command[5:]
        path = name if name.startswith("/") else self.directory + "/" + name
        assert path == window.REMOTE_LOG or path in self.remote
        data = self.remote[path]
        if self.on_retrieve:
            self.on_retrieve()
        limit = len(data) if self.interrupt_at is None else min(self.interrupt_at, len(data))
        for start in range(0, limit, 4096):
            callback(data[start:min(start + 4096, limit)])
        if self.interrupt_at is not None:
            raise EOFError(SECRET)

    def quit(self):
        self.calls.append(("quit",))
        if self.fail_quit:
            raise ftplib.error_temp(SECRET)
        self.close()

    def close(self):
        self.calls.append(("close",))
        self.closes += 1
        if self.fail_close:
            raise OSError(SECRET)


class ConsoleWindowTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.kit = self.root / "kit"
        self.remote = make_kit(self.kit)
        self.output = self.root / "private-results"

    def fake(self):
        return FakeFTP(self.remote)

    def collect(self, ftp, **kwargs):
        return window.collect_window(self.kit, "application-control", execute=True,
                                     output_dir=self.output, ftp_factory=lambda: ftp, **kwargs)

    def test_dry_runs_validate_real_kit_without_constructing_ftp_or_output(self):
        with patch.object(window.ftplib, "FTP", side_effect=AssertionError("offline means zero FTP")):
            report = window.preflight(self.kit, verify_hash=True)
            self.assertEqual(report["mode"], "dry-run")
            self.assertEqual(len(report["candidates"]), 3)
            self.assertEqual(report["prerequisite"]["native_owner_and_mode"], "Unknown")
            report = window.collect_window(self.kit, "application-control", output_dir=self.output)
            self.assertEqual(set(report["controller"].values()), {"Unknown"})
            self.assertEqual(report["operator_run_observation"], "Unknown")
        self.assertFalse(self.output.exists())

    def test_auto_selection_retains_legacy_three_and_six_profile_plan_order(self):
        with patch.object(window.ftplib, "FTP", side_effect=AssertionError("no FTP")):
            legacy = window.preflight(self.kit)
            self.assertEqual([item["id"] for item in legacy["candidates"]], list(LEGACY_CANDIDATES))
            identifiers = ("application-setup", "sbc-continuous", "capture-stream",
                           "application-control", "application-generated", "application-observe")
            six = self.root / "six-kit"
            remote = make_kit(six, identifiers)
            report = window.preflight(six)
            self.assertEqual([item["id"] for item in report["candidates"]], list(identifiers))
            with contextlib.redirect_stdout(io.StringIO()) as output:
                self.assertEqual(window.main(["--kit", str(six), "--preflight"]), 0)
            self.assertEqual([item["id"] for item in json.loads(output.getvalue())["candidates"]], list(identifiers))
        ftp = FakeFTP(remote)
        report = window.preflight(six, execute=True, verify_hash=True, ftp_factory=lambda: ftp)
        self.assertEqual(len(report["candidates"]), 6)
        self.assertTrue(all(item["remote_hash_match"] for item in report["candidates"]))
        self.assertEqual(ftp.closes, 1)
        self.assertEqual([call[1].rsplit("/", 1)[-1] for call in ftp.calls if call[0] == "cwd"],
                         ["ps5-bt-" + identifier for identifier in identifiers])

    def test_explicit_selection_is_bounded_available_and_keeps_requested_order(self):
        six = self.root / "six-kit"
        make_kit(six, SIX_CANDIDATES)
        with patch.object(window.ftplib, "FTP", side_effect=AssertionError("no FTP")):
            report = window.preflight(six, ["capture-stream", "application-setup"])
            self.assertEqual([item["id"] for item in report["candidates"]], ["capture-stream", "application-setup"])
            for identifiers, code in [([], "candidate-selection"), (["not-reviewed"], "candidate-selection"),
                    (["application-control", "application-control"], "candidate-selection"),
                    (list(window.CANDIDATES) + ["application-setup"], "candidate-selection"),
                    (["application-setup"], "candidate-not-in-kit"),
                    (["sbc-continuous"], "candidate-not-in-kit")]:
                with self.subTest(identifiers=identifiers):
                    with self.assertRaises(window.WindowError) as caught:
                        window.preflight(self.kit, identifiers, execute=True)
                    self.assertEqual(caught.exception.code, code)
        # Loading the kit is always first, even for an invalid explicit subset.
        with patch.object(window.mvp_test_kit, "load_kit", side_effect=ValueError("invalid")) as load:
            with self.assertRaises(window.WindowError) as caught:
                window.preflight(self.kit, [])
            self.assertEqual(caught.exception.code, "kit-invalid")
            load.assert_called_once()

    def test_auto_selection_skips_unsupported_legacy_items_and_rejects_empty_match(self):
        mixed = self.root / "mixed-kit"
        make_kit(mixed, ("usb-binding", "application-observe", "sbc-continuous"))
        only_legacy = self.root / "unsupported-kit"
        make_kit(only_legacy, ("usb-binding",))
        with patch.object(window.ftplib, "FTP", side_effect=AssertionError("no FTP")):
            report = window.preflight(mixed)
            self.assertEqual([item["id"] for item in report["candidates"]], ["application-observe", "sbc-continuous"])
            with self.assertRaises(window.WindowError) as caught:
                window.preflight(only_legacy, execute=True)
            self.assertEqual(caught.exception.code, "candidate-selection")

    def test_collect_all_six_profiles_uses_matching_existing_collectors(self):
        six = self.root / "six-kit"
        remote = make_kit(six, SIX_CANDIDATES)
        text = {"application-setup": setup_log(), "application-control": application_log(),
                "application-generated": application_mode_log("generated"),
                "application-observe": application_mode_log("observe"),
                "sbc-continuous": log("sbc-continuous"), "capture-stream": log("capture-stream")}
        for identifier in SIX_CANDIDATES:
            with self.subTest(identifier=identifier):
                ftp = FakeFTP(remote)
                ftp.remote[window.REMOTE_LOG] = text[identifier].encode()
                report = window.collect_window(six, identifier, execute=True, output_dir=self.output,
                                               ftp_factory=lambda: ftp)
                self.assertEqual(report["classification"], "complete_success")
                record = json.loads(Path(report["record"]).read_text())
                self.assertEqual(record["identity"]["candidate"], "ps5-bt-" + identifier)
                self.assertEqual(record["controller_acceptance"], "Unknown")
                self.assertEqual([call[1] for call in ftp.calls if call[0] == "retrbinary"], ["RETR " + window.REMOTE_LOG])
                self.assertEqual(ftp.closes, 1)

    def test_namespace_dedicated_log_is_selected_and_legacy_log_untouched(self):
        kit = self.root / "namespace-kit"
        remote = make_kit(kit, ("application-namespace",))
        remote[window.REMOTE_LOG] = b"old unrelated log must not be read"
        ftp = FakeFTP(remote)
        report = window.collect_window(kit, "application-namespace", execute=True,
            output_dir=self.output, ftp_factory=lambda: ftp)
        self.assertEqual(report["classification"], "complete_success")
        self.assertEqual(report["remote_log"], window.mvp_test_kit.NAMESPACE_LOG)
        self.assertEqual([call[1] for call in ftp.calls if call[0] == "retrbinary"],
                         ["RETR " + window.mvp_test_kit.NAMESPACE_LOG])
        preflight = window.preflight(kit)
        self.assertEqual(preflight["candidates"][0]["id"], "application-namespace")
        self.assertEqual(preflight["prerequisite"], {
            "protected_store_required": False,
            "validation": "not applicable to read-only root/data observation",
            "native_abi": "Unknown", "store_acceptance": "Unknown"})
        self.assertEqual(window.preflight(self.kit)["prerequisite"], {
            "directory": window.CONTROL_DIRECTORY, "required_mode": "0700",
            "native_owner_and_mode": "Unknown", "validation": "native payload required"})

    def test_namespace_wrong_or_mixed_log_plans_fail_before_ftp(self):
        kit = self.root / "namespace-kit"
        make_kit(kit, ("application-namespace",))
        plan_path = kit / "test-plan.json"
        plan = json.loads(plan_path.read_text())
        for path in (window.REMOTE_LOG, "/data/secret", "/data/../secret"):
            plan_path.write_text(json.dumps(plan | {"remote_log": path}))
            with patch.object(window.ftplib, "FTP", side_effect=AssertionError("no FTP")):
                with self.assertRaises(window.WindowError):
                    window.collect_window(kit, "application-namespace", execute=True)
        mixed = self.root / "mixed-namespace-kit"
        make_kit(mixed, ("application-namespace", "application-control"))
        with self.assertRaises(window.WindowError):
            window.preflight(mixed)

    def test_execute_preflight_only_reads_existing_metadata_size_and_optional_hash(self):
        ftp = self.fake()
        report = window.preflight(self.kit, execute=True, verify_hash=True, ftp_factory=lambda: ftp)
        self.assertTrue(all(value["remote_size_match"] and value["remote_hash_match"] for value in report["candidates"]))
        self.assertEqual(ftp.closes, 1)
        self.assertTrue(all(call[0] in {"connect", "login", "cwd", "retrbinary", "voidcmd", "size", "quit", "close"} for call in ftp.calls))
        self.assertNotIn(SECRET, json.dumps(report))
        ftp = self.fake()
        report = window.preflight(self.kit, ["application-control"], execute=True, ftp_factory=lambda: ftp)
        self.assertEqual(report["candidates"][0]["remote_hash_match"], "Unknown")
        self.assertEqual([call[1] for call in ftp.calls if call[0] == "retrbinary"], ["RETR meta.json"])

    def test_metadata_mismatch_format_and_bound_abort_without_other_operations(self):
        key = window.REMOTE_ROOT + "/ps5-bt-application-control/meta.json"
        for data, code in [(b'{"schema":2,"kind":"cloud","filename":"x.elf"}', "slot-metadata-mismatch"),
                           (b'{"schema":2,"schema":2}', "slot-metadata-format"),
                           (b"x" * (window.MAX_META_BYTES + 1), "remote-size-bound")]:
            ftp = self.fake(); ftp.remote[key] = data
            with self.assertRaises(window.WindowError) as caught:
                window.preflight(self.kit, ["application-control"], execute=True, ftp_factory=lambda: ftp)
            self.assertEqual(caught.exception.code, code)
            self.assertEqual(ftp.closes, 1)
            self.assertFalse(any(call[0] == "size" for call in ftp.calls))

    def test_remote_size_hash_and_hash_readback_bounds_fail_closed(self):
        for scenario in range(3):
            ftp = self.fake()
            key = window.REMOTE_ROOT + "/ps5-bt-application-control/ps5-bt-application-control.elf"
            if scenario == 0:
                ftp.size_override = 999
            elif scenario == 1:
                ftp.remote[key] = b"x" * len(ftp.remote[key])
            else:
                ftp.size_override = len(ftp.remote[key])
                ftp.remote[key] += b"oversized"
            with self.assertRaises(window.WindowError) as caught:
                window.preflight(self.kit, ["application-control"], execute=True, verify_hash=True, ftp_factory=lambda: ftp)
            self.assertEqual(caught.exception.code, ["remote-artifact-size", "remote-artifact-hash", "remote-size-bound"][scenario])
            self.assertEqual(ftp.closes, 1)

    def test_collect_preserves_raw_private_log_and_correlates_without_run_inference(self):
        ftp = self.fake()
        report = self.collect(ftp)
        record = json.loads(Path(report["record"]).read_text())
        self.assertEqual(record["classification"], "complete_success")
        self.assertEqual(record["controller_acceptance"], "Unknown")
        self.assertEqual(record["collection"]["operator_run_observation"], "Unknown")
        self.assertEqual(record["collection"]["ps5_log_freshness"], "Unknown")
        self.assertEqual(Path(report["log"]).read_bytes(), self.remote[window.REMOTE_LOG])
        self.assertEqual(Path(report["log"]).stat().st_mode & 0o777, 0o600)
        self.assertEqual(Path(report["record"]).stat().st_mode & 0o777, 0o600)
        self.assertEqual(self.output.stat().st_mode & 0o777, 0o700)
        self.assertEqual([call[1] for call in ftp.calls if call[0] == "retrbinary"], ["RETR " + window.REMOTE_LOG])
        self.assertFalse(any(call[0] in ("cwd", "size", "voidcmd") for call in ftp.calls))
        self.assertEqual(ftp.closes, 1)
        second = self.collect(self.fake())
        self.assertNotEqual(second["log"], report["log"])
        self.assertNotEqual(second["record"], report["record"])

    def test_partial_oversized_invalid_and_wrong_identity_logs_never_become_success(self):
        for scenario in range(4):
            ftp = self.fake()
            if scenario == 0:
                ftp.interrupt_at = self.remote[window.REMOTE_LOG].index(b"event=run_finish")
            elif scenario == 1:
                ftp.remote[window.REMOTE_LOG] = b"x" * (window.MAX_LOG_BYTES + 1)
            elif scenario == 2:
                ftp.remote[window.REMOTE_LOG] = b"\xffinvalid"
            else:
                ftp.remote[window.REMOTE_LOG] = self.remote[window.REMOTE_LOG].replace(BUILD.encode(), b"d" * 64)
            report = self.collect(ftp)
            record = json.loads(Path(report["record"]).read_text())
            self.assertEqual(record["classification"], ["interrupted", "interrupted", "incomplete_evidence", "identity_mismatch"][scenario])
            self.assertLessEqual(Path(report["log"]).stat().st_size, window.MAX_LOG_BYTES)
            self.assertEqual(ftp.closes, 1)
            self.assertNotIn(SECRET, json.dumps(record))

    def test_connect_and_cleanup_failures_are_retained_separately_and_never_claim_payload_entry(self):
        ftp = self.fake(); ftp.fail_connect = True
        report = self.collect(ftp)
        record = json.loads(Path(report["record"]).read_text())
        self.assertEqual(record["classification"], "collection_failed")
        self.assertEqual(record["collection"]["error"], "ftp-connect")
        self.assertEqual(ftp.closes, 1)
        ftp = self.fake(); ftp.fail_quit = True
        report = self.collect(ftp)
        record = json.loads(Path(report["record"]).read_text())
        self.assertEqual(record["classification"], "interrupted")
        self.assertEqual(record["diagnostic_classification"], "complete_success")
        self.assertEqual(record["collection"]["cleanup_error"], "ftp-cleanup")
        self.assertEqual(ftp.closes, 1)

    def test_explicit_human_observations_are_retained_and_cleanup_close_failure_is_visible(self):
        ftp = self.fake()
        report = self.collect(ftp, observations={"before": "normal", "during": "changed", "after": "Unknown"}, run_observation="observed")
        record = json.loads(Path(report["record"]).read_text())
        self.assertEqual(record["classification"], "complete_failure")
        self.assertEqual(record["controller"]["during"], "changed")
        self.assertEqual(record["collection"]["operator_run_observation"], "observed")
        ftp = self.fake(); ftp.fail_connect = True; ftp.fail_close = True
        report = self.collect(ftp)
        record = json.loads(Path(report["record"]).read_text())
        self.assertEqual(record["collection"]["error"], "ftp-connect")
        self.assertEqual(record["collection"]["cleanup_error"], "ftp-cleanup")
        self.assertEqual(ftp.closes, 1)

    def test_elapsed_transfer_budget_stops_trickling_data(self):
        ticks = [0]
        ftp = self.fake()
        ftp.on_retrieve = lambda: ticks.__setitem__(0, window.TOTAL_SECONDS)
        report = self.collect(ftp, clock=lambda: ticks[0])
        record = json.loads(Path(report["record"]).read_text())
        self.assertEqual(record["classification"], "collection_failed")
        self.assertEqual(record["collection"]["error"], "ftp-time-budget")
        self.assertEqual(ftp.closes, 1)

    def test_bad_local_kit_and_directory_reject_before_network(self):
        artifact = self.kit / "application-control/ps5-bt-application-control.elf"
        original = artifact.read_bytes(); artifact.write_bytes(b"modified")
        with patch.object(window.ftplib, "FTP", side_effect=AssertionError("no FTP")):
            with self.assertRaises(window.WindowError):
                window.preflight(self.kit, execute=True)
        artifact.write_bytes(original)
        self.output.mkdir(mode=0o755)
        self.output.chmod(0o755)
        with patch.object(window.ftplib, "FTP", side_effect=AssertionError("no FTP")):
            with self.assertRaises(window.WindowError) as caught:
                self.collect(self.fake())
            self.assertEqual(caught.exception.code, "local-directory-protection")

    def test_local_fsync_failure_is_visible_and_network_is_closed(self):
        ftp = self.fake()
        with patch.object(os, "fsync", side_effect=OSError(SECRET)):
            with self.assertRaises(window.WindowError) as caught:
                self.collect(ftp)
        self.assertEqual(caught.exception.code, "local-log-persistence")
        self.assertEqual(ftp.closes, 1)
        self.assertNotIn(SECRET, str(caught.exception))
        partial = list(self.output.glob("*.part"))
        self.assertEqual(len(partial), 1)
        self.assertEqual(partial[0].stat().st_mode & 0o777, 0o600)

    def test_cli_offline_defaults_unknown_and_errors_never_dump_server_response(self):
        with patch.object(window.ftplib, "FTP", side_effect=AssertionError("no FTP")), contextlib.redirect_stdout(io.StringIO()) as output:
            self.assertEqual(window.main(["--kit", str(self.kit), "--collect", "--candidate", "application-control"]), 0)
        self.assertEqual(json.loads(output.getvalue())["operator_run_observation"], "Unknown")
        ftp = self.fake(); ftp.size_override = 999; ftp.fail_quit = True
        with patch.object(window.ftplib, "FTP", return_value=ftp), contextlib.redirect_stderr(io.StringIO()) as output:
            self.assertEqual(window.main(["--kit", str(self.kit), "--preflight", "--candidate", "application-control", "--execute"]), 1)
        self.assertNotIn(SECRET, output.getvalue())
        self.assertIn("remote-artifact-size", output.getvalue())
        self.assertIn("ftp-cleanup", output.getvalue())


if __name__ == "__main__":
    unittest.main()
