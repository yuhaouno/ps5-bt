# SBC encoder implementation seam

Status: host-proven Rust -> C -> libsbc offline codec component, 2026-10-24.
The root workspace includes `bt-codec`, but the real backend is an explicit
`host-libsbc` feature and is not a dependency of `ps5-runtime`. No PS5 build,
A2DP/RTP, headset, or live-audio integration is present. No hardware or
acoustic-quality claim follows from this work.

## Implemented boundary

`platform/codec/ps5_bt_sbc.{h,c}` is a project-owned C wrapper around the
audited BlueZ libsbc 2.2 encoder. It intentionally supports only:

- 48,000 Hz;
- stereo or joint stereo;
- 16 blocks and 8 subbands;
- Loudness allocation; and
- bitpool 2 through 53.

Each successful call consumes exactly 256 native little-endian `i16` samples
(128 interleaved stereo PCM frames / 512 bytes) and writes exactly one SBC
frame. Frame length is checked against `12 + 2 * bitpool` for stereo and
`13 + 2 * bitpool` for joint stereo, so this subset needs at most 119 output
bytes. Short or extra PCM is Invalid. An output buffer shorter than the exact
frame is BufferTooSmall. These rejections occur before the encoder advances.
PCM, output, and encoder-storage ranges must not overlap.

The ABI contains only fixed-width integers and pointers. It never exposes an
upstream structure or retains PCM/output pointers. `sbc_t` is private to the C
translation unit. The encoder is mutable, single-owner, non-reentrant, and not
thread-safe. Successful init borrows the exact caller allocation until one
successful finish. A second finish is rejected. Rust ownership and `!Send` /
`!Sync` enforcement are provided by `bt-codec` for the host backend.

Valid but unsupported parameters return `-1`; malformed parameters return
`-2`; short output returns `-4`; detected length/consumption inconsistency
returns `-7` and poisons the instance. Unexpected negative libsbc results are
returned unchanged for the Rust `Error::Backend(i32)` mapping. All output counts
are cleared before validation and remain zero on failure.

`crates/bt-codec` is a `no_std` safe owner with no registry dependencies. It
consumes the shared `bt-core::SbcParameters` and `SbcBitpoolRange`, rejects a
selection outside the negotiated range before FFI, and exposes
`EncoderStorage<const N: usize>` plus `Encoder<'storage, N>`. The encoder keeps
an exclusive borrow of the actual aligned caller allocation for its full
lifetime. The owner itself may move safely because C retains pointers only into
that external allocation; safe Rust cannot move or reborrow the allocation
until finish/drop. A raw-pointer marker makes the owner `!Send` and `!Sync`.

Rust prevalidates exact input/output lengths and passes only the calculated
frame capacity to C. It verifies returned sample and byte counts, retains
unknown negative statuses as `Error::Backend(i32)`, poisons on integrity loss,
and never calls encode again after poison. Explicit `finish(self)` marks the
instance consumed before the C call; `Drop` performs the same operation only
when explicit finish has not already consumed it.

## Memory model

The caller obtains byte size and alignment from
`ps5_bt_sbc_storage_size()` / `ps5_bt_sbc_storage_alignment()`. These are
derived at C compile time from the upstream-private `struct sbc_priv`; no Rust
or public wrapper definition copies its 5,600-byte observed layout. Patch
`0002-caller-storage.patch` adds private size/alignment queries and a static
initializer/finalizer beside that structure. The wrapper aligns the private
region after its small state header and validates the caller address/capacity.

The codec build defines `SBC_STATIC_ONLY`, which omits upstream `malloc`/`free`
entry points. Initialization, encoding, and finish allocate nothing. Object
inspection rejects `malloc`, `calloc`, `realloc`, `free`, x86 CPU-runtime
initialization, and optimized primitive imports. ASan/UBSan plus guard bytes
exercise raw buffer limits. This is stronger than merely observing no
per-frame allocation, but it does not prove a future caller supplies a valid
storage object or serializes access.

Rust does not copy either the observed private size or alignment. Every
`Encoder::new` queries both values from the linked C object, checks the
const-generic caller capacity and its actual address, and passes the queried
bounded size back to C. `EncoderStorage` guarantees 64-byte alignment; a future
backend demanding more fails safely rather than silently weakening alignment.

## Upstream source and local changes

`third_party/sbc/upstream/` is the pristine selected source from BlueZ libsbc
release 2.2, commit `b3deb8a5dcfb42d8c10ba1f2f1ac9bd7bf7271cc`, under
LGPL-2.1-or-later. The authoritative archive SHA-256 is
`a1ada76ef35e5af9c2fbd063754dc9e37a8d989417c6eb1ecebb089b1383ae9e`.
Every selected-file digest is recorded and verified by
`third_party/sbc/VENDORED.md` and `tools/test_sbc.py`. The complete LGPL 2.1
text, `AUTHORS`, and file-specific copyright/SPDX notices are retained.

