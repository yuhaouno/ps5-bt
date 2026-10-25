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
    "crt/kernel.c": "9aa60aa84544687041169d92f161dec1b58934459ea29a74dba09d838924e086",
    "crt/kernel.h": "cea69374188ff1cf6daf5730ac519dd9a83fa6781e125d90c5c1b06717010dc8",
    "crt/kernel_iommu.h": "bd7b329afc9a9bf6c50e4406ede46c88b4e871fe4541280f8a7d7746757d9768",
    "crt/elf.h": "ac0b9c99600cfb82031eb845b4f68512dbbf09c2bc42a09a4aaa19a052c98358",
    "crt/nid.c": "6bd670b05ec4a8816688e18cc4425353c2f764f574d0c7f3db5cc4e8c5b7a9dc",
    "crt/nid.h": "f79fd2a5daacde8bae9e070d4bf8158b42b2fdd8fccd9ea5f868a7a099dcb365",
    "crt/klog.h": "19fc44e52797d1a1f9959f5f7e6dee6006f600486f4bae451db862510a2cbe11",
}
USB_SDK_HEADERS = json.loads((ROOT / "tools/usb_sdk_headers.json").read_text())
# Expand the upstream stdint.h symlink into the identical, notice-bearing target
# bytes when provisioning a standalone subset rather than a Git checkout.
SDK_HEADER_ALIASES = {"include/freebsd/stdint.h": "include/freebsd/sys/stdint.h"}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(command, capture=False):
    print("+ " + " ".join(map(str, command)), flush=True)
    return subprocess.run(list(map(str, command)), cwd=ROOT, check=True,
                          text=True, stdout=subprocess.PIPE if capture else None).stdout


def sdk_sources(source=None, usb_metadata=False):
    """A fixed subset, not an SDK installation or unverified moving checkout."""
    directory = Path(source).resolve() if source else ROOT / ".sdk" / SDK_REVISION
    selected = SDK_FILES | (USB_SDK_HEADERS if usb_metadata else {})
    for name, expected in selected.items():
        path = directory / name
        if not path.exists() and not source:
            remote_name = SDK_HEADER_ALIASES.get(name, name)
            url = f"https://raw.githubusercontent.com/ps5-payload-dev/sdk/{SDK_REVISION}/{remote_name}"
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
        for stem in ("ps5-bt", "ps5-bt-native-query", "ps5-bt-usb-metadata",
                     "ps5-bt-capture-preflight", "ps5-bt-capture-once",
                     "ps5-bt-sbc-selftest", "ps5-bt-capture-observe",
                     "ps5-bt-native-l2cap-probe"):
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
        assert (base / "ps5-bt-usb-metadata.elf").is_file()
        invalidate_output("ps5-bt-usb-metadata", base)
        assert not any((base / f"ps5-bt-usb-metadata.{suffix}").exists() for suffix in suffixes)
        assert all((base / f"ps5-bt-native-l2cap-probe.{suffix}").is_file() for suffix in suffixes)
        invalidate_output("ps5-bt-native-l2cap-probe", base)
        assert not any((base / f"ps5-bt-native-l2cap-probe.{suffix}").exists() for suffix in suffixes)
        assert (base / "ps5-bt-native-query.elf").read_text() == "optional"
    print("PASS missing/modified SDK and missing toolchain failure paths")


def host_test(source):
    output = ROOT / "build/ps5/test-runtime"
    output.parent.mkdir(parents=True, exist_ok=True)
    command = [os.environ.get("HOST_CC", "cc"), "-std=c11", "-O1", "-fno-builtin",
         "-Wall", "-Wextra", "-Werror", "-Iplatform/ps5/include",
         "-I" + str(source / "crt"), "platform/ps5/bootstrap/test_runtime.c", "-o", output]
    run(command)
    run([output])
    run([*command, "-DPS5_BT_LOADER_KERNEL_RESOLVER"])
    run([output])
    run([*command, "-DPS5_BT_CAPTURE_PROBE"])
    run([output])
    run([*command, "-DPS5_BT_CAPTURE_PROBE", "-DPS5_BT_LOADER_KERNEL_RESOLVER"])
    run([output])
    print("PASS synthetic host startup resolver/log/error/memory cases (not loader execution)")


def invalidate_output(stem, output=None):
    output = output if output is not None else ROOT / "build/ps5"
    for suffix in ("elf", "map", "manifest.json", "inspect.txt", "disasm.txt"):
        (output / f"{stem}.{suffix}").unlink(missing_ok=True)


