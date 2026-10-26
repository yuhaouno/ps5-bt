# Generated SBC workload in the native application owner

Status: implemented and tested offline. PS5 execution **Requires hardware**.
`ps5_app::generated::GeneratedWorkload` supplies a finite generated signal to
Rust's existing AudioScheduler and a borrowed persistent BlockEncoder. Its sink
hashes and discards complete SBC frames. It neither captures native audio nor
opens a Bluetooth connection, prepares packets, submits ACL or creates credits.
The enclosing native factory remains responsible for explicit encoder finish and
all resource cleanup.

## Admission, storage and configuration

`GeneratedStorage::new()` supplies fixed input, PCM queue, render and encoded
buffers. GeneratedWorkload borrows those stable external fields once; its encoder
is only borrowed for an Encode step. There is no self-reference, per-frame
allocation, background worker or native call inside this module. Runtime assembly
initializes it at final encoder acquisition, after store/listener setup, using
that actual host monotonic observation as epoch. Earlier setup therefore does
not consume the generated workload's own deadline.

The configuration is explicit:

| Field | Admission |
| --- | --- |
| parameters | 48 kHz, joint stereo or stereo, 16 blocks, eight subbands, loudness, bitpool exactly 32 |
| epoch_us | Host monotonic observation at workload admission |
| duration_us | Nonzero, at most 5,000,000; must cover all configured nominal sample positions |
| max_blocks | 1..1,875 blocks of 128 stereo PCM frames |
| max_operations | At least three calls per block, at most 100,000 active calls |
| max_lateness_us | 1..100,000 |

The suggested profile uses 750 blocks, duration 2,000,000 microseconds,
60,000 operations and maximum lateness 100,000 microseconds. Checked deadline
addition and scheduler/sample-clock construction reject overflow. A configuration
is a generated diagnostic contract, not a negotiated headset capability or native
capture format. The native application owner still applies its separate global
run/Stop budgets.

## Lane quanta and observed clock

The owner invokes these independently bounded methods:

```text
GeneratedWorkload::new(GeneratedConfig, &mut GeneratedStorage)
generate(now_us) -> Result<Progress, GeneratedError>
encode(now_us, &mut impl BlockEncoder) -> Result<Progress, GeneratedError>
discard(now_us) -> Result<Progress, GeneratedError>
stop()
snapshot() -> GeneratedSnapshot
```

Progress contains `progressed`, `wake_at_us` and `finished`; it maps directly to
the enclosing owner's bounded Capture/Encode/Send lane scheduling. Those lane
names do not imply native capture or transport in this diagnostic. The owner
supplies actual clock observations at runtime; the workload does not fabricate
future times, perform waits or claim a physical-time result from host simulation.
It returns the earliest relevant sample deadline or its finite run deadline.
The reviewed platform wait adapter remains an enclosing-owner operation.

Generate creates at most one 128-frame block, after the observed clock reaches
its nominal sample deadline. For global sample position `s`, the stereo signal
is `[v, -v]` with `v = ((s % 101) - 50) * 200`. Queue Busy retains the exact
staged block and retries only its existing bounded offer. Input is never silently
replaced or generated faster than the configured sample clock. An initialization
or per-frame failure is not hidden by a new sample epoch.

Encode delegates to the existing AudioScheduler and invokes at most one codec
call. An outstanding unaccepted frame remains unchanged across later polls.
Discard hashes and accepts exactly one fresh retained diagnostic offer, then
throws those bytes away. No encode occurs in Discard. A missing offer is ordinary
idle work. An expired offer is a visible terminal fault and is never hashed as
a successful frame or accepted late. Only this workload's own unsent data may
be discarded; it owns no fragmented media PDU.

Every active call, including waiting and Busy, consumes one finite operation
budget entry. A frozen host clock therefore cannot create an invisible endless
poll loop. Regression, exclusive deadline expiry, exhausted operations, encoder
configuration/error/frame failure or arithmetic failure terminates the workload.
The scheduler's existing pacing, queue ownership and retained-frame semantics
are reused. Its bounded concealment is disabled here. Any skipped/expired PCM,
expired encoded frame, late acceptance or concealed sample becomes an observable
continuity failure, preserving the scheduler counters in the final snapshot.
This strict diagnostic does not claim recovery or audible continuity.

## Completion, stop and evidence

Successful completion requires every configured input block to have been queued,
encoded and accepted by the discard sink, with exact frame byte totals and no
continuity loss. The full 750-block profiles additionally require these pinned
host results:

| Mode | PCM frames | Encoded bytes | First prefix | Whole-run FNV-1a |
| --- | --- | --- | --- | --- |
| Joint stereo, bitpool 32 | 96,000 | 57,750 | `9cfd20cb` | `7784d1fb` |
| Stereo, bitpool 32 | 96,000 | 57,000 | `9cf920f8` | `1f2ed77b` |

The joint profile reuses the established continuous-SBC generated signal and
summary. The stereo summary was pinned by the new host comparison against an
independent persistent existing Encoder. These checks introduce no new codec
source, dependency or algorithm; the selected BlueZ libsbc provenance, license,
ABI and replacement/relink obligations remain unchanged.

Snapshot is a copy of numeric/state evidence: phase, stop reason, original error,
operation count, generated/discarded counts and bytes, generation backpressure,
staged/stopped staging, prefix/digest, observed elapsed host microseconds and
AudioScheduler's complete snapshot. `summary_matches` is None while partial or
for an unpinned custom block count; it is Some(true/false) only after all 750
blocks in a pinned profile have been discarded. A partial run is never a matching
full-profile result. Prefix/digest/counts are the only bitstream summaries;
this API's snapshot contains no PCM or encoded bytes.

Stop is clock-independent and idempotent. It prevents new generation/encoding,
scrubs staging and stops the scheduler, counting its queued/retained diagnostic
input and offer drops. A prior error stays visible. A completed workload remains
Complete when the enclosing application later stops. Drop invokes the same local
stop only. Neither path finishes or drops the separately owned encoder, cancels
HTTP, closes native resources or reports their cleanup success; the native owner
must still explicitly finish and record those results.

Elapsed host observation is neither encode duration nor acoustic latency. The
full profile covers two seconds of nominal sample positions, and native observed
pacing/timing still needs the actual console run. No captured-audio format,
headset compatibility, playable audio, PS5 timing or DualSense coexistence claim
follows from this offline implementation.

## Offline checks

```sh
cargo test --locked -p ps5-app --test generated
cargo test --locked -p ps5-app --features host-candidate --test generated
cargo clippy --locked -p ps5-app --features host-candidate --test generated -- -D warnings
cargo check --locked -p ps5-app --target x86_64-unknown-none
```

Nine host-candidate checks cover observed sample deadline gates, queue Busy and
retained frames without reencoding, clock/deadline/operation/expiry failures,
counted scheduler catch-up, raw codec/configuration/frame errors, independent
Stop/encoder ownership, configuration rejection and mandatory full-profile
summary comparison. The real-codec case runs both joint and stereo 750-block
workloads and compares their whole output digests/lengths with separate
persistent encoders. Test clock observations are scripted, with no sleeps or
physical-time claim. Native factory/application integration and the executable
artifact remain separately checked by the enclosing runtime/build workflow.