No PlayPods/HearBridge source, architecture-specific primitive, assembly,
intrinsic backend, upstream application, tool, or build implementation was
imported. Apply these patches, in order, to a temporary copy:

1. `0001-generic-c-only.patch` removes unavailable architecture headers and
   CPU dispatch. It leaves the generic fixed-point filter, quantizer, packing,
   and decoder algorithms unchanged.
2. `0002-caller-storage.patch` adds the private caller-storage entry points and
   lets a static-only build omit allocator-backed entry points. It makes no
   codec-algorithm change.
3. `0003-freestanding-static.patch` selects the project-owned minimal byte-
   operation/type/errno declarations only for `SBC_STATIC_ONLY`. This removes
   the target-object probe's dependence on host SDK headers without adding an
   allocator, I/O API or codec-algorithm change.

The pristine tree must never be edited in place. The test script rejects an
extra/missing file or any hash change, applies both patches with zero fuzz, and
builds only the patched temporary copy.

`crates/bt-codec/build.rs` lists every pristine file, patch, wrapper input and
helper as a Cargo rerun dependency. With `host-libsbc`, it first requires
`TARGET == HOST`, then asks `tools/test_sbc.py --build-archive` to verify hashes,
apply patches in a temporary directory, compile generic static-only objects and
produce `libps5_bt_sbc.a` in Cargo's output directory. A cross build fails at
that target check before Python or a C compiler is invoked. With the feature
off, the crate remains a portable `no_std` dependency and builds no C code.

## Reproducible host evidence

Run from the repository root:

```sh
python3 tools/test_sbc.py
```

The 2026-10-24 macOS arm64 run used Apple Clang 21.0.0 for host executables and
Homebrew LLVM/Clang 18.1.8 for the target-object probe. It passed:

- pristine file-set and SHA-256 verification;
- zero-fuzz application of both numbered patches;
- ASan/UBSan lifecycle, invalid configuration, null pointer, alignment,
  undersized storage/output, overlap, guard-byte, exact-consumption, rejected
  no-state-change, two-instance, and exactly-once cleanup checks;
- byte-for-byte encoder comparison for all 208 combinations of two modes,
  every bitpool 2 through 53, and silence/pattern input against the
  allocator-backed generic libsbc reference;
- 12 wrapper-frame decodes with the unchanged upstream decoder (two modes,
  bitpool 2/32/53, silence/pattern), each producing one complete 512-byte PCM
  frame, with exact silence and nonzero patterned output checks;
- host-object undefined-symbol inspection proving no allocator, CPU detector,
  or optimized primitive dependency;
- two separately rebuilt static archives linked into the same consumer, with
  identical real joint-stereo frame output after replacement; and
- generic `x86_64-sie-ps5` compilation of `sbc.c`, generic primitives, and the
  wrapper with LLVM 18.1.8, `-mgeneral-regs-only`, freestanding/PIC/no-red-zone
  flags, followed by the same forbidden-import inspection.

The target compile probe uses only LLVM 18 resource headers plus
`ps5_bt_sbc_freestanding.h`; it no longer consumes the host macOS or Linux SDK.
The resulting objects are generic x86-64 ELF compile evidence, not PS5
execution. The existing runtime build remains unchanged and still contains no
allocator/libc codec module.

`--no-sanitize` is available only for diagnosing a host without sanitizer
runtime support; it is not the acceptance command. `--cc` selects another host
compiler. The script uses temporary build directories and does not download
source, modify the pristine tree, or contact hardware.

The real Rust boundary passes independently with:

```sh
cargo test --locked --offline -p bt-codec --features host-libsbc
cargo run --locked --offline -p bt-codec --features host-libsbc \
  --example pcm_to_sbc
cargo clippy --locked --offline -p bt-codec --features host-libsbc \
  --all-targets -- -D warnings
cargo check --locked --offline -p bt-codec --target x86_64-unknown-none
```

The tests cover queried storage bounds, negotiated-range and subset rejection,
exact lengths/consumption, real frame headers, short/extra PCM, short output
without state advance, raw backend-code preservation, poison short-circuit,
explicit finish/drop ownership, and compile-fail `!Send`/`!Sync` checks. The
example pushes 128 stereo frames through the existing `PcmQueue`, renders them
into caller storage, interleaves them, and obtains one real 77-byte joint-stereo
bitpool-32 SBC frame with sync byte `0x9c`. This is host byte/frame evidence,
not audible output.

