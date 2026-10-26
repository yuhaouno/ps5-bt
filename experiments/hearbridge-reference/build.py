#!/usr/bin/env python3
"""Build an isolated reference from pinned local Git objects; never deploy it."""

import argparse
import base64
import hashlib
import io
import json
import os
from pathlib import Path
import secrets
import shutil
import subprocess
import tarfile

HERE = Path(__file__).resolve().parent
HEARBRIDGE = "cd5b7fce1b8226d8850cdd306437550098c6a62b"
SDK = "d9c9519116944a7f1c22d262012c53b80b3520b7"


def snapshot(repo, revision, destination):
    """Ignore working-tree modifications; extract the exact reviewed commit."""
    archive = subprocess.check_output(["git", "-C", str(repo), "archive", revision])
    destination.mkdir()
    with tarfile.open(fileobj=io.BytesIO(archive)) as source:
        source.extractall(destination, filter="data")


def private_write(path, text):
    with os.fdopen(os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), "w") as f:
        f.write(text)


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--hearbridge-source", required=True, type=Path)
    parser.add_argument("--sdk-source", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path, help="New private build directory")
    parser.add_argument("--llvm-config", default="llvm-config")
    parser.add_argument("--queue-pressure", action="store_true",
                        help="Include the separately tested PS5-BT queue-pressure policy")
    args = parser.parse_args()
    llvm = shutil.which(args.llvm_config)
    if not llvm:
        parser.error("LLVM with clang, ld.lld, llvm-ar and llvm-readelf is required")
    bindir = Path(subprocess.check_output([llvm, "--bindir"], text=True).strip())
    for name in ("clang", "ld.lld", "llvm-ar", "llvm-readelf"):
        if not (bindir / name).is_file():
            parser.error(f"Missing LLVM tool: {bindir / name}")
    patch = HERE / "bounded-reference.patch"
    if not patch.is_file():
        parser.error("The checked-in bounded-reference.patch is required")
    output = args.output.resolve()
    output.mkdir(mode=0o700, parents=True, exist_ok=False)
    hb = output / "hearbridge"
    sdk_source = output / "sdk-source"
    sdk = output / "sdk"
    snapshot(args.hearbridge_source.resolve(), HEARBRIDGE, hb)
    snapshot(args.sdk_source.resolve(), SDK, sdk_source)
    # The extracted tree deliberately has no .git; do not let Git discover the
    # enclosing PS5-BT repository and reinterpret paths relative to that root.
    subprocess.run(["patch", "-p1", "-t", "-i", str(patch)], cwd=hb, check=True)
    policy_hashes = {}
    pressure_patch = HERE / "queue-pressure.patch"
    if args.queue_pressure:
        policy_dir = HERE.parent / "queue-pressure"
        for name in ("ps5bt_queue_pressure.c", "ps5bt_queue_pressure.h"):
            source = policy_dir / name
            shutil.copyfile(source, hb / "src/a2dp" / name)
            policy_hashes[name] = sha256(source)
        subprocess.run(["patch", "-p1", "-t", "-i", str(pressure_patch)], cwd=hb, check=True)

    # Archive corresponding sources before generating a private credential or
    # build output. Recipients generate their own password using this script.
    with tarfile.open(output / "corresponding-source.tar.gz", "w:gz") as archive:
        archive.add(hb, arcname="hearbridge")
        archive.add(sdk_source, arcname="sdk-source")
        archive.add(HERE, arcname="recipe")
        if args.queue_pressure:
            archive.add(policy_dir, arcname="queue-pressure")
    password = secrets.token_urlsafe(24)
    authorization = base64.b64encode(f"ps5bt:{password}".encode()).decode()
    private_write(hb / "src/reference_secret.h", f'#define HB_REF_BASIC_AUTH "Basic {authorization}"\n')
    private_write(output / "access.json", json.dumps({"username": "ps5bt", "password": password}) + "\n")
    env = dict(os.environ, PS5_PAYLOAD_SDK=str(sdk), LLVM_CONFIG=llvm,
               READELF=str(bindir / "llvm-readelf"))
    with (output / "build.log").open("w") as log:
        for component in ("crt", "libc", "include", "host", "sce_stubs"):
            command = ["make", "-C", str(sdk_source / component), "install",
                       f"DESTDIR={sdk}", f"LLVM_CONFIG={llvm}"]
            if component == "sce_stubs":
                command.append("SHARED=libkernel_web.so libSceLibcInternal.so libSceNet.so libSceSystemService.so")
            print(f"Building SDK {component}", flush=True)
            subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
        print("Building reference payload", flush=True)
        subprocess.run(["make", "-f", "ps5.mk", "-j4"], cwd=hb, env=env,
                       stdout=log, stderr=subprocess.STDOUT, check=True)
    artifacts = list((hb / "dist").glob("*.elf"))
    if len(artifacts) != 1:
        raise RuntimeError("Expected exactly one separately named reference ELF")
    artifact = artifacts[0]
    manifest = {
        "kind": "isolated-hearbridge-reference", "hardware_accepted": False,
        "execution_scope": "requires explicit competing-reader/timed-credit decision",
        "hearbridge_revision": HEARBRIDGE, "sdk_revision": SDK,
        "llvm_version": subprocess.check_output([llvm, "--version"], text=True).strip(),
        "patch_sha256": sha256(patch), "elf": str(artifact.relative_to(output)),
        "elf_sha256": sha256(artifact), "elf_bytes": artifact.stat().st_size,
        "source_sha256": sha256(output / "corresponding-source.tar.gz"),
    }
    if args.queue_pressure:
        manifest["kind"] = "ps5-bt-queue-pressure-experiment"
        manifest["pressure_patch_sha256"] = sha256(pressure_patch)
        manifest["policy_sources_sha256"] = policy_hashes
    (output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(json.dumps(manifest, indent=2))
    print(f"Private access credential: {output / 'access.json'} (never upload or commit)")


if __name__ == "__main__":
    main()
