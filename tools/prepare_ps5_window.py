#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Prepare verified local PS5 candidate materials; never deploy or run a console."""
from __future__ import annotations

import argparse
import contextlib
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import uuid

import build_ps5
import diagnostic_result
import mvp_test_kit
import package_source
import target_sbc

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_PLAN = ROOT / "tools/window_test_plan.json"
RUST_VERSION = "1.90.0"
RUST_TARGET = "x86_64-unknown-none"
COMMAND_TIMEOUT = 900


class PreparationError(Exception):
    def __init__(self, code, report=None):
        self.code = code
        self.report = report
        super().__init__(code)


def run_command(command, *, root, env, capture=False):
    # A builder starts cargo/clang/patch children. A private POSIX session lets
    # interruption target that owned group, rather than only its Python leader.
    process = subprocess.Popen([str(item) for item in command], cwd=root, env=env,
                               text=True, stdout=subprocess.PIPE if capture else None,
                               stderr=subprocess.PIPE if capture else None,
                               start_new_session=os.name == "posix")
    try:
        stdout, stderr = process.communicate(timeout=30 if capture else COMMAND_TIMEOUT)
    except (subprocess.TimeoutExpired, KeyboardInterrupt):
        try:
            if os.name == "posix":
                os.killpg(process.pid, signal.SIGKILL)
            else:
                process.kill()
        except OSError:
            pass
        # Group termination/reaping is bounded. It is not proof no descendant
        # escaped the group; prepare retains its lock on either primary cause.
        try:
            process.communicate(timeout=5)
        except (OSError, subprocess.TimeoutExpired, KeyboardInterrupt):
            pass
        raise
    if process.returncode:
        raise subprocess.CalledProcessError(process.returncode, command,
                                            output=stdout, stderr=stderr)
    return stdout


def clean_source(root, expected=None):
    head, status = mvp_test_kit.git_state(root)
    if status:
        raise PreparationError("dirty-worktree")
    if expected is not None and head != expected:
        raise PreparationError("head-changed")
    return head


def validate_tools(root, llvm_config, sdk_source, plan, runner=run_command):
    """Use installed rustup run without --install; never provision a toolchain."""
    if sys.version_info < (3, 11):
        raise PreparationError("python-version")
    host_cc = shutil.which(os.environ.get("HOST_CC", "cc"))
    if host_cc is None:
        raise PreparationError("host-compiler-missing")
    codec_needed = any(profile(item)[0] == package_source.VARIANTS[key][0]
                       for item in plan["candidates"] for key in package_source.CODEC_VARIANTS)
    patch_tool = shutil.which("patch") if codec_needed else None
    if codec_needed and patch_tool is None:
        raise PreparationError("patch-tool-missing")
    rustup = shutil.which("rustup")
    if rustup is None:
        fallback = Path.home() / ".cargo/bin/rustup"
        if fallback.is_file():
            rustup = str(fallback)
    if rustup is None:
        raise PreparationError("rustup-missing")
    env = os.environ.copy()
    env["PATH"] = str(Path(rustup).parent) + os.pathsep + env.get("PATH", "")
    env["RUSTUP_TOOLCHAIN"] = RUST_VERSION
    try:
        rustc = runner([rustup, "run", RUST_VERSION, "rustc", "--version"], root=root, env=env, capture=True).strip()
        cargo = runner([rustup, "run", RUST_VERSION, "cargo", "--version"], root=root, env=env, capture=True).strip()
        targets = runner([rustup, "target", "list", "--installed", "--toolchain", RUST_VERSION], root=root, env=env, capture=True).splitlines()
        if not rustc.startswith("rustc 1.90.0 ") or not cargo.startswith("cargo 1.90.0 ") or RUST_TARGET not in targets:
            raise PreparationError("rust-version-or-target")
        components = runner([rustup, "component", "list", "--installed", "--toolchain", RUST_VERSION], root=root, env=env, capture=True).splitlines()
        if not all(any(item.startswith(name + "-") for item in components) for name in ("rustfmt", "clippy")):
            raise PreparationError("rust-components")
        # Reuse the exact builder resolution and codec tool/version checks.
        with contextlib.redirect_stdout(io.StringIO()):
            tools = build_ps5.llvm_tools(llvm_config)
            tools.update({name: tools["clang"].with_name(name) for name in ("llvm-ar", "llvm-nm")})
            for name, path in tools.items():
                target_sbc._require_llvm18(path, name)
        if sdk_source is not None:
            # Explicit source overrides are checked locally and never fetched.
            with contextlib.redirect_stdout(io.StringIO()):
                build_ps5.sdk_sources(sdk_source,
                    any(item["id"] == "usb-binding" for item in plan["candidates"]),
                    any(Path(item["artifact"]).stem.startswith("ps5-bt-application-") for item in plan["candidates"]),
                    any(item["id"] == "application-raw-openat" for item in plan["candidates"]))
    except PreparationError:
        raise
    except Exception:
        raise PreparationError("toolchain-or-sdk-unavailable") from None
    resolved_config = shutil.which(llvm_config)
    if resolved_config is None:
        raise PreparationError("llvm-config-missing")
    return {"rustc": rustc, "cargo": cargo, "target": RUST_TARGET,
            "llvm": "18.1.8", "llvm_config": str(Path(resolved_config).absolute()),
            "host_cc": str(Path(host_cc).absolute()), "patch": patch_tool,
            "tool_sha256": {name: mvp_test_kit.digest(path) for name, path in tools.items()},
            "sdk_source": str(sdk_source) if sdk_source is not None else "pinned builder cache; provisioned only with --build"}, env