`platform/codec/ps5_bt_sbc_selftest.{c,h}` now provides a separate generated-
signal diagnostic core for target integration. It runs two fresh init/finish
cycles and encodes two deterministic frames per cycle at 48 kHz, joint stereo,
16 blocks, 8 subbands, loudness and bitpool 32. The second cycle must match the
first byte-for-byte; the complete result also requires prefix `0x9cfd206f` and
four-frame FNV-1a digest `0x14e0ec79`. Its identity-free report contains only
stages, raw statuses, storage requirements and aggregate counts/digests. It has
no allocator, clock, I/O, transport or retained media pointer. Host ASan/UBSan
and freestanding `x86_64-sie-ps5` compilation pass, but the core is not yet
linked into a PS5 ELF or executed on hardware.

Enabling `host-libsbc` for `x86_64-unknown-none` is an intentional build error.
That proves the current feature cannot quietly compile host C with macOS headers
for the freestanding target. A future target backend needs reviewed headers,
archive/link rules and a separate feature; it must not weaken this gate.

## Distribution and relinking gate

Before any codec-enabled ELF is distributed, the lead must choose and test a
concrete LGPL 2.1 section 6 relinking arrangement. At minimum, recipients need
prominent library attribution, the complete LGPL text, complete selected
library source, these local patches, build scripts, application object
materials, and working instructions that actually replace/relink the library.
A source URL or FFI boundary alone is insufficient. The current diagnostic
source-package tool does not yet know about this codec/relinking workflow. The
isolated archive can be rebuilt for review without Cargo:

```sh
out="$(mktemp -d /tmp/ps5-bt-sbc-archive.XXXXXX)"
python3 tools/test_sbc.py --build-archive "$out"
ar t "$out/libps5_bt_sbc.a"
```

This host archive command and the automated replacement-link check demonstrate
separate rebuildability. They are not by themselves a final LGPL section 6
distribution arrangement for a PS5 ELF.

Source availability establishes neither codec-patent clearance nor Bluetooth
qualification. No high-quality SBC, SBC-XQ, mSBC, SIMD, latency, quality,
headset compatibility, PS5 timing, or deadline result is claimed.

## Exact integration resume instructions

The lead/integration owner should resume in this order:

1. Review `git diff -- crates/bt-codec third_party/sbc platform/codec
   tests/codec tools/test_sbc.py docs/SBC-IMPLEMENTATION.md Cargo.toml
   Cargo.lock`. The root manifest/lock changes are parent-owned; the lock should
   contain only `bt-codec` depending on `bt-core`. Do not modify pristine bytes.
2. Rerun `python3 tools/test_sbc.py`, all four Cargo commands above, and
   `cargo fmt -p bt-codec -- --check`. A feature-on cross-target check is
   expected to fail before Python/CC and should be inspected separately.
3. Independently confirm the archive/source hashes, LGPL notices, and the two
   patch diffs. Review C integer/alias/output invariants and the Rust external
   storage borrow, raw-error preservation, poison and exactly-once cleanup.
4. Decide the final replacement/relink delivery layout and update the diagnostic
   source-package inventory before linking or distributing a combined ELF. The
   host archive is separately rebuildable and replacement-linked; do not add an
   allocator merely for this wrapper.
5. For a real target backend, keep generic C and `SBC_STATIC_ONLY`; retain
   `-mgeneral-regs-only`, no-SIMD, freestanding/PIC, no-red-zone and import
   inspection. Supply reviewed target headers/build inputs instead of reusing
   the host feature or macOS SDK header probe.
6. Integrate the existing safe Rust owner only into offline media flow first.
   Keep its caller storage alive and immobile for the full `Encoder` borrow.
   Keep codec failures visible; do not add hidden buffering, silence insertion,
   retry, bitpool fallback, RTP, or transport behavior in C.
7. After AVDTP negotiation exists, require the peer's negotiated range to
   contain the selected bitpool. The new Rust `bt-a2dp` packetizer advances RTP
   timestamps by the shared parameter model's 128 PCM frames for this selection
   and rejects MTU/short-output errors atomically. AVDTP session binding, pacing
   and expired-unsent-media policy remain Rust concerns.
8. Separately measure initialization/encode time and run repeated init/finish
   on actual PS5 before changing `Requires hardware`. Live A2DP remains blocked
   on transport ownership/credits and must not be inferred from these tests.

The parent task owns root Cargo/Makefile integration, the shared runtime,
README/CHANGELOG/ROADMAP/MVP updates, and any hardware or network action. This
codec task added only the new crate and its previously assigned codec/source/
test/documentation paths. Nothing was staged or committed here.
