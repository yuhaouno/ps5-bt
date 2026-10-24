# Read-only USB probe

The opt-in diagnostic entry `ps5_usb_probe_run()` is separate from transport
initialization. It enumerates actual `/dev` directory entries, opens matching
`ugen<decimal>.<decimal>` names read-only and requests only device descriptors
and complete configuration descriptors. It does not establish Bluetooth
ownership or provide a streaming backend. Hardware outcomes must be recorded
separately from the host tests below.

## Target adapter and bounds

The first metadata ELF SHA-256 was
`f5abd0e7e8d119a5d67fda0b1dc82adc9d872d8fc5d23c41cf2d9958794b2ab9`.
The parent hardware session recorded successful original-project enumeration of
VID:PID `1286:2059`: four interfaces, with HCI-class groups at interface 0
(endpoints `81/01/82/02`) and interface 2 (`84/04/85/05`), plus SCO interfaces
1 and 3. The user confirmed wireless-controller operation. This is descriptor
and diagnostic-cleanup evidence only: the second HCI group is a candidate, not
proof of an independent controller, command route or available transport.

Compile `platform/ps5/diagnostics/usb_probe.c` with
`PS5_BT_USB_PROBE_TARGET`, the project include directories, and the external
pinned SDK's `include/freebsd` and `include` directories. The adapter includes
the actual SDK `sys/dirent.h`, `sys/fcntl.h`, `sys/syscall.h`,
`dev/usb/usb.h` and `dev/usb/usb_ioctl.h`; it does not locally duplicate their
layouts, flags, ioctl values or syscall numbers. The SDK source pin is
`a7685ce8e97ce1f8a4f38253a41d54241c9124f1` (v0.45), as recorded in
[UPSTREAM.md](UPSTREAM.md). SDK headers retain their individual notices.
Project probe implementation and synthetic fixtures are original
GPL-3.0-or-later code; no upstream probe implementation was imported.

The only target syscalls are the existing checked bridge's `open` with
`O_RDONLY`, `getdents`, `ioctl` for `USB_GET_DEVICE_DESC` and
`USB_GET_FULL_DESC`, `USB_GET_CONFIG` and `USB_GET_IFACE_DRIVER`, and `close`.
There is no `O_RDWR` fallback, permission
change, arbitrary control request, string/serial request, endpoint I/O,
detach, configuration/alternate selection, HCI command or controller reset.
The installed loader's existing resolver setup is outside this probe; the
probe itself never reads or writes kernel memory.

Bounds are 64 matching nodes, 4,096 returned directory entries, at most 64
directory reads of 4,096 bytes, 31-byte candidate names, 65,535 configuration
bytes and 16,384 report bytes. A device's nonzero `bNumConfigurations`
determines the explicit zero-based configuration indices requested (at most
255). The existing parser adds its own descriptor/group/endpoint bounds.
Truncated descriptors, mismatched total lengths, parser/report capacity,
node/entry/read limits and all syscall/cleanup failures produce an incomplete
inventory, never a truncated success.

Every acquired device descriptor is closed once, even if metadata or parsing
fails, and the directory descriptor is closed once after enumeration. Failed
close is reported without a retry: a close retry could affect a reused fd.
No device fd is returned or retained after the synchronous call. Descriptor
storage is private static BSS so the loader stack is not used for the roughly
84 KiB workspace; the entry is deliberately nonreentrant. The directory adapter
uses an additional bounded 4 KiB local buffer.

Device/configuration records are zero-initialized. Full descriptors set the
configuration index, data pointer and maximum length explicitly; returned
length must fit the caller's buffer and pass the existing complete-configuration
parser before reporting. Device strings and serial fields are never requested.
The report retains raw VID/PID, validated HCI-class interface/alternate groups
and endpoints, including nonzero alternates, without selecting a transport.

### Active configuration and driver-binding extension

