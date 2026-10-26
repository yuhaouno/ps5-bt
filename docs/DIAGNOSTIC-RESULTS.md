# Local diagnostic result collection

Status: implemented and checked offline. PS5 log persistence remains **Unknown**.
`tools/diagnostic_result.py` reads an already saved local build manifest and log,
then saves a bounded JSON evidence record. It performs no network operation,
console execution, upload or remote log deletion. The original saved log remains
an operator-held input; the JSON never embeds arbitrary log text or file paths.

## Artifact and run correlation

The independently selected Rust `sbc-continuous`, `capture-stream`,
`application-control`, `application-generated` and `application-observe` artifacts
embed the build identity recorded by their manifest:

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

## Control-only application profile

The `application_control` manifest flag selects only
`ps5-bt-application-control`. It follows the same unique outer run identity and
final exit/return rules. Its C preflight can fail or be cancelled before Rust
runtime entry, so the collector records `runtime_entered` and
`preflight_outcome` separately. Missing native imports or a private-directory /
factory failure followed by a complete outer lifecycle are `complete_failure`,
not `missing_entry`. A preflight cancellation before runtime entry is
`incomplete_evidence`; it is not a completed runtime acceptance test. An early
clean Stop that never drives the complete application is also incomplete.

A successful profile requires exactly the current source-defined begin event:

```text
event=application_begin schema=1 mode=control-only transport=Unavailable capture=Unavailable native_format=Unknown codec=Unavailable
```

The runtime logs the actual numeric configuration passed to execution. The
collector requires duration 5,000,000 microseconds, aggregate stop timeout
100,000 microseconds, at most 20,000 turns, 10,000 waits and 64 accepted clients.
These are operation/time budgets, not measured latency or real-time scheduling
guarantees. Clock/turn/wait/client counters must respect those bounds; a
nonzero drive failure, forced stop, unfinished state, stop error or invalid
configuration cannot pass. Stop reasons 1 (Requested) and 2 (Deadline) are the
only successful complete-run reasons. The application must report closed
protocol state and matching completed native resource cleanup.

The factory emits numeric required/active/cleanup/failed masks. The complete
control-only profile requires required mask 7, active mask 0, cleanup mask 7,
failed mask 0 and cleanup uncertainty 0. The bits are factory resource indices:
Clock 0, Store 1, Listener 2, Capture 3 and Encoder 4. Partial startup may report
only a subset cleaned; that evidence is retained without promoting the run to
full startup success. Individual missing mask/cleanup records remain Unknown.
`application_fault_resource` uses the separate MainLoop resource namespace;
`application_failed_resource` uses factory indices. The collector retains these
raw numbers and does not silently reinterpret one namespace as the other.

C's import/entropy/shutdown records are bounded groups of the same ten
whitelisted numeric fields, beginning with `application_import_stage`. A full
successful runtime requires init stage 3 with 23 resolved exports, entropy stage
6, shutdown stage 8, all matching zero status/errno/lookup/cleanup fields, and an
explicit `application_preflight_cleanup_complete` after shutdown. Each group
must be complete and appear in order: init, entropy, Rust begin/report/complete,
shutdown, preflight cleanup. Import `return_hi`/`return_lo` retain the raw signed i64 native return as two
u32 words. Full success requires init/shutdown raw return 0 and the last successful
entropy read between 1 and 16 bytes; zero, negative or oversized entropy returns
cannot pass even with a forged Complete marker. Import state cleanup and native factory cleanup
are separate results; neither implies the other. A missing shutdown or missing
factory cleanup is not repaired by a final status 2. Cancellation/failure can
retain a shorter import report sequence with cleanup Unknown.

HTTP failure records are bounded five-field groups with a source, cause, raw
error bits, cleanup-present flag and cleanup bits. Clean recoverable client
timeouts/cancellations remain observable and need not fail an otherwise completed
service test. Malformed groups, any present/nonzero cleanup error, a failed close
or terminal service/application error cannot pass. A later successful factory
close does not erase an earlier uncertain HTTP client close. Factory and HTTP
cleanup evidence remain distinct in the record.

