#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later

"""Build and test the isolated, caller-storage libsbc encoder wrapper."""

from __future__ import annotations

import argparse
import hashlib
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[1]
VENDORED = ROOT / "third_party" / "sbc"

EXPECTED_HASHES = {
    "COPYING.LIB": "ec60b993835e2c6b79e6d9226345f4e614e686eb57dc13b6420c15a33a8996e5",
    "AUTHORS": "e57876fcdfa3cfdf88db7f6ed971c1f44fda8e9a33c8c1d9999903945e75fd2c",
    "sbc/sbc.c": "42d16642f3eebf51f7f9e84a03f62a1d722c03414ed3265ba183bdaa0d9dccc4",
    "sbc/sbc.h": "ab2e351d97ee25d7836902465ef1e5589c7bc07988e5c2bdc484ac1ffee754ab",
    "sbc/sbc_private.h": "16dd9e1621d73c387ac4b2027320028e963e5a2d09013d9dccec4ede3142a140",
    "sbc/sbc_math.h": "4cbe03013967fd9872c86677f4ff398994010706ffc6f77d39792f44bc8f2fdf",
    "sbc/sbc_tables.h": "fe57949c20000aa3cacc359887ea1ea0cb31146371beb2488f732bc6bdf17933",
    "sbc/sbc_primitives.c": "1badabd8eee20c25a1c0a684354e53e68e8901ca571493060965016ca9c0ec74",
    "sbc/sbc_primitives.h": "9cf89824889d2e0464d4135808ead9a7d70ee223d6e1b73e43969fa969a3951e",
}


def run(command: list[str], **kwargs: object) -> subprocess.CompletedProcess[bytes]:
    return subprocess.run(command, check=True, **kwargs)


def verify_pristine() -> None:
    source = VENDORED / "upstream"
    actual = {path.relative_to(source).as_posix() for path in source.rglob("*") if path.is_file()}
    if actual != set(EXPECTED_HASHES):
        raise RuntimeError(f"unexpected pristine file set: {sorted(actual)}")
    for relative, expected in EXPECTED_HASHES.items():
        digest = hashlib.sha256((source / relative).read_bytes()).hexdigest()
        if digest != expected:
            raise RuntimeError(f"pristine hash mismatch for {relative}: {digest}")


def patched_copy(build: Path, name: str = "patched") -> Path:
    target = build / name
    shutil.copytree(VENDORED / "upstream", target)
    for name in (
        "0001-generic-c-only.patch",
        "0002-caller-storage.patch",
        "0003-freestanding-static.patch",
    ):
        run([
            "patch",
            "--batch",
            "--fuzz=0",
            "--directory",
            str(target),
            "-p1",
            "--input",
            str(VENDORED / "patches" / name),
        ], stdout=subprocess.PIPE)
    return target


def generic_reference_copy(build: Path) -> Path:
    """Keep upstream allocation/API behavior; remove unavailable arch dispatch only."""
    target = build / "generic-reference"
    shutil.copytree(VENDORED / "upstream", target)
    run([
        "patch",
        "--batch",
        "--fuzz=0",
        "--directory",
        str(target),
        "-p1",
        "--input",
        str(VENDORED / "patches" / "0001-generic-c-only.patch"),
    ], stdout=subprocess.PIPE)
    return target


def compile_program(cc: str, output: Path, sources: list[Path], includes: list[Path],
                    extra: list[str]) -> None:
    command = [
        cc,
        "-std=gnu11",
        "-O1",
        "-g",
        "-Wall",
        "-Wextra",
        "-Werror",
        "-Wno-ignored-attributes",
        "-Wno-unused-parameter",
        *extra,
    ]
    for include in includes:
        command.extend(["-I", str(include)])
    command.extend(str(source) for source in sources)
    command.extend(["-o", str(output)])
    run(command)


def compile_object(cc: str, output: Path, source: Path, includes: list[Path],
                   extra: list[str]) -> None:
    command = [
        cc,
        "-std=gnu11",
        "-O1",
        "-Wall",
        "-Wextra",
        "-Werror",
        "-Wno-ignored-attributes",
        "-Wno-unused-parameter",
        "-ffreestanding",
        "-fno-builtin",
        "-ffunction-sections",
        "-fdata-sections",
        *extra,
    ]
    for include in includes:
        command.extend(["-I", str(include)])
    command.extend(["-c", str(source), "-o", str(output)])
    run(command)


def build_static_archive(cc: str, ar: str, output_dir: Path, patched: Path) -> Path:
    output_dir.mkdir(parents=True, exist_ok=True)
    includes = [patched, ROOT / "platform" / "codec"]
    sources = [
        patched / "sbc" / "sbc.c",
        patched / "sbc" / "sbc_primitives.c",
        ROOT / "platform" / "codec" / "ps5_bt_sbc.c",
    ]
    objects = []
    for source in sources:
        output = output_dir / (source.stem + ".o")
        compile_object(cc, output, source, includes,
                       ["-DSBC_STATIC_ONLY", "-fPIC", "-fno-stack-protector"])
        objects.append(output)
    inspect_no_allocator(objects)
    archive = output_dir / "libps5_bt_sbc.a"
    run([ar, "rcs", str(archive), *(str(path) for path in objects)])
    return archive


