# First hardware test payload

No hardware run has occurred. The current C shim deliberately returns
Unsupported. Do not replace that result with synthetic controller metadata in a
payload. Desktop fake packets have separate test-only source files.

## H0: runtime and seam

Resolve the SDK startup scope gate first. Using only an already capable ELF
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
