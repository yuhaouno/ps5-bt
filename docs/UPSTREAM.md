# Upstream inventory and synchronization

Snapshot: 2026-10-24, Asia/Shanghai. Revisions below are immutable research
and build pins. No upstream Bluetooth protocol stack has been imported. The
capture diagnostic uses the inspected PlayPods/HearBridge capture ABI and
conservatively retains GPL-3.0-only for its adaptation; see CAPTURE-PROBE.md.
The optional host-only codec imports the independently audited BlueZ libsbc
selection and recorded generic/caller-storage patches; see SBC-IMPLEMENTATION.md
and third_party/sbc/VENDORED.md. Only the independently selected target SBC
self-test links that codec into a PS5 diagnostic; see SBC-TARGET.md and
HW3-EVIDENCE.md. Production media remains unverified.
The Rust dependencies are workspace paths. The minimal diagnostic ELF uses
three unmodified syscall inputs by default, plus the explicit kernel-resolver
subset documented below for HW-1; stock CRT and patch.c remain excluded. No proprietary binary is required.

## Inventory

Current reuse policy follows [AGENTS.md](../AGENTS.md): a scoped, reviewed C
protocol/connection module may be a transitional implementation behind a
versioned ABI, with the same exact-pin, per-file license, modification and
attribution requirements as any other import. No such protocol module is
imported by this policy update; do not copy a whole upstream application or
inherit its compatibility claims.

The 2026-10-26 bounded MBus follow-up inspected the Ghostcontrol Manba fork at
`0c7abe001387ad140f0481f57896f42c6af2361c`, specifically its README and
GPL-3.0-or-later `shellui_pad.c`, as behavioral references only. No code,
Sony binary or system-process modification was imported or executed. The
reviewed calls manage attachment disconnection/user binding and do not supply
an audio-channel or preserved-event ABI; exact links and limits are recorded
in NATIVE-HCI-RESEARCH.md.

