#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Compile/run synthetic local POSIX file-store conformance, never PS5 or real bonds."""
import argparse
import os
from pathlib import Path
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--cc", default=os.environ.get("CC", "cc"))
    parser.add_argument("--sanitize", action="store_true")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    with tempfile.TemporaryDirectory(prefix="ps5-bt-store-test-") as directory:
        executable = Path(directory) / "conformance"
        command = [args.cc, "-std=c11", "-Wall", "-Wextra", "-Werror", "-Wpedantic", "-g"]
        if args.sanitize:
            command += ["-fsanitize=address,undefined", "-fno-omit-frame-pointer"]
        command += [str(root / "tests/store/conformance.c"), "-o", str(executable)]
        subprocess.run(command, check=True, timeout=60)
        subprocess.run([str(executable)], check=True, timeout=30)


if __name__ == "__main__":
    main()