def profile(candidate):
    matches = [(key, value) for key, value in package_source.VARIANTS.items()
               if value[0] + ".elf" == Path(candidate["artifact"]).name]
    if len(matches) != 1:
        raise PreparationError("unsupported-plan-profile")
    key, (stem, _, flag) = matches[0]
    expected = {"artifact": f"build/ps5/{stem}.elf", "build_manifest": f"build/ps5/{stem}.manifest.json",
                "source_bundle": f"build/ps5/{stem}-source.tar.gz"}
    if any(candidate[name] != value for name, value in expected.items()):
        raise PreparationError("plan-artifact-path")
    aliases = {"usb_metadata": "usb-binding", "capture_once": "capture-settled"}
    if candidate["id"] != aliases.get(key, key.replace("_", "-")):
        raise PreparationError("plan-candidate-profile")
    return stem, flag


def planned_paths(root, output_dir=None, kit_dir=None):
    if output_dir is not None and kit_dir is not None:
        raise PreparationError("choose-output-or-kit-directory")
    if kit_dir is not None:
        kit = Path(kit_dir).absolute()
        output = kit.with_name(kit.name + "-preparation")
    else:
        output = Path(output_dir).absolute() if output_dir is not None else root / "build/ps5" / ("window-" + uuid.uuid4().hex[:12])
        kit = output / "kit"
    for path in (output, kit):
        if path.exists() or path.is_symlink():
            raise PreparationError("output-already-exists")
        resolved = path.resolve()
        if resolved.is_relative_to(root.resolve()):
            relative = resolved.relative_to(root.resolve())
            status = subprocess.run(["git", "check-ignore", "--no-index", "-q", "--", str(relative)], cwd=root, check=False)
            if status.returncode != 0:
                raise PreparationError("in-repository-output-must-be-ignored")
    return output, kit


def commands(root, plan, plan_path, kit, llvm_config, sdk_source):
    steps = []
    for candidate in plan["candidates"]:
        _, flag = profile(candidate)
        build = [sys.executable, str(root / "tools/build_ps5.py"), "--loader-kernel-resolver", flag,
                 "--llvm-config", llvm_config]
        package = [sys.executable, str(root / "tools/package_source.py"), "--manifest", str(root / candidate["build_manifest"])]
        if sdk_source is not None:
            build += ["--sdk-source", str(sdk_source)]
            package += ["--sdk-source", str(sdk_source)]
        steps += [{"candidate": candidate["id"], "action": "build", "command": build},
                  {"candidate": candidate["id"], "action": "package", "command": package}]
    steps.append({"action": "assemble", "command": [sys.executable, str(root / "tools/mvp_test_kit.py"),
        "--assemble", "--plan", str(plan_path), "--kit-dir", str(kit)]})
    return steps


