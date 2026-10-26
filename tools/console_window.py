#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Offline-default next-window checks and bounded read-only diagnostic collection."""
from __future__ import annotations

import argparse
import ftplib
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import sys
import tempfile
import time

import diagnostic_result
import mvp_test_kit

TARGET = "192.168.1.111"
PORT = 2121
REMOTE_ROOT = "/data/pldmgr/payloads/local"
REMOTE_LOG = "/data/ps5-bt-hw1.log"
CONTROL_DIRECTORY = "/data/ps5-bt/control"
CANDIDATES = ("application-control", "application-generated", "application-observe")
DEFAULT_KIT = mvp_test_kit.ROOT / "build/ps5/next-window-kit-20261026"
DEFAULT_OUTPUT = mvp_test_kit.ROOT / "build/ps5/console-window"
MAX_META_BYTES = 16_384
MAX_ELF_BYTES = 4_194_304
MAX_LOG_BYTES = 1_048_576
SOCKET_TIMEOUT = 5
TOTAL_SECONDS = 45


class WindowError(Exception):
    """Fixed local error category; never interpolate an FTP response or secret."""
    def __init__(self, code):
        self.code = code
        self.cleanup_error = None
        super().__init__(code)


def selected_kit(kit_dir, identifiers):
    identifiers = tuple(identifiers)
    if not identifiers or len(identifiers) > 3 or len(set(identifiers)) != len(identifiers) or any(value not in CANDIDATES for value in identifiers):
        raise WindowError("candidate-selection")
    try:
        plan, manifest = mvp_test_kit.load_kit(Path(kit_dir))
        if (plan["target_host"], plan["ftp_port"], plan["remote_root"], plan["remote_log"]) != (TARGET, PORT, REMOTE_ROOT, REMOTE_LOG):
            raise WindowError("kit-target")
        entries = []
        for identifier in identifiers:
            entry = next(item for item in manifest["entries"] if item["candidate"]["id"] == identifier)
            candidate = entry["candidate"]
            expected = "ps5-bt-" + identifier
            if candidate["slot"] != expected or candidate["remote_filename"] != expected + ".elf":
                raise WindowError("kit-slot")
            local = Path(kit_dir) / identifier / candidate["remote_filename"]
            size = entry["artifact_size"]
            if type(size) is not int or not 0 < size <= MAX_ELF_BYTES or local.stat().st_size != size:
                raise WindowError("kit-artifact-size")
            if not re.fullmatch(r"[0-9a-f]{64}", entry["artifact_sha256"]):
                raise WindowError("kit-artifact-hash")
            entries.append(entry)
        return plan, entries
    except WindowError:
        raise
    except Exception:
        raise WindowError("kit-invalid") from None


class ReadOnlySession:
    def __init__(self, factory=None, clock=None):
        self.factory = factory if factory is not None else ftplib.FTP
        self.clock = clock if clock is not None else time.monotonic
        self.deadline = self.clock() + TOTAL_SECONDS
        self.ftp = None
        self.connected = False

    def check(self):
        if self.clock() >= self.deadline:
            raise WindowError("ftp-time-budget")

    def call(self, code, function, *args, **kwargs):
        self.check()
        try:
            value = function(*args, **kwargs)
        except WindowError:
            raise
        except Exception:
            raise WindowError(code) from None
        self.check()
        return value

    def open(self):
        try:
            self.ftp = self.factory()
        except Exception:
            raise WindowError("ftp-open") from None
        self.call("ftp-connect", self.ftp.connect, TARGET, PORT, timeout=SOCKET_TIMEOUT)
        self.connected = True
        self.call("ftp-login", self.ftp.login, "anonymous", "ps5-bt@localhost")

    def retrieve(self, remote, maximum, consume):
        used = 0
        def append(block):
            nonlocal used
            self.check()
            if not isinstance(block, bytes) or used + len(block) > maximum:
                raise WindowError("remote-size-bound")
            used += len(block)
            consume(block)
        self.call("ftp-read", self.ftp.retrbinary, "RETR " + remote, append, blocksize=65_536)
        return used

    def finish(self):
        ftp, self.ftp = self.ftp, None
        if ftp is None:
            return None
        if self.connected:
            try:
                ftp.quit()  # ftplib.quit closes on a successful response.
                return None
            except Exception:
                pass
        try:
            ftp.close()
        except Exception:
            return "ftp-cleanup"
        return "ftp-cleanup" if self.connected else None


def prerequisite():
    return {"directory": CONTROL_DIRECTORY, "required_mode": "0700",
            "native_owner_and_mode": "Unknown", "validation": "native payload required"}


