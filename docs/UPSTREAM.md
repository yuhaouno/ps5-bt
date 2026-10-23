# Upstream inventory and synchronization

Snapshot: 2026-10-23, Asia/Shanghai. Revisions below are **research pins**.
No Bluetooth/audio implementation has been copied, linked, adapted, vendored,
or added as a submodule. The only initial Rust dependencies are workspace paths.
The SDK is an external, candidate build dependency; its runtime is gated.

## Inventory

| Source | Inspected revision | License evidence | Relationship |
| --- | --- | --- | --- |
| [ps5-payload-dev/sdk](https://github.com/ps5-payload-dev/sdk) | `a7685ce8e97ce1f8a4f38253a41d54241c9124f1`, release `v0.45` | root LICENSE plus README: GPL-3.0-or-later default; FreeBSD headers BSD and other file notices apply | candidate external compile/link dependency; startup audit required |
| [FGGstore/FGG-PlayPods](https://github.com/FGGstore/FGG-PlayPods) | `d726e1c32568048aae66f3218c0540adc357e408`, main; latest release observed `1.0` is a separate artifact | root LICENSE is GPL v3; README says GPL-3.0, no project-wide later-version grant established | behavioral reference only |
| [X-F1REBALL-X/HearBridge-PS5](https://github.com/X-F1REBALL-X/HearBridge-PS5) | `cd5b7fce1b8226d8850cdd306437550098c6a62b`, main; latest release observed `v1.3.6` | root LICENSE and NOTICE explicitly GPL-3.0-or-later | behavioral reference only |
| [BlueZ libsbc](https://git.kernel.org/pub/scm/bluetooth/sbc.git) | `b3deb8a5dcfb42d8c10ba1f2f1ac9bd7bf7271cc`, **as recorded by PlayPods**, not an independent current-head audit | PlayPods `third_party/sbc/COPYING.LIB`, `VENDORED.md`: LGPL-2.1-or-later | transitive reference only; no encoder selected |
| [PS5Dev/PS5SDK](https://github.com/PS5Dev/PS5SDK) | repository README and LICENSE examined; no build pin selected | GPL v2 license text; exact per-file GPL-2.0-only vs later-version grants need review | historical ecosystem reference; no dependency |
| [ps5-payload-dev/pacbrew-repo](https://github.com/ps5-payload-dev/pacbrew-repo) | `722187f0ee246c0d7d57401c8a97de1c7f4e2ff0` metadata | packages require individual license/source review | ecosystem discovery only; no package used |
| [ps5-payload-dev/elfldr](https://github.com/ps5-payload-dev/elfldr) | release `v0.27`, tag and default head `2a8da002e5292481ea964daa7342bbf4855f4c33` | README and inspected `elfldr.c`, `pt.c`: GPL-3.0-or-later | external pre-existing loader contract research; no loader implementation imported |
| [ps5-payload-dev/websrv](https://github.com/ps5-payload-dev/websrv) | release `v0.34`: `4d35610914273f8f4fc37af2b1d5bfb069dfba5b`; default head `7748df9d2a43822a5ebfda149f43d4f2696d640b` | README: GPL-3.0-or-later default, xterm.js MIT; inspected `src/ps5/elfldr.c`: GPL-3.0-or-later | optional external homebrew frontend research; no dependency or bundled assets |

SDK v0.43 (`d9c9519116944a7f1c22d262012c53b80b3520b7`) is what HearBridge's
README names. Our v0.45 selection is a build candidate, **not a claim that
HearBridge or our platform is tested with it**. The v0.45 archive downloaded
during research matched SHA-256
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
stock SDK CRT remains outside the approved payload scope. SDK flags can omit
that CRT; a minimal entry must independently satisfy the actual installed
loader contract without importing credential mutation or kernel helpers.
Neither researched loader supplies a direct symbol-resolver callback in its
first argument field: it supplies a `getpid` syscall stub. Do not infer that
field's behavior from the SDK typedef's name.

### Per-file reuse decision

All entries are pinned to the inventory revisions. This is a selection review,
not a statement that the files have been imported. Preserve copyright and
license notices, exact source, local changes, and attribution when adoption is
authorized. Linked SDK components need a complete corresponding-source plan.

| Candidate files | Terms inspected | Recommendation and excluded behavior |
| --- | --- | --- |
| SDK `host/bin/prospero-clang`, `prospero-lld`, `crt/_start.S`, `crt/syscall.c`, `crt/syscall.h`, `include/ps5/payload.h` | explicit GPL-3.0-or-later notices | external wrappers; minimal entry/syscall bridge may be adapted only with explicit provenance, loader-specific validation and source distribution; no stock CRT requirement is inferred |
| SDK `crt/crt.c`, `crt/patch.c`, `crt/kernel.c`, `crt/rtld*.c` | explicit GPL-3.0-or-later notices | scope audit reference; do not adopt the startup credential/syscall mutation or kernel-memory resolver for a minimal diagnostic |
| SDK `include/freebsd/dev/usb/usb.h`, `usb_ioctl.h`, `usb_endian.h`, `usb_freebsd.h` | individual BSD-style notices, including two-clause terms in `usb.h` and `usb_ioctl.h` | use SDK headers externally in C; preserve each file's actual notice if redistributed; not a license grant for proprietary runtime libraries |
| HearBridge `src/bt/usb_hci_desc.c`, `usb_hci_desc.h`, `btchip.c`, `btchip.h` | no overriding file license found; README/NOTICE GPL-3.0-or-later | smallest useful adaptation candidates for descriptor parsing/vendor labels; strengthen malformed/truncated descriptor errors; reject unknown active backends rather than adopting default-profile fallback |
| HearBridge `src/bt/hci_usb.c`, `hci_cmd.c` | no overriding file license found; README/NOTICE GPL-3.0-or-later | behavior reference only: exclude competing IN readers, overwrite rings, timed credit refill, endpoint failover and guessed scan-state restoration |
| HearBridge `src/avcap2.c`, `src/creds.c` | no overriding file license found; README/NOTICE GPL-3.0-or-later | capture ABI research only; exclude credential fallback, heuristic format/channel inference and hidden restart policy; `creds.c` is out of scope |
| PlayPods `src/capture.c`, `src/hci.c`, `src/bt.c` | root GPL v3; README GPL-3.0; no later-version grant established in these files | retain as reference; capture byte-length checks and consume-once cleanup are useful design evidence; any adaptation retains GPL-3.0-only conservatively, subject to final rights review; shared-reader transport remains blocked |
| elfldr `elfldr.c`, `pt.c`; websrv `src/ps5/elfldr.c`, `pt.c` | explicit GPL-3.0-or-later notices | external loader contract reference only; do not import process takeover, credential changes, kernel-memory operations or loader bootstrap |

The initial implementation recommendation is an original bounded descriptor
probe using external SDK headers, plus the existing Rust/C ABI. Keep upstream
quirk decisions documented in C adaptation; no audio stack or application-wide
C import is warranted. Pure parsing adaptation, if selected later, belongs in
its own reviewable change with provenance and hostile-input tests.

## Behavior provenance

All links are pinned. These entries identify what to study or adapt later, not
which claims have been reproduced here.

| Behavior | Evidence | Planned owner / constraints |
| --- | --- | --- |
| Avcap2 capture, Ipmi prerequisite, NID resolution, private parameter block and statuses | [PlayPods capture.c](https://github.com/FGGstore/FGG-PlayPods/blob/d726e1c32568048aae66f3218c0540adc357e408/src/capture.c), [HearBridge avcap2.c](https://github.com/X-F1REBALL-X/HearBridge-PS5/blob/cd5b7fce1b8226d8850cdd306437550098c6a62b/src/avcap2.c) | retain C module loading, resolution, layout and cleanup; never expose private structs to Rust; verify format/status units |
| USB control command requests, USB FS event/ACL transfers and lifetime | [PlayPods hci.c](https://github.com/FGGstore/FGG-PlayPods/blob/d726e1c32568048aae66f3218c0540adc357e408/src/hci.c), [HearBridge hci_usb.c](https://github.com/X-F1REBALL-X/HearBridge-PS5/blob/cd5b7fce1b8226d8850cdd306437550098c6a62b/src/bt/hci_usb.c) | C adapter; ownership and native delivery remain blockers |
| Vendor identification, descriptors, MediaTek endpoint pairing | [btchip.h](https://github.com/X-F1REBALL-X/HearBridge-PS5/blob/cd5b7fce1b8226d8850cdd306437550098c6a62b/src/bt/btchip.h), [usb_hci_desc.c](https://github.com/X-F1REBALL-X/HearBridge-PS5/blob/cd5b7fce1b8226d8850cdd306437550098c6a62b/src/bt/usb_hci_desc.c) | C controller backend; match actual VID/PID/descriptors, fail unknown |
| MediaTek scan pause and restoration | [hci_cmd.c](https://github.com/X-F1REBALL-X/HearBridge-PS5/blob/cd5b7fce1b8226d8850cdd306437550098c6a62b/src/bt/hci_cmd.c) | C quirk policy only after coexistence tests; excluded from first probe |
| Credits, foreign events and shared endpoint risks | [PlayPods bt.c](https://github.com/FGGstore/FGG-PlayPods/blob/d726e1c32568048aae66f3218c0540adc357e408/src/bt.c), HearBridge USB source above | independent Rust accounting; do not adopt timer-based credit refill as proof of available buffers |
| Resident-audio evidence limitations | [RFC](https://github.com/FGGstore/FGG-PlayPods/blob/d726e1c32568048aae66f3218c0540adc357e408/docs/RESIDENT-AUDIO-RFC.md), [hardware summary](https://github.com/FGGstore/FGG-PlayPods/blob/d726e1c32568048aae66f3218c0540adc357e408/docs/hardware-summary.json) | test design reference, not our compatibility evidence |
| Final linker target/script/CRT, 16 KiB segment alignment | [prospero-clang](https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/host/bin/prospero-clang), [prospero-lld](https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/host/bin/prospero-lld) | external SDK toolchain; no handwritten PS5 loader |
| Startup credential/syscall permission mutation | [crt.c](https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/crt/crt.c), [patch.c](https://github.com/ps5-payload-dev/sdk/blob/a7685ce8e97ce1f8a4f38253a41d54241c9124f1/crt/patch.c) | scope blocker to default runtime adoption; no local implementation |

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

## Update procedure

1. Fetch a candidate into an external research checkout; never overwrite the
   existing pin or build from a moving branch. Inspect LICENSE/NOTICE and all
   changed files before reuse. Resolve a tag to a full commit and hash archives.
2. Record repository, commit, license, selected files, relationship (linked,
   vendored, adapted, specification implementation, behavioral reference), reason,
   and maintainer. Review runtime/scope behavior as well as API compatibility.
3. When code is adopted, use one explicit pinned vendor/submodule dependency,
   isolate changes under `platform/ps5`, and keep numbered patches with upstream
   commit, rationale, license and tests. Do not create a parallel copy of the
   whole upstream application. No patches or vendored sources exist today.
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
