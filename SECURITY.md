# Security and project scope

This project has no supported binary releases. Diagnostic execution has limited
evidence on one Slim/13.40 environment; Bluetooth playback, audio quality and
long-run coexistence remain unverified. See docs/HW3-EVIDENCE.md. The project
develops Bluetooth/audio functionality for an
already homebrew-capable environment, not the mechanism that enabled it.

Do not contribute a new exploit/vulnerability chain to obtain initial kernel
access, kernel/hypervisor patch chains, DRM or license bypasses, piracy
functionality, Sony binaries, firmware, protected Sony/DRM keys or commercial
game content. Reviewed minimal changes to this payload's own credentials and
syscall-origin permissions may use only kernel R/W already supplied by the
installed environment, within AGENTS.md's Bluetooth/capture scope. Preserve
errors and document touched structures, offsets, restoration and hardware
unknowns. Missing initial access is a blocker. System-process/driver mutation,
driver detach or dispatcher installation is not authorized by that scope.
The SDK startup audit is recorded in docs/RESEARCH.md and docs/RUNTIME-REVIEW.md.

Local Bluetooth pairing may create protected bond keys for reconnect and
forget-device operations. Never commit or distribute those keys, or disclose
them through logs or the LAN page. Minimal authenticated local device management
may expose only the identity required for device selection; public reports and
status remain redacted. Experimental transport admission follows AGENTS.md and
does not implicitly authorize competing USB readers, timed credit refill or
system-driver changes.

Bluetooth peers and traces are untrusted. Report parser memory-safety errors,
unbounded allocation, unsafe FFI lifetime/length errors, key disclosure, native
traffic loss and unexpected scope violations. Public bug reports must omit
pairing keys, addresses/serials, personal data and exploit payloads.

When the repository is hosted and private vulnerability reporting is enabled,
use its Security tab for sensitive reports. No private contact or disclosure
service exists in this local bootstrap; do not assume a public issue is private.
Use an existing private maintainer channel if available and publish only a
sanitized high-level description otherwise. Maintainers must establish a real
private reporting channel before publishing supported binaries. No response-time
guarantee is claimed yet.
