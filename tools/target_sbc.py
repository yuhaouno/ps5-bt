#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Build the reviewed freestanding SBC archive and export relink materials."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
from typing import Sequence

try:
    from . import test_sbc
except ImportError:
    import test_sbc


ROOT = Path(__file__).resolve().parents[1]
TARGET = "x86_64-sie-ps5"
EXPECTED_UNDEFINED = {
    "memcpy",
    "memset",
    "sbc_encode",
    "sbc_finish_static",
    "sbc_get_codesize",
    "sbc_get_frame_length",
    "sbc_get_static_alignment",
    "sbc_get_static_size",
    "sbc_init_primitives",
    "sbc_init_static",
}
EXPECTED_EXTERNAL = {"memcpy", "memset"}


def _digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _resolve_tool(value: str | Path, name: str) -> Path:
    candidate = Path(value)
    if not candidate.is_file():
        found = shutil.which(str(value))
        if found is None:
            raise ValueError(f"missing {name}: {value}")
        candidate = Path(found)
    return Path(os.path.abspath(candidate))


def _version(tool: Path) -> str:
    return subprocess.run(
        [str(tool), "--version"], check=True, text=True, stdout=subprocess.PIPE
    ).stdout.strip()


def _require_llvm18(value: str | Path, name: str) -> Path:
    tool = _resolve_tool(value, name)
    version = _version(tool)
    if "18.1.8" not in version:
        raise ValueError(f"{name} must be LLVM 18.1.8, got: {version}")
    return tool


def _prepare_output(path: str | Path, kind: str) -> Path:
    output = Path(path).resolve()
    if output.exists() and not output.is_dir():
        raise ValueError(f"output path is not a directory: {output}")
    marker = output / ".ps5-bt-target-sbc-owned.json"
    if output.exists() and any(output.iterdir()):
        if not marker.is_file():
            raise ValueError(f"refusing unowned non-empty output directory: {output}")
        try:
            ownership = json.loads(marker.read_text())
        except (OSError, json.JSONDecodeError) as error:
            raise ValueError(f"invalid output ownership marker: {marker}") from error
        if ownership != {"kind": kind, "schema": 1}:
            raise ValueError(f"output ownership mismatch: {output}")
        for child in output.iterdir():
            if child == marker:
                continue
            if child.is_dir():
                shutil.rmtree(child)
            else:
                child.unlink()
    output.mkdir(parents=True, exist_ok=True)
    marker.write_text(json.dumps({"kind": kind, "schema": 1}, sort_keys=True) + "\n")
    return output


def _symbols(nm: Path, option: str, objects: Sequence[Path]) -> set[str]:
    result = subprocess.run(
        [str(nm), option, "--just-symbol-name", *(str(path) for path in objects)],
        check=True,
        text=True,
        stdout=subprocess.PIPE,
    )
    return {
        line.strip()
        for line in result.stdout.splitlines()
        if line.strip() and not line.strip().endswith(":")
    }


def source_inventory() -> dict[str, str]:
    """Return exact LGPL/library build inputs, excluding app self-test code."""
    patches = sorted((ROOT / "third_party/sbc/patches").glob("*.patch"))
    expected_patches = {
        "0001-generic-c-only.patch",
        "0002-caller-storage.patch",
        "0003-freestanding-static.patch",
    }
    if {path.name for path in patches} != expected_patches:
        raise ValueError("unexpected target SBC patch inventory")
    paths = [
        ROOT / "third_party/sbc/VENDORED.md",
        *sorted((ROOT / "third_party/sbc/upstream").rglob("*")),
        *patches,
        ROOT / "platform/codec/ps5_bt_sbc.c",
        ROOT / "platform/codec/ps5_bt_sbc.h",
        ROOT / "platform/codec/ps5_bt_sbc_freestanding.h",
        ROOT / "tools/test_sbc.py",
        ROOT / "tools/target_sbc.py",
    ]
    files = [path for path in paths if path.is_file()]
    inventory = {
        path.relative_to(ROOT).as_posix(): _digest(path) for path in files
    }
    if len(inventory) != len(files):
        raise ValueError("duplicate target SBC source inventory entry")
    return dict(sorted(inventory.items()))


