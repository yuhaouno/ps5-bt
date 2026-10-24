# USB descriptor diagnostics foundation

The parser/report foundation described below remains I/O-free. A separate
opt-in read-only probe is now implemented in USB-PROBE.md and its first actual
console results are recorded in HW2-EVIDENCE.md. The former integration proposal
below is retained as design history, not a claim that no probe exists.

Status: **host-tested parser and report encoder only**. No USB node is opened,
no ioctl is issued and no PS5 hardware result is claimed. The target remains
PS5 Slim firmware 13.40 with an existing, independently reviewed runtime.

`platform/ps5/diagnostics/usb_descriptors.h` defines a project-owned metadata
interface. It is separate from C ABI v1, `ps5_bt_init`, `BluetoothTransport`
and `PcmSource`. Parsing descriptors grants no transport ownership. The active
Bluetooth adapter remains Unsupported.

## Parser and report contract

`ps5_usb_parse_config(data, len, out)` accepts one complete USB configuration:
`len` must equal the little-endian `wTotalLength`. It checks the configuration,
interface and endpoint minimum lengths, every descriptor's remaining extent,
unique interface/alternate pairs, the number of distinct interfaces, default
alternate availability and exact endpoint counts within each alternate.
Endpoints require an enclosing interface and a valid nonzero endpoint address;
duplicate endpoint addresses within one alternate are rejected. Unknown
descriptor types are skipped only after their full declared extent is checked.
Extra bytes beyond a declared configuration are rejected.

Every interface/alternate matching class/subclass/protocol `e0/01/01` is
preserved in descriptor order, together with **all** its endpoint descriptors.
Alternate zero and nonzero settings remain distinct. Endpoint addresses,
attributes, raw `wMaxPacketSize` and interval are retained. The parser does not
choose an interrupt IN/bulk IN/bulk OUT tuple, infer throughput, select an active
backend, or treat several groups as independent radios. A class match is only
a descriptor candidate. HCI permissions, controller state and native event
routing are Unknown.

Memory and work are bounded: at most 65,535 input bytes, 512 descriptors,
32 candidate groups and 128 retained endpoints. Exceeding a limit returns
Capacity, never a truncated successful inventory. The result is built in local
storage and assigned only after the entire configuration passes; every failure
leaves caller output unchanged. There is no heap, I/O, retained pointer, global
mutable state or callback. Duplicate interface detection rescans only the
bounded, already-validated prefix (at most quadratic in 512 descriptors).
Input/output must be distinct valid objects; concurrent mutation is unsupported.
This is structural validation, not a complete USB conformance validator:
class-specific descriptors and speed-dependent endpoint semantics are opaque.

`ps5_usb_vendor_label(vid)` reports `1286` as Marvell/NXP and `0e8d` as MediaTek;
every other VID is Unknown. Raw VID/PID remain authoritative. A label is never
assigned from the console model, PID alone or an active-profile fallback.

`ps5_usb_encode_report` produces deterministic ASCII `ps5-usb-metadata-v1`
records. All numbers use fixed-width lowercase hexadecimal without `0x`.
The output includes raw VID/PID, the reporting label, configuration metadata,
candidate interfaces/alternates/endpoints and `transport=Unsupported`.
No USB strings, serial numbers, Bluetooth addresses or user data are encoded.
It writes the full report or leaves the destination unchanged, and never writes
a terminating NUL. Capacity errors return the required byte count. Query that
count with a null destination and zero capacity; caller-owned storage can then
be passed unchanged to the existing length-delimited platform logger. The
encoder accepts parser-produced records and defensively checks array bounds;
it is not a deserializer for arbitrary public structs.

## Integration proposal and runtime dependencies

`make usb-descriptors-test` compiles and runs the two C files shown below and is
included in `make check`; `make check-c-sanitize` adds ASan/UBSan. No Rust
dependency or ABI change is needed. Parser/report code can enter a future diagnostic
ELF independently of the transport; it does not presently execute from main.

