#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Exercise the real Rust/C boundary using a stub and a HOST-ONLY fake adapter."""
import os
from pathlib import Path
import shlex
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
BUILD = ROOT / "build" / "host"


def run(args, **kwargs):
    return subprocess.run(args, cwd=ROOT, check=True, **kwargs)


def main():
    BUILD.mkdir(parents=True, exist_ok=True)
    host = next(
        line.removeprefix("host: ")
        for line in subprocess.check_output(["rustc", "-vV"], text=True).splitlines()
        if line.startswith("host: ")
    )
    run(["cargo", "build", "--locked", "-p", "ps5-runtime", "--target", host])
    archive = ROOT / "target" / host / "debug" / "libps5_runtime.a"
    cc = shlex.split(os.environ.get("HOST_CC", "cc"))
    flags = ["-std=c11", "-Wall", "-Wextra", "-Werror", "-Iplatform/ps5/include"]
    libs = ["-lpthread", "-ldl", "-lm"] if sys.platform.startswith("linux") else []
    run(cc + flags + ["tests/abi/stub_contract.c", "platform/ps5/adapter_stub.c",
                     "-o", str(BUILD / "stub-contract")])
    run([str(BUILD / "stub-contract")])
    for name, adapter in [("stub", "platform/ps5/adapter_stub.c"),
                          ("synthetic", "tests/abi/fake_adapter.c")]:
        run(cc + flags + ["platform/ps5/main.c", "platform/ps5/log.c", adapter,
                         str(archive), "-o", str(BUILD / name)] + libs)

    cases = [("stub", "", 2), ("synthetic", "success", 0)]
    cases += [("synthetic", mode, 1) for mode in [
        "abi_mismatch", "backend_status", "zero_token", "send_failure", "timeout",
        "dirty_timeout", "receive_failure", "oversized_length", "unknown_kind",
        "truncated", "foreign_opcode", "controller_failure", "shutdown_failure",
    ]]
    for binary, mode, expected in cases:
        env = dict(os.environ, PS5_BT_TEST_MODE=mode)
        result = subprocess.run([str(BUILD / binary)], env=env, capture_output=True, text=True)
        assert result.returncode == expected, (binary, mode, result)
        assert "event=rust_entry" in result.stderr, (binary, mode, result.stderr)
        if binary == "stub":
            assert "status=unsupported" in result.stderr
        elif mode not in ["abi_mismatch", "backend_status", "zero_token"]:
            assert result.stderr.count("test=synthetic_shutdown") == 1, (mode, result.stderr)
        if mode == "success":
            for field in ["hci_revision=0x00001234", "manufacturer=0x0000abcd", "lmp_subversion=0x00005678"]:
                assert field in result.stderr, result.stderr
            assert "event=probe_complete" in result.stderr
        if mode == "backend_status":
            assert "backend_status_bits=0x0000004d" in result.stderr
        if mode == "controller_failure":
            assert "hci_status=0x0000000c" in result.stderr
        print(f"PASS {binary}/{mode or 'unsupported'} exit={expected}")
    print("Host-only ABI checks passed; no PS5 hardware was accessed.")


if __name__ == "__main__":
    main()
