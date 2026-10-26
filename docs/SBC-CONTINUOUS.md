# Rust continuous target SBC candidate

Status: implemented and checked offline; PS5 execution **Requires hardware**.
The separate `ps5-bt-sbc-continuous` artifact exercises generated PCM through
Rust's existing `AudioScheduler`, persistent `SbcEncoder`, reviewed C wrapper
and the same generic BlueZ libsbc 2.2 target archive as the short C self-test.
The existing `ps5-bt-sbc-selftest` artifact remains independently selectable.
No native capture, Bluetooth submission or audible playback occurs here.

## Separate host and target selection

`bt-codec/host-libsbc` retains its exact `TARGET == HOST` check and host C build.
`bt-codec/target-libsbc` requires Rust `x86_64-unknown-none`, an existing
`PS5_BT_SBC_ARCHIVE` named `libps5_bt_sbc.a`, and `PS5_BT_SBC_NM` pointing to
LLVM 18.1.8 `llvm-nm`. Selecting both features fails explicitly. Neither is a
default feature. `bt-audio` forwards these separate features and exports the
same `SbcEncoder` implementation under `HostSbcEncoder` or `TargetSbcEncoder`
respectively. The existing host API remains available.

The target archive is built first with `tools/target_sbc.py`, then inspected
again by the Rust build script. Every ordinary object must be little-endian
ELF64 x86-64 relocatable code; thin/non-object/wrong-machine archives fail.
The five project wrapper symbols must exist, and the only unresolved runtime
imports may be `memcpy` and `memset`. Unknown allocator, CPU-dispatch or OS
imports fail before Rust linking. The source builder separately checks pristine
hashes, patch inventory, exact internal/external symbols and LLVM version.
A replacement archive may differ in hash but must satisfy this reviewed seam.

The Rust target build uses `static:-bundle`: codec objects remain outside the
Rust static archive. The final ELF links the separately supplied codec archive,
and the established source/replacement bundle provides application objects,
Rust archive, linker script, notices, patches and the tested relinker. This is
necessary for replacement review; a successful build alone is not a license
compliance determination. See [SBC-TARGET.md](SBC-TARGET.md).

## Persistent encoder and fixed buffers

`SbcEncoder` owns the existing safe `bt-codec::Encoder` and borrows an
interleaving scratch slice for its whole lifetime. Its `BlockEncoder`
implementation checks an exact complete stereo block, copies into caller-owned
interleaved storage and delegates to the existing checked seam. Storage bounds,
queried alignment, !Send/!Sync, raw backend errors, integrity poisoning and
consume-once `finish` remain those of the reviewed codec owner. The consuming
`FinishEncoder` trait exposes that result to application resource owners so
cleanup failures need not disappear in `Drop`. Initialization
and encoding use fixed caller-owned storage; there is no per-frame allocation.
This imports no new upstream component and changes no SBC algorithms or patch.
The retained LGPL-2.1-or-later library revision is
`b3deb8a5dcfb42d8c10ba1f2f1ac9bd7bf7271cc`; inventory, ownership and notices
remain in `third_party/sbc/VENDORED.md` and [UPSTREAM.md](UPSTREAM.md).

## Finite generated-only diagnostic

The C-callable entry is:

```c
int32_t ps5_rust_sbc_continuous(int32_t (*now_us)(uint64_t *out));
```

The callback synchronously writes a monotonic microsecond observation and
returns zero, or returns its raw signed native error. It retains no pointer.
Null callback, failed clock, regression and deadline overflow fail explicitly.
This clock is separate from unknown native capture timestamps.

The fixed candidate admits at most 750 encoder blocks, 96,000 stereo PCM
frames and 2,000,000 observed microseconds. It checks the clock before and after
each block. A frozen clock cannot exceed 750 iterations. A synchronous native
clock/codec call must return; this owner cannot preempt a hung foreign call.
There are no sleeps, unbounded polls, retries or transport calls.

For every global sample position `s`, the generated stereo signal is
`v = ((s % 101) - 50) * 200`, `[v, -v]`. Each block has 128 frames. Parameters
are 48 kHz, joint stereo, 16 blocks, eight subbands, loudness and bitpool 32.
The scheduler uses synthetic nominal sample deadlines to validate sample
positions without spinning until physical deadlines. A separate observed clock
only bounds execution. Thus the run is neither a real-time pacing test nor a
measurement of capture-to-headset latency. Complete blocks are hashed and
accepted by a discard sink; no RTP/ACL ownership or credit is fabricated.

The completed 750-block candidate must report:

| Field | Expected |
| --- | --- |
| outcome | 0 / Complete |
| blocks | 750 (`0x2ee`) |
| PCM frames | 96,000 (`0x17700`) |
| encoded bytes | 57,750 (`0xe196`) |
| first SBC prefix | `0x9cfd20cb` |
| whole-run FNV-1a digest | `0x7784d1fb` |
| cleanup attempted | 1, with no cleanup failure |

The core compares the completed full-run prefix/digest with those pinned host
results. Clock budget expiry may produce a partial report with Deadline;
partial counts are not a successful full-run result. Stop occurs on the first
fault, followed by scheduler stop and exactly one explicit encoder finish if
initialization succeeded. Initialization failure does not finish nonexistent
state. The report retains the original error alongside any separate cleanup
error. Logs include only bounded numeric status/count/digest fields, with
elapsed time split into two u32 words; no PCM or encoded frame bytes are logged.

## Build and offline checks

```sh
make ps5-sbc-continuous
python3 tools/package_source.py --manifest build/ps5/ps5-bt-sbc-continuous.manifest.json
cargo test --locked --offline -p bt-audio --features host-libsbc
cargo clippy --locked --offline -p bt-codec -p bt-audio --all-targets --features bt-audio/host-libsbc -- -D warnings
python3 tests/codec/target_archive_test.py
```

The target command requires the established LLVM 18.1.8 toolchain. It emits a
separate ELF, manifest and replacement bundle. The host checks cover repeated
persistent full runs with pinned output, invalid budgets, exclusive deadline,
clock regression/failure and cleanup admission. Existing independent 750-block
host encoder comparison and C boundary/sanitizer/vector tests remain relevant.
Archive-verifier tests cover wrong architecture, invalid archive, missing
wrapper exports and allocator imports; mocked symbol tests are not target
link or hardware evidence.

Execution is a separately authorized hardware experiment. Record artifact hash,
full log, final return status and cleanup result; preserve `/data/ps5-bt-hw1.log`
before another run truncates it. Verify wireless DualSense before/during/after.
The builder supplies identity in the manifest and the runtime's existing durable
log lifecycle. PS5 log durability, target codec behavior/timing, native capture
format, capture-to-encoder connection, headset quality/latency and Bluetooth
ownership/coexistence remain **Requires hardware**. No current console window
is assumed by implementing or building this candidate.