def preflight(kit_dir, identifiers=CANDIDATES, execute=False, verify_hash=False, ftp_factory=None, clock=None):
    _, entries = selected_kit(kit_dir, identifiers)
    result = {"action": "preflight", "mode": "execute-read-only" if execute else "dry-run",
              "target": f"{TARGET}:{PORT}", "prerequisite": prerequisite(), "candidates": []}
    for entry in entries:
        candidate = entry["candidate"]
        result["candidates"].append({"id": candidate["id"], "slot": candidate["slot"],
            "filename": candidate["remote_filename"], "expected_sha256": entry["artifact_sha256"],
            "expected_size": entry["artifact_size"], "slot_metadata": "Unknown", "remote_size_match": "Unknown",
            "remote_hash_match": "Unknown", "hash_readback_planned": bool(verify_hash)})
    if not execute:
        return result
    session = ReadOnlySession(ftp_factory, clock)
    error = None
    try:
        session.open()
        for entry, report in zip(entries, result["candidates"], strict=True):
            candidate = entry["candidate"]
            session.call("ftp-slot", session.ftp.cwd, REMOTE_ROOT + "/" + candidate["slot"])
            data = bytearray()
            session.retrieve("meta.json", MAX_META_BYTES, data.extend)
            try:
                meta = json.loads(data, object_pairs_hook=diagnostic_result.unique_object,
                                  parse_constant=diagnostic_result.reject_constant)
            except Exception:
                raise WindowError("slot-metadata-format") from None
            if not isinstance(meta, dict) or type(meta.get("schema")) is not int or meta.get("schema") != 2 or meta.get("kind") != "local" or meta.get("filename") != candidate["remote_filename"]:
                raise WindowError("slot-metadata-mismatch")
            report["slot_metadata"] = "matching"
            session.call("ftp-binary", session.ftp.voidcmd, "TYPE I")
            size = session.call("ftp-size", session.ftp.size, candidate["remote_filename"])
            if type(size) is not int or size != entry["artifact_size"]:
                raise WindowError("remote-artifact-size")
            report["remote_size_match"] = True
            if verify_hash:
                value = hashlib.sha256()
                length = session.retrieve(candidate["remote_filename"], min(MAX_ELF_BYTES, entry["artifact_size"]), value.update)
                if length != entry["artifact_size"] or value.hexdigest() != entry["artifact_sha256"]:
                    raise WindowError("remote-artifact-hash")
                report["remote_hash_match"] = True
    except WindowError as caught:
        error = caught
    finally:
        cleanup = session.finish()
    if error is not None:
        error.cleanup_error = cleanup
        raise error
    if cleanup:
        raise WindowError(cleanup)
    return result


def private_directory(directory):
    try:
        directory = Path(directory).absolute()
        directory.mkdir(mode=0o700, parents=True, exist_ok=True)
        fd = os.open(directory, os.O_RDONLY | getattr(os, "O_DIRECTORY", 0) | getattr(os, "O_NOFOLLOW", 0))
        try:
            info = os.fstat(fd)
            if not stat.S_ISDIR(info.st_mode) or info.st_mode & 0o777 != 0o700 or info.st_uid != os.geteuid():
                raise WindowError("local-directory-protection")
        finally:
            os.close(fd)
        return directory
    except WindowError:
        raise
    except Exception:
        raise WindowError("local-directory") from None


def publish_log(temporary, complete):
    destination = temporary.with_suffix(".log" if complete else ".partial.log")
    fd = None
    try:
        os.link(temporary, destination)
        temporary.unlink()
        fd = os.open(destination.parent, os.O_RDONLY | getattr(os, "O_DIRECTORY", 0))
        os.fsync(fd)
        return destination
    except Exception:
        raise WindowError("local-log-publication") from None
    finally:
        if fd is not None:
            os.close(fd)