def build_archive(
    clang: str | Path, archiver: str | Path, output_dir: str | Path
) -> Path:
    """Build a deterministic freestanding generic SBC archive for PS5 linking."""
    clang_path = _require_llvm18(clang, "clang")
    ar_path = _require_llvm18(archiver, "llvm-ar")
    nm_path = _require_llvm18(clang_path.with_name("llvm-nm"), "llvm-nm")
    output = _prepare_output(output_dir, "archive")
    test_sbc.verify_pristine()

    with tempfile.TemporaryDirectory(prefix="ps5-bt-target-sbc-") as temporary:
        build = Path(temporary)
        patched = test_sbc.patched_copy(build)
        resource = subprocess.run(
            [str(clang_path), "-print-resource-dir"],
            check=True,
            text=True,
            stdout=subprocess.PIPE,
        ).stdout.strip()
        resource_include = Path(resource) / "include"
        if not resource_include.is_dir():
            raise ValueError(f"missing Clang resource headers: {resource_include}")

        sources = [
            patched / "sbc/sbc.c",
            patched / "sbc/sbc_primitives.c",
            ROOT / "platform/codec/ps5_bt_sbc.c",
        ]
        objects: list[Path] = []
        flags = [
            "-target", TARGET, "-std=gnu11", "-O1", "-Wall", "-Wextra",
            "-Werror", "-Wno-ignored-attributes", "-Wno-unused-parameter",
            "-ffreestanding", "-fno-builtin", "-fPIC", "-fno-stack-protector",
            "-fno-plt", "-mno-red-zone", "-mgeneral-regs-only",
            "-ffunction-sections", "-fdata-sections", "-DSBC_STATIC_ONLY",
            "-nostdinc", "-isystem", str(resource_include),
            "-I", str(patched), "-I", str(ROOT / "platform/codec"),
        ]
        for source in sources:
            object_path = build / f"{source.stem}.o"
            subprocess.run(
                [str(clang_path), *flags, "-c", str(source), "-o", str(object_path)],
                check=True,
            )
            objects.append(object_path)

        undefined = _symbols(nm_path, "--undefined-only", objects)
        defined = _symbols(nm_path, "--defined-only", objects)
        if undefined != EXPECTED_UNDEFINED:
            raise ValueError(
                "unexpected target SBC undefined symbols: "
                f"expected={sorted(EXPECTED_UNDEFINED)} actual={sorted(undefined)}"
            )
        external = undefined - defined
        if external != EXPECTED_EXTERNAL:
            raise ValueError(
                "target SBC external symbols must be only memcpy/memset: "
                f"{sorted(external)}"
            )

        temporary_archive = build / "libps5_bt_sbc.a"
        subprocess.run(
            [str(ar_path), "rcsD", str(temporary_archive), *(str(p) for p in objects)],
            check=True,
        )
        archive = output / temporary_archive.name
        shutil.copy2(temporary_archive, archive)
        manifest = {
            "schema": 1,
            "target": TARGET,
            "clang": _version(clang_path),
            "llvm_ar": _version(ar_path),
            "archive": archive.name,
            "archive_sha256": _digest(archive),
            "undefined_symbols": sorted(undefined),
            "external_symbols": sorted(external),
            "source_inventory": source_inventory(),
        }
        (output / "codec-build.json").write_text(
            json.dumps(manifest, indent=2, sort_keys=True) + "\n"
        )
        return archive


REQUIRED_WRAPPER = {
    "ps5_bt_sbc_storage_size", "ps5_bt_sbc_storage_alignment",
    "ps5_bt_sbc_init", "ps5_bt_sbc_encode", "ps5_bt_sbc_finish",
}


def verify_archive(archive: str | Path, nm: str | Path) -> dict[str, object]:
    """Inspect replacement-friendly target objects and the narrow runtime seam."""
    try:
        from .inspect_archive import inspect
    except ImportError:
        from inspect_archive import inspect
    path = Path(archive).resolve(strict=True)
    nm_path = _require_llvm18(nm, "llvm-nm")
    inspect(path)
    undefined = _symbols(nm_path, "--undefined-only", [path])
    defined = _symbols(nm_path, "--defined-only", [path])
    external = undefined - defined
    if external != EXPECTED_EXTERNAL:
        raise ValueError(f"unexpected target SBC external imports: {sorted(external)}")
    if not REQUIRED_WRAPPER <= defined:
        raise ValueError(f"missing SBC wrapper exports: {sorted(REQUIRED_WRAPPER - defined)}")
    return {"archive_sha256": _digest(path), "external_symbols": sorted(external)}


