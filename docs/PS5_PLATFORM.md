# PS5 platform integration decisions

Source audit: 2026-10-23. User-reported target: **PS5 Slim, firmware 13.40**,
with an existing ELF loader and the ability to return diagnostic logs. No
PS5-BT ELF, controller query or audio path has been run on that console yet.
The installed loader's exact identity, revision and entry convention remain
Unknown. Source inspection is not hardware verification.

Use an original, bounded descriptor probe as the next platform capability.
Keep active built-in HCI transport Unsupported until native ownership is proven.
The default SDK CRT cannot be used within this project's scope. A minimal entry
using an existing loader's demonstrated contract is a separate, viable research
path; omitting the CRT is supported by the compiler wrapper, but does not itself
provide startup, symbol resolution, logging, or cleanup.

## Runtime scope and alternate startup

The pinned SDK v0.45 startup sequence is `_start` -> `__crt_start` -> BSS clear
-> syscall initialization -> kernel-helper initialization -> log initialization
-> libc `__isthreaded` initialization -> `__patch_init` -> runtime linker
initialization -> constructors -> `main` -> destructors -> termination.
`__patch_init` unconditionally changes the running process's credential
capability/attribute arrays and the permitted syscall-origin address range.
This occurs before our `main`, even if our adapter returns Unsupported.
There is no precondition check that turns these writes into a no-op when the
loader already supplied sufficient privileges, and no restoration paired with
these mutations in this startup path. [SDK entry][sdk-start],
[CRT][sdk-crt], [patch scope][sdk-patch].

The kernel initialization also expects loader-provided kernel read/write
descriptors and addresses, chooses firmware-dependent data, and can reject an
unrecognized firmware. The runtime linker resolves module exports using kernel
helpers. Deleting the patch call would not independently establish a small,
permission-preserving runtime. No such local SDK patch is recommended.
[Kernel helper initialization/resolution][sdk-kernel].

| Existing option | Exact source behavior | Decision |
| --- | --- | --- |
| SDK normal executable | `crt/Makefile` combines entry, CRT, syscall/kernel helpers, runtime linker, mdbg and patch objects into `crt1.o` | exclude stock startup from the diagnostic |
| SDK `-nostartfiles` | `prospero-clang` omits automatic `crt1.o`; it does not initialize any replacement runtime | usable build control for a separately reviewed minimal entry; not a runtime approval |
| SDK `-nodefaultlibs` / `-nostdlibc` | wrapper suppresses libc, default kernel and SCE dependency libraries | use only when final artifact inspection confirms every dependency is satisfied |
| SDK `-shared` example | `hello_so` libraries omit automatic CRT, but `hello_so.elf` uses normal executable startup | not a stand-alone payload solution; an already-running audited host would be required |
| Existing loader contract | loader may supply entry arguments and relocation handling without requiring the payload to call SDK CRT | select only the actual source-proven convention; do not assume a `main` entry or libc exists |

Sources: [CRT build][sdk-crt-build], [compiler wrapper][sdk-clang],
[shared-object example][sdk-so]. No SDK no-patch CRT build option or upstream
minimal no-CRT example was found in the pinned README, CRT, samples or wrappers.
This is a bounded search result, not a claim that no other ecosystem runtime
exists. SDK-listed loader names alone are insufficient runtime evidence.

## Current loader/tooling pins and entry evidence

Primary release metadata and remote tags/heads were checked on 2026-10-23.
[UPSTREAM.md](UPSTREAM.md) records publication dates, licenses and publisher
artifact digests. Do not use moving `latest` URLs in build/setup automation.