def compile_archive_vector(cc: str, output: Path, archive_dir: Path,
                           includes: list[Path]) -> None:
    run([
        cc,
        "-std=gnu11",
        "-O1",
        "-Wall",
        "-Wextra",
        "-Werror",
        "-DUSE_PS5_BT_WRAPPER",
        *(item for include in includes for item in ("-I", str(include))),
        str(ROOT / "tests" / "codec" / "sbc_vector_tool.c"),
        "-L", str(archive_dir),
        "-lps5_bt_sbc",
        "-o", str(output),
    ])


def inspect_no_allocator(objects: list[Path]) -> None:
    nm = shutil.which("nm")
    if nm is None:
        raise RuntimeError("nm is required for allocator/import inspection")
    result = run([nm, "-u", *(str(path) for path in objects)], stdout=subprocess.PIPE)
    undefined = result.stdout.decode("utf-8", errors="replace")
    forbidden = re.compile(r"(?:^|[_\s])(malloc|calloc|realloc|free|__cpu_indicator_init|"
                           r"sbc_init_primitives_(?:mmx|sse|neon|armv6|iwmmxt))(?:$|\s)", re.M)
    match = forbidden.search(undefined)
    if match:
        raise RuntimeError(f"forbidden generic codec import: {match.group(1)}")


def compare_reference(wrapper: Path, reference: Path) -> None:
    for mode in (0, 1):
        for bitpool in range(2, 54):
            for signal in ("silence", "pattern"):
                arguments = ["encode", str(mode), str(bitpool), signal]
                wrapped = run([str(wrapper), *arguments], stdout=subprocess.PIPE).stdout
                pristine = run([str(reference), *arguments], stdout=subprocess.PIPE).stdout
                expected = (12 if mode == 0 else 13) + 2 * bitpool
                if len(wrapped) != expected:
                    raise RuntimeError(
                        f"wrong frame length mode={mode} bitpool={bitpool}: {len(wrapped)}"
                    )
                if wrapped != pristine:
                    raise RuntimeError(
                        f"reference mismatch mode={mode} bitpool={bitpool} signal={signal}"
                    )

    for mode in (0, 1):
        for bitpool in (2, 32, 53):
            for signal in ("silence", "pattern"):
                frame = run([
                    str(wrapper), "encode", str(mode), str(bitpool), signal
                ], stdout=subprocess.PIPE).stdout
                decoded = run([str(reference), "decode"], input=frame,
                              stdout=subprocess.PIPE).stdout
                if len(decoded) != 512:
                    raise RuntimeError("reference decoder returned a partial PCM frame")
                if signal == "silence" and any(decoded):
                    raise RuntimeError("reference decoder did not reproduce SBC silence")
                if signal == "pattern" and not any(decoded):
                    raise RuntimeError("reference decoder unexpectedly produced only silence")


def find_llvm18_clang() -> Path | None:
    candidates: list[Path] = []
    llvm_config = os.environ.get("LLVM_CONFIG")
    if llvm_config:
        candidates.append(Path(llvm_config).resolve().with_name("clang"))
    candidates.extend([
        Path("/opt/homebrew/opt/llvm@18/bin/clang"),
        Path("/tmp/ps5-bt-research-20261023/llvm18/llvm@18/18.1.8/bin/clang"),
    ])
    for candidate in candidates:
        if candidate.is_file() and os.access(candidate, os.X_OK):
            version = run([str(candidate), "--version"], stdout=subprocess.PIPE).stdout
            if b"version 18.1.8" in version:
                return candidate
    return None