def prepare(*, root=ROOT, plan_path=DEFAULT_PLAN, llvm_config="llvm-config-18", sdk_source=None,
            output_dir=None, kit_dir=None, build=False, runner=run_command, tool_validator=validate_tools):
    root = Path(root).resolve()
    plan_path = Path(plan_path).resolve()
    head = clean_source(root)
    try:
        plan_bytes = diagnostic_result.read_bounded(plan_path, 1_048_576)
        plan = mvp_test_kit.load_plan(plan_path, root)
        if len(plan["candidates"]) > len(package_source.VARIANTS):
            raise PreparationError("plan-candidate-bound")
        for item in plan["candidates"]:
            profile(item)
    except PreparationError:
        raise
    except Exception:
        raise PreparationError("plan-invalid") from None
    output, kit = planned_paths(root, output_dir, kit_dir)
    sdk_source = Path(sdk_source).resolve() if sdk_source is not None else None
    tools, env = tool_validator(root, llvm_config, sdk_source, plan)
    steps = commands(root, plan, plan_path, kit, tools["llvm_config"], sdk_source)
    report = {"schema": 1, "status": "dry-run", "git_head": head,
              "plan_sha256": hashlib.sha256(plan_bytes).hexdigest(), "plan": str(plan_path),
              "output_directory": str(output), "kit_directory": str(kit), "tools": tools,
              "console_operations": "none; preparation never deploys or launches", "steps": steps, "completed": []}
    if not build:
        return report
    # All variants share Cargo's target archive and build/ps5 paths. Serialize
    # preparers; never remove another process's lock or retry a partial build.
    lock = root / "build/ps5/.prepare-window.lock"
    lock.parent.mkdir(parents=True, exist_ok=True)
    try:
        lock_fd = os.open(lock, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    except OSError:
        raise PreparationError("preparation-lock-present") from None
    lock_info = os.fstat(lock_fd)
    lock_identity = (lock_info.st_dev, lock_info.st_ino)
    retain_lock = False
    command_in_flight = False
    try:
        lock_data = (json.dumps({"pid": os.getpid(), "git_head": head}) + "\n").encode()
        if os.write(lock_fd, lock_data) != len(lock_data):
            raise PreparationError("preparation-lock-write")
        output.mkdir(mode=0o700, parents=True, exist_ok=False)
        diagnostic_result.write_atomic(output / "preparation-started.json", {**report, "status": "preparing"})
        try:
            for step in steps:
                clean_source(root, head)
                if diagnostic_result.read_bounded(plan_path, 1_048_576) != plan_bytes:
                    raise PreparationError("plan-changed")
                command_in_flight = True
                try:
                    runner(step["command"], root=root, env=env)
                except subprocess.CalledProcessError as error:
                    # A normal nonzero exit is known finished. A signal can
                    # leave its own children running outside the direct child.
                    command_in_flight = error.returncode < 0
                    raise
                else:
                    command_in_flight = False
                report["completed"].append({key: value for key, value in step.items() if key != "command"})
            clean_source(root, head)
            _, kit_manifest = mvp_test_kit.load_kit(kit)
            if kit_manifest.get("git_head") != head:
                raise PreparationError("assembled-kit-head")
            report["artifacts"] = kit_manifest["entries"]
            report["status"] = "ready"
        except (Exception, KeyboardInterrupt) as error:
            retain_lock = command_in_flight
            report["status"] = "failed"
            if isinstance(error, subprocess.TimeoutExpired):
                report["failure"] = "local-command-timeout"
            elif isinstance(error, KeyboardInterrupt):
                report["failure"] = "local-command-interrupted"
            elif retain_lock:
                report["failure"] = "local-command-completion-unknown"
            else:
                report["failure"] = error.code if isinstance(error, PreparationError) else "local-build-or-package-command"
            report["lock_retained"] = retain_lock
            report["command_cleanup"] = "Unknown" if retain_lock else "not_required"
            if retain_lock:
                report["remediation"] = ["Inspect the recorded preparer PID and its build descendants; reconcile shared outputs before manually removing the preparation lock. Do not retry automatically."]
            # No successful assembly/identity claim is emitted for a prefix.
        diagnostic_result.write_atomic(output / "identity.json", report)
    finally:
        os.close(lock_fd)
        # Any unexpected exit while a command was active is also uncertain,
        # including interruption while writing the failure identity itself.
        if not (retain_lock or command_in_flight):
            try:
                info = lock.stat()
                if (info.st_dev, info.st_ino) == lock_identity:
                    lock.unlink()
            except FileNotFoundError:
                pass
    return report


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    actions = parser.add_mutually_exclusive_group()
    actions.add_argument("--build", action="store_true", help="perform local builds, source packages and final kit assembly")
    actions.add_argument("--doctor", action="store_true", help="read-only preflight, identical to the default dry-run")
    parser.add_argument("--plan", type=Path, default=DEFAULT_PLAN)
    parser.add_argument("--llvm-config", default=os.environ.get("LLVM_CONFIG", "llvm-config-18"))
    parser.add_argument("--sdk-source", type=Path, default=os.environ.get("PS5_SDK_SOURCE"))
    directories = parser.add_mutually_exclusive_group()
    directories.add_argument("--output-dir", type=Path)
    directories.add_argument("--kit-dir", type=Path)
    args = parser.parse_args(argv)
    try:
        report = prepare(plan_path=args.plan, llvm_config=args.llvm_config, sdk_source=args.sdk_source,
                         output_dir=args.output_dir, kit_dir=args.kit_dir, build=args.build)
    except Exception as error:
        report = {"schema": 1, "status": "blocked", "failure": error.code if isinstance(error, PreparationError) else "local-preparation-check",
                  "remediation": ["Use a clean committed checkout.",
                    "Install Rust 1.90.0: rustup toolchain install 1.90.0 --profile minimal --component rustfmt --component clippy; rustup target add x86_64-unknown-none --toolchain 1.90.0. No automatic installation is attempted.",
                    "Provide LLVM 18.1.8 llvm-config plus clang/ld.lld/llvm-readobj/llvm-objdump/llvm-ar/llvm-nm.",
                    "Use a fresh output namespace and the reviewed plan; explicit SDK overrides must match all pinned sources."]}
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0 if report["status"] in ("dry-run", "ready") else 1


if __name__ == "__main__":
    raise SystemExit(main())