def collect_window(kit_dir, identifier, execute=False, output_dir=DEFAULT_OUTPUT,
                   observations=None, run_observation="Unknown", ftp_factory=None, clock=None):
    _, entries = selected_kit(kit_dir, [identifier])
    if observations is None:
        observations = {phase: "Unknown" for phase in ("before", "during", "after")}
    if set(observations) != {"before", "during", "after"} or any(value not in diagnostic_result.OBSERVATIONS for value in observations.values()) or run_observation not in ("Unknown", "observed"):
        raise WindowError("operator-observations")
    entry = entries[0]
    result = {"action": "collect", "mode": "execute-read-only" if execute else "dry-run",
              "candidate": identifier, "remote_log": REMOTE_LOG, "output_directory": str(output_dir),
              "controller": dict(observations), "operator_run_observation": run_observation,
              "expected_sha256": entry["artifact_sha256"]}
    if not execute:
        return result
    manifest_path = Path(kit_dir) / identifier / Path(entry["candidate"]["build_manifest"]).name
    try:
        manifest_data = diagnostic_result.read_bounded(manifest_path, diagnostic_result.MAX_MANIFEST_BYTES)
        diagnostic_result.manifest_identity(manifest_data)
    except Exception:
        raise WindowError("collector-manifest") from None
    directory = private_directory(output_dir)
    session = ReadOnlySession(ftp_factory, clock)
    error = None
    try:
        fd, name = tempfile.mkstemp(prefix=identifier + "-", suffix=".part", dir=directory)
    except Exception:
        raise WindowError("local-log-open") from None
    temporary = Path(name)
    try:
        with os.fdopen(fd, "wb") as output:
            try:
                session.open()
                def save(block):
                    try:
                        if output.write(block) != len(block):
                            raise WindowError("local-log-write")
                    except Exception:
                        raise WindowError("local-log-write") from None
                session.retrieve(REMOTE_LOG, MAX_LOG_BYTES, save)
            except WindowError as caught:
                error = caught.code
            finally:
                cleanup = session.finish()
            output.flush()
            os.fsync(output.fileno())
    except Exception:
        # A private partial file is retained if persistence itself failed.
        session.finish()
        raise WindowError("local-log-persistence") from None
    log_path = publish_log(temporary, error is None)
    try:
        if mvp_test_kit.digest(manifest_path) != entry["build_manifest_sha256"]:
            raise WindowError("kit-changed-during-collection")
        record = diagnostic_result.collect(manifest_path, log_path, observations)
    except WindowError:
        raise
    except Exception:
        record = {"schema": 1, "classification": "incomplete_evidence",
                  "identity": diagnostic_result.manifest_identity(manifest_data),
                  "input_sha256": {"manifest": hashlib.sha256(manifest_data).hexdigest(),
                                    "log": mvp_test_kit.digest(log_path)},
                  "controller": dict(observations), "ps5_persistence": "Unknown", "audio_acceptance": "Unknown",
                  "issues": ["collector_rejected_saved_log"]}
    record["collection"] = {"protocol": "FTP read-only", "transfer_complete": error is None,
        "error": error, "cleanup_error": cleanup, "operator_run_observation": run_observation,
        "saved_log": log_path.name, "ps5_log_freshness": "Unknown"}
    if error is not None or cleanup:
        record["diagnostic_classification"] = record["classification"]
        record["classification"] = "interrupted" if log_path.stat().st_size else "collection_failed"
    result_path = log_path.with_suffix(".json")
    try:
        diagnostic_result.write_atomic(result_path, record)
    except Exception:
        raise WindowError("local-result-persistence") from None
    result.update(classification=record["classification"], log=str(log_path), record=str(result_path))
    return result


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    actions = parser.add_mutually_exclusive_group(required=True)
    actions.add_argument("--preflight", action="store_true")
    actions.add_argument("--collect", action="store_true")
    parser.add_argument("--kit", type=Path, default=DEFAULT_KIT)
    parser.add_argument("--candidate", action="append", choices=CANDIDATES)
    parser.add_argument("--execute", action="store_true", help="explicit read-only FTP operation; never upload or launch")
    parser.add_argument("--verify-hash", action="store_true", help="preflight bounded remote ELF hash readback")
    parser.add_argument("--output-dir", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--run-observation", choices=("Unknown", "observed"), default="Unknown")
    for phase in ("before", "during", "after"):
        parser.add_argument("--controller-" + phase, choices=sorted(diagnostic_result.OBSERVATIONS), default="Unknown")
    args = parser.parse_args(argv)
    if args.collect and (not args.candidate or len(args.candidate) != 1):
        parser.error("--collect requires exactly one --candidate")
    if args.collect and args.verify_hash:
        parser.error("--verify-hash applies only to --preflight")
    try:
        if args.preflight:
            report = preflight(args.kit, args.candidate or CANDIDATES, args.execute, args.verify_hash)
        else:
            report = collect_window(args.kit, args.candidate[0], args.execute, args.output_dir,
                {phase: getattr(args, "controller_" + phase) for phase in ("before", "during", "after")}, args.run_observation)
        print(json.dumps(report, indent=2, sort_keys=True))
        if args.collect and args.execute and report["classification"] != "complete_success":
            return 2
        return 0
    except WindowError as error:
        suffix = "; cleanup=" + error.cleanup_error if error.cleanup_error else ""
        print("console window check failed: " + error.code + suffix, file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
