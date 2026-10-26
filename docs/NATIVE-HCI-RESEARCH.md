# Native Bluetooth ownership research

Snapshot: 2026-10-23; hardware-result audit updated 2026-10-25. Target reported
by the user: PS5 Slim, firmware 13.40. The optional native presence query has now
executed: its export resolved and the single call returned -1. Own USB metadata
also found two HCI-class interface groups on `1286:2059`. Neither result
establishes controller ownership, command/event routing or credits.

**Result:** this bounded public-source search establishes a read-only native
stack presence query worth testing, but no usable PS5 command/event/ACL broker.
Keep `BluetoothTransport` Unsupported. The query below can narrow the next
investigation without consuming USB input or submitting HCI commands.

Policy clarification, 2026-10-26: the research results and existing candidate
bounds below remain evidence, not a permanent ban on implementing or proposing
other mechanisms. Source-backed isolated diagnostics follow AGENTS.md's
experimental admission rules and may measure an unresolved routing/ownership
fact without pre-proving that observation. Existing required API/layout,
traffic bounds, owned ACL allowances, cleanup and scope limits still apply.
This report supplies no new active candidate and does not approve competing
USB readers, timer-based credit refill or system-driver mutation.

## Post-HW2 ownership decision and next bounded experiment

The runtime facts do not reveal a usable broker. The native query's resolved
symbol plus `-1` result proves that this specific call path executed, not that a
FreeBSD Bluetooth stack or controller node is available. USB interface 2 has a
second set of HCI-shaped endpoints, but descriptor class and endpoint numbers do
not establish a separate command state, event route, connection-handle domain or
ACL credit pool.

The renewed 2026-10-25 window has now executed the clean `942eb9d` binding
candidate. All four driver queries returned 0, but names were `Unknown` and
binding classification stayed unknown. Three unrelated device-node opens still
failed; owned cleanup succeeded and the user confirmed normal wireless input.
See HW3-EVIDENCE.md for the exact artifact and complete log. This closes the
binding execution gap without establishing a dispatcher or changing the active
transport gate. The separately selected
[unconnected native L2CAP socket availability probe](NATIVE-L2CAP-PROBE.md)
has now executed: the UDP control and close succeeded; Bluetooth creation
returned -1/errno 47 (`EAFNOSUPPORT`), with complete return and normal wireless
input. This closes the requested socket branch for the current environment.
A different native channel route could leave HCI and credits with the system,
but no usable alternate interface is yet established.
The one-command raw-HCI acceptance test below still requires a source-proven
broker registration/ACL-send ABI; there is no ready active-HCI artifact.

The pinned SDK headers expose the FreeBSD-shaped `AF_BLUETOOTH`, raw HCI protocol
134 and `SIOC_HCI_RAW_NODE_LIST_NAMES`. They also mark Netgraph list-names as a
read-only message. That is compile-time interface evidence only. No inspected
PS5 source establishes that firmware 13.40 implements this socket/ioctl path,
that its ioctl has a bounded/cancellable wait, or that opening it preserves the
native owner's traffic. In the pinned FreeBSD source, raw HCI receive data is
copied to matching sockets, but outbound raw HCI accepts command packets rather
than providing this project's required command-and-ACL transport. A monitor or
command socket would still lack owned handles and enforceable ACL reservations.
Therefore no raw-socket or direct-USB active candidate is authorized now.

The completed USB binding test does not need repeating. The new availability
probe performs only one control socket creation/close and one unconnected
L2CAP creation/close, preserving immediate errno and cleanup status. It never
opens raw HCI, binds, connects or reads events. Its 15-second external observation
ceiling is not a cancellable kernel-call deadline. Any unresolved call or failed
close stops the experiment; successful allocation alone proves no route.
The completed negative result does not justify repeating the test, scanning
protocol values or changing credentials. The source comparator maps prison
rejection to errno 43, whereas missing domain returns 47; PS5 internals remain
Unknown. See NATIVE-L2CAP-PROBE.md for the reviewed interpretation.

