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
