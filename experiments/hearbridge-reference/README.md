# First-audio reference candidate

This is a separately built HearBridge reference, not a PS5-BT backend or a
performance improvement claim. First establish one real headset connection and
audible sample on the user's Slim / 13.40, then use the result to choose the
minimum integration and an actual comparison. Directory probes remain on hold.

The local patch isolates private state, removes takeover/tile installation and
optional game/rest watchers, gates the LAN page with a generated credential,
disables raw HCI/debug/backup endpoints, and reports cleanup uncertainty.
It ends after one session, with a 180-second cooperative session limit and a
separate 30-second media limit. Existing tone generation runs without capture;
capture is opened only when the operator switches the tone off. This makes the
first audible result independent of capture startup and format inference.

**Build preparation is authorized; execution of the retained competing USB
readers and timed credit fallback requires an explicit separate scope decision.**
Do not upload or run the stock baseline as a shortcut around that decision.

## Source, license and build

- HearBridge: [X-F1REBALL-X/HearBridge-PS5](https://github.com/X-F1REBALL-X/HearBridge-PS5),
  `cd5b7fce1b8226d8850cdd306437550098c6a62b` (version 1.3.6).
  Copyright 2026 X-F1REBALL-X, GPL-3.0-or-later, as recorded by its LICENSE/NOTICE.
  The full upstream payload source list in `ps5.mk` is used only in the isolated
  reference; none is linked into the Rust product. Existing per-file notices
  and third-party attribution remain in the corresponding-source archive.
- SDK: [ps5-payload-dev/sdk](https://github.com/ps5-payload-dev/sdk),
  `d9c9519116944a7f1c22d262012c53b80b3520b7` (v0.43, upstream's documented choice).
  GPL-3.0-or-later default; preserve BSD/header-specific notices. Build stock
  `crt`, `libc`, `include`, `host`, and four public C import stubs only:
  kernel_web, SceLibcInternal, SceNet, SceSystemService. No Sony binaries,
  firmware, import harvesting or `sce_stubs/stubs` generation is required.
- `bounded-reference.patch` records all local source changes. It is a derivative
  under GPL-3.0-or-later; retain upstream ownership and this modification record.
  Its purpose is to remove optional side effects and bound/observe a reference
  run. It does not repair shared transport ownership or establish real credits.

Use Python 3.12+, Git, make, patch, and LLVM (validated with 18.1.8). The source
arguments are local Git repositories containing the exact commits; snapshots
come from Git objects, ignoring their working-tree modifications. The output
directory must not already exist and should be under ignored `build/`:

```sh
python3 experiments/hearbridge-reference/build.py \
  --hearbridge-source build/reference/hearbridge \
  --sdk-source build/reference/sdk-v043-source \
  --llvm-config /opt/homebrew/opt/llvm@18/bin/llvm-config \
  --output build/reference/first-audio-candidate
```

The builder produces a separately named ELF, import-checked target build log,
manifest with exact hashes, corresponding-source archive including both pins
and the recipe, and private `access.json` credentials. It never deploys or
starts a payload. Builds use a fresh random password, so independent artifacts
have different hashes. The source archive excludes that credential; recipients
generate their own. Keep the private header, access file, ELF's embedded secret
and all later bond state out of public artifacts and Git. HTTP Basic is access
control on the trusted local test LAN, not encrypted transport.

To rebuild the already-patched corresponding-source archive without Git/network,
extract it into a new private directory (mode 0700). From that directory, export
`LLVM_CONFIG` to the local LLVM binary, then run:

```sh
set -e
export PS5_PAYLOAD_SDK="$PWD/sdk"
for part in crt libc include host; do
  make -C "sdk-source/$part" install DESTDIR="$PS5_PAYLOAD_SDK" || exit
done
make -C sdk-source/sce_stubs install DESTDIR="$PS5_PAYLOAD_SDK" \
  SHARED='libkernel_web.so libSceLibcInternal.so libSceNet.so libSceSystemService.so'
python3 - <<'PY'
import base64, json, secrets, sys
from pathlib import Path
sys.path.insert(0, 'recipe')
from build import private_write
password = secrets.token_urlsafe(24)
auth = base64.b64encode(('ps5bt:' + password).encode()).decode()
private_write(Path('hearbridge/src/reference_secret.h'),
              '#define HB_REF_BASIC_AUTH "Basic ' + auth + '"\n')
private_write(Path('access.json'), json.dumps({'username': 'ps5bt', 'password': password}))
PY
READELF="$("$LLVM_CONFIG" --bindir)/llvm-readelf" make -C hearbridge -f ps5.mk -j4
```

Focused checks in the extracted tree (no console access):

```sh
make -C hearbridge test-control
cc -std=c11 -Wall -Wextra -Werror -Ihearbridge/src hearbridge/tests/test_reference_stop.c -o test-reference-stop
./test-reference-stop
```

## Retained mechanism and unresolved facts

The unchanged connection/audio chain is USB HCI → inquiry/pairing → ACL/L2CAP →
SDP → AVDTP/A2DP/SBC, with the upstream tone/capture paths. `hci_usb.c` queues
40 event and 20 ACL reads on endpoints also used by the system. Its fixed rings
can overwrite old traffic. `acl_pool.c` decrements outstanding counts after
elapsed time when completion events are missing. These are experimental
upstream behaviors, not owned allowance, preserved system events or reconciled
credits. A finite run cannot prove coexistence. Record local queue losses and
fallback counts; unobserved foreign traffic is still Unknown.

Retained HCI traffic includes controller buffer/address/SSP queries, event-mask
write `0x0c01` with `0x7dbfffffffffffff`, conditional SSP enable `0x0c56`,
inquiry/cancel/name requests, pairing/authentication/encryption and the selected
headset's connection/ACL/disconnection. Inquiry mode and scan-enable helpers
also write shared controller settings (MediaTek-specific pause paths are
conditional on the detected chip). The prior event mask is unknown and not
restored; SSP enable is not restored. Inquiry/scan restoration is attempted by
upstream but is not proof of restored system state. The run decision must
include these shared-controller writes, not just competing reads. No radio
reset or driver detach was found in this selected connection path.

The SDK's pre-main runtime remains part of this reference. Its `patch.c` reads
this process's `proc + 0x3e8`, then writes syscall-origin bounds at the resulting
object's `+0xf0/+0xf8` to `0/~0`. Through `proc + 0x40` it changes own ucred
capabilities at `+0x60` (`caps[5]=0x1c`, `[7]=0x40`, `[15]|=0x40`) and attributes
at `+0x80` (`attrs[3]|=0x80`). `rtld_so.c` uses own-process VM entry protection
at `+0x64` for executable mappings. These unsafe layouts and broad startup
permissions are **Requires hardware**, retained for baseline compatibility,
and are not restored by the stock runtime. The runtime uses the existing
loader's kernel-R/W handles; it does not establish initial access. Absence of
that access blocks execution. Do not change system-process credentials, detach
drivers, reset the radio or install a dispatcher.

SDK 0.43 has explicit 13.40 layout cases; that is source evidence, not hardware
acceptance. Loader compatibility, USB ownership, capture units/PCM format,
cleanup quiescence and headset behavior remain unverified. Existing PS5-BT
native ABI and production permission/credit gates are unchanged.

## One admitted run

After the explicit scope decision, confirm a current console window. Use one
reference entry, verify its remote hash against `manifest.json`, and prepare
immediate log collection. Do not load the on-hold raw-openat diagnostic.

1. Confirm wireless controller input before launch and put XM4 in pairing mode.
   Launch once, open the private reference UI on its reported port (8090 first),
   and use the local `access.json` credential. Do not expose the server publicly.
   Start with a low headset volume for the existing 1 kHz test tone.
2. Select only the XM4, approve pairing, connect, and use the existing tone to
   separate Bluetooth output from capture-format uncertainty. If the tone is
   audible, observe a short game/system capture sample within the same budget.
3. Record the first failed stage or audible result, then Stop. Observe the
   controller during and after. Collect logs immediately; retain artifact hash,
   failure/cleanup status, credit fallback and queue-loss counters, and user
   observations. Keep device/bond files private; never collect bond keys as logs.
4. Stop on controller anomaly, deadline or uncertain cleanup. No automatic
   rerun, transport reset, process killing or escalation. A native call/join can
   overrun the cooperative deadline; a timeout is a failure, not clean shutdown.

The candidate's implementation and tested bounds are recorded with its build
result in the current handoff. A successful tone proves the output chain only;
captured game audio, latency, continuity, reconnection and coexistence need
their own observations. After first audio, choose one measured defect (for
example dropouts under the same game/headset conditions) for the first A/B
improvement. Rewriting C into Rust is not itself a success criterion.
