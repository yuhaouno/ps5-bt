# Software wave closeout: HW-1 artifact

Date: 2026-10-23. This development wave is complete on the software side.
Physical PS5 access is unavailable. **Next action: HW-1 physical validation.**
No PS5 execution, controller access, pairing or audio success is claimed.
PS5 Slim / firmware 13.40 remains the intended first target, unverified.
HW-2 has not started; no further implementation is scheduled in this wave.

## Implemented and built

- Portable safe `no_std` contracts, typed HCI command/event/ACL representation
  and capability decoding, bounded L2CAP framing/reassembly, validated shared SBC
  selections, and bounded SDP framing/data elements. No discovery client,
  connection state machine, codec encoder or media pipeline is implemented.
- Versioned small C ABI, safe Rust platform wrapper and explicit Unsupported
  adapter. Host-only USB descriptor parser/report and a separate opt-in native
  stack presence query are isolated from active transport.
- Original minimal startup/linker with three unmodified hash-verified SDK syscall
  inputs. No stock credential-changing CRT, kernel helper or proprietary binary.
- `make` produces `build/ps5/ps5-bt.elf`, with matching map, inspection,
  disassembly and clean-commit manifest. `make ps5-native-query` produces a
  separate optional artifact; do not use it for the first physical test.
- Offline corresponding-source packaging includes all tracked project source,
  tests and documentation, the selected SDK sources and their retained notices.

## Verification evidence

Available host: macOS arm64. Rust 1.90.0, freestanding x86-64 target, LLVM/LLD
18.1.8 and the source subset are pinned; exact native tool hashes are recorded
per build. See [TOOLCHAIN.md](TOOLCHAIN.md) for setup and observed environment.

Passed: 34 Rust tests, formatting, workspace check, Clippy with warnings denied,
15 C/Rust seam scenarios, strict C descriptor/query/main-hook tests, ASan/UBSan,
freestanding archive inspection, startup failure tests, source/toolchain failure
tests, 12 malformed ELF cases per variant, and source-package integrity/content/
determinism tests. Remote Linux/macOS CI execution and cross-host byte identity
are not claimed.

The baseline artifact is 54,168 bytes, ET_DYN x86-64, with three 16 KiB-aligned
RX/R/RW loads, 22 symbol-free RELATIVE relocations, and no imports, dependency
libraries, TLS or constructors. ELF SHA-256:

```text
a4a42640ec604e69aec535f5ddb015e8eafd1ebe6efe8a7386de7a7e2cb01cb0
```

An independent worker extracted the actual source package, checked all tracked
file bytes/modes and the three SDK hashes/notices, created the README-prescribed
local Git snapshot, and rebuilt in a fresh project/target directory with
`CARGO_NET_OFFLINE=true` and only the bundled SDK tree. The ELF and Rust archive
were byte-identical to the originals using the same installed toolchain. The
manifest changed only its local Git commit. Repeated source packaging was also
byte-identical for identical inputs. This verifies the available local rebuild
path, not a pristine OS installation, cross-host reproducibility or hardware.
Final manifests/source packages are regenerated after this documentation commit.

## Unverified behavior and active risks

Actual loader ABI, readable getpid stub layout, existing permissions, stderr
routing, stack/BSS initialization and clean return need physical evidence. The
strict stub guard returns 125 on an unsupported prefix without guessing offsets.
Wireless DualSense continuity, controller family/features/credits, firmware and
headset compatibility, audio capture, encoding, latency and stability are Unknown.

Native command/event/ACL ownership remains unresolved. Existing competing USB
readers are not an acceptable coexistence solution; no live backend is enabled.
A successful future stack-version query would not establish controller identity
or routing. The optional diagnostic OS call itself has no measured PS5 timing.
All pairing, test-tone and captured-audio milestones remain incomplete.

## Upstream and licenses

SDK v0.45 at `a7685ce8e97ce1f8a4f38253a41d54241c9124f1` supplies only
`crt/syscall.c`, `crt/syscall.h` and `crt/payload.h`, under their preserved
John Törnblom GPL-3.0-or-later notices, with no local patches. Corresponding
source accompanies the diagnostic ELF. Original project work is GPL-3.0-or-later.
See [UPSTREAM.md](UPSTREAM.md) and [source delivery](SOURCE-DISTRIBUTION.md).

PlayPods and HearBridge remain credited behavioral references, not imported
Bluetooth/audio code. libsbc 2.2 was audited as LGPL-2.1-or-later but is not
imported or linked; allocator/storage, replacement/relinking and distribution
work remain future gates. No codec or patent-clearance claim follows.

## First action when hardware becomes available

Follow [the exact HW-1 procedure](HARDWARE-TEST.md) with the baseline
`build/ps5/ps5-bt.elf` and a compatible existing loader. Expected logs:

```text
event=rust_entry abi=1 phase=bootstrap
event=adapter_unavailable status=unsupported
```

Expected payload result is 2. Capture the full log, actual loader identity,
artifact hash, observed return/error, loader responsiveness and wireless
DualSense behavior before/during/after. No headset or audio is expected. Stop
there and review physical results before considering any HW-2 experiment.
