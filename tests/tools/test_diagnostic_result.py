#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Finite offline evidence collection; no console, audio or network access."""
import contextlib
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tools"))
import diagnostic_result as result

BUILD = "a" * 64
NORMAL = {"before": "normal", "during": "normal", "after": "normal"}


def manifest(candidate="sbc-continuous"):
    return {"diagnostic_build_id": BUILD, "diagnostic_candidate": f"ps5-bt-{candidate}",
            candidate.replace("-", "_"): True, "sha256": "b" * 64, "size": 120000,
            "git_head": "c" * 40, "git_status": []}


def log(candidate="sbc-continuous", status=2):
    if candidate == "sbc-continuous":
        fields = {"outcome": 0, "blocks": 750, "pcm_frames": 96000, "bytes": 57750,
                  "elapsed_us_hi": 0, "elapsed_us_lo": 12345, "prefix": 0x9cfd20cb,
                  "digest": 0x7784d1fb, "cleanup_attempted": 1}
        body = [result.SBC_CONFIG] + [f"sbc_continuous_{key}=0x{value:08x}" for key, value in fields.items()] + ["event=sbc_continuous_complete"]
    else:
        body = [result.CAPTURE_CONFIG, "capture_stream_kind=0x00000003",
                "capture_stream_read_status_bits=0x81950002", "capture_stream_bytes=0x00000000",
                "capture_stream_discontinuity=0x00000000", "capture_stream_stop_bits=0x00000000",
                "capture_stream_close_bits=0x00000000", "event=capture_stream_cleanup_complete",
                "event=capture_stream_complete"]
    return "\n".join(["event=payload_start log=/data/ps5-bt-hw1.log",
                      f"event=run_begin build_id={BUILD} candidate=ps5-bt-{candidate}", *body,
                      f"event=run_finish build_id={BUILD}", f"event=payload_exit status={status}",
                      f"event=payload_return status={status}", ""])


def imports(stage, status=0, resolved=0, cleanup_flags=0, cleanup_errno=0, errno=0):
    values = {"stage": stage, "status_bits": status, "errno": errno, "lookup_hi": 0,
              "lookup_lo": 0, "return_hi": 0, "return_lo": 16 if stage == 6 and status == 0 else 0,
              "resolved": resolved, "cleanup_flags": cleanup_flags,
              "cleanup_errno": cleanup_errno}
    return [f"application_import_{key}=0x{value:08x}" for key, value in values.items()]


def application_log(fields=None, preflight=None, status=2):
    body = [result.APPLICATION_PREFLIGHT]
    if preflight == "failed":
        body += imports(2, status=0xffffffff, resolved=3)
        body += ["event=application_preflight_failed"]
    elif preflight == "cancelled":
        body += imports(2, status=1, resolved=3)
        body += ["event=application_preflight_cancelled"]
    elif preflight == "entropy_cancelled":
        body += imports(3, resolved=23) + imports(4, status=1)
        body += ["event=application_preflight_cancelled"] + imports(8)
        body += ["event=application_preflight_cleanup_complete"]
    else:
        values = {**result.APPLICATION_BUDGET, "result": 0, "drive_entered": 1, "factory_entered": 1,
                  "stop_reason": 2, "drive_failure": 0, "clock_calls": 1000, "turns": 1000,
                  "waits": 990, "forced": 0, "finished": 1, "stop_error_bits": 0,
                  "cleanup_uncertain": 0, "required_mask": 7, "active_mask": 0, "cleanup_mask": 7,
                  "cleanup_failed_mask": 0, "clients": 2, "client_failures": 0, "protocols_closed": 1}
        if fields is not None:
            values.update(fields)
        body += imports(3, resolved=23) + imports(6)
        body += [result.APPLICATION_CONFIG]
        body += [f"application_{key}=0x{value:08x}" for key, value in values.items()]
        body += ["event=application_complete"] + imports(8)
        body += ["event=application_preflight_cleanup_complete"]
    return "\n".join([f"event=run_begin build_id={BUILD} candidate=ps5-bt-application-control", *body,
                      f"event=run_finish build_id={BUILD}", f"event=payload_exit status={status}",
                      f"event=payload_return status={status}", ""])


def application_mode_log(mode, fields=None, workload=None, preflight=None, status=2):
    mask = 23 if mode == "generated" else 15
    overrides = {"required_mask": mask, "cleanup_mask": mask, "cleanup_turns": 5, "stop_reason": 1}
    if fields:
        overrides.update(fields)
    text = application_log(overrides, preflight=preflight, status=status)
    text = text.replace("candidate=ps5-bt-application-control", "candidate=ps5-bt-application-" + mode)
    text = text.replace(result.APPLICATION_CONFIG, result.GENERATED_CONFIG if mode == "generated" else result.OBSERVE_CONFIG)
    if preflight is None:
        if mode == "generated":
            values = {"phase": 1, "error": 0, "blocks": 750, "discarded_blocks": 750, "bytes": 57750,
                      "operations": 3000, "prefix": 0x9cfd20cb, "digest": 0x7784d1fb,
                      "summary_matches": 2, "elapsed_hi": 0, "elapsed_lo": 1_999_000,
                      "concealed": 0, "rebuffer": 0, "expired": 0, "stopped_frames": 0,
                      "stopped_blocks": 0, "codec_bits": 0, "completion_observed": 1}
        else:
            values = {"reason": 2, "error": 0, "reads": 190, "data": 0, "empty": 190, "zero": 0,
                      "overruns": 0, "ended": 0, "native_errors": 0, "owner_failures": 0,
                      "gaps": 0, "bytes_hi": 0, "bytes_lo": 0, "max_bytes": 0,
                      "last_raw_bits": 0x81950002, "last_kind": 3, "last_discontinuity": 0,
                      "native_time_present": 0, "native_value_hi": 0, "native_value_lo": 0,
                      "native_domain": 0, "host_hi": 0, "host_lo": 2_990_000,
                      "started_hi": 0, "started_lo": 1_000_000, "elapsed_hi": 0, "elapsed_lo": 2_000_000,
                      "max_reads": 256, "duration_us": 2_000_000, "first_settle_us": 100_000, "interval_us": 10_000}
        if workload:
            values.update(workload)
        numeric = "".join(f"application_{mode}_{key}=0x{value:08x}\n" for key, value in values.items())
        text = text.replace("event=application_complete", numeric + "event=application_complete")
    return text


