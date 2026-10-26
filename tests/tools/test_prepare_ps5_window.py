#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Local Git/plan/kit validation with mocked build/package subprocess work."""
import contextlib
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import time
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tools"))
import prepare_ps5_window as preparer

REPO = Path(__file__).resolve().parents[2]


def sha(data):
    return hashlib.sha256(data).hexdigest()


def git(root, *args):
    return subprocess.run(["git", *args], cwd=root, check=True, capture_output=True, text=True).stdout.strip()


class LocalCommands:
    def __init__(self, root):
        self.root = root
        self.head = git(root, "rev-parse", "HEAD")
        self.calls = []
        self.fail_at = None
        self.change_source = False

    def __call__(self, command, *, root, env, capture=False):
        assert root == self.root and not capture
        assert all(isinstance(item, str) for item in command)
        self.calls.append(command)
        if self.fail_at == len(self.calls):
            raise subprocess.CalledProcessError(1, command)
        tool = Path(command[1]).name
        if tool == "build_ps5.py":
            key, (stem, _, flag) = next((key, value) for key, value in preparer.package_source.VARIANTS.items() if value[2] in command)
            artifact = root / "build/ps5" / (stem + ".elf")
            artifact.parent.mkdir(parents=True, exist_ok=True)
            artifact.write_bytes(("synthetic-ELF-" + key).encode())
            archive = root / "target/x86_64-unknown-none/release/libps5_runtime.a"
            archive.parent.mkdir(parents=True, exist_ok=True)
            archive.write_bytes(("fresh-Rust-archive-" + key).encode())
            data = {"git_head": self.head, "git_status": [], "sha256": sha(artifact.read_bytes()),
                    "size": artifact.stat().st_size, key: True, "loader_kernel_resolver": True,
                    "runtime": "minimal-syscall-v6", "diagnostic_build_id": "a" * 64,
                    "diagnostic_candidate": stem, "rust_archive_sha256": sha(archive.read_bytes())}
            artifact.with_suffix(".manifest.json").write_text(json.dumps(data))
            if self.change_source:
                (root / "tracked.txt").write_text("changed source")
        elif tool == "package_source.py":
            path = Path(command[command.index("--manifest") + 1])
            data = json.loads(path.read_text())
            archive = root / "target/x86_64-unknown-none/release/libps5_runtime.a"
            assert sha(archive.read_bytes()) == data["rust_archive_sha256"], "package ran after another build overwrote Rust archive"
            destination = path.with_name(path.name.replace(".manifest.json", "-source.tar.gz"))
            payloads = {"ps5-bt-source/build-manifest.json": path.read_bytes()}
            if any(data.get(key) for key in preparer.package_source.CODEC_VARIANTS):
                payloads.update({"ps5-bt-source/relink/relink.py": b"synthetic replacement script",
                                 "ps5-bt-source/relink/bundle-manifest.json": b"{}",
                                 "ps5-bt-source/project/third_party/sbc/upstream/COPYING.LIB": b"synthetic license"})
            with tarfile.open(destination, "w:gz") as output:
                for name, value in payloads.items():
                    entry = tarfile.TarInfo(name); entry.size = len(value)
                    output.addfile(entry, io.BytesIO(value))
        else:
            assert tool == "mvp_test_kit.py" and "--assemble" in command
            plan = Path(command[command.index("--plan") + 1])
            kit = Path(command[command.index("--kit-dir") + 1])
            with patch.object(preparer.mvp_test_kit, "ROOT", root), patch.object(preparer.mvp_test_kit, "git_state", return_value=(self.head, [])), contextlib.redirect_stdout(io.StringIO()):
                preparer.mvp_test_kit.assemble(plan, kit)
        return ""


class WindowPreparerTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / "clone"; self.root.mkdir(); self.root = self.root.resolve()
        git(self.root, "init", "-q")
        (self.root / ".gitignore").write_text("/build/\n/target/\n")
        (self.root / "tracked.txt").write_text("original")
        plan = json.loads((REPO / "tools/mvp_test_plan.json").read_text())
        selected = [candidate for candidate in plan["candidates"] if candidate["id"] in ("application-control", "sbc-continuous")]
        for index, candidate in enumerate(selected, 1): candidate["order"] = index
        plan["candidates"] = selected
        self.plan = self.root / "plan.json"; self.plan.write_text(json.dumps(plan))
        git(self.root, "add", ".")
        git(self.root, "-c", "user.name=Fixture", "-c", "user.email=fixture@localhost", "commit", "-qm", "Clean fixture")
        self.output = self.root / "build/ps5/test-window"
        self.commands = LocalCommands(self.root)

    def tools(self, root, llvm, sdk, plan):
        return {"rustc": "rustc 1.90.0 (fixture)", "cargo": "cargo 1.90.0 (fixture)",
                "llvm": "18.1.8", "llvm_config": "/verified LLVM/bin/llvm-config"}, {"PATH": "fixture", "RUSTUP_TOOLCHAIN": "1.90.0"}

    def prepare(self, **kwargs):
        return preparer.prepare(root=self.root, plan_path=self.plan, output_dir=self.output,
                                runner=self.commands, tool_validator=self.tools, **kwargs)

    def test_dry_run_uses_real_plan_and_clean_git_without_build_or_output(self):
        report = self.prepare()
        self.assertEqual(report["status"], "dry-run")
        self.assertEqual([step["action"] for step in report["steps"]], ["build", "package", "build", "package", "assemble"])
        self.assertEqual(self.commands.calls, [])
        self.assertFalse(self.output.exists())
        self.assertEqual(report["git_head"], git(self.root, "rev-parse", "HEAD"))
        self.assertIn("none", report["console_operations"])

    def test_build_packages_immediately_then_uses_real_kit_assembly_and_identity(self):
        report = self.prepare(build=True)
        self.assertEqual(report["status"], "ready")
        self.assertEqual(len(self.commands.calls), 5)
        for index in (0, 2):
            self.assertEqual(Path(self.commands.calls[index][1]).name, "build_ps5.py")
            self.assertEqual(Path(self.commands.calls[index + 1][1]).name, "package_source.py")
        self.assertEqual(len(report["artifacts"]), 2)
        self.assertEqual(json.loads((self.output / "identity.json").read_text())["status"], "ready")
        self.assertEqual(json.loads((self.output / "preparation-started.json").read_text())["status"], "preparing")
        self.assertFalse((self.root / "build/ps5/.prepare-window.lock").exists())
        self.assertEqual(git(self.root, "status", "--porcelain"), "")

    def test_failed_build_or_package_stops_without_assembling_or_claiming_success(self):
        for failure in (1, 2, 3, 4):
            output = self.root / "build/ps5" / ("failure-" + str(failure))
            commands = LocalCommands(self.root); commands.fail_at = failure
            report = preparer.prepare(root=self.root, plan_path=self.plan, output_dir=output, build=True,
                                      runner=commands, tool_validator=self.tools)
            self.assertEqual(report["status"], "failed")
            self.assertEqual(len(commands.calls), failure)
            self.assertFalse((output / "kit").exists())
            self.assertEqual(json.loads((output / "identity.json").read_text())["status"], "failed")
            self.assertFalse((self.root / "build/ps5/.prepare-window.lock").exists())

    def test_dirty_or_changed_head_and_plan_fail_closed(self):
        (self.root / "untracked.txt").write_text("unrelated user work")
        with self.assertRaises(preparer.PreparationError) as error: self.prepare(build=True)
        self.assertEqual(error.exception.code, "dirty-worktree")
        self.assertEqual(self.commands.calls, [])
        self.assertFalse(self.output.exists())
        (self.root / "untracked.txt").unlink()
        self.commands.change_source = True
        report = self.prepare(build=True)
        self.assertEqual(report["status"], "failed")
        self.assertEqual(report["failure"], "dirty-worktree")
        self.assertEqual(len(self.commands.calls), 1)

    def test_output_collision_unignored_path_and_lock_are_never_overwritten(self):
        self.output.mkdir(parents=True); marker = self.output / "user.txt"; marker.write_text("keep")
        with self.assertRaises(preparer.PreparationError): self.prepare(build=True)
        self.assertEqual(marker.read_text(), "keep")
        with self.assertRaises(preparer.PreparationError) as error:
            preparer.prepare(root=self.root, plan_path=self.plan, output_dir=self.root / "not-ignored", tool_validator=self.tools)
        self.assertEqual(error.exception.code, "in-repository-output-must-be-ignored")
        lock = self.root / "build/ps5/.prepare-window.lock"; lock.write_text("owned elsewhere")
        with self.assertRaises(preparer.PreparationError):
            preparer.prepare(root=self.root, plan_path=self.plan, output_dir=self.root / "build/ps5/another", build=True, tool_validator=self.tools)
        self.assertEqual(lock.read_text(), "owned elsewhere")
        self.assertEqual(self.commands.calls, [])

    def test_failed_lock_initialization_closes_and_releases_only_its_owned_lock(self):
        with patch.object(preparer.os, "write", side_effect=OSError("fixture disk failure")):
            with self.assertRaises(OSError): self.prepare(build=True)
        self.assertFalse((self.root / "build/ps5/.prepare-window.lock").exists())
        self.assertFalse(self.output.exists())
        self.assertEqual(self.commands.calls, [])

    @unittest.skipUnless(os.name == "posix", "owned process groups require POSIX")
    def test_real_timeout_kills_owned_group_and_retains_lock_against_racing_retry(self):
        started = self.root / "build/child-started"
        escaped_write = self.root / "build/child-after-timeout"
        child = ("import sys,time;from pathlib import Path;"
                 "Path(sys.argv[1]).write_text('started');time.sleep(0.8);"
                 "Path(sys.argv[2]).write_text('late')")
        parent = ("import subprocess,sys;"
                  "subprocess.run([sys.executable,'-c',sys.argv[1],sys.argv[2],sys.argv[3]],check=True)")
        calls = []
        def actual_command(command, *, root, env, capture=False):
            calls.append(command)
            return preparer.run_command([sys.executable, "-c", parent, child,
                                         str(started), str(escaped_write)],
                                        root=root, env=os.environ.copy(), capture=capture)
        with patch.object(preparer, "COMMAND_TIMEOUT", 0.4):
            report = preparer.prepare(root=self.root, plan_path=self.plan,
                                      output_dir=self.output, build=True,
                                      runner=actual_command, tool_validator=self.tools)
        self.assertTrue(started.exists(), "real grandchild must have started")
        time.sleep(0.6)
        self.assertFalse(escaped_write.exists(), "owned group survived termination")
        self.assertEqual(report["status"], "failed")
        self.assertEqual(report["failure"], "local-command-timeout")
        self.assertTrue(report["lock_retained"])
        self.assertEqual(report["command_cleanup"], "Unknown")
        self.assertEqual(report["completed"], [])
        self.assertFalse((self.output / "kit").exists())
        self.assertTrue((self.root / "build/ps5/.prepare-window.lock").exists())
        saved = json.loads((self.output / "identity.json").read_text())
        self.assertEqual(saved["failure"], "local-command-timeout")
        self.assertTrue(saved["lock_retained"])
        with self.assertRaises(preparer.PreparationError) as caught:
            preparer.prepare(root=self.root, plan_path=self.plan,
                             output_dir=self.root / "build/another-attempt", build=True,
                             runner=actual_command, tool_validator=self.tools)
        self.assertEqual(caught.exception.code, "preparation-lock-present")
        self.assertEqual(len(calls), 1)

    def test_interruption_signal_and_unknown_completion_keep_lock_and_failure_identity(self):
        for index, error in enumerate((KeyboardInterrupt(),
                                       subprocess.CalledProcessError(-9, ["fixture"]),
                                       RuntimeError("fixture uncertain completion"))):
            output = self.root / "build" / ("uncertain-" + str(index))
            def failed_command(command, **kwargs):
                raise error
            report = preparer.prepare(root=self.root, plan_path=self.plan,
                                      output_dir=output, build=True,
                                      runner=failed_command, tool_validator=self.tools)
            self.assertEqual(report["status"], "failed")
            self.assertTrue(report["lock_retained"])
            self.assertEqual(report["command_cleanup"], "Unknown")
            self.assertEqual(report["failure"], "local-command-interrupted" if index == 0
                             else "local-command-completion-unknown")
            self.assertEqual(json.loads((output / "identity.json").read_text())["status"], "failed")
            lock = self.root / "build/ps5/.prepare-window.lock"
            self.assertTrue(lock.exists())
            # Only this synthetic test reconciles its own callback (no child).
            lock.unlink()

    def test_exact_kit_directory_and_external_sdk_paths_are_argument_safe(self):
        kit = self.root / "build/ps5/exact kit"
        report = preparer.prepare(root=self.root, plan_path=self.plan, kit_dir=kit,
                                  sdk_source=self.root / "SDK with spaces", tool_validator=self.tools)
        self.assertEqual(report["kit_directory"], str(kit))
        self.assertEqual(report["output_directory"], str(kit) + "-preparation")
        for step in report["steps"][:-1]:
            index = step["command"].index("--sdk-source")
            self.assertEqual(step["command"][index + 1], str(self.root / "SDK with spaces"))
        self.assertIn("--kit-dir", report["steps"][-1]["command"])

    def test_profiles_derive_existing_flags_and_reject_changed_plan_paths(self):
        startup = preparer.mvp_test_kit.load_plan(REPO / "tools/startup_test_plan.json", REPO)
        for candidate in startup["candidates"]:
            stem, flag = preparer.profile(candidate)
            self.assertEqual(flag, "--" + candidate["id"])
            self.assertEqual(stem, "ps5-bt-" + candidate["id"])
        changed = dict(startup["candidates"][0], artifact="build/ps5/elsewhere.elf")
        with self.assertRaises(preparer.PreparationError): preparer.profile(changed)

    def test_new_application_profile_is_discovered_without_a_preparer_flag_table(self):
        candidate = dict(json.loads(self.plan.read_text())["candidates"][0])
        stem = "ps5-bt-application-setup"
        candidate.update(id="application-setup", artifact=f"build/ps5/{stem}.elf",
                         build_manifest=f"build/ps5/{stem}.manifest.json", source_bundle=f"build/ps5/{stem}-source.tar.gz")
        with patch.dict(preparer.package_source.VARIANTS, application_setup=(stem, set(), "--application-setup")):
            self.assertEqual(preparer.profile(candidate), (stem, "--application-setup"))

    def test_namespace_plan_build_package_assemble_uses_fixed_log_and_profile(self):
        self.plan.write_bytes((REPO / "tools/namespace_test_plan.json").read_bytes())
        git(self.root, "add", "plan.json")
        git(self.root, "-c", "user.name=Fixture", "-c", "user.email=fixture@localhost", "commit", "-qm", "test: select namespace")
        self.commands = LocalCommands(self.root)
        report = self.prepare(build=True)
        self.assertEqual(report["status"], "ready")
        self.assertEqual(len(self.commands.calls), 3)
        self.assertIn("--application-namespace", self.commands.calls[0])
        plan, _ = preparer.mvp_test_kit.load_kit(self.output / "kit")
        self.assertEqual(plan["remote_log"], preparer.mvp_test_kit.NAMESPACE_LOG)
        self.assertEqual([item["id"] for item in plan["candidates"]], ["application-namespace"])

    def test_external_plan_change_stops_before_packaging_and_preserves_failure_identity(self):
        external = self.root.parent / "external-plan.json"
        external.write_bytes(self.plan.read_bytes())
        base = self.commands
        def changed(command, **kwargs):
            result = base(command, **kwargs)
            external.write_bytes(external.read_bytes() + b"\n")
            return result
        report = preparer.prepare(root=self.root, plan_path=external, output_dir=self.output, build=True,
                                  runner=changed, tool_validator=self.tools)
        self.assertEqual(report["status"], "failed")
        self.assertEqual(report["failure"], "plan-changed")
        self.assertEqual(len(base.calls), 1)
        self.assertFalse((self.output / "kit").exists())
        self.assertEqual(json.loads((self.output / "identity.json").read_text())["status"], "failed")

    def test_tool_validation_uses_real_builder_llvm_checks_and_never_installs(self):
        binaries = self.root / "build/fake-tools"; binaries.mkdir(parents=True)
        config = binaries / "llvm-config"
        config.write_text('#!/bin/sh\nif [ "$1" = "--bindir" ]; then dirname "$0"; else echo 18.1.8; fi\n'); config.chmod(0o755)
        for name in ("clang", "ld.lld", "llvm-readobj", "llvm-objdump", "llvm-ar", "llvm-nm"):
            path = binaries / name; path.write_text('#!/bin/sh\necho "LLVM version 18.1.8"\n'); path.chmod(0o755)
        rustup = binaries / "rustup"; rustup.write_text("fixture")
        calls = []
        def query(command, **kwargs):
            calls.append(command)
            if "rustc" in command: return "rustc 1.90.0 (fixture)\n"
            if "cargo" in command: return "cargo 1.90.0 (fixture)\n"
            if "target" in command: return "x86_64-unknown-none\n"
            return "rustfmt-host\nclippy-host\n"
        original_which = preparer.shutil.which
        def which(name):
            return str(rustup) if name == "rustup" else original_which(name)
        plan = preparer.mvp_test_kit.load_plan(self.plan, self.root)
        with patch.object(preparer.shutil, "which", side_effect=which):
            summary, env = preparer.validate_tools(self.root, str(config), None, plan, runner=query)
            self.assertEqual(summary["llvm"], "18.1.8")
            self.assertEqual(len(summary["tool_sha256"]), 6)
            self.assertEqual(env["RUSTUP_TOOLCHAIN"], "1.90.0")
            self.assertFalse(any("--install" in call or "add" in call for call in calls))
            def missing_target(command, **kwargs):
                if "target" in command: return "aarch64-apple-darwin\n"
                return query(command, **kwargs)
            with self.assertRaises(preparer.PreparationError) as caught:
                preparer.validate_tools(self.root, str(config), None, plan, runner=missing_target)
            self.assertEqual(caught.exception.code, "rust-version-or-target")
            def missing_component(command, **kwargs):
                if "component" in command: return "rustfmt-host\n"
                return query(command, **kwargs)
            with self.assertRaises(preparer.PreparationError) as caught:
                preparer.validate_tools(self.root, str(config), None, plan, runner=missing_component)
            self.assertEqual(caught.exception.code, "rust-components")
            (binaries / "llvm-nm").write_text('#!/bin/sh\necho "LLVM version 17.0.0"\n')
            with self.assertRaises(preparer.PreparationError):
                preparer.validate_tools(self.root, str(config), None, plan, runner=query)


if __name__ == "__main__":
    unittest.main()
