#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Execute the x86-64 error/argument adapter with a fake bridge, never syscalls."""
import os
import platform
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parent.parent
flags = []
if platform.system() == "Darwin":
    flags = ["-arch", "x86_64"]
elif platform.machine() not in ("x86_64", "AMD64"):
    print("SKIP x86-64 assembly execution: host architecture is unsupported")
    raise SystemExit(0)
with tempfile.TemporaryDirectory(prefix="ps5-bt-syscall-") as directory:
    binary = Path(directory) / "test"
    subprocess.run([os.environ.get("HOST_CC", "cc"), *flags,
                    "-Wall", "-Wextra", "-Werror",
                    "tests/platform/syscall_checked_test.c",
                    "tests/platform/syscall_fake.S",
                    "platform/ps5/bootstrap/syscall_checked.S", "-o", str(binary)],
                   cwd=root, check=True)
    subprocess.run([str(binary)], check=True)
print("PASS x86-64 carry/error/MAP_FAILED, six arguments and stack alignment (synthetic)")
