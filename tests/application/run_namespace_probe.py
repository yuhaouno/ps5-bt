#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Strict synthetic root/data checks; no console or host namespace access."""
import argparse
import os
from pathlib import Path
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cc", default=os.environ.get("CC", "cc"))
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    with tempfile.TemporaryDirectory(prefix="ps5-bt-namespace-test-") as directory:
        for stem in ("namespace_probe_test", "namespace_main_test"):
            for sanitize in (False, True):
                executable = Path(directory) / (stem + ("-sanitized" if sanitize else ""))
                command = [args.cc, "-std=c11", "-Wall", "-Wextra", "-Werror",
                           "-pedantic", "-g", "-I" + str(root / "platform/ps5/include")]
                if sanitize:
                    command += ["-fsanitize=address,undefined", "-fno-omit-frame-pointer"]
                command += [str(root / "tests/application" / (stem + ".c")),
                            "-o", str(executable)]
                subprocess.run(command, check=True, timeout=60)
                environment = os.environ.copy()
                if sanitize:
                    environment["ASAN_OPTIONS"] = "halt_on_error=1"
                    environment["UBSAN_OPTIONS"] = "halt_on_error=1"
                subprocess.run([str(executable)], check=True, timeout=30, env=environment)


if __name__ == "__main__":
    main()