def try_target_objects(clang: Path | None, build: Path, patched: Path,
                       includes: list[Path]) -> str:
    if clang is None:
        return "LLVM 18.1.8 clang unavailable; generic PS5-target objects not compiled"
    target_dir = build / "target-objects"
    target_dir.mkdir()
    common = [
        "-target", "x86_64-sie-ps5",
        "-DSBC_STATIC_ONLY",
        "-fPIC",
        "-fno-stack-protector",
        "-fno-plt",
        "-mno-red-zone",
        "-mgeneral-regs-only",
    ]
    resource = run([str(clang), "-print-resource-dir"],
                   stdout=subprocess.PIPE).stdout.decode().strip()
    common.extend(["-nostdinc", "-isystem", str(Path(resource) / "include")])
    objects = []
    for source in (
        patched / "sbc" / "sbc.c",
        patched / "sbc" / "sbc_primitives.c",
        ROOT / "platform" / "codec" / "ps5_bt_sbc.c",
        ROOT / "platform" / "codec" / "ps5_bt_sbc_selftest.c",
    ):
        output = target_dir / (source.stem + ".o")
        try:
            compile_object(str(clang), output, source, includes, common)
        except subprocess.CalledProcessError:
            return ("LLVM 18.1.8 is present, but public target C headers are unavailable; "
                    "generic PS5-target objects not compiled")
        objects.append(output)
    inspect_no_allocator(objects)
    return "generic x86_64-sie-ps5 objects compiled and allocator imports rejected"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--cc", default=os.environ.get("HOST_CC", os.environ.get("CC", "cc")))
    parser.add_argument("--ar", default=os.environ.get("AR", "ar"))
    parser.add_argument("--build-archive", type=Path)
    parser.add_argument("--no-sanitize", action="store_true")
    parser.add_argument("--keep-build", action="store_true")
    args = parser.parse_args()

    verify_pristine()
    temporary = tempfile.TemporaryDirectory(prefix="ps5-bt-sbc-test-")
    build = Path(temporary.name)
    patched = patched_copy(build)
    if args.build_archive is not None:
        archive = build_static_archive(args.cc, args.ar,
                                       args.build_archive.resolve(), patched)
        print(archive)
        temporary.cleanup()
        return 0
    generic_reference = generic_reference_copy(build)
    includes = [patched, ROOT / "platform" / "codec"]
    codec_sources = [patched / "sbc" / "sbc.c", patched / "sbc" / "sbc_primitives.c"]
    wrapper_source = ROOT / "platform" / "codec" / "ps5_bt_sbc.c"

    sanitizer = [] if args.no_sanitize else [
        "-fsanitize=address,undefined", "-fno-omit-frame-pointer"
    ]
    self_test = build / "sbc-wrapper-test"
    compile_program(args.cc, self_test, [*codec_sources, wrapper_source,
                    ROOT / "tests" / "codec" / "sbc_wrapper_test.c"], includes,
                    ["-DSBC_STATIC_ONLY", *sanitizer])
    environment = os.environ.copy()
    # Apple Clang's ASan runtime does not implement LeakSanitizer. The object
    # import check below independently rejects allocator use in this codec.
    environment.setdefault("ASAN_OPTIONS", "detect_leaks=0:abort_on_error=1")
    run([str(self_test)], env=environment)

    generated_self_test = build / "sbc-generated-self-test"
    compile_program(
        args.cc,
        generated_self_test,
        [
            *codec_sources,
            wrapper_source,
            ROOT / "platform" / "codec" / "ps5_bt_sbc_selftest.c",
            ROOT / "tests" / "codec" / "sbc_selftest_test.c",
        ],
        includes,
        ["-DSBC_STATIC_ONLY", *sanitizer],
    )
    run([str(generated_self_test)], env=environment)

    wrapper_tool = build / "sbc-wrapper-vector"
    compile_program(args.cc, wrapper_tool, [*codec_sources, wrapper_source,
                    ROOT / "tests" / "codec" / "sbc_vector_tool.c"], includes,
                    ["-DSBC_STATIC_ONLY", "-DUSE_PS5_BT_WRAPPER"])
    reference_tool = build / "sbc-reference-vector"
    compile_program(args.cc, reference_tool,
                    [generic_reference / "sbc" / "sbc.c",
                     generic_reference / "sbc" / "sbc_primitives.c",
                     ROOT / "tests" / "codec" / "sbc_vector_tool.c"],
                    [generic_reference], [])
    compare_reference(wrapper_tool, reference_tool)

    audit_objects = []
    for source in (
        *codec_sources,
        wrapper_source,
        ROOT / "platform" / "codec" / "ps5_bt_sbc_selftest.c",
    ):
        output = build / (source.stem + "-audit.o")
        compile_object(args.cc, output, source, includes, ["-DSBC_STATIC_ONLY"])
        audit_objects.append(output)
    inspect_no_allocator(audit_objects)

    replacement_source = patched_copy(build, "replacement-source")
    first_archive_dir = build / "archive-first"
    replacement_archive_dir = build / "archive-replacement"
    build_static_archive(args.cc, args.ar, first_archive_dir, patched)
    build_static_archive(args.cc, args.ar, replacement_archive_dir,
                         replacement_source)
    first_link = build / "archive-first-vector"
    replacement_link = build / "archive-replacement-vector"
    compile_archive_vector(args.cc, first_link, first_archive_dir, includes)
    compile_archive_vector(args.cc, replacement_link, replacement_archive_dir,
                           [replacement_source, ROOT / "platform" / "codec"])
    relink_args = ["encode", "1", "32", "pattern"]
    first_frame = run([str(first_link), *relink_args], stdout=subprocess.PIPE).stdout
    replacement_frame = run([str(replacement_link), *relink_args],
                            stdout=subprocess.PIPE).stdout
    if first_frame != replacement_frame or len(first_frame) != 77:
        raise RuntimeError("replacement archive relink output mismatch")
    target_status = try_target_objects(find_llvm18_clang(), build, patched, includes)

    if args.keep_build:
        kept = Path.cwd() / "sbc-test-build"
        if kept.exists():
            raise RuntimeError(f"refusing to replace existing {kept}")
        shutil.copytree(build, kept)
        print(f"kept build tree: {kept}")
    print("SBC pristine hashes, patches, bounds, lifecycle, all 208 encoder vectors,")
    print("12 unchanged-decoder vectors, no-allocation object inspection, and")
    print("separate replacement-archive relinking passed.")
    print(target_status + ".")
    temporary.cleanup()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
