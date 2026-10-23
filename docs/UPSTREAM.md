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

SDK v0.43 (`d9c9519116944a7f1c22d262012c53b80b3520b7`) is what HearBridge's
README names. Our v0.45 selection is a build candidate, **not a claim that
HearBridge or our platform is tested with it**. The v0.45 archive downloaded
during research matched SHA-256
`5fac64093d0e3e69f2584ce7935f24f703f622d9681f1495c85b9f00d1291a2c`.
URL: [versioned archive](https://github.com/ps5-payload-dev/sdk/releases/download/v0.45/ps5-payload-sdk.zip).
Release publication observed: 2026-10-21. Do not use `latest` in build scripts.

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