The 2026-10-25 post-socket-result review also inspected
[`AngelLopezTorres/ps5-wireless-pads`](https://github.com/AngelLopezTorres/ps5-wireless-pads/tree/33fa2fe4225f67748f688ee1016b5fab7cbb5605)
at `33fa2fe4225f67748f688ee1016b5fab7cbb5605`. LICENSE is GPL v3 and NOTICE
explicitly identifies GPL-3.0-or-later/PadBridge ancestry. It is a behavioral
reference only: no source import, modification, linking or execution. Its
competing-reader and timed-credit policies do not resolve our ownership gate;
see NATIVE-HCI-RESEARCH.md.

The native L2CAP availability experiment uses declarations/constants from the
same pinned SDK and reviews FreeBSD revision
`d106002967eff712894ab8b5337bbbff106ce5e0` as behavioral evidence only. No
FreeBSD Bluetooth implementation or headers are copied into its link. Exact
files, license review and runtime limitations are in NATIVE-L2CAP-PROBE.md.

| Source | Inspected revision | License evidence | Relationship |
| --- | --- | --- | --- |
| [ps5-payload-dev/sdk](https://github.com/ps5-payload-dev/sdk) | `a7685ce8e97ce1f8a4f38253a41d54241c9124f1`, release `v0.45` | root LICENSE plus README: GPL-3.0-or-later default; FreeBSD headers BSD and other file notices apply | three selected external source files in reviewed minimal-runtime link; stock CRT excluded |
| [FGGstore/FGG-PlayPods](https://github.com/FGGstore/FGG-PlayPods) | `d726e1c32568048aae66f3218c0540adc357e408`, main; latest release observed `1.0` is a separate artifact | root LICENSE is GPL v3; README says GPL-3.0, no project-wide later-version grant established | behavioral reference only |
| [X-F1REBALL-X/HearBridge-PS5](https://github.com/X-F1REBALL-X/HearBridge-PS5) | `cd5b7fce1b8226d8850cdd306437550098c6a62b`, main; latest release observed `v1.3.6` | root LICENSE and NOTICE explicitly GPL-3.0-or-later | behavioral reference only |
| [BlueZ libsbc](https://git.kernel.org/pub/scm/bluetooth/sbc.git) | `b3deb8a5dcfb42d8c10ba1f2f1ac9bd7bf7271cc`, release 2.2; independently checked Git/archive and selected hashes | all seven selected code files explicitly LGPL-2.1-or-later; retained COPYING.LIB and AUTHORS | vendored pristine selection plus three recorded patches; optional host encoder and independently selected target self-test; no live media integration |
| [PS5Dev/PS5SDK](https://github.com/PS5Dev/PS5SDK) | repository README and LICENSE examined; no build pin selected | GPL v2 license text; exact per-file GPL-2.0-only vs later-version grants need review | historical ecosystem reference; no dependency |
| [ps5-payload-dev/pacbrew-repo](https://github.com/ps5-payload-dev/pacbrew-repo) | `722187f0ee246c0d7d57401c8a97de1c7f4e2ff0` metadata | packages require individual license/source review | ecosystem discovery only; no package used |
| [ps5-payload-dev/elfldr](https://github.com/ps5-payload-dev/elfldr) | release `v0.27`, tag and default head `2a8da002e5292481ea964daa7342bbf4855f4c33` | README and inspected `elfldr.c`, `pt.c`: GPL-3.0-or-later | external pre-existing loader contract research; no loader implementation imported |
| [ps5-payload-dev/websrv](https://github.com/ps5-payload-dev/websrv) | release `v0.34`: `4d35610914273f8f4fc37af2b1d5bfb069dfba5b`; default head `7748df9d2a43822a5ebfda149f43d4f2696d640b` | README: GPL-3.0-or-later default, xterm.js MIT; inspected `src/ps5/elfldr.c`: GPL-3.0-or-later | optional external homebrew frontend research; no dependency or bundled assets |

SDK v0.43 (`d9c9519116944a7f1c22d262012c53b80b3520b7`) is what HearBridge's
README names. Our v0.45 selection supplies the minimal syscall-source subset,
**not a claim that HearBridge or our platform is hardware-tested with it**. The
v0.45 archive downloaded during research matched SHA-256
`5fac64093d0e3e69f2584ce7935f24f703f622d9681f1495c85b9f00d1291a2c`.
URL: [versioned archive](https://github.com/ps5-payload-dev/sdk/releases/download/v0.45/ps5-payload-sdk.zip).
Release publication observed: 2026-10-21. Do not use `latest` in build scripts.

On 2026-10-23, GitHub primary release metadata and `git ls-remote` were checked
again. SDK, PlayPods and HearBridge default heads still equal the research pins.
The newest published SDK, ELF loader and web server releases observed were
[SDK v0.45](https://github.com/ps5-payload-dev/sdk/releases/tag/v0.45),
[elfldr v0.27](https://github.com/ps5-payload-dev/elfldr/releases/tag/v0.27)
(published 2026-10-21 17:42:19 UTC), and
[websrv v0.34](https://github.com/ps5-payload-dev/websrv/releases/tag/v0.34)
(published 2026-08-02 19:51:42 UTC). The newer websrv default branch is not its
published release. These are setup research pins, not claims about the loader
already installed on the user's console. No loader or homebrew binary was
downloaded or installed for this audit.

GitHub release metadata reports SHA-256
`82f6227610172c98bad6efe68219fc93a10027035991f47eb4b75532d023407d`
for `elfldr-ps5.elf` v0.27 and
`54730c867c6e1148536fdcb370e63a7762d989ea87b62488ad4caff64d43f263`
for `websrv-ps5.elf` v0.34. These two values are **publisher metadata**, not
independently downloaded artifact verification. They must be checked against
any externally obtained artifact before using it. An SDK upgrade or a newer
loader release alone does not resolve native HCI ownership.

## Platform selection for the next milestone

[PS5_PLATFORM.md](PS5_PLATFORM.md) records the source-level startup audit,
loader entry and relocation contracts, a read-only descriptor probe plan,
controller-quirk provenance, and the capture interface discrepancies. The
stock SDK CRT remains outside the approved payload scope. The reviewed
`minimal-syscall-v6` build now uses original project entry/runtime code and a
linker script, plus the selected SDK subset below. The explicit HW-1 variant
initializes the already-supplied kernel-R/W transport and resolves own-process
exports. It changes no credentials or syscall bounds. The recorded Slim/13.40
run completed with final 2; see HW1-REPAIR.md for its limits.
Neither researched loader supplies a direct symbol-resolver callback in its
first argument field: it supplies a `getpid` syscall stub. Do not infer that
field's behavior from the SDK typedef's name.

### Per-file reuse decision

All entries are pinned to the inventory revisions. The SDK subset below, capture
diagnostic adaptation and optional host/target-self-test libsbc selection are implemented.
Other entries remain research/candidate decisions. Preserve copyright and license notices,
exact source, local changes and attribution when adopting further material.

| Candidate files | Terms inspected | Recommendation and excluded behavior |
| --- | --- | --- |
| SDK `crt/syscall.c`, `crt/syscall.h`, `crt/payload.h` | Copyright (C) 2024 John Törnblom; explicit GPL-3.0-or-later notices | selected unmodified external source subset; hashes enforced; no local patches |
| SDK `samples/notify_debug/main.c` | Copyright (C) 2023 John Törnblom; explicit GPL-3.0-or-later notice | interface/layout provenance for the diagnostic notification call; no sample source, import library or module loader enters the build; the project-owned bounded request construction and runtime resolution are host-tested |
| SDK `host/bin/prospero-clang`, `host/bin/prospero-lld`, `crt/_start.S`, `host/elf_x86_64.x` | explicit GPL-3.0-or-later notices | toolchain/startup research only; not build inputs; project uses direct LLVM tools and original startup/linker sources |
| SDK `crt/crt.c`, `crt/patch.c`, `crt/kernel.c`, `crt/rtld*.c` | explicit GPL-3.0-or-later notices | stock CRT/patch/runtime loader remain excluded; kernel.c is now an explicit audited own-process resolver input, as described below |
| SDK `include/freebsd/dev/usb/usb.h`, `usb_ioctl.h`, `usb_endian.h`, `usb_freebsd.h` | individual BSD-style notices, including two-clause terms in `usb.h` and `usb_ioctl.h` | use SDK headers externally in C; preserve each file's actual notice if redistributed; not a license grant for proprietary runtime libraries |
| HearBridge `src/bt/usb_hci_desc.c`, `usb_hci_desc.h`, `btchip.c`, `btchip.h` | no overriding file license found; README/NOTICE GPL-3.0-or-later | smallest useful adaptation candidates for descriptor parsing/vendor labels; strengthen malformed/truncated descriptor errors; reject unknown active backends rather than adopting default-profile fallback |
| HearBridge `src/bt/hci_usb.c`, `hci_cmd.c` | no overriding file license found; README/NOTICE GPL-3.0-or-later | behavior reference only: exclude competing IN readers, overwrite rings, timed credit refill, endpoint failover and guessed scan-state restoration |
| HearBridge `src/avcap2.c`, `src/creds.c` | no overriding file license found; README/NOTICE GPL-3.0-or-later | capture ABI research only; exclude credential fallback, heuristic format/channel inference and hidden restart policy; `creds.c` is out of scope |
| PlayPods `src/capture.c`, `src/hci.c`, `src/bt.c` | root GPL v3; README GPL-3.0; no later-version grant established in these files | retain as reference; capture byte-length checks and consume-once cleanup are useful design evidence; any adaptation retains GPL-3.0-only conservatively, subject to final rights review; shared-reader transport remains blocked |
| elfldr `elfldr.c`, `pt.c`; websrv `src/ps5/elfldr.c`, `pt.c` | explicit GPL-3.0-or-later notices | external loader contract reference only; do not import process takeover, credential changes, kernel-memory operations or loader bootstrap |

The shipped descriptor component is an original bounded C parser and metadata
report with hostile-input host tests; the parser itself performs no USB access.
The separate USB probe now uses SDK headers and checked syscalls, with actual
read-only evidence in HW2-EVIDENCE.md. Keep upstream quirk decisions documented
in C adaptation; no audio
stack or application-wide C import is warranted. Any later parsing adaptation
belongs in its own reviewable change with provenance and hostile-input tests.

## Selected SDK build inputs

The opt-in USB build additionally selects the 34 original SDK headers pinned in
`tools/usb_sdk_headers.json`, including syscall/directory/ioctl declarations and
their type dependencies. Their per-file Berkeley, NetBSD and other notices are
retained in source bundles; the project GPL text does not replace them. No SDK
header is modified. The standalone provisioner expands upstream stdint.h's
symlink to its original sys/stdint.h target bytes, with the same notices/hash.
Capture adds no new SDK source, offset, module binary or privilege setter.

[tools/build_ps5.py](../tools/build_ps5.py) selects only these paths from SDK
revision `a7685ce8e97ce1f8a4f38253a41d54241c9124f1`. It verifies SHA-256 for
automatic immutable-URL downloads into ignored `.sdk/` and for external
`PS5_SDK_SOURCE` checkouts. No moving branch or release binary is a link input.

| Exact external path | SHA-256 | Role |
| --- | --- | --- |
| `crt/syscall.c` | `7c2f8120c8e5059d387aaed0a144258c140a5ba5360f6f23da2a3442470e24b2` | compiled userland syscall bridge and initialization |
| `crt/syscall.h` | `463fc16da04c2b7b98a41cbf71e0901faa32a4a9a19421a52bc27dfb52186577` | declarations and numeric syscall definitions |
| `crt/payload.h` | `e4803d043c38fbb60f2d027501d572db9b81a14af0b09b460129cbc84ecb617d` | loader argument structure |

All three retain the exact `Copyright (C) 2024 John Törnblom` notice and the
GPL version 3-or-any-later-version grant, warranty disclaimer and GPL-copy
notice. They are used unmodified; there are **no local SDK patches**. The
original project `bootstrap/start.S`, `runtime.c` and `minimal.ld` are not
adaptations of the SDK startup/linker script. The SDK script was tried and
rejected because this subset produced unaligned PT_LOAD starts; see
[RUNTIME-REVIEW.md](RUNTIME-REVIEW.md). SDK `crt/crt.c`, `crt/patch.c`,
`crt1.o`, libc and import libraries are excluded. The default minimal variant
also excludes kernel.c; the explicit HW-1 variant selects it as detailed below. No upstream loader implementation or proprietary binary is bundled.

### HW-1 existing-loader resolver subset, 2026-10-24

The explicit flag `--loader-kernel-resolver` adds unmodified `crt/kernel.c`
and `crt/nid.c` from the same SDK revision. Their required hash-verified headers
are `crt/kernel.h`, `crt/kernel_iommu.h`, `crt/elf.h`, `crt/nid.h` and
`crt/klog.h`. Exact SHA-256 values for all ten provisioned files are pinned in
[tools/build_ps5.py](../tools/build_ps5.py), recorded in every manifest and
verified again when packaging corresponding source.

Each added file's explicit GPL-3.0-or-later notice was inspected before use.
Copyright is John Törnblom 2024, except kernel_iommu.h 2026. Original bytes,
notices, GPL text and source are retained in corresponding-source bundles.
No local SDK patch is applied. Selected functions initialize the loader's
existing pipe/socket kernel-R/W channel and resolve this process's exports.
No initial exploit, credential setter, stock CRT, patch.c or upstream loader
implementation is linked. The source's kernel metadata and transport-state
writes require the invariants recorded in RUNTIME-REVIEW.md.

The project-owned syscall_checked.S wraps the unmodified SDK syscall bridge
with carry/error normalization, preserving all six arguments and MAP_FAILED.
It does not introduce a syscall instruction. Runtime v6 never inspects code
bytes. The recorded hardware returned full Rust lifecycle and final 2 with
this explicit resolver; that is diagnostic execution, not Bluetooth/audio.
The optional native query resolved its export and returned -1.

The earlier internal-dlsym hypothesis also consulted etaHEN/etaHEN,
revision `dafa13b562ddb137a4b4a97b9aaa287c0c57cc9c`,
`Source Code/bootstrapper/source/elfldr.c`; its root GPL v3 text and file
GPL-3.0-or-later notice were inspected. This was a behavioral reference only:
no source was copied and the installed loader was not identified as that build.
Earlier process-list sightings did not establish that hypothesis.

Before distributing a linked ELF, provide the exact selected corresponding
source, retained notices, GPL text and project/build sources. A GitHub link alone
is not a complete corresponding-source distribution. No release distribution
exists yet. Any future source changes need separate provenance and patches;
the current file-hash checks intentionally reject modified SDK inputs.

## Behavior provenance

All links are pinned. These entries identify what to study or adapt later, not
which claims have been reproduced here.

| Behavior | Evidence | Planned owner / constraints |
| --- | --- | --- |
| Avcap2 capture, Ipmi prerequisite, NID resolution, private parameter block and statuses | [PlayPods capture.c](https://github.com/FGGstore/FGG-PlayPods/blob/d726e1c32568048aae66f3218c0540adc357e408/src/capture.c), [HearBridge avcap2.c](https://github.com/X-F1REBALL-X/HearBridge-PS5/blob/cd5b7fce1b8226d8850cdd306437550098c6a62b/src/avcap2.c) | retain C module loading, resolution, layout and cleanup; never expose private structs to Rust; verify format/status units |
| USB control command requests, USB FS event/ACL transfers and lifetime | [PlayPods hci.c](https://github.com/FGGstore/FGG-PlayPods/blob/d726e1c32568048aae66f3218c0540adc357e408/src/hci.c), [HearBridge hci_usb.c](https://github.com/X-F1REBALL-X/HearBridge-PS5/blob/cd5b7fce1b8226d8850cdd306437550098c6a62b/src/bt/hci_usb.c) | C adapter; ownership and native delivery remain blockers |
| Vendor identification, descriptors, MediaTek endpoint pairing | [btchip.h](https://github.com/X-F1REBALL-X/HearBridge-PS5/blob/cd5b7fce1b8226d8850cdd306437550098c6a62b/src/bt/btchip.h), [usb_hci_desc.c](https://github.com/X-F1REBALL-X/HearBridge-PS5/blob/cd5b7fce1b8226d8850cdd306437550098c6a62b/src/bt/usb_hci_desc.c) | C controller backend; match actual VID/PID/descriptors, fail unknown |
| MediaTek scan pause and restoration | [hci_cmd.c](https://github.com/X-F1REBALL-X/HearBridge-PS5/blob/cd5b7fce1b8226d8850cdd306437550098c6a62b/src/bt/hci_cmd.c) | source-reviewed C quirk candidates may be implemented; production activation needs coexistence evidence and proven restoration; excluded from the existing first probe |
| Credits, foreign events and shared endpoint risks | [PlayPods bt.c](https://github.com/FGGstore/FGG-PlayPods/blob/d726e1c32568048aae66f3218c0540adc357e408/src/bt.c), HearBridge USB source above | raw-HCI accounting by its reviewed owner, or system-owned accounting behind a native channel ABI; never use timer-based credit refill |
| Resident-audio evidence limitations | [RFC](https://github.com/FGGstore/FGG-PlayPods/blob/d726e1c32568048aae66f3218c0540adc357e408/docs/RESIDENT-AUDIO-RFC.md), [hardware summary](https://github.com/FGGstore/FGG-PlayPods/blob/d726e1c32568048aae66f3218c0540adc357e408/docs/hardware-summary.json) | test design reference, not our compatibility evidence |
| SDK linker target/script/CRT and 16 KiB alignment research | [prospero-clang](https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/host/bin/prospero-clang), [prospero-lld](https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/host/bin/prospero-lld) | original project minimal startup/linker script and direct LLVM 18.1.8 tools now used; no project PS5 loader |
| Startup credential/syscall permission mutation | [crt.c](https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/crt/crt.c), [patch.c](https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/crt/patch.c) | stock startup remains excluded; a separate reviewed minimal own-process permission adapter follows AGENTS.md, not blanket adoption of patch.c |
| Debug notification request layout and libkernel entry point | [notify_debug sample](https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/samples/notify_debug/main.c) | visible HW-1 lifecycle marker only; bounded text, no imported library/module load, and failure remains explicit |

## License decisions and unresolved questions

Original bootstrap contributions use GPL-3.0-or-later. This is a conservative
project choice compatible with a GPL-based PS5 distribution, not a claim that
viewing reference code automatically licenses all new work under GPL. If
PlayPods material is reused under GPL-3.0-only, the combined distribution may
need GPL v3 specifically; do not relabel that material as later-version licensed.
An FFI boundary does not remove obligations of linked/adapted code; see the
[GNU linking FAQ](https://www.gnu.org/licenses/gpl-faq.en.html#GPLStaticVsDynamic). Separate
files alone do not make a combined binary permissively licensed.

Before redistribution, audit every selected SDK component and its notices,
provide license texts, corresponding source, local patches and build scripts,
and review installation-information obligations where applicable. Source links
alone are not a complete source-distribution plan. BlueZ codec reuse adds LGPL
requirements, including a suitable relinking/replacement arrangement if linked
statically. No codec/patent clearance or Bluetooth qualification is claimed.
AAC/LDAC and other codec implementations require a separate review. GPL code
availability is not evidence about codec patents or trademark permission.

SDK documentation says some artifacts originate from PS5SDK. Do not resolve
that lineage's per-file license questions by guessing from GitHub's badge.
Our root license does not replace third-party licenses. No Sony library file
will be stored, fetched from a console, or redistributed by these build scripts.

## Discovery and native management adapters, 2026-10-26

The discovery, protected file store, LAN service/page, runner and TCP listener
are original project code; no new external crate, protocol stack or web framework
is imported. DEVICE-DISCOVERY.md records the exact Core/Assigned Numbers baseline
and reference-only Linux revision used to corroborate event layouts.
BOND-FILE-STORE.md and CONTROL-LISTENER.md record POSIX, pinned SDK/header and BSD
source reviews. Native structs/constants stay inside C translation units built
against the selected target's own reviewed headers. Host feature success does
not establish target import/layout or runtime availability. Static web assets
are embedded source and must remain in corresponding-source distributions.

## Update procedure

1. Fetch a candidate into an external research checkout; never overwrite the
   existing pin or build from a moving branch. Inspect LICENSE/NOTICE and all
   changed files before reuse. Resolve a tag to a full commit and hash archives.
2. Record repository, commit, license, selected files, relationship (linked,
   vendored, adapted, specification implementation, behavioral reference), reason,
   and maintainer. Review runtime/scope behavior as well as API compatibility.
3. Keep the selected SDK subset as one explicit revision/hash-locked external
   dependency. A future update must change source hashes, runtime review,
   machine-readable lock and this inventory together after behavior/license
   review. Preserve originals; record any later adaptations as numbered patches
   with upstream commit, rationale, license and tests. Do not copy the whole
   upstream application. No SDK patches or tracked vendored sources exist today.
4. Diff capture, USB, controller quirks, cleanup, and import/runtime changes.
   Update only the adapter if the C contract is unchanged. If the contract must
   change, version it and add conformance tests before changing Rust callers.
5. Run desktop parser/ABI tests, cross-archive inspection and reviewed ELF link
   checks, then the hardware matrix including DualSense and cleanup. Record
   failures explicitly. Do not promote hardware status from host CI results.
6. Save the exact build manifest, source and patch artifacts for any release;
   update this inventory, the machine-readable lock, changelog and credits.

## Credits

Credit FGGstore and FGG-PlayPods contributors for the audio/transport research and
resident-audio evidence; X-F1REBALL-X and HearBridge contributors for controller,
capture, diagnostics and coexistence research; ZiZc3 for the MediaTek fixes
credited by HearBridge; John Törnblom and ps5-payload-dev contributors for the
Payload SDK; PS5SDK contributors for historical tooling; and BlueZ/libsbc
contributors for the codec reference. This independent project is not an
official fork, Rust translation, endorsement, or compatibility guarantee.