If later source evidence identifies an actual dispatcher/broker, its first
active acceptance diagnostic must be separately named and keep these bounds:
one registered owner, one ordered receive stream, one Read Local Version command,
a one-second command deadline and ten-second whole-test ceiling, no retry/reset,
no inquiry/pairing/scan/event-mask changes, and exactly-once deregistration and
resource cleanup. It must log command routing, every foreign packet disposition,
initial owned ACL allowance, submissions and Number Of Completed Packets by
owned handle/epoch. Unknown/foreign completions, queue loss, timeout or credit
underflow are terminal failures; elapsed time never restores credit. Wireless
pad input and reconnection are checked before/during/after. This design is ready
for a proven broker, but implementing it against today's headers would guess the
very ownership contract it is meant to test.

## Evidence and interpretation

### MBus lead, bounded follow-up on 2026-10-26

The public Ghostcontrol Manba fork was inspected at
[`0c7abe001387ad140f0481f57896f42c6af2361c`](https://github.com/NikoBellikJR31/Ghostcontrol-PS5-USB-Controller-Patcher-Manba-/tree/0c7abe001387ad140f0481f57896f42c6af2361c).
The reviewed `shellui_pad.c` declares GPL-3.0-or-later; no code or binary was
imported, modified, executed or deployed. Its author records a system MBus
Bluetooth-event thread, but the
[actual calls](https://github.com/NikoBellikJR31/Ghostcontrol-PS5-USB-Controller-Patcher-Manba-/blob/0c7abe001387ad140f0481f57896f42c6af2361c/SOURCE%20MODIFIEE%20PAYLOAD/shellui_pad.c#L3162)
only disconnect attachments or bind them to users. They do not establish a
bounded copied event subscription, audio channel create/send/receive ABI,
owner registration or ACL credit reservation. Invoking those calls or attaching
to a system process would not answer the missing transport question and is not
a prepared experiment. A self-only module-name probe would establish presence
at most and is not worth another console run. The next useful MBus experiment
requires an actual read-only subscription or self-owned channel ABI with
documented record bounds, cancellation and cleanup first.

### Additional ownership lead checked after the socket result

`AngelLopezTorres/ps5-wireless-pads` was inspected at
`33fa2fe4225f67748f688ee1016b5fab7cbb5605`. Its
[NOTICE](https://github.com/AngelLopezTorres/ps5-wireless-pads/blob/33fa2fe4225f67748f688ee1016b5fab7cbb5605/NOTICE)
identifies PadBridge ancestry and GPL-3.0-or-later terms. No source or binary was
imported or executed. Its
[USB contract](https://github.com/AngelLopezTorres/ps5-wireless-pads/blob/33fa2fe4225f67748f688ee1016b5fab7cbb5605/src/hci_usb.h)
explicitly retains competing system endpoint reads; its
[credit policy](https://github.com/AngelLopezTorres/ps5-wireless-pads/blob/33fa2fe4225f67748f688ee1016b5fab7cbb5605/src/host_int.h)
uses a 60 ms fallback for missing completion reports. This is another shared
reader, not the missing native dispatcher. Rest-mode or process-replacement
features do not establish traffic ownership, so it supplies no next active
backend candidate under this project's integrity rules.

**Proven source** means the cited public text/source was inspected, not that its
behavior was verified on PS5. **Reported** means an author's hardware claim or
symbol inventory without a demonstrated implementation/ABI. **Unknown** means
the needed PS5/13.40 evidence is absent from this search.

| Question | Evidence | Status and consequence |
| --- | --- | --- |
| Does the PS5 payload SDK provide a native Bluetooth broker? | SDK v0.45 `include/ps5` and `sce_stubs` searched for `bluetooth`, `scebt`, and `bthci`; only the Pad stub names `scePadGetBluetoothAddress`. No Bluetooth-specific header or broker implementation was found. [SDK Pad stub][sdk-pad] | **Proven source**, bounded negative result. A symbol name supplies neither a safe call signature nor traffic ownership. |
| Are FreeBSD Bluetooth interface definitions available to compile against? | SDK headers define `AF_BLUETOOTH`, `BLUETOOTH_PROTO_HCI`, `sockaddr_hci` and HCI node query ioctls. [Socket header][sdk-socket], [Bluetooth socket header][sdk-btsocket] | **Proven source**. These are imported FreeBSD headers; PS5 kernel implementation, registration, native controller attachment and access rights are **Unknown**. |
| Is there a passive query before opening any Bluetooth socket? | FreeBSD 11.4 declares `net.bluetooth.version` as an integer with `CTLFLAG_RD`. SDK libkernel stubs list `sysctlbyname`, `sysctlnametomib`, and `sysctl`. [Bluetooth globals][fb-globals], [SDK stub][sdk-kernel] | **Proven source**; runtime symbol availability and the OID on PS5/13.40 are **Unknown**. Use only the bounded proposal below. |
| Can FreeBSD obtain capabilities without a fresh controller HCI command? | `ng_btsocket_hci_raw_control` maps GET_BUFFER/GET_FEATURES to Netgraph queries; `ng_hci_rcvmsg` returns cached `unit->buffer`/`unit->features`. [Socket implementation][fb-socket], [HCI implementation][fb-hci] | **Proven source on FreeBSD**. This is a potential later query route, conditional on separate PS5 evidence. A free-buffer snapshot is no reservation. |
| Does FreeBSD have a real copied receive path rather than competing USB reads? | `ng_btsocket_hci_raw_data_input` iterates socket PCBs and uses `m_dup` before appending to each matching socket. `ng_hci(4)` documents its raw hook. [Implementation][fb-socket], [manual, HOOKS][fb-hci-man] | **Proven source on FreeBSD**; no inspected source establishes that PS5 native Bluetooth uses this path. Raw visibility does not grant per-owner commands, handles or ACL credits. |
| Would the FreeBSD raw HCI socket implement our existing ACL transport? | `ng_hci_raw_rcvdata` accepts HCI command packets and rejects other packet types; it queues commands in the unit's command queue. [HCI implementation][fb-hci] | **Proven source on FreeBSD**. It is not by itself a command-and-ACL backend. Kernel L2CAP sockets would require a separately reviewed architecture boundary. |
| Do PS4 native Bluetooth exports supply a PS5 broker contract? | OpenOrbis declares only `void sceBluetoothHidGetDeviceInfo();` with no arguments/layout documentation. PS4LibDoc 9.00 lists HID init/registration/report/device-information exports, not an HCI ownership ABI. [Header][oo-hid], [metadata][ps4-hid] | **Proven source** of a placeholder header; **Reported** PS4 symbol metadata. No callable PS5 signature, buffer bounds or A2DP/raw-HCI routing follows. Do not call through guessed typedefs. |
| Are two independent HCI functions already proven? | GhosttoothAPI's author describes two functions and a 5.50 test, but also says the kernel reads the same endpoints and events are lost. [Author's post][ghost-post] | **Reported**, not PS5-BT verification or a 13.40 ownership proof. Distinct endpoint groups alone do not establish an idle independent controller or preserved native traffic. |
| Is safe built-in traffic ownership resolved? | None of the inspected paths establishes a PS5 broker's exact ABI, native controller binding, owner registration, ordered loss reporting and enforceable ACL reservation. | **Unknown**. No active built-in HCI experiment is unblocked by this report. |

The GhosttoothAPI report is retained only as an explicitly unverified topology
lead. It admits event competition; its endpoint claims do not justify copying
the rejected shared-reader design. No Ghosttooth binary or source was imported.

## One concrete next runtime query

After the minimal entry has passed its own load/log/return check, resolve the
already-loaded libkernel `sysctlbyname` symbol using the reviewed runtime. The
SDK's declaration is in [sys/sysctl.h][sdk-sysctl]. If resolution fails, record
the raw resolver status and stop this query; do not load a module or guess an
address/signature. Use the established System V integer/pointer ABI:

```c
int value = 0;
size_t length = sizeof(value);
int result = sysctlbyname("net.bluetooth.version", &value, &length, NULL, 0);
```

This is a proposal, not shipped code. Make exactly one named value request,
with fixed caller storage and no retry, tree walk or wildcard enumeration.
Log the exact requested name, resolver status, return value, returned length,
and `errno` only if the runtime has a reviewed way to obtain it. Interpret the
value only after success and `length == sizeof(value)`; record other lengths
as invalid. The upstream value identifies the stack interface version, not
Bluetooth Core version, controller vendor, HCI/LMP version, or headset support.
Do not infer any of those from a numeric result.

The reviewed [FreeBSD `sysctlbyname` implementation][fb-byname] first resolves a
name and then calls `sysctl` with the supplied `newp == NULL`, `newlen == 0`.
Its [name lookup][fb-nametomib] passes the name as input to the special
`name2oid` query; [that handler][fb-sysctl] resolves and returns an OID without
changing Bluetooth configuration. Non-null input to name lookup is not a
Bluetooth setting write. This is source evidence for the proposed operation,
not proof that PS5's proprietary implementation is identical.

| Result | Permitted conclusion / next decision |
| --- | --- |
| Symbol unavailable | This runtime cannot use this named interface; retain the status. Do not substitute guessed syscall numbers. |
| OID absent / unsupported | This named query is unavailable in the current process/environment. It does not prove that every native Bluetooth service is absent. |
| Permission failure | Existing permissions block the query. Stop; do not change credentials or syscall permissions. |
| Success with correct length | This environment exposes that value. Its native controller association and safe routing remain Unknown. Seek PS5-specific binding/ABI evidence before any socket or HCI experiment. |
| Other error / length mismatch | Preserve the exact result and treat the probe as failed. No active fallback. |

The query needs no `socket`, `bind`, `connect`, `read`, `recv`, `send`, `ioctl`,
module load, Netgraph hook changes or USB handle. It does not obtain a controller
session and must not make `ps5_bt_init` succeed. Keep the diagnostic separate
from the transport ABI. In particular, do not use `NgMkSockNode`: socket-node
creation is not a read-only topology query.

## Conditions for a later native query or broker

Even a positive stack-version query leaves these gates open:

1. **Implementation and binding:** PS5-specific evidence that a particular
   Bluetooth domain/service exists on 13.40 and is the native owner of the USB
   function observed by the descriptor probe. A generic SDK header, an exported
   name, or FreeBSD ancestry does not establish this.
2. **Read-only ABI:** verified socket/service layouts, accepted requests and
   cleanup semantics. In FreeBSD the cached GET_FEATURES/GET_BUFFER handlers are
   attractive, but no PS5 implementation or permissible node name has been
   established. Do not discover one by node creation, hook replacement or
   endpoint reads. GET_BUFFER cannot allocate transmission capacity.
3. **Active routing:** a source-backed owner-registration mechanism and command
   serialization, guaranteed preservation of native events/ACL, exact handle
   ownership, ordered delivery, bounded loss reporting, and enforceable per-owner
   credits. A copied monitor stream alone cannot satisfy these requirements.
4. **Independent-function alternative:** actual descriptors plus evidence of
   independent command/state/credit domains and the native owner's existing
   allocation. No detach/claim/reset/scan manipulation is authorized. The user
   must not infer controller identity from Slim/Fat/Pro branding.
5. **Hardware:** successful clean-return and metadata queries; wireless
   DualSense operation before/during/after, with exact loader, firmware, raw
   statuses and sanitized logs. No controller or pairing keys are requested.

FreeBSD's raw socket attach checks `PRIV_NETBLUETOOTH_RAW` to mark privileged
sockets; its security filters and ioctls have distinct checks. Do not simplify
that into either "all raw sockets require root" or "opening a socket grants
controller ownership." Neither is established for PS5. No socket probe is
proposed at this stage. [FreeBSD attach/control/filter source][fb-socket].

## Reproducible source inventory and search boundary

No source was copied into this repository. External public source/JSON metadata
was inspected in temporary research directories; no Sony firmware or library
binary was obtained. Primary pins below can be fetched independently. Select
only named text files; do not use SDK instructions to extract proprietary stubs.

| Repository / revision | Files inspected for this question | License / reuse decision |
| --- | --- | --- |
| `ps5-payload-dev/sdk` `a7685ce8e97ce1f8a4f38253a41d54241c9124f1` (v0.45) | `README.md`, `LICENSE`, `crt/syscall.h`, `sce_stubs/libkernel.c`, `sce_stubs/libScePad.c`, `include/freebsd/sys/{socket,sysctl}.h`, `include/freebsd/netgraph/bluetooth/include/ng_btsocket.h`; searched `include/ps5`, `sce_stubs` | GPL-3.0-or-later default; imported FreeBSD headers retain individual notices. Research only here; existing SDK review is in [UPSTREAM.md](UPSTREAM.md). |
| `freebsd/freebsd-src` `d106002967eff712894ab8b5337bbbff106ce5e0` (`releng/11.4`) | `COPYRIGHT`; `sys/netgraph/bluetooth/common/ng_bluetooth.c`, `socket/ng_btsocket_hci_raw.c`, `hci/ng_hci_main.c`, `include/ng_btsocket.h`; `share/man/man4/{ng_btsocket,ng_hci}.4`; `lib/libc/gen/{sysctlbyname,sysctlnametomib}.c`; `sys/kern/kern_sysctl.c` | Bluetooth files/manuals and name lookup have BSD two-clause notices; `sysctlbyname.c` has Beer-Ware revision 42; kernel sysctl has its own notices. No implementation selected or copied. Pin matches the SDK's header lineage, not a PS5 kernel identification. |
| `OpenOrbis/OpenOrbis-PS4-Toolchain` `0a1aaf9dd4a92695538bdeb09fb056d06dd11725` | `README.md`, `LICENSE`, `include/orbis/BluetoothHid.h` | README/root license GPL v3; no overriding notice in the tiny header. Research only, no header reuse and no ABI adoption. |
| `idc/ps4libdoc` `a71315e7f36e312ae71e9e3a92982e9ffbfc725f` (`9.00`) | `README.md`, tree paths and `system/common/lib/libSceBluetoothHid.sprx.json` | No LICENSE/COPYING path in that tree. Public symbol metadata inspected only; no redistribution, code generation or signature reuse. README schema also checked at main `8bff4863ac2f788149bbbf13a7eddc1222aec08b`. |
| TheStonedModder's GhosttoothAPI author post, 2026-09-27, observed 2026-10-23 | [GBAtemp thread opening post][ghost-post], relevant function/ownership claims only | Public report with no source pin or reuse license established. Reported evidence only; no payload obtained. |

For a checkout at each recorded pin, the key searches are:

```sh
rg -ni 'bluetooth|scebt|bthci' include/ps5 sce_stubs
rg -n 'AF_BLUETOOTH|AF_NETGRAPH' include/freebsd/sys/socket.h
rg -n 'sysctlbyname|sysctlnametomib|sysctl' sce_stubs/libkernel.c
rg -n 'SYSCTL_INT|version|CTLFLAG_RD' sys/netgraph/bluetooth/common/ng_bluetooth.c
rg -n 'm_dup|GET_BUFFER|GET_FEATURES|PRIV_NETBLUETOOTH_RAW' sys/netgraph/bluetooth/socket/ng_btsocket_hci_raw.c
rg -n 'NG_HCI_CMD_PKT|GET_BUFFER|GET_FEATURES|cmdq' sys/netgraph/bluetooth/hci/ng_hci_main.c
git ls-tree -r --name-only HEAD
```

The first three commands target the SDK; the next three target FreeBSD. Use
the last command in the pinned PS4LibDoc tree, select the BluetoothHid JSON,
and inspect exported names without assuming signatures. The JSON has a UTF-8
BOM; decode as `utf-8-sig` when using Python. GitHub API rate limiting affected
later inventory requests; `git ls-remote` and a filtered shallow metadata
checkout supplied the 9.00 pin instead. Search indexes were used for discovery,
not as substitutes for the pinned primary files.

No public PS5 broker contract was found in this inventory. This does **not**
claim such a service is globally impossible or absent from all firmware. Search
scope does not include proprietary modules, kernel memory, new exploits,
credential changes or unpublished SDK material. The existing competing USB
reader approach remains rejected for the reasons in
[PS5_PLATFORM.md](PS5_PLATFORM.md#controller-quirks-and-native-hci-ownership).

## Validation

All pinned external file links below were checked for successful retrieval;
repository pins were resolved through primary GitHub metadata or Git refs.
Local Markdown targets and whitespace were checked. No build, host protocol
test or PS5 runtime/hardware test is claimed for this documentation-only work.
The supplemental query remains a proposal, with implementation owned by the
platform/runtime work and the lead's integration review.

[sdk-pad]: https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/sce_stubs/libScePad.c
[sdk-kernel]: https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/sce_stubs/libkernel.c
[sdk-socket]: https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/include/freebsd/sys/socket.h
[sdk-btsocket]: https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/include/freebsd/netgraph/bluetooth/include/ng_btsocket.h
[sdk-sysctl]: https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/include/freebsd/sys/sysctl.h
[fb-globals]: https://github.com/freebsd/freebsd-src/blob/d106002967eff712894ab8b5337bbbff106ce5e0/sys/netgraph/bluetooth/common/ng_bluetooth.c
[fb-socket]: https://github.com/freebsd/freebsd-src/blob/d106002967eff712894ab8b5337bbbff106ce5e0/sys/netgraph/bluetooth/socket/ng_btsocket_hci_raw.c
[fb-hci]: https://github.com/freebsd/freebsd-src/blob/d106002967eff712894ab8b5337bbbff106ce5e0/sys/netgraph/bluetooth/hci/ng_hci_main.c
[fb-hci-man]: https://github.com/freebsd/freebsd-src/blob/d106002967eff712894ab8b5337bbbff106ce5e0/share/man/man4/ng_hci.4
[fb-byname]: https://github.com/freebsd/freebsd-src/blob/d106002967eff712894ab8b5337bbbff106ce5e0/lib/libc/gen/sysctlbyname.c
[fb-nametomib]: https://github.com/freebsd/freebsd-src/blob/d106002967eff712894ab8b5337bbbff106ce5e0/lib/libc/gen/sysctlnametomib.c
[fb-sysctl]: https://github.com/freebsd/freebsd-src/blob/d106002967eff712894ab8b5337bbbff106ce5e0/sys/kern/kern_sysctl.c
[oo-hid]: https://github.com/OpenOrbis/OpenOrbis-PS4-Toolchain/blob/0a1aaf9dd4a92695538bdeb09fb056d06dd11725/include/orbis/BluetoothHid.h
[ps4-hid]: https://github.com/idc/ps4libdoc/blob/a71315e7f36e312ae71e9e3a92982e9ffbfc725f/system/common/lib/libSceBluetoothHid.sprx.json
[ghost-post]: https://gbatemp.net/threads/ghosttoothapi-custom-payload-framework-for-bluetooth-audio-and-bluetooth-gamepads.684768/