The next runtime hook should be a separate, opt-in metadata entry such as
`ps5_usb_probe_metadata(const struct ps5_usb_metadata_ops *, ...)`, never a call
inside `ps5_bt_init`. This is a **proposal, not a shipped API**. It needs verified
resolved `open`, `ioctl`, `close`, bounded directory-enumeration operations and
raw error access from the minimal startup. B1 currently resolves only its
private logging/exit operations, so no resolver or function signature is assumed
here. Discovery must enumerate bounded actual candidate nodes rather than
hardcoding a console-specific node or endpoints. Exhausted enumeration/storage
must be reported incomplete, not successful discovery.

Use the external, pinned SDK v0.45 headers `<dev/usb/usb.h>` and
`<dev/usb/usb_ioctl.h>` for actual ioctl constants/layouts, never locally invented
numbers or ABI replicas. The only proposed metadata requests are
`USB_GET_DEVICE_DESC`, `USB_GET_DEVICEINFO` and `USB_GET_FULL_DESC`. Zero their
records, bound `ugd_maxlen`, set explicit configuration index, and reject
`ugd_actlen` beyond capacity or inconsistent with the complete configuration.
Opening must use `O_RDONLY`; denied permission is a reported blocker without
an O_RDWR fallback or escalation. Preserve open/ioctl/close errors separately;
close every acquired descriptor exactly once even after a parse failure.

The future probe excludes `USB_DO_REQUEST`, all `USB_FS_*`, endpoint reads or
writes, detach/claim/reset, alternate/configuration changes, HCI commands,
scan changes, capture and credential/kernel mutations. Even descriptor ioctl
noninterference and O_RDONLY compatibility require hardware verification;
source review cannot establish the proprietary kernel's behavior.

## Host validation

From the repository root (executables stay in ignored `build/`):

```sh
mkdir -p build/host/usb-diagnostics
cc -std=c11 -Wall -Wextra -Werror -pedantic \
  -Iplatform/ps5/diagnostics \
  platform/ps5/diagnostics/usb_descriptors.c \
  tests/platform/usb_descriptors_test.c \
  -o build/host/usb-diagnostics/test
build/host/usb-diagnostics/test
cc -std=c11 -Wall -Wextra -Werror -pedantic \
  -fsanitize=address,undefined -fno-omit-frame-pointer \
  -Iplatform/ps5/diagnostics \
  platform/ps5/diagnostics/usb_descriptors.c \
  tests/platform/usb_descriptors_test.c \
  -o build/host/usb-diagnostics/test-sanitized
build/host/usb-diagnostics/test-sanitized
```

Tests cover every fixture truncation, all 65,536 declared total lengths,
all endpoint-address bytes, all descriptor-length bytes at known header offsets,
malformed tails, duplicate and missing groups/endpoints/default alternates,
multiple interfaces/alternate settings, exact resource limits and overflow,
all 65,536 VIDs, deterministic reports, all short report capacities and 20,000
deterministic byte mutations. Rejected parses retain a poisoned output intact;
short report writes leave the destination intact. Fixtures are original,
synthetic USB metadata, not captures from a PS5 or an upstream controller.
These tests do not prove USB access, firmware support or DualSense coexistence.

## Provenance and remaining hardware evidence

Implementation is original project code, GPL-3.0-or-later. No upstream parser,
SDK header, ioctl definition or driver code is copied or vendored. Descriptor
field positions were checked against the externally installed SDK headers at
v0.45 / `a7685ce8e97ce1f8a4f38253a41d54241c9124f1`.
The vendor labels and candidate-class behavior are research references from
HearBridge `btchip.h`/`usb_hci_desc.c` at
`cd5b7fce1b8226d8850cdd306437550098c6a62b`; they are not compatibility claims.
See [platform research](PS5_PLATFORM.md#read-only-usb-descriptor-probe) and
[source/license inventory](UPSTREAM.md) for exact pinned source links and credit
to X-F1REBALL-X, HearBridge contributors, ZiZc3, John Törnblom and SDK contributors.

Required hardware evidence remains: reviewed loader identity/entry behavior,
artifact hash and firmware record, successful minimal runtime entry/return,
actual read-only node/ioctl outcomes including raw denials, complete descriptor
records, exactly-once cleanup, and wireless DualSense before/during/after the
probe. Native HCI routing/command credit ownership remains a separate blocker
even if metadata succeeds. No live probe or hardware success is fabricated.
