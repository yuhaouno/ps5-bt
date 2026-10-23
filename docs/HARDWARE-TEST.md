# First hardware test payload

No hardware run has occurred. The current C shim deliberately returns
Unsupported. Do not replace that result with synthetic controller metadata in a
payload. Desktop fake packets have separate test-only source files.

Development wave closeout (2026-10-23): physical PS5 access is currently
unavailable. **The next action is HW-1 physical validation using the baseline
ELF below. HW-2 has not started.** The optional query ELF is software-tested
only and is not part of this first physical test.

## Ready diagnostic: HW-1 entry, logging and return

First target: **PS5 Slim, firmware 13.40, unverified**. The default `make`
produces `build/ps5/ps5-bt.elf`. This is a hardware test candidate with no audio,
pairing, HCI command or USB operation. It uses the reviewed
[minimal startup](RUNTIME-REVIEW.md); its matching manifest records the exact
source commit, tool versions and SHA-256. Keep that manifest with the log.

The latest published loader versions checked on 2026-10-23 are
[elfldr v0.27](https://github.com/ps5-payload-dev/elfldr/releases/tag/v0.27),
commit `2a8da002e5292481ea964daa7342bbf4855f4c33`, and
[websrv v0.34](https://github.com/ps5-payload-dev/websrv/releases/tag/v0.34),
commit `4d35610914273f8f4fc37af2b1d5bfb069dfba5b`. These are inspected loader
contracts, **not evidence either can be installed or run on firmware 13.40**.
Use only a compatible loader already available in the homebrew environment.
This project supplies no loader installation or exploit procedure. Record the
actual loader/version; a different entry ABI needs review before this test.

1. Confirm wireless DualSense input works; leave the headset disconnected.
2. Build with `make`, or use the supplied ELF together with its manifest and
   corresponding source bundle. Check the ELF hash against the manifest.
3. If **elfldr v0.27 is already listening on its configured port** (default
   9021), run the following from the repository root, replacing `PS5_IP`.
   This sends the diagnostic once and captures at most 64 KiB for 15 seconds.
   For an already configured websrv, use its existing ELF launch workflow and
   capture its payload console instead; do not send TCP bytes to a web port.

```sh
python3 - PS5_IP <<'PY'
from pathlib import Path
import socket, sys, time
artifact = Path("build/ps5/ps5-bt.elf")
output = Path("build/ps5/hw1.log")
captured = bytearray()
outcome = "connection closed"
with socket.create_connection((sys.argv[1], 9021), timeout=10) as stream:
    stream.sendall(artifact.read_bytes())
    stream.shutdown(socket.SHUT_WR)
    deadline = time.monotonic() + 15
    while len(captured) < 65536:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            outcome = "capture deadline; payload completion UNKNOWN"
            break
        stream.settimeout(remaining)
        try:
            block = stream.recv(min(4096, 65536 - len(captured)))
        except TimeoutError:
            outcome = "capture deadline; payload completion UNKNOWN"
            break
        if not block:
            break
        captured.extend(block)
    else:
        outcome = "capture limit; payload completion UNKNOWN"
output.write_bytes(captured)
print(captured.decode("utf-8", errors="replace"), end="")
print(f"\n{outcome}; log saved to {output}")
PY
```

4. Expected payload output:

```text
event=rust_entry abi=1 phase=bootstrap
event=adapter_unavailable status=unsupported
```

Expected payload result is **2**, not zero: the transport is deliberately
Unsupported. A closed network connection alone does not prove clean return;
also check the loader is responsive and wireless DualSense input still works.
There is no audible output. Do not pair, reset a radio or change permissions.

5. Return the complete PS5-BT/payload log, actual loader name/version or artifact
   hash, ELF hash, whether the loader resumed/remains responsive, and DualSense
   input before/during/after. Include any displayed return code or error. Slim
   and 13.40 are already recorded; note any change. Redact unrelated addresses,
   account details and keys. No headset identity is needed for this stage.

No Rust log can mean an early startup failure, loader incompatibility or stderr
routing failure; report the observed result without guessing. Startup results:
120 invalid arguments; 121 syscall bridge init failed; 122 write unavailable;
123 exit unavailable; 124 incomplete log write; 125 unsupported getpid stub
prefix. A 125 stops before invoking the stub. Do not provide library dumps or
attempt alternate offsets. Panic result 101 is a separate failure investigation.

Only after this entry/return test passes should an optional native presence
query or descriptor experiment run. See [NATIVE-QUERY.md](NATIVE-QUERY.md).
None of these diagnostics closes the active transport ownership gate below.

## H0: runtime and seam

Use the reviewed minimal startup and an already capable ELF
environment, log build ID, Rust version, SDK commit, ABI version, entry and return.
Run C -> Rust -> C logging and the unsupported-init path. Confirm the loader
regains control and DualSense works wirelessly before/during/after. Capture
panic termination only as a separate isolated experiment. Do not change process
credentials, kernel state or system processes to make this succeed.

## H1: controller metadata

Implement a reviewed C read-only descriptor backend: enumerate actual candidate
USB device nodes, inspect descriptors and distinguish built-in controller
identity from external USB devices. Report observed VID/PID/device revision,
interface/endpoint descriptors and permission errors. No USB FS input reads,
controller detach, reset or alternate-interface mutation during enumeration.
Node names and endpoint numbers from upstream are candidate observations only.
If descriptor access requires extra privileges, return the exact platform code
and record a blocker.

## H2: bounded HCI information

Requires a demonstrated routing/ownership mechanism that preserves the native
stack's traffic. Multiple pending reads are not that mechanism. A reviewer must
record how native commands/events/ACL are delivered, how responses are associated
with our commands, and why the native owner remains correct. If unavailable,
stop after H1 and report the blocker; do not claim M1 completed.

Issue one read command at a time respecting command credits, with a proposed
1 s per-command deadline and 10 s overall ceiling using a monotonic clock:
Read Local Version Information, Supported Commands, Supported Features,
Read Buffer Size, then supported relevant extended feature pages. Record raw
numeric fields, HCI status, missing responses, actual elapsed time and validity.
No inferred defaults after a timeout. A diagnostic command failure is visible;
do not reset the controller or retry indefinitely. Current runtime contains only
the first local-version query as a synthetic seam exercise; it is not an H2
implementation and must not be attached to a live backend as-is.

No inquiry, pairing, scan changes, event-mask changes, ACL/audio traffic,
process takeover, credential fallback or controller firmware upload in H0–H2.
Clean up every owned resource; check wireless pad input and pad reconnection
after shutdown. Stop immediately on lost/foreign traffic or observed pad failure.

## Report

Record exact source revision and dirty state, archive/ELF SHA-256, Rust/LLVM/SDK
versions, loader/runtime and entry mode, console model as reported or Unknown,
system firmware, USB descriptors, HCI/LMP fields with status/validity, test
duration, DualSense actions/results, Wi-Fi state, errors and cleanup result.
Sanitize addresses, pairing keys, serials, user names, IPs and unrelated content.
Record failures as well as passes. Do not store captured game PCM in fixtures.

M1 passes only with actual controller data through Rust/C, no scope violation,
correct native traffic handling, and clean return with normal wireless
DualSense operation. It does not establish audio/headset compatibility.