def build(args):
    stem = artifact_stem(args)
    invalidate_output(stem)
    if (args.capture_preflight or args.capture_once or args.capture_observe) and not args.loader_kernel_resolver:
        raise ValueError("capture diagnostics require the explicit existing-loader kernel resolver")
    tools = llvm_tools(args.llvm_config)
    source = sdk_sources(args.sdk_source, args.usb_metadata)
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
    sources.append(ROOT / "platform/ps5/bootstrap/syscall_checked.S")
    if args.loader_kernel_resolver:
        sources += [source / "crt/kernel.c", source / "crt/nid.c"]
    if args.native_query:
        sources.append(ROOT / "platform/ps5/diagnostics/native_query.c")
    if args.native_l2cap_probe:
        sources.append(ROOT / "platform/ps5/diagnostics/native_l2cap_probe.c")
    if args.usb_metadata:
        sources += [ROOT / "platform/ps5/diagnostics/usb_probe.c",
                    ROOT / "platform/ps5/diagnostics/usb_descriptors.c"]
    if args.capture_preflight or args.capture_once or args.capture_observe:
        sources += [ROOT / "platform/ps5/diagnostics/capture_probe.c",
                    ROOT / "platform/ps5/diagnostics/capture_observe.c"]
    if args.sbc_selftest:
        sources.append(ROOT / "platform/codec/ps5_bt_sbc_selftest.c")
    objects = []
    flags = ["-target", "x86_64-sie-ps5", "-std=gnu11", "-O1", "-ffreestanding",
             "-fno-builtin", "-fPIC", "-fno-stack-protector", "-fno-plt",
             "-mno-red-zone", "-mgeneral-regs-only", "-ffunction-sections", "-fdata-sections",
             "-Wall", "-Wextra", "-Werror", "-Iplatform/ps5/include", "-I" + str(source / "crt")]
    if args.loader_kernel_resolver:
        flags += ["-DPS5_BT_LOADER_KERNEL_RESOLVER"]
    if args.native_query:
        flags.append("-DPS5_BT_NATIVE_QUERY")
    if args.native_l2cap_probe:
        flags.append("-DPS5_BT_NATIVE_L2CAP_PROBE")
    if args.usb_metadata:
        flags.append("-DPS5_BT_USB_METADATA")
    if args.capture_preflight or args.capture_once or args.capture_observe:
        flags.append("-DPS5_BT_CAPTURE_PROBE")
    if args.capture_observe:
        flags.append("-DPS5_BT_CAPTURE_OBSERVE")
    if args.capture_preflight:
        flags.append("-DPS5_BT_CAPTURE_PREFLIGHT")
    if args.sbc_selftest:
        flags.append("-DPS5_BT_SBC_SELFTEST")
    for index, path in enumerate(sources):
        obj = output / f"{stem}-{index}-{path.stem}.o"
        extra = ["-Wno-sign-compare"] if path.name == "kernel.c" else []
        if args.usb_metadata and path.name == "usb_probe.c":
            extra += ["-DPS5_BT_USB_PROBE_TARGET", "-isystem", source / "include/freebsd", "-isystem", source / "include",
                      "-MD", "-MF", output / f"{stem}-usb-headers.d"]
        run([tools["clang"], *flags, *extra, "-c", path, "-o", obj])
        objects.append(obj)
    archive = ROOT / "target/x86_64-unknown-none/release/libps5_runtime.a"
    codec_archive = None
    if args.sbc_selftest:
        from target_sbc import build_archive
        codec_archive = build_archive(tools["clang"], tools["clang"].with_name("llvm-ar"),
                                      output / f"{stem}-codec")
    link_flags = ["-m", "elf_x86_64", "-pie", "--no-dynamic-linker",
         "-e", "_start", "-T", ROOT / "platform/ps5/bootstrap/minimal.ld", "-z", "max-page-size=0x4000",
         "--gc-sections", "--no-undefined", "--build-id=none", "--strip-debug", "--hash-style=gnu",
         "--undefined=ps5_bootstrap_lookup", "--undefined=__crt_syscall", "--wrap=__crt_syscall"]
    run([tools["ld.lld"], *link_flags,
         "-Map=" + str(output / f"{stem}.map"), *objects, archive,
         *([codec_archive] if codec_archive else []), "-o", elf])
    try:
        report = inspect_bytes(elf.read_bytes(), allow_kernel_resolver=args.loader_kernel_resolver)
    except ValueError:
        elf.unlink(missing_ok=True)
        raise
    report["runtime"] = "minimal-syscall-v6"
    report["native_query"] = args.native_query
    report["native_l2cap_probe"] = args.native_l2cap_probe
    report["usb_metadata"] = args.usb_metadata
    report["capture_preflight"] = args.capture_preflight
    report["capture_once"] = args.capture_once
    report["capture_observe"] = args.capture_observe
    report["sbc_selftest"] = args.sbc_selftest
    report["loader_kernel_resolver"] = args.loader_kernel_resolver
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
    if args.native_l2cap_probe:
        name = "platform/ps5/diagnostics/native_l2cap_probe.h"
        report["project_sources"][name] = digest(ROOT / name)
    if args.capture_preflight or args.capture_once or args.capture_observe:
        for name in ("platform/ps5/diagnostics/capture_probe.h",
                     "platform/ps5/diagnostics/capture_observe.h"):
            report["project_sources"][name] = digest(ROOT / name)
    if args.sbc_selftest:
        from target_sbc import export_relink, source_inventory
        report["codec_sources"] = source_inventory()
        report["codec_archive_sha256"] = digest(codec_archive)
        relink_manifest = export_relink(
            output / f"{stem}-relink", objects, archive,
            ROOT / "platform/ps5/bootstrap/minimal.ld", codec_archive,
            lld=tools["ld.lld"], readobj=tools["llvm-readobj"],
            objdump=tools["llvm-objdump"], output_name=f"{stem}.elf")
        report["relink_files"] = {
            str(path.relative_to(ROOT)): digest(path)
            for path in sorted(relink_manifest.parent.rglob("*")) if path.is_file()
        }
        for name in ("platform/codec/ps5_bt_sbc_selftest.h",):
            report["project_sources"][name] = digest(ROOT / name)
    if args.usb_metadata:
        for name in ("usb_probe.h", "usb_descriptors.h"):
            path = ROOT / "platform/ps5/diagnostics" / name
            report["project_sources"][str(path.relative_to(ROOT))] = digest(path)
        dependencies = (output / f"{stem}-usb-headers.d").read_text().replace("\\\n", " ").split()
        report["sdk_headers"] = {str(Path(name).relative_to(source)): digest(Path(name))
                                 for name in dependencies if Path(name).is_relative_to(source / "include")}
        if report["sdk_headers"] != USB_SDK_HEADERS:
            raise ValueError("USB SDK header dependency inventory differs from the reviewed pin")
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


