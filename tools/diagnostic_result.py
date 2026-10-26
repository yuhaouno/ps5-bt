#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Collect a bounded local diagnostic log without copying arbitrary log data."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import sys
import tempfile

MAX_MANIFEST_BYTES = 1_048_576
MAX_LOG_BYTES = 1_048_576
MAX_LINE_BYTES = 2048
MAX_LINES = 16_384
HEX64 = re.compile(r"[0-9a-f]{64}\Z")
VARIANTS = {
    "sbc_continuous": "ps5-bt-sbc-continuous",
    "capture_stream": "ps5-bt-capture-stream",
    "application_control": "ps5-bt-application-control",
}
ALL_VARIANTS = ("native_query", "native_l2cap_probe", "usb_metadata", "capture_preflight",
                "capture_once", "capture_observe", "capture_stream", "sbc_selftest", "sbc_continuous", "application_control")
OBSERVATIONS = {"Unknown", "normal", "changed"}
SBC_CONFIG = "event=sbc_continuous_begin schema=1 generated=1 nominal_clock=synthetic sink=discard"
CAPTURE_CONFIG = "event=capture_stream_begin schema=1 reads_max=1 format=Unknown native_time=Unknown"
SBC_KEYS = {"outcome", "blocks", "pcm_frames", "bytes", "elapsed_us_hi", "elapsed_us_lo",
            "prefix", "digest", "cleanup_attempted"}
CAPTURE_KEYS = {"stage", "status_bits", "raw_status_bits", "lookup_hi", "lookup_lo",
                "cleanup_flags", "raw_stop_bits", "raw_close_bits", "raw_terminate_bits",
                "owner_error", "clock_bits", "kind", "read_status_bits", "bytes", "discontinuity",
                "stop_bits", "close_bits"}


APPLICATION_CONFIG = "event=application_begin schema=1 mode=control-only transport=Unavailable capture=Unavailable native_format=Unknown codec=Unavailable"
APPLICATION_PREFLIGHT = "event=application_preflight_begin schema=1 radio=Unavailable capture=Unavailable"
APPLICATION_BUDGET = {"duration_us": 5_000_000, "stop_timeout_us": 100_000,
                      "max_turns": 20_000, "max_waits": 10_000, "max_clients": 64}
APPLICATION_KEYS = set(APPLICATION_BUDGET) | {
    "result", "drive_entered", "factory_entered", "stop_reason", "drive_failure",
    "clock_calls", "turns", "waits", "cleanup_turns", "forced", "finished", "stop_error_bits",
    "fault_resource", "fault_bits", "fault_lane", "failed_resource", "error_stage", "error_bits",
    "native_status_bits", "error_flags", "lookup_hi", "lookup_lo", "native_cleanup_bits",
    "cleanup_uncertain", "cleanup_error_bits", "clients", "client_failures", "listener_status_bits",
    "listener_errno_bits", "listener_cleanup_bits", "listener_flags", "protocols_closed",
    "required_mask", "active_mask", "cleanup_mask", "cleanup_failed_mask",
}
IMPORT_KEYS = ("stage", "status_bits", "errno", "lookup_hi", "lookup_lo", "return_hi", "return_lo",
               "resolved", "cleanup_flags", "cleanup_errno")
