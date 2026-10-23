#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Build the reviewed minimal runtime; never deploy or select a stock CRT."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import urllib.request

from inspect_elf import inspect_bytes

ROOT = Path(__file__).resolve().parent.parent
SDK_REVISION = "a7685ce8e97ce1f8a4f38253a41d54241c9124f1"
SDK_FILES = {
    "crt/syscall.c": "7c2f8120c8e5059d387aaed0a144258c140a5ba5360f6f23da2a3442470e24b2",
    "crt/syscall.h": "463fc16da04c2b7b98a41cbf71e0901faa32a4a9a19421a52bc27dfb52186577",
    "crt/payload.h": "e4803d043c38fbb60f2d027501d572db9b81a14af0b09b460129cbc84ecb617d",
}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(command, capture=False):
    print("+ " + " ".join(map(str, command)), flush=True)
    return subprocess.run(list(map(str, command)), cwd=ROOT, check=True,
                          text=True, stdout=subprocess.PIPE if capture else None).stdout


def sdk_sources(source=None):
    """A fixed subset, not an SDK installation or unverified moving checkout."""
    directory = Path(source).resolve() if source else ROOT / ".sdk" / SDK_REVISION
    for name, expected in SDK_FILES.items():
        path = directory / name
        if not path.exists() and not source:
            url = f"https://raw.githubusercontent.com/ps5-payload-dev/sdk/{SDK_REVISION}/{name}"
            print(f"Fetching pinned SDK source: {url}", flush=True)
            with urllib.request.urlopen(url, timeout=30) as response:
                content = response.read(1024 * 1024)
            if hashlib.sha256(content).hexdigest() != expected:
                raise ValueError(f"SDK download hash mismatch: {name}")
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(content)
        if not path.is_file() or digest(path) != expected:
            raise ValueError(f"SDK source missing or hash mismatch: {path}")
    return directory


def llvm_tools(config):
    path = shutil.which(config)
    if not path:
        raise ValueError("LLVM_CONFIG must identify LLVM 18.1.8 llvm-config")
    version = run([path, "--version"], capture=True).strip()
    if version != "18.1.8":
        raise ValueError(f"expected LLVM 18.1.8, got {version}")
    bindir = Path(run([path, "--bindir"], capture=True).strip())
    # Externally extracted Homebrew bottles retain their intended pour path.
    if not (bindir / "clang").is_file():
        bindir = Path(path).resolve().parent
    tools = {name: bindir / name for name in ("clang", "ld.lld", "llvm-readobj", "llvm-objdump")}
    for name, binary in tools.items():
        if not binary.is_file():
            raise ValueError(f"missing LLVM tool: {name} in {bindir}")
    return tools


def self_test():
    with tempfile.TemporaryDirectory() as temporary:
        base = Path(temporary)
        for modified in (False, True):
            if modified:
                (base / "crt").mkdir()
                (base / "crt/syscall.c").write_text("wrong source")
            try:
                sdk_sources(base)
            except ValueError:
                pass
            else:
                raise AssertionError("missing/modified SDK source accepted")
        try:
            llvm_tools(str(base / "missing-llvm-config"))
        except ValueError:
            pass
        else:
            raise AssertionError("missing toolchain accepted")
        fake_config = base / "fake-llvm-config"
        fake_config.write_text("#!/bin/sh\nprintf '17.0.0\\n'\n")
        fake_config.chmod(0o755)
        try:
            llvm_tools(str(fake_config))
        except ValueError:
            pass
        else:
            raise AssertionError("unsupported LLVM version accepted")
        suffixes = ("elf", "map", "manifest.json", "inspect.txt", "disasm.txt")
        for stem in ("ps5-bt", "ps5-bt-native-query"):
            for suffix in suffixes:
                (base / f"{stem}.{suffix}").write_text(stem)
        invalidate_output("ps5-bt-native-query", base)
        for suffix in suffixes:
            assert (base / f"ps5-bt.{suffix}").read_text() == "ps5-bt"
            assert not (base / f"ps5-bt-native-query.{suffix}").exists()
        (base / "ps5-bt-native-query.elf").write_text("optional")
        invalidate_output("ps5-bt", base)
        assert (base / "ps5-bt-native-query.elf").read_text() == "optional"
        assert not any((base / f"ps5-bt.{suffix}").exists() for suffix in suffixes)
    print("PASS missing/modified SDK and missing toolchain failure paths")


def host_test(source):
    output = ROOT / "build/ps5/test-runtime"
    output.parent.mkdir(parents=True, exist_ok=True)
    run([os.environ.get("HOST_CC", "cc"), "-std=c11", "-O1", "-fno-builtin",
         "-Wall", "-Wextra", "-Werror", "-Iplatform/ps5/include",
         "-I" + str(source / "crt"), "platform/ps5/bootstrap/test_runtime.c", "-o", output])
    run([output])
    print("PASS synthetic host startup resolver/log/error/memory cases (not loader execution)")


def invalidate_output(stem, output=None):
    output = output if output is not None else ROOT / "build/ps5"
    for suffix in ("elf", "map", "manifest.json", "inspect.txt", "disasm.txt"):
        (output / f"{stem}.{suffix}").unlink(missing_ok=True)