| Component | Latest published release observed | Release source pin | Default head observed |
| --- | --- | --- | --- |
| SDK | [v0.45](https://github.com/ps5-payload-dev/sdk/releases/tag/v0.45) | `a7685ce8e97ce1f8a4f38253a41d54241c9124f1` | same |
| elfldr | [v0.27](https://github.com/ps5-payload-dev/elfldr/releases/tag/v0.27) | `2a8da002e5292481ea964daa7342bbf4855f4c33` | same |
| websrv | [v0.34](https://github.com/ps5-payload-dev/websrv/releases/tag/v0.34) | `4d35610914273f8f4fc37af2b1d5bfb069dfba5b` | `7748df9d2a43822a5ebfda149f43d4f2696d640b` |

For elfldr v0.27, `elfldr_prepare_exec` places the argument block address in
`rdi`, pushes the interrupted instruction pointer as a return address, and
enters `base + e_entry`. The block matches the six-field shape in the SDK's
`payload_args_t`: first callable address, pipe/pair pointers, kernel pipe/data
addresses, and output-status pointer. Crucially, `elfldr_payload_args` fills the
first field with the resolved **`getpid` stub**, not `sceKernelDlsym`, despite
the SDK field name `sys_dynlib_dlsym`. The websrv v0.34 ELF payload path and
the inspected newer default branch do the same. A direct-resolver-only entry
must reject this convention; it must not interpret a successful `getpid` call
as successful symbol lookup. [elfldr entry and arguments][elfldr-source],
[websrv release loader][websrv-release], [websrv current loader][websrv-head],
[SDK argument definition][sdk-args].

SDK `crt/syscall.c` explicitly accommodates a real dlsym callback or a supplied
syscall stub, selects the syscall instruction within that stub, and bridges the
System V function ABI to the syscall ABI. That file alone does not change
credentials or syscall-origin permissions. Any adaptation must retain GPL
provenance, validate its exact expected convention, and stop on unsupported
instruction layout or denied permissions. A stub displacement found in source
is not proof that the user's installed loader supplies the expected bytes.
Do not replace this check with guessed addresses or firmware offsets.
[SDK syscall bridge][sdk-syscall].

The inspected elfldr and websrv ELF loaders copy `PT_LOAD` file contents, map
segments, and process section-table `SHT_RELA` entries only for
`R_X86_64_RELATIVE`. Other relocation types are ignored in that loop; it is not
a general dynamic import resolver. The ELF must therefore retain usable
section headers, have no unresolved imported symbols or unsupported allocated
relocations, and receive a complete artifact-specific segment/relocation audit.
Their temporary segment mirror is allocated with `malloc` and only file bytes
are copied: the minimal entry must explicitly initialize its BSS rather than
assume that region was zeroed. Stack alignment, preserved registers, panic
termination and clean return require both generated-code inspection and a
hardware run. [elfldr load/entry][elfldr-source],
[websrv load/entry][websrv-release].

These external loaders themselves contain process creation/takeover,
credential and kernel operations. This project does not copy, implement,
install or invoke those setup paths. The research establishes the interface
to an already-running environment, not approval of every operation in an
external loader. Firmware 13.40 compatibility and the exact running version
must be recorded from that environment; neither newest release metadata nor
SDK firmware tables establish a successful PS5-BT load.

## Read-only USB descriptor probe

Keep this separate from `ps5_bt_init`: metadata enumeration does not create a
transport session or authorize HCI commands. The C implementation may use
standard `opendir`/`readdir`, bounded path construction, `open`, `ioctl` and
`close`, with external SDK `<dev/usb/usb.h>` and `<dev/usb/usb_ioctl.h>`.
The exact metadata calls already used in HearBridge are:

| Call | SDK record and returned evidence |
| --- | --- |
| `USB_GET_DEVICE_DESC` | `struct usb_device_descriptor`: little-endian VID/PID and device class; use `UGETW` for USB words |
| `USB_GET_DEVICEINFO` | `struct usb_device_info`: bus/address, configuration index, bounded product/vendor text; serial numbers need not be logged |
| `USB_GET_FULL_DESC` | zeroed `struct usb_gen_descriptor`, caller buffer in `ugd_data`, u16 capacity in `ugd_maxlen`, explicit `ugd_config_index`; check `ugd_actlen` before parsing |

Definitions: [SDK USB ioctl header][sdk-usb-ioctl], [USB descriptor header][sdk-usb].
Working-source usage: HearBridge `device_id`, `read_config`, `try_node` in
[hci_usb.c][hb-usb]. HearBridge opens candidate nodes with `O_RDWR` before
performing metadata queries and later arms transfers. For our probe, try
read-only access and fail on insufficient permissions; do not silently escalate
or reuse the full transport-open function. Read-only-open compatibility is
**Untested** on PS5. Limit enumeration and descriptor storage explicitly;
return capacity/unsupported/error for incomplete metadata rather than infer
hardcoded `/dev/ugen0.2`, endpoint addresses or maximum-packet sizes.

HearBridge describes these descriptor queries as read-only with no device
requests. This audit has not established the proprietary PS5 kernel's ioctl
implementation. Our allowed probe call set excludes `USB_DO_REQUEST`, every
`USB_FS_*` call, endpoint `read`/`write`, alternate-setting/configuration changes,
driver detach/claim, resets and all credential helpers. Verify DualSense before,
during and after the first probe and record all raw ioctl failures.

Require complete configuration data and walk only bounded descriptors:
`bLength >= 2`, length within remaining bytes, full interface/endpoint lengths,
valid enclosing interface/alternate setting, and no accepted partial result
after malformed/truncated input. Inventory alternate setting 0 interfaces
with class/subclass/protocol `e0/01/01`, interrupt IN and bulk IN/OUT endpoints.
Preserve every matching group; a second group is not proof of an independent
controller or of a free radio. Descriptor parsing can be tested on a host;
permission and noninterference remain hardware requirements.
[HearBridge parser][hb-desc], [PlayPods ownership caveat][pp-hci].

## Controller quirks and native HCI ownership

| Evidence at the pinned revision | Applicable decision |
| --- | --- |
| HearBridge `btchip.c/h` maps VID `1286` to Marvell/NXP and `0e8d` to MediaTek | report labels alongside raw VID/PID; unknown vendors remain Unknown, never active default-profile fallback |
| HearBridge descriptor parser documents MediaTek `0e8d:3603` bulk IN `82` paired with bulk OUT `02`, with `01` spare | preserve descriptor evidence; do not select endpoints from console model or enable stall-driven endpoint switching |
| HearBridge `hci_cmd.c` pauses native scan for MediaTek, saves state and later restores it; unread state falls back to a presumed default | exclude from the descriptor probe; no guessed restoration, scan mutation or compatibility promise |
| HearBridge README credits ZiZc3 and PR #6 for MediaTek scan/pipe fixes | retain credit; upstream reports are not our hardware measurements |

Sources: [vendor/profile code][hb-chip], [descriptor pairing][hb-desc],
[scan changes][hb-cmd], [upstream credit][hb-readme],
[ZiZc3 report](https://github.com/X-F1REBALL-X/HearBridge-PS5/pull/6).

The pinned HearBridge transport explicitly describes native and payload readers
competing for the same endpoints, lost events, and timed credit replenishment.
It also overwrites full packet rings. The pinned PlayPods code makes loss sticky
and flags foreign command replies, but explicitly states that dropping foreign
packets cannot return them to the native reader. Neither is a traffic router.
Increasing pending reads or temporarily yielding reads cannot preserve the
event already consumed by the wrong owner. [HearBridge transport][hb-usb],
[PlayPods transport][pp-hci], [PlayPods foreign-event accounting][pp-bt].

| Integration option | What would unblock it | Present status |
| --- | --- | --- |
| Native-supported command/event/ACL broker | a real documented/source-inspected interface that serializes commands and forwards all native traffic, with per-owner handles and credits | Unknown: no such interface established in the inspected SDK/upstreams |
| A separate HCI function/interface | evidence of independent controller state and native ownership, enforceable ACL budget, and exclusive function access without disturbing DualSense | Unknown: descriptors alone are insufficient |
| Existing permissioned exclusive controller ownership | existing environment already grants it and separately preserves wireless DualSense on its own controller; no detach/reset/driver changes introduced here | Unknown; cannot assume from USB open success |
| More reads, opcode/handle filtering, file lock, timed refill, event/ACL split readers | none supplies missing native delivery or command serialization | rejected for active built-in backend |

Until one valid option is demonstrated, even Read Local Version Information is
blocked: it consumes command credit and needs its completion routed correctly.
Keep the ABI v1 `BluetoothTransport` Unsupported. A future router must provide
one ordered tagged stream, complete frames, bounded queues, exact cancellation
and reaping, preserved native events, and owned completion accounting. No
controller reset, silent overflow or retry can repair unknown traffic loss.

## Audio capture interface and adoption boundary

Both pinned applications load the already-installed `libSceIpmi.sprx` before
`libSceAvcap2.sprx`, resolve Avcap2 exports by NID, and use Initialize -> OpenAudio
-> Start -> ReadAudio -> Stop -> Close -> Terminate. No proprietary library is
downloaded, extracted or redistributed. These are reverse-engineered upstream
function typedefs, not a stable SDK capture API. [PlayPods capture][pp-capture],
[HearBridge capture][hb-capture].

| Interface observation | Consequence for a future adapter |
| --- | --- |
| Both request a private `0x38`-byte OpenAudio parameter block and source 2; PlayPods names mode 0 raw 48 kHz stereo f32 | keep layout/NIDs in C and mark format Requires hardware; never expose private blocks to Rust |
| PlayPods `capture_read(float *, size_t)` treats capacity and positive result as **bytes**, validates whole stereo f32 frames; HearBridge requests `0x2000` bytes and converts to s16 frames | for confirmed stereo f32, frames = bytes / 8, samples = frames * 2; existing `PcmSource` remains caller-owned interleaved f32 with whole frame counts |
| Both use a 16-byte metadata buffer; HearBridge presets its size, PlayPods does not decode it | no proven capture timestamp/time origin; report optional timestamp absent until demonstrated |
| Upstream codes include Empty `81950002`, Overrun `81950004`, Ended `81950008`, Invalid `81950001` | preserve raw status; Empty may mean no data, Overrun must signal discontinuity, Ended is terminal, other failure stays visible |
| HearBridge guesses f32/s16 and channel count from content/rate and retries capture with credential elevation after default mode fails | do not adopt heuristics, credential fallback or hidden restarts; missing permissions stop capture |
| PlayPods clears owned resources once before cleanup and logs native cleanup failures | preserve errors and consume-once ownership; never retry an uncertain close blindly |

HearBridge's default-first comment is not proof that it never changes
credentials: its open loop tries three authorization modes and its `creds.c`
mutates authid/capabilities. PlayPods capture has no equivalent elevation call,
but the default SDK startup still runs before it. [Capture open loop][hb-capture],
[credential helper][hb-creds], [SDK CRT][sdk-crt].

Keep audio ABI functions proposed until format, valid byte counts, permissions,
nonblocking behavior/deadlines, finite sample handling, discontinuity reporting,
metadata and cleanup are measured. Capture integration follows audible tone
through an ownership-proven transport; no source import is needed for this
research milestone. Per-file licensing decisions are in [UPSTREAM.md](UPSTREAM.md).

## Validation and next evidence

Research-only change: no build/test execution is claimed here. The recorded host
baseline remains [MVP-STATE.md](MVP-STATE.md); it proves neither a PS5 runtime nor
native HCI routing. Commit permalinks were checked against external research
checkouts and primary repositories. Relative links were checked locally.

The next hardware record must identify the exact installed loader/runtime,
artifact hash, firmware 13.40, console model, entry convention, actual logs,
normal/panic return behavior and wireless DualSense state. A successful minimal
ELF can unblock descriptor-only probing. It cannot unblock command traffic,
pairing, ACL audio, capture permissions or compatibility by itself.

[sdk-start]: https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/crt/_start.S
[sdk-crt]: https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/crt/crt.c
[sdk-patch]: https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/crt/patch.c
[sdk-kernel]: https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/crt/kernel.c
[sdk-crt-build]: https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/crt/Makefile
[sdk-clang]: https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/host/bin/prospero-clang
[sdk-so]: https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/samples/hello_so/Makefile
[sdk-args]: https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/include/ps5/payload.h
[sdk-syscall]: https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/crt/syscall.c
[sdk-usb-ioctl]: https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/include/freebsd/dev/usb/usb_ioctl.h
[sdk-usb]: https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/include/freebsd/dev/usb/usb.h
[elfldr-source]: https://github.com/ps5-payload-dev/elfldr/blob/2a8da002e5292481ea964daa7342bbf4855f4c33/elfldr.c
[websrv-release]: https://github.com/ps5-payload-dev/websrv/blob/4d35610914273f8f4fc37af2b1d5bfb069dfba5b/src/ps5/elfldr.c
[websrv-head]: https://github.com/ps5-payload-dev/websrv/blob/7748df9d2a43822a5ebfda149f43d4f2696d640b/src/ps5/elfldr.c
[hb-usb]: https://github.com/X-F1REBALL-X/HearBridge-PS5/blob/cd5b7fce1b8226d8850cdd306437550098c6a62b/src/bt/hci_usb.c
[hb-desc]: https://github.com/X-F1REBALL-X/HearBridge-PS5/blob/cd5b7fce1b8226d8850cdd306437550098c6a62b/src/bt/usb_hci_desc.c
[hb-chip]: https://github.com/X-F1REBALL-X/HearBridge-PS5/blob/cd5b7fce1b8226d8850cdd306437550098c6a62b/src/bt/btchip.c
[hb-cmd]: https://github.com/X-F1REBALL-X/HearBridge-PS5/blob/cd5b7fce1b8226d8850cdd306437550098c6a62b/src/bt/hci_cmd.c
[hb-readme]: https://github.com/X-F1REBALL-X/HearBridge-PS5/blob/cd5b7fce1b8226d8850cdd306437550098c6a62b/README.md
[hb-capture]: https://github.com/X-F1REBALL-X/HearBridge-PS5/blob/cd5b7fce1b8226d8850cdd306437550098c6a62b/src/avcap2.c
[hb-creds]: https://github.com/X-F1REBALL-X/HearBridge-PS5/blob/cd5b7fce1b8226d8850cdd306437550098c6a62b/src/creds.c
[pp-hci]: https://github.com/FGGstore/FGG-PlayPods/blob/d726e1c32568048aae66f3218c0540adc357e408/src/hci.c
[pp-bt]: https://github.com/FGGstore/FGG-PlayPods/blob/d726e1c32568048aae66f3218c0540adc357e408/src/bt.c
[pp-capture]: https://github.com/FGGstore/FGG-PlayPods/blob/d726e1c32568048aae66f3218c0540adc357e408/src/capture.c