HTTP_KEYS = ("failure_source", "failure_cause", "failure_raw_bits", "cleanup_present", "cleanup_bits")


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def read_bounded(path: Path, maximum: int) -> bytes:
    """Read regular local files only, bounded even if the file grows after stat."""
    fd = os.open(path, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_NONBLOCK", 0))
    with os.fdopen(fd, "rb") as source:
        info = os.fstat(source.fileno())
        if not stat.S_ISREG(info.st_mode) or info.st_size > maximum:
            raise ValueError("input must be a bounded regular file")
        data = source.read(maximum + 1)
        if len(data) > maximum or len(data) != info.st_size:
            raise ValueError("input changed size or exceeded its byte bound")
        return data


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate manifest JSON key")
        result[key] = value
    return result


def reject_constant(_):
    raise ValueError("nonfinite manifest number")


def manifest_identity(data: bytes) -> dict:
    try:
        manifest = json.loads(data, object_pairs_hook=unique_object,
                              parse_constant=reject_constant)
    except (json.JSONDecodeError, UnicodeDecodeError, RecursionError) as error:
        raise ValueError("invalid manifest JSON") from error
    if not isinstance(manifest, dict):
        raise ValueError("manifest must be an object")
    build_id = manifest.get("diagnostic_build_id")
    candidate = manifest.get("diagnostic_candidate")
    if not isinstance(build_id, str) or not HEX64.fullmatch(build_id):
        raise ValueError("manifest has no valid embedded diagnostic identity; legacy mode unsupported")
    active = []
    for flag in ALL_VARIANTS:
        value = manifest.get(flag, False)
        if not isinstance(value, bool):
            raise ValueError("manifest variant flags must be booleans")
        if value:
            active.append(flag)
    if len(active) != 1 or VARIANTS.get(active[0]) != candidate:
        raise ValueError("manifest candidate does not match its sole supported variant")
    artifact_hash = manifest.get("sha256")
    git_head = manifest.get("git_head")
    size = manifest.get("size")
    if not isinstance(artifact_hash, str) or not HEX64.fullmatch(artifact_hash):
        raise ValueError("invalid manifest artifact hash")
    if not isinstance(git_head, str) or not re.fullmatch(r"[0-9a-f]{40}", git_head):
        raise ValueError("invalid manifest source revision")
    if isinstance(size, bool) or not isinstance(size, int) or not 0 < size <= 67_108_864:
        raise ValueError("invalid manifest artifact size")
    source_status = manifest.get("git_status")
    if not isinstance(source_status, list):
        raise ValueError("manifest must record source status")
    return {"build_id": build_id, "candidate": candidate,
            "artifact_sha256": artifact_hash, "artifact_size": size,
            "git_head": git_head, "source_clean": source_status == []}


def application_report(body: list[str]) -> dict:
    """Control-only runtime and its separately owned C import/entropy lifecycle."""
    numbers: dict[str, list[int]] = {}
    imports: list[dict[str, int]] = []
    import_positions: list[list[int]] = []
    http: list[dict[str, int]] = []
    http_positions: list[int] = []
    positions: dict[str, list[int]] = {}
    issues = []
    for index, line in enumerate(body):
        match = re.fullmatch(r"application_([a-z_]+)=0x([0-9a-f]{8})", line)
        if match:
            key, value = match[1], int(match[2], 16)
            if key.startswith("import_") and key[7:] in IMPORT_KEYS:
                key = key[7:]
                if key == "stage":
                    imports.append({})
                    import_positions.append([])
                    if len(imports) > 3:
                        raise ValueError("too many application import reports")
                if not imports or key in imports[-1]:
                    issues.append("malformed_import_report")
                else:
                    imports[-1][key] = value
                    import_positions[-1].append(index)
            elif key.startswith("http_") and key[5:] in HTTP_KEYS:
                key = key[5:]
                if key == "failure_source":
                    http.append({})
                    if len(http) > 4:
                        raise ValueError("too many application HTTP reports")
                if not http or key in http[-1]:
                    issues.append("malformed_http_report")
                else:
                    http[-1][key] = value
                    http_positions.append(index)
            elif key in APPLICATION_KEYS:
                numbers.setdefault(key, []).append(value)
                positions.setdefault(key, []).append(index)
                if len(numbers[key]) > 8:
                    raise ValueError("too many repeated application numeric records")
        elif line.startswith("application_"):
            key = line.split("=", 1)[0][12:]
            if key in APPLICATION_KEYS or (key.startswith("import_") and key[7:] in IMPORT_KEYS) or (key.startswith("http_") and key[5:] in HTTP_KEYS):
                issues.append("malformed_numeric_marker")
                if key.startswith("http_") and key[5:] in HTTP_KEYS:
                    issues.append("malformed_http_report")

    def one(key):
        values = numbers.get(key, [])
        return values[0] if len(values) == 1 else None

    preflight_starts = [i for i, line in enumerate(body) if line.startswith("event=application_preflight_begin")]
    runtime_starts = [i for i, line in enumerate(body) if line.startswith("event=application_begin")]
    completions = [i for i, line in enumerate(body) if line == "event=application_complete"]
    cleanup_events = [i for i, line in enumerate(body) if line == "event=application_preflight_cleanup_complete"]
    preflight_failed = "event=application_preflight_failed" in body
    preflight_cancelled = "event=application_preflight_cancelled" in body
    runtime_failed = "event=application_failed" in body
    runtime_entered = bool(runtime_starts)
    malformed_imports = any(set(group) != set(IMPORT_KEYS) for group in imports)
    if malformed_imports:
        issues.append("malformed_import_report")
    if len(preflight_starts) != 1 or body[preflight_starts[0]] != APPLICATION_PREFLIGHT:
        issues.append("missing_or_duplicate_preflight_configuration")
    if preflight_failed and preflight_cancelled:
        issues.append("ambiguous_preflight_outcome")
    for event in ("event=application_preflight_failed", "event=application_preflight_cancelled", "event=application_failed"):
        if body.count(event) > 1:
            issues.append("duplicate_application_outcome")
    failure = preflight_failed or runtime_failed
    failure |= any(group.get("status_bits", 0) not in (0, 1) or group.get("cleanup_flags", 0) != 0
                   or group.get("cleanup_errno", 0) != 0 for group in imports)
    if any(group.get("status_bits") == 1 for group in imports):
        preflight_cancelled = True
    if preflight_cancelled:
        preflight_outcome = "cancelled"
    elif failure and not runtime_entered:
        preflight_outcome = "failed"
    elif runtime_entered:
        preflight_outcome = "runtime_entered"
    else:
        preflight_outcome = "Unknown"

    # Imported-function state is consumed separately after the Rust factory has
    # released all descriptors. Missing shutdown is never inferred from return2.
    imports_clean = (len(imports) == 3 and not malformed_imports
        and imports[0]["stage"] == 3 and imports[0]["status_bits"] == 0 and imports[0]["resolved"] == 23
        and imports[1]["stage"] in (4, 5, 6, 7) and imports[2]["stage"] == 8
        and imports[2]["status_bits"] == 0
        and all(group["cleanup_flags"] == group["cleanup_errno"] == 0 for group in imports)
        and len(cleanup_events) == 1 and import_positions[-1][-1] < cleanup_events[0])
    preflight_cleanup = "reported_complete" if imports_clean else "Unknown"
    if any(group.get("cleanup_flags", 0) or group.get("cleanup_errno", 0) for group in imports):
        preflight_cleanup = "failed"
    factory_masks = ("required_mask", "active_mask", "cleanup_mask", "cleanup_failed_mask", "cleanup_uncertain")
    factory_cleanup = "Unknown"
    if all(one(key) is not None for key in factory_masks):
        required, active, cleaned, failed, uncertain = (one(key) for key in factory_masks)
        coherent = required == 7 and not (active & ~required or cleaned & ~required or failed & ~cleaned) and uncertain in (0, 1)
        if not coherent:
            failure = True
            issues.append("factory_cleanup_contract_mismatch")
        elif active or failed or uncertain:
            factory_cleanup = "failed"
            failure = True
        else:
            factory_cleanup = "reported_complete"
    for key in ("error_bits", "native_status_bits", "error_flags", "cleanup_error_bits", "native_cleanup_bits",
                "listener_status_bits", "listener_errno_bits", "listener_cleanup_bits", "listener_flags", "stop_error_bits"):
        if any(value != 0 for value in numbers.get(key, [])):
            failure = True
    http_cleanup_failed = False
    sources = []
    for group in http:
        if tuple(group) != HTTP_KEYS:
            issues.append("malformed_http_report")
            failure = True
            continue
        source, cause = group["failure_source"], group["failure_cause"]
        present, bits = group["cleanup_present"], group["cleanup_bits"]
        sources.append(source)
        if source not in range(1, 5) or cause not in range(1, 15) or present not in (0, 1) or (present == 0 and bits != 0):
            issues.append("malformed_http_report")
            failure = True
        if present or bits or cause == 9:
            http_cleanup_failed = True
            failure = True
        if source in (2, 4):
            failure = True
    if len(sources) != len(set(sources)) or "malformed_http_report" in issues:
        issues.append("malformed_http_report")
        failure = True
    if "failed_resource" in numbers:
        failure = True
    if one("result") is not None and one("result") != 0:
        failure = True
    required = set(APPLICATION_BUDGET) | {"result", "drive_entered", "factory_entered", "stop_reason",
        "drive_failure", "clock_calls", "turns", "waits", "forced", "finished", "stop_error_bits",
        "clients", "client_failures", "protocols_closed", *factory_masks}
    if runtime_entered:
        if len(runtime_starts) != 1 or body[runtime_starts[0]] != APPLICATION_CONFIG:
            issues.append("application_profile_mismatch")
            failure = True
        if any(one(key) is None for key in required):
            issues.append("missing_or_duplicate_numeric_evidence")
        if len(completions) != 1:
            issues.append("completion_not_confirmed")
        if preflight_cancelled or preflight_failed:
            failure = True
            issues.append("runtime_after_failed_preflight")
        expected = {**APPLICATION_BUDGET, "result": 0, "drive_entered": 1, "factory_entered": 1,
                    "drive_failure": 0, "forced": 0, "finished": 1, "stop_error_bits": 0,
                    "cleanup_mask": 7, "cleanup_failed_mask": 0, "active_mask": 0,
                    "required_mask": 7, "cleanup_uncertain": 0, "protocols_closed": 1}
        # An early clean Stop may return result0 before the factory/drive enters.
        # Preserve that observation, but never call it a completed runtime test.
        not_driven = one("drive_entered") == 0 or one("factory_entered") == 0
        early_stop = (one("result") == 0 and one("stop_reason") == 1
                      and (one("cleanup_mask") != 7 or one("protocols_closed") == 0))
        if not_driven:
            issues.append("runtime_not_driven")
        if early_stop:
            issues.append("runtime_not_fully_started")
        skipped = {"drive_entered", "factory_entered", "cleanup_mask"} if not_driven else set()
        if early_stop:
            skipped |= {"cleanup_mask", "protocols_closed"}
        if any(one(key) is not None and one(key) != value for key, value in expected.items()
               if key not in skipped):
            failure = True
            issues.append("candidate_contract_mismatch")
        if one("stop_reason") is not None and one("stop_reason") not in (1, 2):
            failure = True
            issues.append("candidate_contract_mismatch")
        if one("turns") is not None and not 1 <= one("turns") <= 20_000:
            failure = True
        if one("clock_calls") is not None and one("turns") is not None and one("clock_calls") > one("turns"):
            failure = True
        if one("clock_calls") == 0 and not not_driven and not early_stop:
            failure = True
        if one("waits") is not None and (one("waits") > 10_000 or (one("turns") is not None and one("waits") > one("turns"))):
            failure = True
        if one("cleanup_turns") is not None and one("cleanup_turns") > 6:
            failure = True
        if one("clients") is not None and one("clients") > 64:
            failure = True
        if one("client_failures") is not None and one("clients") is not None and one("client_failures") > one("clients"):
            failure = True
        if imports_clean:
            valid_success_imports = (tuple(group["stage"] for group in imports) == (3, 6, 8)
                and tuple(group["resolved"] for group in imports) == (23, 0, 0)
                and all(group["status_bits"] == group["errno"] == group["lookup_hi"] == group["lookup_lo"] == 0 for group in imports)
                and imports[0]["return_hi"] == imports[0]["return_lo"] == 0
                and imports[2]["return_hi"] == imports[2]["return_lo"] == 0
                and imports[1]["return_hi"] == 0 and 1 <= imports[1]["return_lo"] <= 16)
            if not valid_success_imports:
                failure = True
                issues.append("preflight_contract_mismatch")
        if len(runtime_starts) == len(completions) == 1:
            start, end = runtime_starts[0], completions[0]
            if start >= end or any(not start < index < end for index in [*http_positions, *(i for indices in positions.values() for i in indices)]):
                issues.append("diagnostic_marker_order_invalid")
            if not (imports_clean and import_positions[0][-1] < import_positions[1][0]
                    and import_positions[1][-1] < start < end < import_positions[2][0]):
                issues.append("preflight_marker_order_invalid")
    else:
        issues.append("runtime_not_entered")
        if numbers or completions:
            issues.append("runtime_entry_not_confirmed")
    if factory_cleanup != "reported_complete" or preflight_cleanup != "reported_complete" or http_cleanup_failed:
        issues.append("cleanup_not_confirmed")
    cleanup = "failed" if http_cleanup_failed or "failed" in (factory_cleanup, preflight_cleanup) else "reported_complete" if factory_cleanup == preflight_cleanup == "reported_complete" else "Unknown"
    if len(preflight_starts) == 1 and imports and preflight_starts[0] >= import_positions[0][0]:
        issues.append("preflight_marker_order_invalid")
    return {"configuration": {"mode": "control-only", "transport": "Unavailable", "capture": "Unavailable",
                "native_format": "Unknown", "codec": "Unavailable", "profile_budget": APPLICATION_BUDGET.copy()},
            "configuration_confirmed": body.count(APPLICATION_CONFIG) == 1 and all(one(key) == value for key, value in APPLICATION_BUDGET.items()),
            "numeric": numbers, "imports": imports, "http": http, "backend_status_bits": [], "runtime_entered": runtime_entered,
            "preflight_outcome": preflight_outcome, "preflight_cleanup": preflight_cleanup,
            "factory_cleanup": factory_cleanup, "cleanup": cleanup, "failure": bool(failure),
            "issues": sorted(set(issues))}


def candidate_report(candidate: str, body: list[str]) -> dict:
    if candidate == VARIANTS["application_control"]:
        return application_report(body)
    prefix = "sbc_continuous_" if candidate == VARIANTS["sbc_continuous"] else "capture_stream_"
    allowed = SBC_KEYS if prefix == "sbc_continuous_" else CAPTURE_KEYS
    numbers: dict[str, list[int]] = {}
    issues = []
    for line in body:
        match = re.fullmatch(re.escape(prefix) + r"([a-z_]+)=0x([0-9a-f]{8})", line)
        if match and match[1] in allowed:
            numbers.setdefault(match[1], []).append(int(match[2], 16))
            if len(numbers[match[1]]) > 8:
                raise ValueError("too many repeated numeric diagnostic records")
        elif line.startswith(prefix) and line.split("=", 1)[0][len(prefix):] in allowed:
            issues.append("malformed_numeric_marker")
    raw_statuses = []
    for line in body:
        match = re.fullmatch(r"backend_status_bits=0x([0-9a-f]{8})", line)
        if match:
            raw_statuses.append(int(match[1], 16))
    if len(raw_statuses) > 8:
        raise ValueError("too many backend status records")

    def one(key):
        values = numbers.get(key, [])
        return values[0] if len(values) == 1 else None

    if prefix == "sbc_continuous_":
        required = SBC_KEYS
        configuration = {"generated": True, "nominal_clock": "synthetic", "sink": "discard",
                         "max_blocks": 750, "elapsed_budget_us": 2_000_000}
        config_line = SBC_CONFIG
        cleanup_failed = "event=sbc_continuous_cleanup_failed" in body
        cleanup_ok = one("cleanup_attempted") == 1 and not cleanup_failed
        failure = cleanup_failed or bool(raw_statuses) or (one("outcome") is not None and one("outcome") != 0)
        complete_line, failed_line = "event=sbc_continuous_complete", "event=sbc_continuous_failed"
        expected = {"outcome": 0, "blocks": 750, "pcm_frames": 96_000, "bytes": 57_750,
                    "prefix": 0x9cfd20cb, "digest": 0x7784d1fb, "cleanup_attempted": 1}
        if all(one(k) is not None for k in required):
            elapsed = (one("elapsed_us_hi") << 32) | one("elapsed_us_lo")
            configuration["observed_elapsed_us"] = elapsed
            if any(one(key) != value for key, value in expected.items()) or elapsed >= 2_000_000:
                failure = True
                issues.append("candidate_contract_mismatch")
    else:
        required = {"kind", "read_status_bits", "bytes", "discontinuity", "stop_bits", "close_bits"}
        configuration = {"reads_max": 1, "elapsed_budget_us": 2_000_000,
                         "format": "Unknown", "native_time": "Unknown"}
        config_line = CAPTURE_CONFIG
        cleanup_failed = any(value != 0 for key in ("stop_bits", "close_bits", "cleanup_flags") for value in numbers.get(key, []))
        cleanup_ok = one("stop_bits") == 0 and one("close_bits") == 0 and body.count("event=capture_stream_cleanup_complete") == 1 and not cleanup_failed
        failure = cleanup_failed or any(key in numbers for key in ("owner_error", "clock_bits")) or any(value != 0 for value in numbers.get("status_bits", []))
        failure |= "event=capture_stream_clock_or_deadline_failed" in body
        complete_line, failed_line = "event=capture_stream_complete", "event=capture_stream_failed"
        kind, length, raw = one("kind"), one("bytes"), one("read_status_bits")
        if kind is not None and length is not None and raw is not None:
            discontinuity = one("discontinuity")
            if kind == 1:
                valid = 0 < length == raw <= 8192 and discontinuity == 0
            else:
                expected = {2: (0, 0), 3: (0x81950002, 0),
                            4: (0x81950004, 1), 5: (0x81950008, 0)}
                valid = length == 0 and expected.get(kind) == (raw, discontinuity)
            if not valid:
                failure = True
                issues.append("candidate_contract_mismatch")
        configuration["pcm_format_verified"] = "Unknown"
    if any(len(numbers.get(key, [])) != 1 for key in required):
        issues.append("missing_or_duplicate_numeric_evidence")
    if body.count(config_line) != 1:
        issues.append("missing_or_duplicate_configuration")
    if body.count(config_line) == 1 and body.count(complete_line) == 1:
        config_index, complete_index = body.index(config_line), body.index(complete_line)
        numeric_indices = [i for i, line in enumerate(body)
                           if line.startswith(prefix) or line.startswith("backend_status_bits=")]
        if config_index >= complete_index or any(not config_index < i < complete_index for i in numeric_indices):
            issues.append("diagnostic_marker_order_invalid")
        if prefix == "capture_stream_" and "event=capture_stream_cleanup_complete" in body and body.index("event=capture_stream_cleanup_complete") >= complete_index:
            issues.append("diagnostic_marker_order_invalid")
    if not cleanup_ok:
        issues.append("cleanup_not_confirmed")
    # Success-like substrings and prefixed complete markers never satisfy this.
    if body.count(complete_line) != 1:
        issues.append("completion_not_confirmed")
    if any(line == failed_line or line.startswith(failed_line + " ") for line in body):
        failure = True
    return {"configuration": configuration, "numeric": numbers,
            "backend_status_bits": raw_statuses, "cleanup": "failed" if cleanup_failed else "reported_complete" if cleanup_ok else "Unknown",
            "failure": failure, "issues": sorted(set(issues))}


def collect(manifest_path: Path, log_path: Path, observations: dict[str, str]) -> dict:
    if set(observations) != {"before", "during", "after"} or any(v not in OBSERVATIONS for v in observations.values()):
        raise ValueError("explicit before/during/after controller observations are required")
    manifest_data = read_bounded(manifest_path, MAX_MANIFEST_BYTES)
    identity = manifest_identity(manifest_data)
    log_data = read_bounded(log_path, MAX_LOG_BYTES)
    try:
        text = log_data.decode("ascii")
    except UnicodeDecodeError as error:
        raise ValueError("diagnostic log must be ASCII text, never binary capture") from error
    raw_lines = text.splitlines(keepends=True)
    if len(raw_lines) > MAX_LINES or any(len(line) > MAX_LINE_BYTES for line in raw_lines):
        raise ValueError("diagnostic log exceeds line bounds")
    if any(any(ord(c) < 32 and c not in "\r\n" for c in line) for line in raw_lines):
        raise ValueError("diagnostic log contains control bytes")
    truncated = bool(raw_lines and not raw_lines[-1].endswith("\n"))
    lines = [line.rstrip("\r\n") for line in raw_lines if line.endswith("\n")]
    starts = [(i, line) for i, line in enumerate(lines) if line.startswith("event=run_begin")]
    finishes = [(i, line) for i, line in enumerate(lines) if line.startswith("event=run_finish")]
    issues = []
    result = {"schema": 1, "classification": "missing_entry", "identity": identity,
              "input_sha256": {"manifest": digest(manifest_data), "log": digest(log_data)},
              "controller": dict(observations),
              "controller_acceptance": "changed" if "changed" in observations.values() else "reported_normal" if all(v == "normal" for v in observations.values()) else "Unknown",
              "ps5_persistence": "Unknown", "audio_acceptance": "Unknown",
              "host_persistence_protocol": "private temporary file + file fsync + exclusive link + directory fsync; confirmed only by successful collector write",
              "lifecycle": {"begin": False, "finish": False, "payload_exit": None, "payload_return": None, "truncated_tail": truncated},
              "diagnostic": None, "issues": issues}
    if not starts:
        issues.append("run_begin_missing")
        if finishes:
            result["classification"] = "identity_mismatch"
            issues.append("finish_without_begin")
        return result
    expected_start = f'event=run_begin build_id={identity["build_id"]} candidate={identity["candidate"]}'
    expected_finish = f'event=run_finish build_id={identity["build_id"]}'
    if len(starts) != 1 or starts[0][1] != expected_start or len(finishes) > 1 or (finishes and (finishes[0][1] != expected_finish or finishes[0][0] <= starts[0][0])):
        result["classification"] = "identity_mismatch"
        issues.append("ambiguous_or_wrong_identity")
        return result
    result["lifecycle"]["begin"] = True
    begin = starts[0][0]
    end = finishes[0][0] if finishes else len(lines)
    result["lifecycle"]["finish"] = bool(finishes)
    report = candidate_report(identity["candidate"], lines[begin + 1:end])
    result["diagnostic"] = report
    issues.extend(report["issues"])
    exits, returns = [], []
    malformed = False
    for i, line in enumerate(lines):
        if line.startswith("event=payload_exit") or line.startswith("event=payload_return"):
            match = re.fullmatch(r"event=payload_(exit|return) status=(-?[0-9]{1,11})(?: \(expected Unsupported\))?", line)
            if not match or not -(1 << 31) <= int(match[2]) < (1 << 31):
                malformed = True
                continue
            (exits if match[1] == "exit" else returns).append((i, int(match[2])))
    ordered = len(exits) == len(returns) == 1 and end < exits[0][0] < returns[0][0]
    if len(exits) == 1:
        result["lifecycle"]["payload_exit"] = exits[0][1]
    if len(returns) == 1:
        result["lifecycle"]["payload_return"] = returns[0][1]
    if not finishes or not ordered or malformed or truncated:
        result["classification"] = "interrupted"
        issues.append("missing_or_ambiguous_termination")
    elif report["failure"] or exits[0][1] != 2 or returns[0][1] != 2 or "changed" in observations.values():
        result["classification"] = "complete_failure"
        if "changed" in observations.values():
            issues.append("controller_changed")
    elif report["issues"] or not identity["source_clean"]:
        result["classification"] = "incomplete_evidence"
        if not identity["source_clean"]:
            issues.append("uncommitted_build_inputs")
    else:
        result["classification"] = "complete_success"
    result["issues"] = sorted(set(issues))
    return result


def write_atomic(path: Path, record: dict) -> None:
    """Never clobber a prior result; failures after publication stay visible."""
    path = path.absolute()
    data = (json.dumps(record, indent=2, sort_keys=True, allow_nan=False) + "\n").encode()
    if len(data) > 65_536:
        raise ValueError("result exceeds output byte bound")
    directory_fd = os.open(path.parent, os.O_RDONLY | getattr(os, "O_DIRECTORY", 0))
    temporary = None
    try:
        fd, name = tempfile.mkstemp(prefix=".ps5-bt-result-", suffix=".tmp", dir=path.parent)
        temporary = Path(name)
        with os.fdopen(fd, "wb") as output:
            output.write(data)
            output.flush()
            os.fsync(output.fileno())
        os.link(temporary, path)  # atomic, exclusive: an existing result is an error
        temporary.unlink()
        temporary = None
        os.fsync(directory_fd)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)
        os.close(directory_fd)


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--log", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    for phase in ("before", "during", "after"):
        parser.add_argument(f"--controller-{phase}", choices=sorted(OBSERVATIONS), required=True)
    args = parser.parse_args(argv)
    try:
        record = collect(args.manifest, args.log, {phase: getattr(args, f"controller_{phase}") for phase in ("before", "during", "after")})
        write_atomic(args.output, record)
    except (OSError, ValueError) as error:
        print(f"diagnostic result not confirmed durable: {error}", file=sys.stderr)
        return 1
    print(f'{record["classification"]}: {args.output}')
    return 0 if record["classification"] == "complete_success" else 2


if __name__ == "__main__":
    raise SystemExit(main())
