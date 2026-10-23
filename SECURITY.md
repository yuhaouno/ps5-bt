# Security and project scope

This bootstrap has no supported binary releases. All deployed console behavior
remains unverified. The project supplies Bluetooth/audio functionality for an
already homebrew-capable environment, not the mechanism that enabled it.

Do not contribute exploits, kernel/hypervisor patch chains, new privilege
escalation, DRM or license bypasses, piracy functionality, Sony binaries,
firmware, keys or commercial game content. If an operation needs permissions
missing from the supplied environment, preserve the error and document a blocker.
The current SDK startup scope issue is recorded in docs/RESEARCH.md.

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
