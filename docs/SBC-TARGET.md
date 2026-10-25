# Target SBC self-test and relinking

Status: the generated-signal self-test executed successfully on the recorded
Slim / firmware 13.40 environment. Both init/finish cycles, all four frames and
the pinned digest matched the host, with user-confirmed normal wireless input.
See HW3-EVIDENCE.md for exact artifact identity and the complete log. This path
encodes a generated signal only; captured-audio and live Bluetooth integration
remain unimplemented. No hardware encode duration was measured.

## Target archive boundary

`tools/target_sbc.py` builds the selected BlueZ libsbc 2.2 generic encoder plus
the project-owned caller-storage wrapper as `libps5_bt_sbc.a`. It first verifies
the pristine source inventory and hashes, then applies all three patches with
zero fuzz in a temporary directory. The third patch selects the small
`ps5_bt_sbc_freestanding.h` compatibility surface only when
`SBC_STATIC_ONLY` is defined.

The target compile uses LLVM 18.1.8, `x86_64-sie-ps5`, Clang resource headers
only, and the reviewed flags:

```text
-ffreestanding -fno-builtin -fPIC -fno-stack-protector -fno-plt
-mno-red-zone -mgeneral-regs-only -ffunction-sections -fdata-sections
-DSBC_STATIC_ONLY -nostdinc
```

There is no target libc or host SDK header dependency. Object inspection
requires the exact combined undefined-symbol set recorded by the script. Every
internal `sbc_*` reference must resolve inside the archive; the only external
symbols may be `memcpy` and `memset`, supplied by the existing freestanding
runtime. Allocators, I/O, CPU dispatch and optimized primitive imports are not
allowed.

The Python API used by the PS5 builder is:

```python
archive = build_archive(clang, llvm_ar, output_dir)
inventory = source_inventory()
```

`build_archive` refuses a nonempty directory it does not own. Repeated builds
may replace only a directory bearing its exact schema/kind marker. It emits the
archive and `codec-build.json`, including tool versions, archive hash, exact
source hashes and symbol audit.

## Generated-signal payload

`ps5_bt_sbc_selftest_run` uses caller-owned aligned storage and performs two
fresh init/finish cycles. Each cycle encodes the same two generated 48 kHz
joint-stereo frames with 16 blocks, 8 subbands, loudness allocation and bitpool
32. The second cycle must be byte-identical to the first. The expected first
frame prefix is `9cfd206f`; the FNV-1a digest across all four 77-byte frames is
`14e0ec79`.

The report contains only schema, phase, raw codec/cleanup statuses, storage
requirements, counts and those deterministic summaries. It contains no PCM,
encoded frame, address, key or device identity. Input capacity, alignment and
overlap failures are explicit. Initialization, every encode and every cleanup
are finite; there is no allocation, sleep, retry, transport or hidden state.

A successful ELF link or matching digest is a codec/ABI result, not proof of
headset audio, capture format, timing, controller ownership or Bluetooth
coexistence. The first console run must record the complete report and verify
wireless DualSense before and after the test.

## LGPL replacement materials

The combined codec ELF must be accompanied by working replacement/relink
materials. The builder calls:

```python
manifest = export_relink(
    output_dir,
    app_objects,
    rust_archive,
    linker_script,
    codec_archive,
    lld=lld,
    readobj=llvm_readobj,
    objdump=llvm_objdump,
    output_name="ps5-bt-sbc-selftest.elf",
)
```

The export copies the ordered application objects, Rust archive, linker script,
default codec archive, complete selected library source/notices/patches and
build helpers. It emits hashes for every immutable input and a standalone
`relink.py`. The exporter immediately performs one baseline link, validates the
ELF with the repository structural inspector using the established loader
kernel-resolver allowance, and saves the link map, LLVM inspection,
disassembly and result hashes.

From inside an exported bundle, a recipient can replace only the library
archive and produce another inspected ELF:

```sh
python3 relink.py \
  --codec /absolute/path/to/rebuilt/libps5_bt_sbc.a \
  --output /absolute/path/to/relinked.elf \
  --lld /absolute/path/to/ld.lld \
  --readobj /absolute/path/to/llvm-readobj \
  --objdump /absolute/path/to/llvm-objdump
```

The fixed command retains the original entry point, linker script, 16 KiB page
alignment, garbage collection, no-undefined rule, loader lookup roots and
syscall wrapping. It uses an archive group so a replacement codec's
`memcpy`/`memset` dependencies can resolve from the supplied Rust runtime.
Immutable application materials are hash-checked before every relink; the
replacement codec is intentionally allowed to have a different hash, and its
actual hash is recorded with the result.

These materials support LGPL 2.1 replacement-link review for this specific
artifact. They do not waive attribution/source obligations, establish patent
clearance, or make the codec a Bluetooth-qualified implementation. Preserve
`COPYING.LIB`, upstream authorship, all selected source, local patches and the
working scripts with any distributed codec-enabled ELF.
