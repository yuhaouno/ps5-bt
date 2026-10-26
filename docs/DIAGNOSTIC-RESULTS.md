# Local diagnostic result collection

Status: implemented and checked offline. PS5 log persistence remains **Unknown**.
`tools/diagnostic_result.py` reads an already saved local build manifest and log,
then saves a bounded JSON evidence record. It performs no network operation,
console execution, upload or remote log deletion. The original saved log remains
an operator-held input; the JSON never embeds arbitrary log text or file paths.

## Artifact and run correlation

The new independently selected Rust `sbc-continuous` and `capture-stream`
artifacts embed the build identity recorded by their manifest:

```text
event=run_begin build_id=<64 lowercase hex digits> candidate=<artifact stem>
...
event=run_finish build_id=<same 64 digits>
event=payload_exit status=2
event=payload_return status=2
```

The embedded identity hashes the selected C sources/headers, compiled Rust and
optional codec archive, SDK inventory, compile flags, linker script and candidate
name. It identifies these build inputs; the separate manifest records the final
ELF SHA-256. The two identities have different purposes and no self-hash is
embedded in the ELF. `diagnostic_build_id` and `diagnostic_candidate` are mandatory. The candidate
must match exactly one true supported variant flag in the manifest. The
collector rejects missing identities and unsupported historical variants;
it does not retroactively authenticate baseline logs. The old diagnostic
artifacts and existing evidence remain unchanged.

Exactly one matching begin and finish must appear in order. The exit and return
must each appear exactly once, after finish, with bounded signed i32 status.
A complete newline is required for a marker to count. An embedded marker
substring, repeated run, wrong candidate/build identity, truncated tail or a
stray completion event cannot become a successful run. The report also records
manifest/log SHA-256, the manifest's ELF SHA-256/size, source revision and whether
the build used clean source. It does not read or reverify an ELF: its artifact
hash is explicitly the manifest's declared identity. Hashes correlate local
inputs; text logs are not cryptographic execution attestations and can be forged.

The runtime writes its existing log channel and closes it at exit. Neither
that behavior nor host-side result fsync establishes power-loss persistence,
reboot survival or complete PS5 logging. Preserve each original log before the
next launch truncates `/data/ps5-bt-hw1.log`. A missing final marker remains
observable even if the operator does not know why execution stopped.

## Explicit observations and usage

All three wireless-controller observations are required. Use `Unknown` whenever
there was no actual observation; the collector never fills in normal operation.
Input files and results should remain under ignored `build/` or another private
local evidence directory. Do not stage raw capture, keys or private logs.

```sh
python3 tools/diagnostic_result.py \
  --manifest build/ps5/ps5-bt-sbc-continuous.manifest.json \
  --log build/runs/sbc-continuous-saved.log \
  --controller-before normal \
  --controller-during Unknown \
  --controller-after normal \
  --output build/runs/sbc-continuous-result.json
```

The output directory must already exist. Every output is created exclusively;
a prior result is never overwritten. Select a fresh output filename per run.
Controller observations are only `Unknown`, `normal` or `changed`, without free
text that could accidentally retain identities or other sensitive material.
A reported change makes the completed experiment a failure. Unknown observations
remain Unknown even when the numerical diagnostic completed successfully.

Process exit codes distinguish collection from experiment outcome:

| Exit | Meaning |
| --- | --- |
| 0 | Complete diagnostic success record saved using the host persistence protocol |
| 2 | Non-success/partial evidence record saved using that protocol; inspect its classification |
| 1 | Invalid input or result write/durability failure; no durable-record confirmation |

## Classification and retained evidence

| Classification | Meaning |
| --- | --- |
| `complete_success` | Unique matching lifecycle, expected termination and complete successful candidate evidence from a clean-source manifest |
| `complete_failure` | Matching complete lifecycle reports a failed status, candidate contract/error, cleanup fault or changed controller operation |
| `interrupted` | Matching entry but missing, truncated, unordered or ambiguous finish/termination |
| `missing_entry` | No complete identity-bearing entry was present |
| `identity_mismatch` | Wrong, repeated or inconsistent build/candidate/run identity |
| `incomplete_evidence` | Lifecycle completed but required config/numeric/cleanup evidence is incomplete, duplicated or from uncommitted build inputs |

The JSON retains lifecycle presence, actual final numeric statuses, candidate
configuration, whitelisted numeric counters/status bit patterns, reported cleanup
state, controller observations, fixed issue codes and input/artifact hashes.
Missing cleanup is **Unknown**, rather than inferred from a successful return.
Numeric candidate reports must precede their completion event and follow their
configuration marker. A completion marker alone does not prove configuration,
frame counts or cleanup. The source-clean check does not claim remote CI or
hardware validation.

The continuous SBC profile checks the exact 750 blocks / 96,000 frames /
57,750 bytes, prefix `9cfd20cb`, digest `7784d1fb`, observed elapsed time below
two seconds, and explicit cleanup attempt without a cleanup failure. Nominal
sample time remains synthetic and the sink discards complete SBC blocks. The
separate capture profile retains one read's kind, raw status, byte count,
discontinuity and stop/close results. Kind, raw status, length and flags must
match the native ABI exactly: Zero is raw 0; Empty/Overrun/Ended use
`81950002`/`81950004`/`81950008`; only Overrun sets discontinuity. Data requires
1..8,192 bytes equal to the positive raw return and no discontinuity.
Empty/Zero are lifecycle results; Data
does not establish the PCM format or native timestamp units. These format and
playback fields remain **Unknown** in the result.

Only fixed known numeric keys and fixed profile strings are copied. Unknown
lines, metadata blobs, addresses, arbitrary text, PCM, encoded frames and key
material are never copied into JSON. A raw log hash is retained without the
log contents. Extend the whitelist and profile validation deliberately when
adding another artifact or changing a diagnostic contract.

## Resource and persistence bounds

Each input must be a regular local file of at most 1 MiB. Logs have at most
16,384 lines of at most 2,048 bytes each and contain ASCII text without control
bytes. FIFO/device/symlink input is refused where the host supports no-follow
opens. JSON duplicate keys, invalid identity/types, unsupported variants and
oversized inputs fail explicitly. Result JSON is bounded to 64 KiB. Numeric
records are bounded per key, and required success counters may appear only once.

The result writer creates a private mode-0600 temporary file in the output
directory, writes and flushes it, fsyncs the file, atomically links it to the
new destination without replacing anything, removes the temporary name, then
fsyncs the parent directory. It reports write/link/fsync failures. A directory
fsync failure can occur after the destination becomes visible: exit 1 still
means durability was not confirmed; retain that evidence for inspection and
choose a new output filename after resolving the failure. No record silently
claims a completed persistence protocol when its write call failed.

The host persistence protocol depends on the actual host filesystem and successful
collector completion. The record names that protocol conditionally; it does not
present host fsync as proof about the console's storage. PS5 persistence and
all headset audio acceptance remain explicitly Unknown.

Run the deterministic offline parser/write-fault checks with:

```sh
python3 tests/tools/test_diagnostic_result.py
```

Tests cover complete profiles, interrupted and truncated logs, identity reuse,
fake/early completion, failed status and digest/count/timing mismatches, missing
cleanup, controller Unknown/changed, dirty source, duplicate keys, input bounds,
private atomic no-clobber output and both file/directory fsync failures. These
are collection tests against synthetic logs, not hardware evidence.