def build(args):
    stem = "ps5-bt-native-query" if args.native_query else "ps5-bt"
    invalidate_output(stem)
    tools = llvm_tools(args.llvm_config)
    source = sdk_sources(args.sdk_source)
    host_test(source)
    review = ROOT / "docs/RUNTIME-REVIEW.md"
    if not review.is_file():
        raise ValueError("missing fixed runtime review: docs/RUNTIME-REVIEW.md")
    rust_version = run(["rustc", "--version"], capture=True).strip()
    if not rust_version.startswith("rustc 1.90.0 "):
        raise ValueError(f"expected pinned Rust 1.90.0, got {rust_version}")
    run(["cargo", "build", "-p", "ps5-runtime", "--release", "--target",
         "x86_64-unknown-none", "--target-dir", ROOT / "target", "--locked"])
    output = ROOT / "build/ps5"
    output.mkdir(parents=True, exist_ok=True)
    elf = output / f"{stem}.elf"
    elf.unlink(missing_ok=True)
    sources = [ROOT / "platform/ps5/bootstrap/start.S",
               ROOT / "platform/ps5/bootstrap/runtime.c", ROOT / "platform/ps5/main.c",
               ROOT / "platform/ps5/adapter_stub.c", source / "crt/syscall.c"]
    if args.native_query:
        sources.append(ROOT / "platform/ps5/diagnostics/native_query.c")
    objects = []
    flags = ["-target", "x86_64-sie-ps5", "-std=gnu11", "-O1", "-ffreestanding",
             "-fno-builtin", "-fPIC", "-fno-stack-protector", "-fno-plt",
             "-mno-red-zone", "-mgeneral-regs-only", "-ffunction-sections", "-fdata-sections",
             "-Wall", "-Wextra", "-Werror", "-Iplatform/ps5/include", "-I" + str(source / "crt")]
    if args.native_query:
        flags.append("-DPS5_BT_NATIVE_QUERY")
    for index, path in enumerate(sources):
        obj = output / f"{stem}-{index}-{path.stem}.o"
        run([tools["clang"], *flags, "-c", path, "-o", obj])
        objects.append(obj)
    archive = ROOT / "target/x86_64-unknown-none/release/libps5_runtime.a"
    run([tools["ld.lld"], "-m", "elf_x86_64", "-pie", "--no-dynamic-linker",
         "-e", "_start", "-T", ROOT / "platform/ps5/bootstrap/minimal.ld", "-z", "max-page-size=0x4000",
         "--gc-sections", "--no-undefined", "--build-id=none", "--strip-debug", "--hash-style=gnu",
         "--undefined=ps5_bootstrap_lookup",
         "-Map=" + str(output / f"{stem}.map"), *objects, archive, "-o", elf])
    try:
        report = inspect_bytes(elf.read_bytes())
    except ValueError:
        elf.unlink(missing_ok=True)
        raise
    report["runtime"] = "minimal-syscall-v5"
    report["native_query"] = args.native_query
    report["rust"] = rust_version
    report["llvm"] = run([tools["clang"], "--version"], capture=True).strip()
    report["lld"] = run([tools["ld.lld"], "--version"], capture=True).strip()
    report["tool_sha256"] = {name: digest(path) for name, path in tools.items()}
    report["sdk_revision"] = SDK_REVISION
    report["sdk_sources"] = SDK_FILES
    report["git_head"] = run(["git", "rev-parse", "HEAD"], capture=True).strip()
    report["git_status"] = run(["git", "status", "--porcelain"], capture=True).splitlines()
    report["rust_archive_sha256"] = digest(archive)
    report["project_sources"] = {str(path.relative_to(ROOT)): digest(path)
                                 for path in sources if path.is_relative_to(ROOT / "platform")}
    report["project_sources"]["platform/ps5/bootstrap/minimal.ld"] = digest(ROOT / "platform/ps5/bootstrap/minimal.ld")
    report["project_sources"]["platform/ps5/bootstrap/bootstrap.h"] = digest(ROOT / "platform/ps5/bootstrap/bootstrap.h")
    if args.native_query:
        name = "platform/ps5/diagnostics/native_query.h"
        report["project_sources"][name] = digest(ROOT / name)
    report["runtime_review_sha256"] = digest(review)
    (output / f"{stem}.manifest.json").write_text(json.dumps(report, indent=2) + "\n")
    inspection = run([tools["llvm-readobj"], "--file-headers", "--program-headers",
                      "--sections", "--symbols", "--dyn-symbols", "--relocations",
                      "--dynamic-table", "--needed-libs", elf], capture=True)
    (output / f"{stem}.inspect.txt").write_text(inspection)
    disassembly = run([tools["llvm-objdump"], "--disassemble", elf], capture=True)
    (output / f"{stem}.disasm.txt").write_text(disassembly)
    print(f"PASS inspectable bootstrap ELF: {elf}\nSHA-256 {report['sha256']}")
    print("UNTESTED on PS5; no live Bluetooth adapter, deployment or firmware compatibility claim.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sdk-source", default=os.environ.get("PS5_SDK_SOURCE"))
    parser.add_argument("--llvm-config", default=os.environ.get("LLVM_CONFIG", "llvm-config-18"))
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--native-query", action="store_true",
                        help="build separate single read-only native query diagnostic")
    args = parser.parse_args()
    try:
        self_test() if args.self_test else build(args)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        if not args.self_test:
            invalidate_output("ps5-bt-native-query" if args.native_query else "ps5-bt")
        parser.exit(1, f"PS5 build failed: {error}\n")