def setup_log(fields=None, outcome="complete", init_status=0, status=2, schema=2):
    values = {"stage": 6, "status_bits": 0, "errno": 0, "lookup_hi": 0, "lookup_lo": 0,
              "created_mask": 0, "verified_mask": 15, "opened_mask": 15, "closed_mask": 15,
              "cleanup_failed_mask": 0, "cleanup_errno": 0, "cleanup_raw_bits": 0,
              "prior_cleanup_mask": 0, "operations": 13, "raw_bits": 0}
    if schema == 2:
        values["validation_failed_mask"] = 0
    if fields:
        values.update(fields)
    body = [result.SETUP_CONFIG if schema == 2 else result.SETUP_CONFIG_V1]
    if init_status:
        body += imports(2, status=init_status, resolved=3)
    else:
        body += imports(3, resolved=23)
        body += [f"application_setup_{key}=0x{value:08x}" for key, value in values.items()]
        body += imports(8) + ["event=application_setup_imports_closed"]
    body += ["event=application_setup_" + outcome]
    return "\n".join([f"event=run_begin build_id={BUILD} candidate=ps5-bt-application-setup", *body,
                      f"event=run_finish build_id={BUILD}", f"event=payload_exit status={status}",
                      f"event=payload_return status={status}", ""])


class CollectorTest(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.manifest = self.root / "manifest.json"
        self.log = self.root / "saved.log"
        self.output = self.root / "result.json"

    def collect(self, text=None, source=None, observations=None):
        self.manifest.write_text(json.dumps(manifest() if source is None else source))
        self.log.write_text(log() if text is None else text)
        return result.collect(self.manifest, self.log, NORMAL if observations is None else observations)

    def test_setup_existing_or_created_fixed_namespace_success_and_redaction(self):
        for created, operations in [(0, 13), (1, 17), (2, 17), (3, 21)]:
            text = setup_log({"created_mask": created, "operations": operations})
            text = text.replace("event=application_setup_complete", "application_setup_private_path=/private-secret\napplication_setup_address=0xdeadbeef\napplication_setup_token=do-not-copy\nevent=application_setup_complete")
            record = self.collect(text, manifest("application-setup"))
            self.assertEqual(record["classification"], "complete_success")
            report = record["diagnostic"]
            self.assertEqual(report["cleanup"], "reported_complete")
            self.assertEqual(report["numeric"]["created_mask"], [created])
            self.assertEqual([group["stage"] for group in report["imports"]], [3, 8])
            self.assertEqual(report["configuration"]["native_format"], "Unknown")
            for private in ("private-secret", "deadbeef", "do-not-copy"):
                self.assertNotIn(private, json.dumps(record))
            self.assertEqual(record["ps5_persistence"], "Unknown")

    def test_setup_bad_masks_operations_status_lookup_raw_or_cleanup_fail(self):
        for fields in [{"stage": 5}, {"status_bits": 1}, {"status_bits": 0xffffffff},
                       {"errno": 5}, {"lookup_lo": 1}, {"lookup_hi": 0xffffffff},
                       {"created_mask": 4}, {"verified_mask": 7}, {"opened_mask": 31},
                       {"closed_mask": 7}, {"cleanup_failed_mask": 1}, {"cleanup_failed_mask": 16},
                       {"cleanup_errno": 4}, {"cleanup_raw_bits": 0xffffffff},
                       {"prior_cleanup_mask": 1}, {"operations": 0}, {"operations": 33},
                       {"raw_bits": 0xffffffff}, {"raw_bits": 4}]:
            with self.subTest(fields=fields):
                self.assertEqual(self.collect(setup_log(fields), manifest("application-setup"))["classification"], "complete_failure")

    def test_setup_legacy_schema_preserves_success_cancel_and_failure(self):
        examples = [(setup_log(schema=1), "complete_success"),
                    (setup_log({"status_bits": 1}, outcome="cancelled", schema=1), "incomplete_evidence"),
                    (setup_log({"stage": 2, "status_bits": 0xfffffffc, "verified_mask": 0,
                                "opened_mask": 1, "closed_mask": 1, "operations": 4},
                               outcome="failed", status=1, schema=1), "complete_failure")]
        for text, expected in examples:
            with self.subTest(expected=expected):
                record = self.collect(text, manifest("application-setup"))
                self.assertEqual(record["classification"], expected)
                self.assertEqual(record["diagnostic"]["configuration"]["schema"], 1)
                self.assertNotIn("validation_failed_mask", record["diagnostic"]["numeric"])
        mixed = setup_log({"validation_failed_mask": 0}, schema=1)
        record = self.collect(mixed, manifest("application-setup"))
        self.assertEqual(record["classification"], "complete_failure")
        self.assertIn("setup_contract_mismatch", record["diagnostic"]["issues"])

    def test_setup_validation_bits_retain_policy_evidence_at_applicable_stages(self):
        for stage, created, masks in [(2, 0, (1, 2, 4, 7)), (3, 0, (1, 2, 4, 8, 15)),
                                      (4, 0, (1, 2, 4, 8, 15)), (4, 1, (16, 32, 63)),
                                      (5, 0, (1, 2, 4, 8, 16, 32, 63))]:
            for rejected in masks:
                component = 1 << (stage - 2)
                fields = {"stage": stage, "status_bits": 0xfffffffc,
                          "validation_failed_mask": rejected, "created_mask": created,
                          "opened_mask": component * 2 - 1, "closed_mask": component * 2 - 1,
                          "verified_mask": component - 1, "operations": 4}
                with self.subTest(stage=stage, rejected=rejected, created=created):
                    record = self.collect(setup_log(fields, outcome="failed", status=1), manifest("application-setup"))
                    self.assertEqual(record["classification"], "complete_failure")
                    self.assertNotIn("setup_contract_mismatch", record["diagnostic"]["issues"])
                    self.assertEqual(record["diagnostic"]["numeric"]["validation_failed_mask"], [rejected])
                    self.assertEqual(record["diagnostic"]["descriptor_cleanup"], "reported_complete")
        # A pre-open type rejection has no current-component descriptor.
        fields = {"stage": 3, "status_bits": 0xfffffffc, "validation_failed_mask": 1,
                  "opened_mask": 1, "closed_mask": 1, "verified_mask": 1, "operations": 5}
        record = self.collect(setup_log(fields, outcome="failed", status=1), manifest("application-setup"))
        self.assertNotIn("setup_contract_mismatch", record["diagnostic"]["issues"])

    def test_setup_validation_bits_reject_incoherent_context_or_status(self):
        base = {"stage": 2, "status_bits": 0xfffffffc, "validation_failed_mask": 2,
                "opened_mask": 1, "closed_mask": 1, "verified_mask": 0, "operations": 4}
        for changed in ({"validation_failed_mask": 0}, {"validation_failed_mask": 64},
                        {"validation_failed_mask": 8}, {"validation_failed_mask": 16},
                        {"validation_failed_mask": 32}, {"stage": 1}, {"stage": 6},
                        {"status_bits": 0}, {"status_bits": 1}, {"status_bits": 0xffffffff},
                        {"errno": 1}, {"raw_bits": 0xffffffff}, {"lookup_lo": 1},
                        {"verified_mask": 1}, {"opened_mask": 0, "closed_mask": 0},
                        {"validation_failed_mask": 1, "opened_mask": 0, "closed_mask": 0},
                        {"stage": 3, "validation_failed_mask": 16, "opened_mask": 3, "closed_mask": 3},
                        {"stage": 4, "validation_failed_mask": 32, "opened_mask": 7, "closed_mask": 7},
                        {"stage": 5, "validation_failed_mask": 8, "opened_mask": 7, "closed_mask": 7}):
            with self.subTest(changed=changed):
                record = self.collect(setup_log(base | changed, outcome="failed", status=1), manifest("application-setup"))
                self.assertEqual(record["classification"], "complete_failure")
                self.assertIn("setup_contract_mismatch", record["diagnostic"]["issues"])
        for text in (setup_log({"validation_failed_mask": 1}),
                     setup_log({"status_bits": 1, "validation_failed_mask": 1}, outcome="cancelled"),
                     setup_log({"status_bits": 0xffffffff, "errno": 1, "validation_failed_mask": 1}, outcome="failed")):
            self.assertIn("setup_contract_mismatch", self.collect(text, manifest("application-setup"))["diagnostic"]["issues"])

    def test_setup_validation_field_requires_schema_order_and_exact_numeric_marker(self):
        text = setup_log()
        line = "application_setup_validation_failed_mask=0x00000000\n"
        variants = [text.replace(line, ""), text.replace(line, line + line),
                    text.replace(line, "application_setup_validation_failed_mask=0\n"),
                    text.replace(line, "").replace("event=application_setup_imports_closed", line + "event=application_setup_imports_closed"),
                    text.replace("schema=2", "schema=3", 1)]
        for changed in variants:
            with self.subTest(changed=changed):
                record = self.collect(changed, manifest("application-setup"))
                self.assertNotEqual(record["classification"], "complete_success")
        # Values outside the fixed whitelist remain absent from durable JSON.
        redacted = text.replace(line, line + "application_setup_uid=0x12345678\napplication_setup_stat_bytes=private-stat\n")
        record = self.collect(redacted, manifest("application-setup"))
        self.assertEqual(record["classification"], "complete_success")
        self.assertNotIn("12345678", json.dumps(record))
        self.assertNotIn("private-stat", json.dumps(record))

    def test_setup_raw_failure_retains_signed_native_patterns_and_creation(self):
        record = self.collect(setup_log({"stage": 4, "status_bits": 0xffffffff, "errno": 5,
            "raw_bits": 0xffffffff, "created_mask": 1, "opened_mask": 7,
            "verified_mask": 3, "closed_mask": 7, "operations": 9}, outcome="failed", status=1), manifest("application-setup"))
        self.assertEqual(record["classification"], "complete_failure")
        self.assertEqual(record["diagnostic"]["numeric"]["raw_bits"], [0xffffffff])
        self.assertEqual(record["diagnostic"]["numeric"]["created_mask"], [1])
        self.assertEqual(record["diagnostic"]["descriptor_cleanup"], "reported_complete")

    def test_setup_partial_clean_cancel_or_init_cancel_never_pass(self):
        for text in [setup_log({"stage": 3, "status_bits": 1, "verified_mask": 1,
                               "opened_mask": 3, "closed_mask": 3, "operations": 5}, outcome="cancelled"),
                     setup_log({"status_bits": 1}, outcome="cancelled"),
                     setup_log(outcome="cancelled", init_status=1)]:
            record = self.collect(text, manifest("application-setup"))
            self.assertEqual(record["classification"], "incomplete_evidence")
            self.assertFalse(record["diagnostic"]["failure"])
        record = self.collect(setup_log(outcome="failed", init_status=0xffffffff, status=1), manifest("application-setup"))
        self.assertEqual(record["classification"], "complete_failure")
        self.assertFalse(record["diagnostic"]["setup_entered"])
        self.assertEqual(record["diagnostic"]["imports_cleanup"], "Unknown")

    def test_setup_init_cancel_preserves_unclassified_nonzero_lookup_as_incomplete(self):
        text = setup_log(outcome="cancelled", init_status=1)
        text = text.replace("application_import_resolved=0x00000003", "application_import_resolved=0x00000004")
        text = text.replace("application_import_lookup_hi=0x00000000", "application_import_lookup_hi=0x00000001")
        text = text.replace("application_import_lookup_lo=0x00000000", "application_import_lookup_lo=0x00000007")
        record = self.collect(text, manifest("application-setup"))
        self.assertEqual(record["classification"], "incomplete_evidence")
        report = record["diagnostic"]
        self.assertFalse(report["failure"])
        self.assertFalse(report["setup_entered"])
        self.assertEqual(report["imports_cleanup"], "Unknown")
        self.assertEqual(report["imports"][0]["stage"], 2)
        self.assertEqual(report["imports"][0]["status_bits"], 1)
        self.assertEqual(report["imports"][0]["resolved"], 4)
        self.assertEqual((report["imports"][0]["lookup_hi"] << 32) | report["imports"][0]["lookup_lo"], 0x100000007)
        # Native observation survives cancellation; independent cleanup/explicit
        # failure evidence still makes the completed outer lifecycle a failure.
        for changed in [text.replace("application_import_cleanup_flags=0x00000000", "application_import_cleanup_flags=0x00000001"),
                        text.replace("event=application_setup_cancelled", "event=application_setup_failed")]:
            self.assertEqual(self.collect(changed, manifest("application-setup"))["classification"], "complete_failure")

    def test_setup_required_fields_and_cleanup_marker_must_occur_once(self):
        text = setup_log()
        for key in result.SETUP_KEYS:
            line = next(line for line in text.splitlines(keepends=True) if line.startswith("application_setup_" + key + "="))
            for changed in [text.replace(line, ""), text.replace(line, line + line)]:
                with self.subTest(key=key):
                    self.assertNotEqual(self.collect(changed, manifest("application-setup"))["classification"], "complete_success")
        for event in [result.SETUP_CONFIG, "event=application_setup_imports_closed", "event=application_setup_complete"]:
            for changed in [text.replace(event + "\n", ""), text.replace(event + "\n", event + "\n" + event + "\n")]:
                self.assertNotEqual(self.collect(changed, manifest("application-setup"))["classification"], "complete_success")
        malformed = text.replace("application_setup_operations=0x0000000d", "application_setup_operations=13")
        self.assertNotEqual(self.collect(malformed, manifest("application-setup"))["classification"], "complete_success")

    def test_setup_import_groups_no_entropy_and_close_uncertainty(self):
        text = setup_log()
        for before, after in [("application_import_resolved=0x00000017", "application_import_resolved=0x00000016"),
                              ("application_import_stage=0x00000008", "application_import_stage=0x00000006"),
                              ("application_import_lookup_lo=0x00000000", "application_import_lookup_lo=0x00000001"),
                              ("application_import_return_lo=0x00000000", "application_import_return_lo=0xffffffff"),
                              ("application_import_errno=0x00000000", "application_import_errno=0x00000005"),
                              ("application_import_cleanup_flags=0x00000000", "application_import_cleanup_flags=0x00000001")]:
            with self.subTest(after=after):
                self.assertEqual(self.collect(text.replace(before, after, 1), manifest("application-setup"))["classification"], "complete_failure")
        shutdown = "\n".join(imports(8)) + "\n"
        partial = text.replace(shutdown, "").replace("event=application_setup_imports_closed\n", "")
        self.assertEqual(self.collect(partial, manifest("application-setup"))["classification"], "incomplete_evidence")
        badclose = setup_log({"status_bits": 0xffffffff, "cleanup_failed_mask": 8,
                             "cleanup_errno": 4, "cleanup_raw_bits": 0xffffffff}, outcome="failed")
        record = self.collect(badclose, manifest("application-setup"))
        self.assertEqual(record["classification"], "complete_failure")
        self.assertEqual(record["diagnostic"]["cleanup"], "failed")
        with self.assertRaises(ValueError):
            self.collect(text.replace("event=application_setup_imports_closed", "\n".join(imports(6)) + "\nevent=application_setup_imports_closed"), manifest("application-setup"))

    def test_setup_marker_order_configuration_and_interrupted_lifecycle(self):
        text = setup_log()
        line = "application_setup_operations=0x0000000d\n"
        reordered = [text.replace(line, "").replace(result.SETUP_CONFIG, line.rstrip() + "\n" + result.SETUP_CONFIG),
                     text.replace(line, "").replace("event=application_setup_imports_closed", "event=application_setup_imports_closed\n" + line.rstrip()),
                     text.replace("event=application_setup_imports_closed\n", "").replace("event=application_setup_complete", "event=application_setup_complete\nevent=application_setup_imports_closed"),
                     text.replace(result.SETUP_CONFIG, result.SETUP_CONFIG.replace("0700", "0755"))]
        for changed in reordered:
            self.assertNotEqual(self.collect(changed, manifest("application-setup"))["classification"], "complete_success")
        for changed in [text.rstrip("\n"), text[:text.index("event=run_finish")],
                        text.replace("event=payload_return status=2\n", "")]:
            self.assertEqual(self.collect(changed, manifest("application-setup"))["classification"], "interrupted")
        mixed = manifest("application-setup")
        mixed["application_control"] = True
        with self.assertRaises(ValueError):
            self.collect(text, mixed)

    def test_complete_numeric_evidence_preserves_hashes_and_redacts_arbitrary_input(self):
        secret = "00001111222233334444555566667777"
        text = log().replace("event=sbc_continuous_complete", f"link_key={secret}\nmetadata=do-not-copy\nPCM=do-not-copy\nevent=sbc_continuous_complete")
        record = self.collect(text)
        self.assertEqual(record["classification"], "complete_success")
        self.assertEqual(record["diagnostic"]["cleanup"], "reported_complete")
        self.assertEqual(record["input_sha256"]["log"], result.digest(text.encode()))
        encoded = json.dumps(record)
        self.assertNotIn(secret, encoded)
        self.assertNotIn("do-not-copy", encoded)
        self.assertEqual(record["ps5_persistence"], "Unknown")
        self.assertEqual(record["audio_acceptance"], "Unknown")

    def test_truncation_missing_finish_and_partial_final_line_are_interrupted(self):
        original = log()
        for text in [original[:original.index("event=run_finish")], original.rstrip("\n"),
                     original[:original.index("event=payload_return")], original.replace(f"event=run_finish build_id={BUILD}\n", "")]:
            with self.subTest(text=text[-70:]):
                self.assertEqual(self.collect(text)["classification"], "interrupted")

    def test_duplicate_or_foreign_identity_cannot_be_combined_into_success(self):
        begin = f"event=run_begin build_id={BUILD} candidate=ps5-bt-sbc-continuous\n"
        finish = f"event=run_finish build_id={BUILD}\n"
        for text in [log().replace(begin, begin + begin), log().replace(finish, finish + finish),
                     log().replace("build_id=" + BUILD, "build_id=" + "d" * 64, 1),
                     log().replace("candidate=ps5-bt-sbc-continuous", "candidate=ps5-bt-capture-stream"),
                     log().replace(finish, f"event=run_finish build_id={'d' * 64}\n")]:
            self.assertEqual(self.collect(text)["classification"], "identity_mismatch")

    def test_success_marker_alone_or_as_substring_is_not_a_completed_experiment(self):
        text = "\n".join([f"event=run_begin build_id={BUILD} candidate=ps5-bt-sbc-continuous",
                           "event=sbc_continuous_complete", f"event=run_finish build_id={BUILD}",
                           "event=payload_exit status=2", "event=payload_return status=2", ""])
        self.assertEqual(self.collect(text)["classification"], "incomplete_evidence")
        self.assertEqual(self.collect(log().replace("event=sbc_continuous_complete", "prefix event=sbc_continuous_complete"))["classification"], "incomplete_evidence")

    def test_faked_early_completion_and_reported_errors_cannot_pass(self):
        text = log().replace("event=sbc_continuous_complete\n", "").replace(result.SBC_CONFIG, result.SBC_CONFIG + "\nevent=sbc_continuous_complete")
        self.assertEqual(self.collect(text)["classification"], "incomplete_evidence")
        text = log().replace("event=sbc_continuous_complete", "backend_status_bits=0xfffffff9\nevent=sbc_continuous_complete")
        self.assertEqual(self.collect(text)["classification"], "complete_failure")
        text = log("capture-stream").replace("event=capture_stream_complete", "event=capture_stream_clock_or_deadline_failed\nevent=capture_stream_complete")
        self.assertEqual(self.collect(text, manifest("capture-stream"))["classification"], "complete_failure")

    def test_missing_entry_and_foreign_finish_remain_distinct(self):
        self.assertEqual(self.collect("event=payload_return status=2\n")["classification"], "missing_entry")
        self.assertEqual(self.collect(f"event=run_finish build_id={BUILD}\n")["classification"], "identity_mismatch")

    def test_failure_status_and_wrong_counts_or_digest_fail_even_with_complete_marker(self):
        for text in [log(status=1), log().replace("sbc_continuous_outcome=0x00000000", "sbc_continuous_outcome=0x00000004"),
                     log().replace("sbc_continuous_digest=0x7784d1fb", "sbc_continuous_digest=0x7784d1fa"),
                     log().replace("sbc_continuous_elapsed_us_hi=0x00000000", "sbc_continuous_elapsed_us_hi=0x00000001")]:
            self.assertEqual(self.collect(text)["classification"], "complete_failure")

    def test_cleanup_missing_or_duplicate_required_number_never_proves_success(self):
        cleanup = "sbc_continuous_cleanup_attempted=0x00000001\n"
        for text in [log().replace(cleanup, ""), log().replace(cleanup, cleanup + cleanup)]:
            self.assertEqual(self.collect(text)["classification"], "incomplete_evidence")
        self.assertEqual(self.collect(log().replace("event=sbc_continuous_complete", "event=sbc_continuous_cleanup_failed\nevent=sbc_continuous_failed"))["classification"], "complete_failure")

    def test_capture_empty_is_lifecycle_success_without_a_pcm_claim(self):
        record = self.collect(log("capture-stream"), manifest("capture-stream"))
        self.assertEqual(record["classification"], "complete_success")
        self.assertEqual(record["diagnostic"]["numeric"]["kind"], [3])
        self.assertEqual(record["diagnostic"]["configuration"]["format"], "Unknown")
        self.assertEqual(record["diagnostic"]["configuration"]["pcm_format_verified"], "Unknown")
        failure = log("capture-stream").replace("capture_stream_stop_bits=0x00000000", "capture_stream_stop_bits=0xfffffff9")
        self.assertEqual(self.collect(failure, manifest("capture-stream"))["classification"], "complete_failure")

    def test_capture_kind_raw_length_and_flags_must_match_native_abi(self):
        base = log("capture-stream")
        cases = [(2, 0, 0, 0), (3, 0x81950002, 0, 0),
                 (4, 0x81950004, 0, 1), (5, 0x81950008, 0, 0),
                 (1, 8192, 8192, 0)]
        def capture(kind, raw, length, flag):
            return (base.replace("capture_stream_kind=0x00000003", f"capture_stream_kind=0x{kind:08x}")
                    .replace("capture_stream_read_status_bits=0x81950002", f"capture_stream_read_status_bits=0x{raw:08x}")
                    .replace("capture_stream_bytes=0x00000000", f"capture_stream_bytes=0x{length:08x}")
                    .replace("capture_stream_discontinuity=0x00000000", f"capture_stream_discontinuity=0x{flag:08x}"))
        for args in cases:
            with self.subTest(valid=args):
                self.assertEqual(self.collect(capture(*args), manifest("capture-stream"))["classification"], "complete_success")
        for args in [(3, 0, 0, 0), (4, 0x81950004, 0, 0), (2, 1, 0, 0),
                     (5, 0x81950002, 0, 0), (1, 0, 0, 0), (1, 8193, 8193, 0),
                     (1, 8, 8, 1), (1, 8, 4, 0), (3, 0x81950002, 1, 0),
                     (6, 0x81950002, 0, 0)]:
            with self.subTest(invalid=args):
                self.assertEqual(self.collect(capture(*args), manifest("capture-stream"))["classification"], "complete_failure")
        missing_cleanup = base.replace("capture_stream_close_bits=0x00000000\n", "")
        record = self.collect(missing_cleanup, manifest("capture-stream"))
        self.assertEqual(record["classification"], "incomplete_evidence")
        self.assertEqual(record["diagnostic"]["cleanup"], "Unknown")

    def test_dirty_source_and_explicit_controller_observations_are_not_promoted(self):
        source = manifest()
        source["git_status"] = ["M source.rs"]
        record = self.collect(source=source)
        self.assertEqual(record["classification"], "incomplete_evidence")
        self.assertNotIn("source.rs", json.dumps(record))
        record = self.collect(observations={phase: "Unknown" for phase in NORMAL})
        self.assertEqual(record["controller_acceptance"], "Unknown")
        record = self.collect(observations={**NORMAL, "during": "changed"})
        self.assertEqual(record["classification"], "complete_failure")
        for observations in [{}, {**NORMAL, "during": "assumed_normal"}]:
            with self.assertRaises(ValueError):
                self.collect(observations=observations)

    def test_manifest_variants_legacy_identity_and_duplicate_json_keys_are_rejected(self):
        for source in [{**manifest(), "capture_stream": True}, {**manifest(), "diagnostic_build_id": None},
                       {**manifest(), "sbc_continuous": 1}, {**manifest(), "diagnostic_candidate": "ps5-bt-other"}]:
            with self.assertRaises(ValueError):
                self.collect(source=source)
        self.manifest.write_text('{"sha256": "a", "sha256": "b"}')
        self.log.write_text(log())
        with self.assertRaisesRegex(ValueError, "duplicate"):
            result.collect(self.manifest, self.log, NORMAL)

    def test_byte_line_and_binary_input_bounds_are_enforced(self):
        self.collect()
        for payload in [b"x" * (result.MAX_LOG_BYTES + 1), b"x" * (result.MAX_LINE_BYTES + 1), b"\x00\n", b"\xff\n"]:
            self.log.write_bytes(payload)
            with self.assertRaises(ValueError):
                result.collect(self.manifest, self.log, NORMAL)
        self.log.write_text(log())
        self.manifest.write_bytes(b" " * (result.MAX_MANIFEST_BYTES + 1))
        with self.assertRaises(ValueError):
            result.collect(self.manifest, self.log, NORMAL)

    def test_duplicate_termination_and_fake_embedded_return_are_interrupted(self):
        for text in [log() + "event=payload_return status=2\n", log().replace("event=payload_return", "prefix event=payload_return")]:
            self.assertEqual(self.collect(text)["classification"], "interrupted")

    def test_application_control_complete_profile_records_radio_unavailable(self):
        record = self.collect(application_log(), manifest("application-control"))
        self.assertEqual(record["classification"], "complete_success")
        diagnostic = record["diagnostic"]
        self.assertTrue(diagnostic["runtime_entered"])
        self.assertTrue(diagnostic["configuration_confirmed"])
        self.assertEqual(diagnostic["cleanup"], "reported_complete")
        self.assertEqual(diagnostic["configuration"]["transport"], "Unavailable")
        self.assertEqual(diagnostic["configuration"]["native_format"], "Unknown")
        self.assertEqual([group["stage"] for group in diagnostic["imports"]], [3, 6, 8])
        self.assertEqual(record["audio_acceptance"], "Unknown")

    def test_application_preflight_failure_and_cancellation_never_infer_runtime_entry(self):
        record = self.collect(application_log(preflight="failed", status=1), manifest("application-control"))
        self.assertEqual(record["classification"], "complete_failure")
        self.assertFalse(record["diagnostic"]["runtime_entered"])
        self.assertEqual(record["diagnostic"]["preflight_outcome"], "failed")
        self.assertEqual(record["diagnostic"]["cleanup"], "Unknown")
        for stage in ["cancelled", "entropy_cancelled"]:
            record = self.collect(application_log(preflight=stage), manifest("application-control"))
            self.assertEqual(record["classification"], "incomplete_evidence")
            self.assertFalse(record["diagnostic"]["runtime_entered"])
            self.assertEqual(record["diagnostic"]["preflight_outcome"], "cancelled")
            self.assertEqual(record["diagnostic"]["factory_cleanup"], "Unknown")
        self.assertEqual(record["diagnostic"]["preflight_cleanup"], "reported_complete")

    def test_application_native_factory_failure_and_cleanup_fault_remain_complete_failures(self):
        for fields in [{"result": 1, "stop_reason": 3, "failed_resource": 1, "error_bits": 0xfffffffb},
                       {"cleanup_uncertain": 1}, {"cleanup_failed_mask": 2}, {"active_mask": 1},
                       {"forced": 1}, {"finished": 0}, {"stop_error_bits": 0xfffffffd}]:
            with self.subTest(fields=fields):
                record = self.collect(application_log(fields), manifest("application-control"))
                self.assertEqual(record["classification"], "complete_failure")
        text = application_log().replace("application_import_cleanup_flags=0x00000000", "application_import_cleanup_flags=0x00000001", 1)
        self.assertEqual(self.collect(text, manifest("application-control"))["classification"], "complete_failure")

    def test_application_gates_config_counts_and_budget_are_checked(self):
        for fields in [{"max_turns": 20001}, {"duration_us": 1}, {"turns": 20001},
                       {"waits": 10001}, {"clients": 65}, {"client_failures": 3},
                       {"clock_calls": 1001}, {"clock_calls": 0}, {"drive_failure": 4}, {"stop_reason": 8}]:
            with self.subTest(fields=fields):
                self.assertEqual(self.collect(application_log(fields), manifest("application-control"))["classification"], "complete_failure")
        text = application_log().replace("transport=Unavailable capture=Unavailable native_format=Unknown codec=Unavailable", "transport=Ready capture=Unavailable native_format=Unknown codec=Unavailable")
        self.assertEqual(self.collect(text, manifest("application-control"))["classification"], "complete_failure")

    def test_application_missing_cleanup_duplicate_markers_and_partial_logs_do_not_pass(self):
        original = application_log()
        for text in [original.replace("application_cleanup_mask=0x00000007\n", ""),
                     original.replace("event=application_preflight_cleanup_complete\n", ""),
                     original.replace("event=application_complete", "event=application_complete\nevent=application_complete"),
                     original.replace(result.APPLICATION_CONFIG, result.APPLICATION_CONFIG + "\n" + result.APPLICATION_CONFIG),
                     original.replace("application_turns=0x000003e8", "application_turns=0x000003e8\napplication_turns=0x000003e8")]:
            self.assertNotEqual(self.collect(text, manifest("application-control"))["classification"], "complete_success")
        text = original[:original.index("event=application_complete")]
        self.assertEqual(self.collect(text, manifest("application-control"))["classification"], "interrupted")

    def test_application_preflight_group_integrity_order_and_redaction(self):
        original = application_log()
        for text in [original.replace("application_import_errno=0x00000000\n", "", 1),
                     original.replace("application_import_resolved=0x00000017", "application_import_resolved=0x00000016"),
                     original.replace("application_import_errno=0x00000000", "application_import_errno=0x00000001", 1),
                     original.replace("event=application_complete\n", "").replace(result.APPLICATION_CONFIG, result.APPLICATION_CONFIG + "\nevent=application_complete")]:
            self.assertNotEqual(self.collect(text, manifest("application-control"))["classification"], "complete_success")
        secret = "00001111222233334444555566667777"
        text = original.replace("event=application_complete", f"application_token={secret}\napplication_path=/private/operator/path\napplication_address=192.168.1.9\nevent=application_complete")
        record = self.collect(text, manifest("application-control"))
        self.assertEqual(record["classification"], "complete_success")
        encoded = json.dumps(record)
        for text in [secret, "/private/operator/path", "192.168.1.9"]:
            self.assertNotIn(text, encoded)

    def test_application_http_cleanup_groups_are_explicit_and_fail_closed(self):
        def with_http(values):
            lines = "".join(f"application_http_{key}=0x{value:08x}\n" for key, value in values.items())
            return application_log().replace("event=application_complete", lines + "event=application_complete")
        clean = {"failure_source": 1, "failure_cause": 4, "failure_raw_bits": 0,
                 "cleanup_present": 0, "cleanup_bits": 0}
        record = self.collect(with_http(clean), manifest("application-control"))
        self.assertEqual(record["classification"], "complete_success") # recoverable client timeout
        self.assertEqual(record["diagnostic"]["http"], [clean])
        for group in [{**clean, "cleanup_present": 1, "cleanup_bits": 35},
                      {**clean, "cleanup_bits": 35}, {**clean, "cleanup_present": 2},
                      {**clean, "failure_source": 4}, {**clean, "failure_source": 2},
                      {**clean, "failure_cause": 9}, {**clean, "failure_cause": 99},
                      {key: value for key, value in clean.items() if key != "cleanup_bits"}]:
            self.assertEqual(self.collect(with_http(group), manifest("application-control"))["classification"], "complete_failure")
        malformed = with_http(clean).replace("event=application_complete", "application_http_cleanup_bits=invalid\nevent=application_complete")
        self.assertEqual(self.collect(malformed, manifest("application-control"))["classification"], "complete_failure")
        duplicate = with_http(clean).replace("event=application_complete", "".join(f"application_http_{key}=0x{value:08x}\n" for key, value in clean.items()) + "event=application_complete")
        self.assertEqual(self.collect(duplicate, manifest("application-control"))["classification"], "complete_failure")

    def test_application_entropy_native_return_is_bounded_and_retained(self):
        original = application_log()
        for value in [0, 17, 0xffffffff]:
            text = original.replace("application_import_return_lo=0x00000010", f"application_import_return_lo=0x{value:08x}")
            record = self.collect(text, manifest("application-control"))
            self.assertEqual(record["classification"], "complete_failure")
            self.assertEqual(record["diagnostic"]["imports"][1]["return_lo"], value)

    def test_application_clean_early_stop_is_incomplete_without_full_runtime_acceptance(self):
        fields = {"result": 0, "drive_entered": 0, "factory_entered": 0}
        text = application_log()
        start = text.index("application_result=")
        end = text.index("event=application_complete")
        text = text[:start] + "".join(f"application_{key}=0x{value:08x}\n" for key, value in fields.items()) + text[end:]
        record = self.collect(text, manifest("application-control"))
        self.assertEqual(record["classification"], "incomplete_evidence")
        self.assertTrue(record["diagnostic"]["runtime_entered"])
        self.assertEqual(record["diagnostic"]["factory_cleanup"], "Unknown")

    def test_application_generated_requires_full_real_clock_workload(self):
        record = self.collect(application_mode_log("generated"), manifest("application-generated"))
        self.assertEqual(record["classification"], "complete_success")
        diagnostic = record["diagnostic"]
        self.assertEqual(diagnostic["configuration"]["mode"], "generated-sbc")
        self.assertEqual(diagnostic["configuration"]["capture"], "Unavailable")
        self.assertEqual(diagnostic["workload"]["configuration"]["nominal_clock"], "host-monotonic")
        self.assertTrue(diagnostic["workload"]["complete"])
        self.assertEqual(record["audio_acceptance"], "Unknown")
        for values in [{"blocks": 749}, {"discarded_blocks": 749}, {"bytes": 57749},
                       {"prefix": 0x9cfd20ca}, {"digest": 0x7784d1fa}, {"summary_matches": 0},
                       {"summary_matches": 1}, {"operations": 2249}, {"operations": 60001},
                       {"elapsed_lo": 1_997_333}, {"elapsed_lo": 2_000_000}, {"elapsed_hi": 1},
                       {"codec_bits": 0xffffffff}, {"concealed": 1}, {"rebuffer": 1},
                       {"expired": 1}, {"stopped_frames": 1}, {"stopped_blocks": 1},
                       {"error": 2}, {"phase": 3}, {"phase": 9},
                       {"completion_observed": 2}, {"operations": 2250}]:
            with self.subTest(values=values):
                self.assertEqual(self.collect(application_mode_log("generated", workload=values),
                                              manifest("application-generated"))["classification"], "complete_failure")

    def test_application_generated_clean_partial_or_stopped_workload_is_incomplete(self):
        partial = {"phase": 2, "blocks": 10, "discarded_blocks": 9, "bytes": 9 * 77,
                   "operations": 40, "summary_matches": 0, "elapsed_lo": 100000,
                   "stopped_frames": 128, "stopped_blocks": 1, "completion_observed": 0}
        record = self.collect(application_mode_log("generated", workload=partial), manifest("application-generated"))
        self.assertEqual(record["classification"], "incomplete_evidence")
        self.assertFalse(record["diagnostic"]["workload"]["complete"])
        self.assertIn("generated_workload_not_complete", record["issues"])
        record = self.collect(application_mode_log("generated", workload={**partial, "phase": 0}), manifest("application-generated"))
        self.assertEqual(record["classification"], "incomplete_evidence")

    def test_application_generated_complete_frames_without_post_discard_clock_stay_incomplete(self):
        text = application_mode_log("generated", workload={"completion_observed": 0})
        record = self.collect(text, manifest("application-generated"))
        self.assertEqual(record["classification"], "incomplete_evidence")
        self.assertFalse(record["diagnostic"]["workload"]["complete"])
        self.assertIn("generated_completion_not_observed", record["issues"])

    def test_application_observe_empty_and_nonempty_remain_raw_unknown_pcm(self):
        record = self.collect(application_mode_log("observe"), manifest("application-observe"))
        self.assertEqual(record["classification"], "complete_success")
        configuration = record["diagnostic"]["workload"]["configuration"]
        self.assertFalse(configuration["nonempty_bytes_observed"])
        for key in ("pcm_format_verified", "pcm_success", "sample_frames", "gap_sample_frames", "native_time_units"):
            self.assertEqual(configuration[key], "Unknown")
        self.assertTrue(configuration["bounded_raw_observation_completed"])
        self.assertEqual(record["diagnostic"]["configuration"]["capture"], "raw-observe")
        self.assertEqual(record["diagnostic"]["configuration"]["codec"], "Unavailable")
        values = {"data": 1, "empty": 188, "overruns": 1, "gaps": 1, "bytes_lo": 7,
                  "max_bytes": 7, "last_kind": 1, "last_raw_bits": 7}
        record = self.collect(application_mode_log("observe", workload=values), manifest("application-observe"))
        self.assertEqual(record["classification"], "complete_success")
        self.assertTrue(record["diagnostic"]["workload"]["configuration"]["nonempty_bytes_observed"])
        self.assertEqual(record["audio_acceptance"], "Unknown")
        # Native scheduling overshoot is retained rather than rounded to a guessed1ms.
        record = self.collect(application_mode_log("observe", workload={"elapsed_lo": 2_050_000}), manifest("application-observe"))
        self.assertEqual(record["classification"], "complete_success")
        self.assertEqual(record["diagnostic"]["workload"]["configuration"]["deadline_overshoot_us"], 50_000)

    def test_application_observe_zero_overrun_and_unaligned_data_are_raw_lifecycle_results(self):
        for values in [{"empty": 0, "zero": 190, "last_kind": 2, "last_raw_bits": 0},
                       {"empty": 0, "overruns": 190, "gaps": 190, "last_kind": 4,
                        "last_raw_bits": 0x81950004, "last_discontinuity": 1},
                       {"empty": 0, "data": 190, "bytes_lo": 190 * 7, "max_bytes": 7,
                        "last_kind": 1, "last_raw_bits": 7}]:
            with self.subTest(values=values):
                record = self.collect(application_mode_log("observe", workload=values), manifest("application-observe"))
                self.assertEqual(record["classification"], "complete_success")
                self.assertEqual(record["diagnostic"]["workload"]["configuration"]["sample_frames"], "Unknown")
                self.assertEqual(record["audio_acceptance"], "Unknown")

    def test_application_observe_counts_raw_status_time_and_config_are_coherent(self):
        cases = [{"reads": 191, "empty": 191}, {"reads": 257}, {"data": 1}, {"bytes_lo": 1},
                 {"max_bytes": 1}, {"bytes_hi": 1}, {"overruns": 1}, {"gaps": 1},
                 {"last_kind": 3, "last_raw_bits": 0}, {"last_discontinuity": 1},
                 {"last_kind": 9}, {"native_time_present": 1}, {"native_value_lo": 1},
                 {"native_domain": 1}, {"host_lo": 1_000_001}, {"host_lo": 1_100_001}, {"host_lo": 3_000_001},
                 {"elapsed_lo": 1_999_999}, {"elapsed_lo": 5_000_001}, {"elapsed_hi": 1},
                 {"reason": 3}, {"reason": 5}, {"error": 7}, {"owner_failures": 1},
                 {"native_errors": 1}, {"duration_us": 1}, {"max_reads": 257},
                 {"interval_us": 1}, {"first_settle_us": 0},
                 {"started_hi": 0xffffffff, "started_lo": 0xffffffff}]
        for values in cases:
            with self.subTest(values=values):
                record = self.collect(application_mode_log("observe", workload=values), manifest("application-observe"))
                self.assertEqual(record["classification"], "complete_failure")
        # Last Data must be consistent with all aggregate byte/max counters.
        for values in [{"data": 1, "empty": 189, "bytes_lo": 7, "max_bytes": 8,
                        "last_kind": 1, "last_raw_bits": 7},
                       {"data": 2, "empty": 188, "bytes_lo": 16, "max_bytes": 8,
                        "last_kind": 1, "last_raw_bits": 1}]:
            self.assertEqual(self.collect(application_mode_log("observe", workload=values), manifest("application-observe"))["classification"], "complete_failure")

    def test_application_observe_uses_actual_count_below_derived_continuous_ceiling(self):
        for count in [1, 20, 57, 189, 190]:
            values = {"reads": count, "empty": count,
                      "host_lo": 1_000_000 + 100_000 + (count - 1) * 10_000}
            record = self.collect(application_mode_log("observe", workload=values), manifest("application-observe"))
            self.assertEqual(record["classification"], "complete_success")
            self.assertEqual(record["diagnostic"]["numeric"]["observe_reads"], [count])
            self.assertEqual(record["diagnostic"]["workload"]["configuration"]["cadence_read_ceiling"], 190)
        # Preserve the old helper/diagnostic profile elsewhere, but do not accept
        # its 100 ms cadence or 20-read cap as this independently selected artifact.
        for changed in [{"interval_us": 100_000}, {"max_reads": 20},
                        {"reads": 191, "empty": 191}, {"max_reads": 0}]:
            self.assertEqual(self.collect(application_mode_log("observe", workload=changed), manifest("application-observe"))["classification"], "complete_failure")

    def test_application_observe_cancelled_ended_and_owner_failures_do_not_prove_sustained_run(self):
        zero = {"reason": 1, "reads": 0, "empty": 0, "last_kind": 0, "last_raw_bits": 0,
                "host_lo": 0, "elapsed_lo": 0}
        ended = {"reason": 4, "reads": 1, "empty": 0, "ended": 1, "last_kind": 5,
                 "last_raw_bits": 0x81950008, "host_lo": 1_100_000, "elapsed_lo": 100_000}
        for values in [zero, ended]:
            record = self.collect(application_mode_log("observe", workload=values), manifest("application-observe"))
            self.assertEqual(record["classification"], "incomplete_evidence")
            self.assertFalse(record["diagnostic"]["workload"]["complete"])
        failure = {**zero, "reason": 5, "error": 6, "reads": 1, "owner_failures": 1}
        record = self.collect(application_mode_log("observe", workload=failure), manifest("application-observe"))
        self.assertEqual(record["classification"], "complete_failure")
        native = {"reason": 5, "error": 7, "reads": 1, "empty": 0, "native_errors": 1,
                  "last_kind": 6, "last_raw_bits": 0xffffff9d, "host_lo": 1_100_000,
                  "elapsed_lo": 100_000}
        self.assertEqual(self.collect(application_mode_log("observe", workload=native), manifest("application-observe"))["classification"], "complete_failure")

    def test_application_workload_missing_duplicate_and_wrong_profile_evidence_never_passes(self):
        for mode, keys in [("generated", result.GENERATED_KEYS), ("observe", result.OBSERVE_KEYS)]:
            original = application_mode_log(mode)
            for key in keys:
                name = f"application_{mode}_{key}="
                marker = next(line for line in original.splitlines(keepends=True) if line.startswith(name))
                for changed in [original.replace(marker, ""), original.replace(marker, marker + marker),
                                original.replace(marker, name + "invalid\n")]:
                    with self.subTest(mode=mode, key=key):
                        self.assertNotEqual(self.collect(changed, manifest("application-" + mode))["classification"], "complete_success")
            for changed in [original.replace("application_required_mask=", "application_ignored_mask=", 1),
                            original.replace("event=application_complete\n", "").replace(
                                result.GENERATED_CONFIG if mode == "generated" else result.OBSERVE_CONFIG,
                                (result.GENERATED_CONFIG if mode == "generated" else result.OBSERVE_CONFIG) + "\nevent=application_complete")]:
                self.assertNotEqual(self.collect(changed, manifest("application-" + mode))["classification"], "complete_success")
            for field in ["required_mask", "cleanup_mask"]:
                marker = next(line for line in original.splitlines(keepends=True) if line.startswith(f"application_{field}="))
                changed = original.replace(marker, f"application_{field}=0x00000007\n")
                self.assertEqual(self.collect(changed, manifest("application-" + mode))["classification"], "complete_failure")
            wrong = original.replace(result.GENERATED_CONFIG if mode == "generated" else result.OBSERVE_CONFIG,
                                     result.APPLICATION_CONFIG)
            self.assertEqual(self.collect(wrong, manifest("application-" + mode))["classification"], "complete_failure")

    def test_new_application_variants_keep_manifest_exclusivity_preflight_and_redaction(self):
        for mode in ("generated", "observe"):
            original = application_mode_log(mode)
            private = "private-pcm-and-native-metadata-0011223344556677"
            changed = original.replace("event=application_complete", f"application_{mode}_raw_pcm={private}\napplication_{mode}_opaque_metadata={private}\napplication_token={private}\nevent=application_complete")
            record = self.collect(changed, manifest("application-" + mode))
            self.assertEqual(record["classification"], "complete_success")
            self.assertNotIn(private, json.dumps(record))
            source = {**manifest("application-" + mode), "application_control": True}
            with self.assertRaises(ValueError):
                self.collect(original, source)
            cancelled = application_mode_log(mode, preflight="cancelled")
            self.assertEqual(self.collect(cancelled, manifest("application-" + mode))["classification"], "incomplete_evidence")
            failed = application_mode_log(mode, preflight="failed", status=1)
            self.assertEqual(self.collect(failed, manifest("application-" + mode))["classification"], "complete_failure")
            self.assertEqual(self.collect(original.replace("transport=Unavailable", "transport=Ready"), manifest("application-" + mode))["classification"], "complete_failure")

    def test_atomic_write_is_private_and_refuses_to_overwrite(self):
        record = self.collect()
        result.write_atomic(self.output, record)
        self.assertEqual(json.loads(self.output.read_text()), record)
        self.assertEqual(self.output.stat().st_mode & 0o777, 0o600)
        with self.assertRaises(FileExistsError):
            result.write_atomic(self.output, {"changed": True})
        self.assertEqual(json.loads(self.output.read_text()), record)
        self.assertEqual(list(self.root.glob(".ps5-bt-result-*")), [])

    def test_file_and_directory_fsync_failures_are_visible_without_silent_success(self):
        record = self.collect()
        with patch.object(os, "fsync", side_effect=OSError("file fsync failed")):
            with self.assertRaisesRegex(OSError, "file fsync"):
                result.write_atomic(self.output, record)
        self.assertFalse(self.output.exists())
        with patch.object(os, "fsync", side_effect=[None, OSError("directory fsync failed")]):
            with self.assertRaisesRegex(OSError, "directory fsync"):
                result.write_atomic(self.output, record)
        self.assertTrue(self.output.exists()) # publication happened; caller receives durability failure
        self.assertEqual(list(self.root.glob(".ps5-bt-result-*")), [])

    def test_cli_persists_failure_evidence_and_returns_distinct_status(self):
        self.collect(log(status=1))
        args = ["--manifest", str(self.manifest), "--log", str(self.log), "--output", str(self.output)]
        for phase in NORMAL:
            args.extend([f"--controller-{phase}", "Unknown"])
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(result.main(args), 2)
        self.assertEqual(json.loads(self.output.read_text())["classification"], "complete_failure")
        with contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(result.main(args), 1)


if __name__ == "__main__":
    unittest.main()