The second probe preserves the first artifact identity above and adds only
read-only binding metadata. SDK v0.45 supplies both additional ioctls and
their actual records; no further SDK header dependency is introduced.
Behavior was checked against FreeBSD source revision
`d106002967eff712894ab8b5337bbbff106ce5e0`,
[`sys/dev/usb/usb_generic.c`](https://github.com/freebsd/freebsd-src/blob/d106002967eff712894ab8b5337bbbff106ce5e0/sys/dev/usb/usb_generic.c#L719).
Its `USB_GET_CONFIG` returns the current zero-based configuration index, not
the descriptor's configuration value. Only that successfully queried and
validated full configuration supplies the interface count; failure or an
out-of-range index causes no guessed interface queries.

At most 32 interface **indices**, `0..bNumInterfaces-1`, are queried. These
indices are not assumed equal to descriptor interface numbers. The driver
request sets its interface index and a 128-byte zero-initialized buffer.
Returned length must fit that buffer. Only a nameunit token containing ASCII
letters, digits, underscore, dot or hyphen, terminated by colon or NUL, may
enter the log. The remainder, which can contain a device description, never
enters the log. A successful empty string, including FreeBSD's zero
`ugd_actlen` empty-binding path, is labelled `Unknown`, never "free" or
"unowned". Invalid tokens and syscall errors remain explicit. Successful
nonempty names still report `ownership=Unknown`.

This extension is a metadata snapshot, not a driver lock or routing proof;
binding/configuration can change between requests. It performs no claim,
detach, alternate selection, endpoint read, USB FS operation or HCI command.
Its hardware results must be recorded separately after the second run.

## Results and limits

`ps5_usb_probe_with_ops` returns zero only after directory completion, successful
metadata reads/parsing and cleanup; one marks an incomplete inventory; minus
one is invalid caller arguments. `ps5_usb_probe_run` returns that result unchanged.
The outer diagnostic main owns its final lifecycle exit mapping; return zero
does not replace the Rust adapter's separate Unsupported status.

Logs retain negative normalized syscall results separately for directory
open/enumeration, device open, descriptor ioctls and each close. Parser values
are labelled `parse_configuration`, not errno. Project-local directory failures
have reserved results: `-10001` malformed directory records/length,
`-10002` exhausted directory-read budget, and `-10003` overlong ugen-prefixed
name. They are logged as `local_error` with a distinct `incomplete` reason,
never as a raw syscall errno. Ordinary negative enumeration syscall results
remain unchanged under `raw_result`. A complete inventory with zero
nodes is not a found Bluetooth controller. The metadata encoder always reports
`transport=Unsupported`.

The first deployed source ignored names longer than 31 bytes and used an
ambiguous `-1` for local directory failures. The offline revision now marks an
overlong ugen-prefixed name incomplete while ignoring unrelated long names;
an unterminated ugen-prefixed name supplied by a callback also fails closed.
No unknown names are opened by guessing node numbers. This boundary revision
is host-tested only and does not inherit hardware verification from either
earlier ELF. The second binding-query build was deployed but its execution was
not confirmed; see [hardware evidence](HW2-EVIDENCE.md).

Even read-only descriptor ioctls could interact with a proprietary kernel's
native driver. Source and host tests do not prove noninterference. Hardware
acceptance needs artifact hash/source state, actual raw read-only outcomes,
complete reports, final lifecycle/cleanup and human confirmation that wireless
DualSense worked before/during/after. No successful descriptor inventory grants
permission to consume native HCI events or send commands/ACL packets.

## Host checks

`tests/platform/usb_probe_test.c` injects synchronous project-owned callbacks;
it performs no real USB syscalls. Tests cover a valid synthetic HCI interface,
all configuration indices, fd zero, denied directory/device opens, descriptor
ioctl errors, malformed device/configuration records, oversized returned length,
enumeration and close failures, exactly-once cleanup, exact node/entry limits
and their overflow, irrelevant names and invalid callback tables.
The binding extension also tests raw failures, invalid active configuration,
zero-length empty bindings, successful sanitized names, description exclusion,
control-byte rejection and oversized returned driver length.
Local-error labels are checked separately from raw syscall failures. Tests also
reject unterminated ugen-prefixed names and verify malformed/path-like names
are never opened. Target directory byte-layout validation is cross-compiled,
not executed by the callback tests.

```sh
cc -std=c11 -Wall -Wextra -Werror -pedantic \
  -Iplatform/ps5/diagnostics \
  platform/ps5/diagnostics/usb_probe.c \
  platform/ps5/diagnostics/usb_descriptors.c \
  tests/platform/usb_probe_test.c -o /tmp/ps5-usb-probe-test
/tmp/ps5-usb-probe-test
```

Add `-fsanitize=address,undefined -fno-omit-frame-pointer` for sanitizer
validation. These tests exercise the callback core, not the PS5 directory
ABI, ioctl semantics, device permissions or native coexistence. Cross-compiling
the target adapter checks SDK header compatibility but not hardware behavior.

## Continuation checklist

1. Preserve the deployed ELF, manifest and exact source hash before edits.
2. Run only the opt-in metadata diagnostic, with the known baseline runtime.
3. Retrieve its durable log, retaining operation-specific raw failures.
4. Ask for wireless-controller confirmation; do not infer it from successful
   payload exit or USB descriptors.
5. The long-name and local-sentinel diagnostics are now fixed offline. Preserve
   the new artifact identity and retest on hardware when it becomes available.
6. Keep active transport and capture separate; no endpoint operation may be
   silently added to this read-only entry.