RELINK_SCRIPT = r'''#!/usr/bin/env python3
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

BUNDLE = Path(__file__).resolve().parent
sys.dont_write_bytecode = True
sys.path.insert(0, str(BUNDLE / "support"))
from inspect_elf import inspect_bytes

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def tool(value, name):
    path = Path(os.path.abspath(value))
    if not path.is_file():
        raise ValueError(f"missing {name}: {path}")
    version = subprocess.run([str(path), "--version"], check=True, text=True,
                             stdout=subprocess.PIPE).stdout.strip()
    if "18.1.8" not in version:
        raise ValueError(f"{name} must be LLVM 18.1.8: {version}")
    return path, version

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--codec", type=Path,
                        default=BUNDLE / "codec/libps5_bt_sbc.a")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--lld", required=True)
    parser.add_argument("--readobj", required=True)
    parser.add_argument("--objdump", required=True)
    args = parser.parse_args()
    manifest = json.loads((BUNDLE / "bundle-manifest.json").read_text())
    for relative, expected in manifest["immutable_sha256"].items():
        path = BUNDLE / relative
        if not path.is_file() or digest(path) != expected:
            raise ValueError(f"immutable relink input mismatch: {relative}")
    codec = args.codec.resolve()
    if not codec.is_file():
        raise ValueError(f"missing replacement codec archive: {codec}")
    lld, lld_version = tool(args.lld, "ld.lld")
    readobj, readobj_version = tool(args.readobj, "llvm-readobj")
    objdump, objdump_version = tool(args.objdump, "llvm-objdump")
    output = (args.output.resolve() if args.output else
              BUNDLE / "artifacts" / manifest["output_name"])
    output.parent.mkdir(parents=True, exist_ok=True)
    output.unlink(missing_ok=True)
    map_path = output.with_suffix(".map")
    objects = [BUNDLE / value for value in manifest["app_objects"]]
    rust = BUNDLE / manifest["rust_archive"]
    linker_script = BUNDLE / manifest["linker_script"]
    command = [str(lld), "-m", "elf_x86_64", "-pie", "--no-dynamic-linker",
               "-e", "_start", "-T", str(linker_script), "-z",
               "max-page-size=0x4000", "--gc-sections", "--no-undefined",
               "--build-id=none", "--strip-debug", "--hash-style=gnu",
               "--undefined=ps5_bootstrap_lookup", "--undefined=__crt_syscall",
               "--wrap=__crt_syscall", "-Map=" + str(map_path),
               *(str(path) for path in objects), "--start-group", str(rust),
               str(codec), "--end-group", "-o", str(output)]
    subprocess.run(command, check=True)
    structure = inspect_bytes(output.read_bytes(), allow_kernel_resolver=True)
    structure_path = output.with_suffix(".structure.json")
    structure_path.write_text(json.dumps(structure, indent=2, sort_keys=True) + "\n")
    inspect_path = output.with_suffix(".inspect.txt")
    disasm_path = output.with_suffix(".disasm.txt")
    inspection = subprocess.run(
        [str(readobj), "--file-headers", "--program-headers", "--sections",
         "--symbols", "--dyn-symbols", "--relocations", "--dynamic-table",
         "--needed-libs", str(output)], check=True, text=True,
        stdout=subprocess.PIPE).stdout
    inspect_path.write_text(inspection)
    disassembly = subprocess.run([str(objdump), "--disassemble", str(output)],
                                 check=True, text=True,
                                 stdout=subprocess.PIPE).stdout
    disasm_path.write_text(disassembly)
    result = {
        "schema": 1,
        "elf": str(output),
        "elf_sha256": digest(output),
        "codec": str(codec),
        "codec_sha256": digest(codec),
        "map_sha256": digest(map_path),
        "structure_sha256": digest(structure_path),
        "inspection_sha256": digest(inspect_path),
        "disassembly_sha256": digest(disasm_path),
        "lld": lld_version,
        "llvm_readobj": readobj_version,
        "llvm_objdump": objdump_version,
    }
    result_path = output.with_suffix(".relink-result.json")
    result_path.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(output)

if __name__ == "__main__":
    main()
'''