def artifact_stem(args):
    if args.native_l2cap_probe:
        return "ps5-bt-native-l2cap-probe"
    if args.capture_observe:
        return "ps5-bt-capture-observe"
    if args.sbc_selftest:
        return "ps5-bt-sbc-selftest"
    if args.capture_preflight:
        return "ps5-bt-capture-preflight"
    if args.capture_once:
        return "ps5-bt-capture-once"
    if args.usb_metadata:
        return "ps5-bt-usb-metadata"
    return "ps5-bt-native-query" if args.native_query else "ps5-bt"


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sdk-source", default=os.environ.get("PS5_SDK_SOURCE"))
    parser.add_argument("--llvm-config", default=os.environ.get("LLVM_CONFIG", "llvm-config-18"))
    parser.add_argument("--self-test", action="store_true")
    variants = parser.add_mutually_exclusive_group()
    variants.add_argument("--native-query", action="store_true",
                        help="build separate single read-only native query diagnostic")
    variants.add_argument("--native-l2cap-probe", action="store_true",
                        help="create/close one control and one unconnected L2CAP socket")
    variants.add_argument("--usb-metadata", action="store_true",
                          help="build separate read-only USB metadata diagnostic")
    variants.add_argument("--capture-preflight", action="store_true",
                          help="load capture prerequisites and resolve exports only")
    variants.add_argument("--capture-once", action="store_true",
                          help="perform one bounded metadata-only capture cycle")
    variants.add_argument("--capture-observe", action="store_true",
                          help="observe first capture data for at most 20 reads/two seconds")
    variants.add_argument("--sbc-selftest", action="store_true",
                          help="encode deterministic generated PCM without Bluetooth or capture")
    parser.add_argument("--loader-kernel-resolver", action="store_true",
                        help="resolve own-process exports through existing loader kernel R/W")
    args = parser.parse_args()
    try:
        self_test() if args.self_test else build(args)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        if not args.self_test:
            invalidate_output(artifact_stem(args))
        parser.exit(1, f"PS5 build failed: {error}\n")