Only fixed numeric error/counter/mask fields and fixed profile strings are
persisted. Import resolution addresses, private-directory paths, HTTP bearer
material, arbitrary `application_*` fields and request contents are excluded.
`configuration_confirmed` distinguishes matching actual runtime configuration
from the required profile shown for a preflight that never entered Rust.
Transport, capture and codec remain Unavailable and native format remains
Unknown; this profile establishes no Bluetooth/audio activation. All headset
audio acceptance and PS5 log persistence remain Unknown even after a complete
control-only result.

## Generated and raw-observation application profiles

The additional manifest flags `application_generated` and `application_observe`
select exactly `ps5-bt-application-generated` and `ps5-bt-application-observe`.
They retain the existing unique outer identity/termination, redaction, 23-export
preflight/entropy/shutdown and global budget checks. The unchanged preflight begin
marker describes the imports stage before a workload is admitted; the runtime
begin event distinguishes the actual mode. A manifest with multiple variant
flags or a mismatched candidate is rejected. The control-only profile remains
bound to its original begin event and resource mask; these additions do not
allow it to claim a codec/capture operation.

The new exact runtime begin events are:

```text
event=application_begin schema=1 mode=generated-sbc transport=Unavailable capture=Unavailable native_format=Unknown codec=diagnostic
event=application_begin schema=1 mode=capture-observe transport=Unavailable capture=raw-observe native_format=Unknown codec=Unavailable
```

All application profiles require the actual global configuration of 5,000,000
observed microseconds, 20,000 turns, 10,000 waits, 64 clients and 100,000 us normal
stop policy, with the same bounded counter checks. The generated profile requires
native resource/cleanup masks 23 (Clock/Store/Listener/Encoder); the observation
profile requires masks 15 (Clock/Store/Listener/Capture). Full success requires
active/failed masks zero, complete cleanup, no uncertainty and no native/HTTP
failure. Partial startup/cancellation cannot inherit the control-only mask 7 as
proof that its additional resource was acquired or cleaned. Workload metrics
must each occur once, between the correct runtime begin and completion. Missing,
duplicated, malformed, out-of-order or cross-profile evidence cannot complete
that workload successfully. All earlier import, HTTP and factory failure rules
continue to apply.

### Generated PCM through the native application loop

`application_generated_*` fields retain phase, error, generated/discarded blocks,
discarded bytes, operation count, prefix, digest, summary match, observed elapsed
microseconds (two u32 words), conceal/rebuffer/expiry counters, stopped frames/
blocks, raw codec status bits and the required `completion_observed` flag. Phase values are Running 0, Complete 1,
Stopped 2 and Failed 3; summary is absent 0, mismatch 1 or match 2. These numeric
categories are versioned with the source-defined candidate, not guessed from
arbitrary log text.

A completed workload requires phase 1/error 0, exactly 750 generated and discarded
blocks, 57,750 bytes, prefix `9cfd20cb`, digest `7784d1fb` and summary match 2.
Operations must be 2,251..60,000, including the separate final completion
observation; `application_generated_completion_observed` must be exactly 1. Concealment, rebuffer, expiry, stopped frames,
stopped blocks and codec error bits must all be zero. Its pacing uses the actual
host-monotonic observations, unlike the earlier separately selected SBC
candidate's synthetic nominal clock. The earliest last-block deadline is
`ceil(749 * 128 * 1,000,000 / 48,000) = 1,997,334` us; successful elapsed evidence
must be at least that value and strictly below the exclusive 2,000,000 us budget.
The confirming observation is sampled after final codec/discard/hash work; a
stale pre-operation timestamp cannot certify the completed duration. A summary
at exactly two seconds cannot satisfy this profile.

A coherent clean partial/Stopped workload is `incomplete_evidence`, even if the
outer application returned cleanly. Settled Complete frame counts with
`completion_observed=0` are also incomplete: Stop may have preempted the final
clock check. They cannot inherit the last pre-operation timestamp as final elapsed
evidence. Missing/duplicated flags remain incomplete and a flag outside 0/1 fails. Running state, missing summary or an early
Stop cannot be promoted to the full 750-block result. A Failed phase, nonzero
error/codec status, impossible count/byte relation, continuity fault or full
summary/digest/timing mismatch is a failure. The sink deliberately discards the
encoded blocks; no network/ACL submission, capture, headset output or audio
acceptance is inferred.