def export_relink(
    output_dir: str | Path,
    app_objects: Sequence[str | Path],
    rust_archive: str | Path,
    linker_script: str | Path,
    codec_archive: str | Path,
    *,
    lld: str | Path,
    readobj: str | Path,
    objdump: str | Path,
    output_name: str = "ps5-bt-sbc-selftest.elf",
) -> Path:
    """Export relink inputs, a replacement script, and one inspected baseline ELF."""
    lld_path = _require_llvm18(lld, "ld.lld")
    readobj_path = _require_llvm18(readobj, "llvm-readobj")
    objdump_path = _require_llvm18(objdump, "llvm-objdump")
    output = _prepare_output(output_dir, "relink")
    if not app_objects:
        raise ValueError("at least one application object is required")
    if Path(output_name).name != output_name or not output_name.endswith(".elf"):
        raise ValueError("output_name must be one ELF filename")

    object_dir = output / "objects"
    archive_dir = output / "archives"
    codec_dir = output / "codec"
    linker_dir = output / "linker"
    source_dir = output / "source"
    support_dir = output / "support"
    for directory in (
        object_dir, archive_dir, codec_dir, linker_dir, source_dir, support_dir
    ):
        directory.mkdir()

    copied_objects: list[str] = []
    for index, value in enumerate(app_objects):
        source = Path(value).resolve()
        if not source.is_file():
            raise ValueError(f"missing application object: {source}")
        destination = object_dir / f"{index:02d}-{source.name}"
        shutil.copy2(source, destination)
        copied_objects.append(destination.relative_to(output).as_posix())

    rust_source = Path(rust_archive).resolve()
    linker_source = Path(linker_script).resolve()
    codec_source = Path(codec_archive).resolve()
    for path, label in (
        (rust_source, "Rust archive"),
        (linker_source, "linker script"),
        (codec_source, "codec archive"),
    ):
        if not path.is_file():
            raise ValueError(f"missing {label}: {path}")
    rust_copy = archive_dir / rust_source.name
    codec_copy = codec_dir / "libps5_bt_sbc.a"
    linker_copy = linker_dir / "minimal.ld"
    shutil.copy2(rust_source, rust_copy)
    shutil.copy2(codec_source, codec_copy)
    shutil.copy2(linker_source, linker_copy)

    for relative in source_inventory():
        source = ROOT / relative
        destination = source_dir / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, destination)
    inspect_source = ROOT / "tools/inspect_elf.py"
    inspect_copy = support_dir / "inspect_elf.py"
    shutil.copy2(inspect_source, inspect_copy)

    script = output / "relink.py"
    script.write_text(RELINK_SCRIPT)
    script.chmod(0o755)
    immutable = [
        *(output / value for value in copied_objects),
        rust_copy,
        linker_copy,
        script,
        inspect_copy,
        *(path for path in source_dir.rglob("*") if path.is_file()),
    ]
    immutable_hashes = {
        path.relative_to(output).as_posix(): _digest(path) for path in immutable
    }
    manifest = {
        "schema": 1,
        "target": TARGET,
        "output_name": output_name,
        "app_objects": copied_objects,
        "rust_archive": rust_copy.relative_to(output).as_posix(),
        "linker_script": linker_copy.relative_to(output).as_posix(),
        "default_codec": codec_copy.relative_to(output).as_posix(),
        "default_codec_sha256": _digest(codec_copy),
        "immutable_sha256": dict(sorted(immutable_hashes.items())),
    }
    manifest_path = output / "bundle-manifest.json"
    manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    subprocess.run(
        [
            str(script), "--lld", str(lld_path), "--readobj",
            str(readobj_path), "--objdump", str(objdump_path),
        ],
        check=True,
    )
    return manifest_path


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--build-archive", type=Path)
    parser.add_argument("--verify-archive", type=Path)
    parser.add_argument("--nm", default="llvm-nm")
    parser.add_argument("--clang", default="clang")
    parser.add_argument("--ar", default="llvm-ar")
    args = parser.parse_args()
    if (args.build_archive is None) == (args.verify_archive is None):
        parser.error("select exactly one of --build-archive or --verify-archive")
    if args.verify_archive is not None:
        print(json.dumps(verify_archive(args.verify_archive, args.nm), sort_keys=True))
    else:
        print(build_archive(args.clang, args.ar, args.build_archive))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
