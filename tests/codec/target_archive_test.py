#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Reject architecture and runtime seams before target Rust links an archive."""
from pathlib import Path
import struct
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tools"))
import target_sbc


def archive(path, machine=62):
    body = bytearray(64)
    body[:7] = b"\x7fELF\x02\x01\x01"
    struct.pack_into("<HH", body, 16, 1, machine)
    header = b"codec.o/".ljust(16) + b"0".ljust(12) + b"0".ljust(6) + b"0".ljust(6) + b"644".ljust(8) + b"64".ljust(10) + b"`\n"
    path.write_bytes(b"!<arch>\n" + header + body)


class TargetArchiveTest(unittest.TestCase):
    def verify(self, path, undefined=None, defined=None):
        if undefined is None:
            undefined = target_sbc.EXPECTED_EXTERNAL
        if defined is None:
            defined = target_sbc.REQUIRED_WRAPPER
        with patch.object(target_sbc, "_require_llvm18", return_value=Path("llvm-nm")), patch.object(target_sbc, "_symbols", side_effect=[undefined, defined]):
            return target_sbc.verify_archive(path, "llvm-nm")

    def test_valid_seam_preserves_hash_and_imports(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "libps5_bt_sbc.a"
            archive(path)
            report = self.verify(path)
            self.assertEqual(report["archive_sha256"], target_sbc._digest(path))
            self.assertEqual(report["external_symbols"], ["memcpy", "memset"])

    def test_wrong_machine_and_non_archive_fail_before_symbols(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "libps5_bt_sbc.a"
            archive(path, machine=183)
            with self.assertRaisesRegex(ValueError, "x86-64"):
                self.verify(path)
            path.write_bytes(b"!<thin>\n")
            with self.assertRaisesRegex(ValueError, "ordinary ar archive"):
                self.verify(path)

    def test_missing_export_and_allocator_import_fail(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "libps5_bt_sbc.a"
            archive(path)
            with self.assertRaisesRegex(ValueError, "missing SBC wrapper exports"):
                self.verify(path, defined=target_sbc.REQUIRED_WRAPPER - {"ps5_bt_sbc_finish"})
            with self.assertRaisesRegex(ValueError, "unexpected target SBC external imports"):
                self.verify(path, undefined=target_sbc.EXPECTED_EXTERNAL | {"malloc"})


if __name__ == "__main__":
    unittest.main()