### Continuous bounded raw capture observation

`application_observe_*` fields retain finish/error category; reads, Data, Empty,
Zero, Overrun, Ended, native/owner errors and unknown-gap events; total byte count
as two u32 words; maximum read bytes; the final read kind/raw status/discontinuity;
and separate host/native-time metadata. Actual configured maximum reads, duration,
first settle and interval are required numeric evidence, as are workload start
and observed elapsed microseconds. The selected `CONTINUOUS` profile must report
256 reads, 2,000,000 us, 100,000 us and 10,000 us respectively. The collector derives
`cadence_read_ceiling = min(max_reads, 1 + (duration_us - first_settle_us - 1) /
interval_us) = 190` with integer division. Actual reads can be fewer; the collector
never substitutes 190 for observed counts or requires that exact count. A 191st
read cannot fit this exclusive window and fails. The reusable helper's retained
`BOUNDED` 20-read/100 ms profile and historical C diagnostic keep their own bounds;
they do not match this artifact's selected configuration.

Observation reason values are none 0, Cancelled 1, Deadline 2, ReadBudget 3,
Ended 4 and Failed 5. A completed bounded observation requires Deadline, at least
one read, coherent counters and clean factory/native cleanup. Elapsed must reach
two seconds and remain within the five-second global application bound. Native
calls/waits may overshoot their requested duration, so the JSON retains
`deadline_overshoot_us`; it does not invent a 1 ms native-latency guarantee or
round away that observation. Cancelled or Ended with clean cleanup remains
incomplete for the continuous observation target, preserving its actual reason.
Native/owner errors or uncertain cleanup are failures.

Successful read counters sum exactly to actual owner read advances. Errors may
retain an additional owner advance, without rewriting the original owner budget.
Total/max byte counts must fit the number of Data reads and 8,192-byte read bound.
The final kind/raw/discontinuity must match the current native ABI: Data is a
positive 1..8,192-byte raw return; Zero is 0; Empty/Overrun/Ended use
`81950002`/`81950004`/`81950008`; only Overrun is discontinuous. Gap events equal
observed overruns for this specific adapter. The last host read observation must
be at least `started + first_settle + (reads - 1) * interval` and no later than
the workload's final observation, with checked absolute arithmetic. This minimum
also rejects catch-up bursts reported as regularly paced reads. No-read final summaries must remain all zero;
they never imply data or a native timestamp.

The current Avcap2 adapter supplies no typed native timestamp: the native-time
present flag, value words and domain must all be zero. This is absence of a
known timestamp, not verification of zero-valued native time. The sixteen opaque
native metadata bytes are never retained by this collector. No native unit
conversion, PCM frame division or lost-sample estimate occurs. Even a seven-byte
Data read is only a byte-length observation.

Empty-only, Zero-only or Overrun observations can complete the bounded raw
procedure with clean cleanup; they are not PCM success. Data sets only
`nonempty_bytes_observed`. PCM format/success, sample-frame counts, sample gaps,
native time units, headset audio and PS5 persistence all remain Unknown. The
JSON's `bounded_raw_observation_completed` certifies the checked reported
procedure, not playable audio, verified continuous PCM or stable capture timing.

These new profiles still copy only whitelisted numeric counters/statuses and
fixed configuration strings. Raw PCM/SBC, opaque metadata, addresses, secrets,
paths, request text and arbitrary `application_generated_*` or
`application_observe_*` keys are excluded. Hash correlation remains a property
of saved local inputs; it does not authenticate execution on PS5.

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
private atomic no-clobber output and both file/directory fsync failures. The
application profiles additionally cover preflight failure/cancellation, native
factory faults, unavailable gates, actual budget limits, missing/duplicate masks,
import-group integrity/order and secret/path/address exclusion. Generated tests
pin full workload versus clean partial Stop, bitstream/count/operation/real-clock
bounds and faults. Observation tests cover exact counters/raw statuses, zero/data/
overrun outcomes without PCM claims, actual counts below the 190-read cadence
ceiling, forged excess polls/old profile configuration, exclusive cadence, explicit native-time
absence, duration/overshoot, early Ended/Cancel and every missing/duplicate numeric
field. These
are collection tests against synthetic logs, not hardware evidence.
