# SBC integration gate

Audit date: 2026-10-23. Host evidence only; no PS5 codec execution, audio
streaming or qualification. This document selects a candidate and proposes an
interface. At that audit snapshot no encoder source was imported. The subsequent
2026-10-24 host-only implementation is documented in SBC-IMPLEMENTATION.md and
third_party/sbc/VENDORED.md; its source, caller-storage changes and tests now
exist. SBC-TARGET.md and HW3-EVIDENCE.md now record the independently selected
target archive/self-test and its PS5 execution; live audio remains unverified.
The original audit below is historical design evidence, not the current
implementation status or a permanent generic-only/Rust-only restriction.
Current experimental/optimized/transitional module scope follows AGENTS.md;
preserve the existing codec ABI, generic build and source/relink checks until
an explicitly reviewed implementation updates them.

## Authoritative source and license

The direct [BlueZ libsbc repository](https://git.kernel.org/pub/scm/bluetooth/sbc.git)
was cloned outside the repository. `git ls-remote` independently resolved both
HEAD and peeled release tag `2.2` to
`b3deb8a5dcfb42d8c10ba1f2f1ac9bd7bf7271cc` (Release 2.2, 2026-02-09).
Annotated tag object: `96cde3ae64b963a91f4432693700bf36c8512a3b`.
This confirms the inherited PlayPods research pin independently; no PlayPods or
other application source is selected. The newest release in the
[authoritative directory](https://www.kernel.org/pub/linux/bluetooth/) observed
on the audit date is 2.2.

Archive: [sbc-2.2.tar.xz](https://www.kernel.org/pub/linux/bluetooth/sbc-2.2.tar.xz).
SHA-256: `a1ada76ef35e5af9c2fbd063754dc9e37a8d989417c6eb1ecebb089b1383ae9e`.
This digest matches downloaded
[sha256sums.asc](https://www.kernel.org/pub/linux/bluetooth/sha256sums.asc).
No signing key or signature was authenticated; a matching digest is integrity
and provenance evidence, not authenticated publisher identity. The archive's
`sbc.c` and `sbc_primitives.c` were byte-compared with the direct Git checkout.

Select only `COPYING.LIB`, `AUTHORS`, and these library files under `sbc/`:
`sbc.c`, `sbc.h`, `sbc_private.h`, `sbc_math.h`, `sbc_tables.h`,
`sbc_primitives.c`, `sbc_primitives.h`. Each of these seven code files was read
and explicitly declares **LGPL-2.1-or-later**. The root `COPYING.LIB` supplies
LGPL 2.1. Notices credit Marcel Holtmann, Henryk Ploetz, Brad Midgley, Nokia
Corporation, and Intel Corporation as applicable per file. Preserve actual
per-file notices, `AUTHORS`, source provenance, and the complete license text.
The proposed generic-only patch eliminates includes of architecture headers;
no optimized assembly files, upstream tools, application code or build-system
implementation is selected. The upstream decoder remains in `sbc.c`; only the
encoder would be exposed by our wrapper.

Original selected-file SHA-256 values:

| File | SHA-256 |
| --- | --- |
| COPYING.LIB | ec60b993835e2c6b79e6d9226345f4e614e686eb57dc13b6420c15a33a8996e5 |
| AUTHORS | e57876fcdfa3cfdf88db7f6ed971c1f44fda8e9a33c8c1d9999903945e75fd2c |
| sbc/sbc.c | 42d16642f3eebf51f7f9e84a03f62a1d722c03414ed3265ba183bdaa0d9dccc4 |
| sbc/sbc.h | ab2e351d97ee25d7836902465ef1e5589c7bc07988e5c2bdc484ac1ffee754ab |
| sbc/sbc_private.h | 16dd9e1621d73c387ac4b2027320028e963e5a2d09013d9dccec4ede3142a140 |
| sbc/sbc_math.h | 4cbe03013967fd9872c86677f4ff398994010706ffc6f77d39792f44bc8f2fdf |
| sbc/sbc_tables.h | fe57949c20000aa3cacc359887ea1ea0cb31146371beb2488f732bc6bdf17933 |
| sbc/sbc_primitives.c | 1badabd8eee20c25a1c0a684354e53e68e8901ca571493060965016ca9c0ec74 |
| sbc/sbc_primitives.h | 9cf89824889d2e0464d4135808ead9a7d70ee223d6e1b73e43969fa969a3951e |

LGPL linking is compatible with this project's original GPL-3.0-or-later work;
retain the library's own grant. Distribution must provide prominent library
attribution, license text, complete modified library source, patch notices and
build scripts, plus application source/object materials and working relinking
instructions under an appropriate LGPL 2.1 section 6 arrangement. Keep the
library separately rebuildable; test replacement/relinking before releasing an
ELF. A source URL alone is insufficient. Source availability does not establish
patent clearance, Bluetooth qualification or permission to redistribute other
libraries. See [LGPL 2.1](https://www.gnu.org/licenses/old-licenses/lgpl-2.1.html)
and [UPSTREAM.md](UPSTREAM.md).

## Actual build and runtime requirements

An external experiment compiled the original `sbc.c` and generic primitives with
LLVM 18.1.8 for `x86_64-sie-ps5`, using external SDK headers at the documented
SDK revision. Flags: `-std=gnu11 -O1 -ffreestanding -fno-builtin -fPIC
-fno-stack-protector -fno-plt -mno-red-zone -mgeneral-regs-only
-ffunction-sections -fdata-sections`. No ELF was linked with the encoder.
The original `sbc.c` compiles, with three existing Clang ignored-attribute
warnings at its struct-field alignment declarations; a production strict build
must use a documented narrowly scoped warning setting or fix those declarations.
No floating-point operation, OS call, thread, TLS or libm symbol was found in
the selected target objects. Generic fixed-point arithmetic uses integer/pointer
arguments and memory buffers. `-mgeneral-regs-only` also prevents compiler SIMD.

`llvm-nm -u` on original `sbc.c`: `malloc`, `free`, `memcpy`, `memset`,
`sbc_init_primitives`. On unmodified x86 primitives: `__cpu_indicator_init`,
`__cpu_model`, `memcpy`, `sbc_init_primitives_mmx`, `sbc_init_primitives_sse`.
The intrinsic CPU initializer is unconditional on x86 even if optimized
backends are disabled. Thus the stock source needs more than an allocator.

The experimental generic-only adaptation removes five architecture includes,
the x86 CPU-dispatch helper, and optimized dispatch calls. It leaves all filter,
quantizer and packing algorithms unchanged. Its object requires only `memcpy`;
`sbc_init_primitives` is supplied internally. Future import should preserve
pristine source plus a numbered `0001-generic-only.patch`, document the changed
files/date, and test generic output against the unchanged decoder. Do not
satisfy CPU-runtime dependencies with fabricated stubs or claim SIMD support.

Measured on both host arm64 and target x86-64: `sizeof(sbc_t) = 32`,
`sizeof(struct sbc_priv) = 5600`, private alignment 16. `sbc_init` makes **one
5615-byte malloc request**, aligns the resulting private pointer to 16 bytes,
and keeps the original base for `free`. No allocation occurs in `sbc_encode`.
A tracked host run observed one allocation across init plus 10,000 encodes,
zero frees until finish, and exactly one free at finish. No allocation sizing
assumption should cross into Rust as a copied upstream structure definition.

Recommendation: retain proven upstream initialization and add a narrow opaque C
wrapper, with a bounded fallible initialization allocation and no allocation per
frame. The wrapper's 32-byte public context can be caller-owned opaque storage;
the private 5615-byte allocation lives until explicit close/drop, and is never
retained beyond its owning encoder. Allocation failure maps to an explicit
project error, never success. **Current minimal PS5 runtime has memcpy/memset but
no allocator or libc module integration**, so executable codec integration is
blocked until those dependencies are source-audited and provided.

Alternative if no approved allocator path exists: a small caller-storage
initializer beside upstream private definitions, using an aligned 5600-byte
private arena and C-managed context, with no changes to codec algorithms.
The wrapper must query/check storage size and alignment and bind its Rust
lifetime; a movable self-referential Rust object is unacceptable. C may instead
use a single bounded pool with exclusive fallible ownership; release/reacquire
and multiple-instance failures need tests. Neither alternative is implemented
or approved here. Do not rewrite the codec to remove its small init allocation.

## Shared model and proposed encoder API

Parameter and framing evidence: [A2DP v1.4, sections 4.3.2.6, 4.3.3.1 and
B.9](https://www.bluetooth.org/DocMan/handlers/DownloadDoc.ashx?doc_id=544797).
The [Bluetooth SIG A2DP test suite p21, sections 3.2.1.1.5
and 3.2.2](https://files.bluetooth.com/wp-content/uploads/dlm_uploads/2025/02/A2DP.TS_.p21.pdf)
confirms legal sample clocks, block/subband choices, mode-dependent bitpool
bounds, and the 320/512 kbit/s mono/two-channel caps. This test suite is not
itself a Bluetooth Specification or a conformance certificate. Frame-length
arithmetic was checked against the pinned library's `sbc_get_frame_length`
and the synthetic experiment. Pure-model validation compares rational bitrate
without rounding; it does not imply all represented settings are implemented
by our future encoder. Min/max capability bitpools are 2..250; a negotiated
range still needs selection validation against mode and bitrate.

Lead decision: pure `SbcSampleRate`, `SbcChannelMode`, `SbcBlocks`, `SbcSubbands`,
`SbcAllocation`, `SbcBitpoolRange`, and `SbcParameters` belong in `bt-core`.
Codec and future AVDTP consume the same model. Selected enum values are distinct
from capability masks. A range is negotiated state, not proof of a connected
headset. Check selected bitpool against both that range and SBC mode/subband and
A2DP bitrate limits. No raw upstream structs or bitfield layouts cross the ABI.

The initial encoder subset is 48 kHz stereo or joint stereo, 16 blocks,
8 subbands, Loudness, bitpool 2..=53. A conservative proposed initial bitpool is
32, only when accepted by the peer. Unsupported valid configurations return
`Error::Unsupported`; malformed/out-of-range selections return `Error::Invalid`.
No implicit fallback or dynamic bitpool change. This subset does not claim
high-quality SBC, SBC-XQ, mSBC, or other codec support.

Proposed Rust contract:

```text
Encoder::new(parameters, negotiated_range, storage/allocator) -> Result<Encoder, Error>
Encoder::encode_frame(&mut self, pcm: &[i16], output: &mut [u8])
    -> Result<EncodedFrame { pcm_frames: usize, bytes: usize }, Error>
```

Accept exactly 256 native i16 interleaved samples (128 stereo PCM frames),
never an arbitrary byte slice or float-valued C argument. Input/output borrows
end when the call returns; only the encoder's internal allocation survives.
Rust validates exact sample count and output capacity before calling C.
Short/extra PCM is Invalid; insufficient output is BufferTooSmall; neither
consumes PCM nor advances encoder state. C success requires exactly 512 consumed
PCM bytes and the calculated frame length written. Unexpected upstream negative
statuses retain their code as `Error::Backend(i32)`; surprising positive lengths
are IntegrityLost and poison the encoder. Preserve sign/width conversion checks.

For this subset, frame length is `12 + 2*bitpool` (stereo) or
`13 + 2*bitpool` (joint). At bitpool32 this is 76/77 bytes, respectively;
at53 it is 118/119. A caller-owned **119-byte frame buffer** is sufficient for
this subset; it is not a universal SBC maximum. RTP packetization is separate.
Each frame is 128/48000 seconds; RTP timestamps advance by 128, independent
of channel count. Do not round frame duration to integer milliseconds.

The actual C ABI should expose only `ps5_bt_sbc_init`, `ps5_bt_sbc_encode`,
`ps5_bt_sbc_finish` plus storage size/alignment queries if caller-owned. These
are proposed project symbols, not upstream APIs. Use integer scalar config,
opaque byte storage/handle, i16 pointer plus sample count, byte pointer plus
capacity, explicit output counts and fixed i32 status. Keep `sbc_t` private.
Encoder is single-owner, mutable, not Send/Sync; finish is consumed exactly once.
C FFI contracts and pointer validation need dedicated hostile-boundary tests.

`PcmSource` remains interleaved f32 with whole-frame semantics. Rust is the
preferred owner of finite sample validation, clamped/rounded i16 conversion,
generated tone, PCM buffering,
queue capacity and discontinuity handling. These are later authorized audio
work; no codec buffering, capture restart, unbounded queue or silence insertion
is hidden in the codec wrapper. Scoped transitional C protocol/connection
modules and platform conversion adapters follow AGENTS.md without weakening
this wrapper's contract. A NaN policy and precise saturation/rounding vectors
must be agreed before implementing capture conversion.

## Experiment evidence and remaining gates

External work directory: `/tmp/ps5-bt-sbc-audit.I7uRCF` (ephemeral, not a build
dependency). Apple Clang 21 built the unmodified `sbc.c` plus adapted generic
primitives and an original synthetic test driver. The pinned upstream decoder
in the same library decoded output; this is a reference round trip, **not an
independent implementation or official SIG conformance test**. FFmpeg was not
available. No test vector assets or proprietary SIG reference source imported.

Passed 156,000 frames: both stereo modes, every bitpool 2..53, 375 frames each
of silence, 440/880-Hz left/right sine, periodic impulses, and alternating
full-scale PCM. Checked complete 512-byte input consumption, exact encoded
length, output guard bytes, sync/bitpool fields, decoded 48kHz/mode and whole
512-byte PCM output. Silence decoded to all zeros. Full AddressSanitizer and
UndefinedBehaviorSanitizer run passed with no suppression (the preliminary run
also disabled shift/overflow checks; the subsequent full run is authoritative).
At joint/32, measured sine correlation after an integer 73-sample codec delay:
left 0.9999999469, right 0.9999999330. This measures this generated host signal,
not acoustic latency, PS5 quality, CPU performance or interoperability.

Required next checks: persist a reproducible original driver in the eventual
codec crate; failure-allocation and exactly-once cleanup tests; raw C bounds,
wrong configuration, null pointer and integer-conversion tests; safe Rust
prevalidation/no-state-change tests; byte-for-byte generic/reference fixtures;
independent decoder where available; target link/import inspection and relink
exercise; actual PS5 initialization/encode/deadline measurements. A2DP RTP/HCI,
headset negotiation, audible tone, captured audio, DualSense coexistence and
latency remain **Requires hardware** and dependent on transport ownership.
