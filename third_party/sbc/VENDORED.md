# Vendored BlueZ libsbc source

This directory contains only the selected library source from BlueZ libsbc
release 2.2. It is third-party code under LGPL-2.1-or-later, not relicensed by
the PS5-BT project.

## Immutable source

- Repository: `https://git.kernel.org/pub/scm/bluetooth/sbc.git`
- Release: `2.2` (2026-02-09)
- Commit: `b3deb8a5dcfb42d8c10ba1f2f1ac9bd7bf7271cc`
- Annotated tag object: `96cde3ae64b963a91f4432693700bf36c8512a3b`
- Archive: `https://www.kernel.org/pub/linux/bluetooth/sbc-2.2.tar.xz`
- Archive SHA-256: `a1ada76ef35e5af9c2fbd063754dc9e37a8d989417c6eb1ecebb089b1383ae9e`

The archive digest and all selected-file hashes were independently checked on
2026-10-24 before import. `upstream/` is a byte-for-byte pristine selection:

| File | SHA-256 |
| --- | --- |
| `COPYING.LIB` | `ec60b993835e2c6b79e6d9226345f4e614e686eb57dc13b6420c15a33a8996e5` |
| `AUTHORS` | `e57876fcdfa3cfdf88db7f6ed971c1f44fda8e9a33c8c1d9999903945e75fd2c` |
| `sbc/sbc.c` | `42d16642f3eebf51f7f9e84a03f62a1d722c03414ed3265ba183bdaa0d9dccc4` |
| `sbc/sbc.h` | `ab2e351d97ee25d7836902465ef1e5589c7bc07988e5c2bdc484ac1ffee754ab` |
| `sbc/sbc_private.h` | `16dd9e1621d73c387ac4b2027320028e963e5a2d09013d9dccec4ede3142a140` |
| `sbc/sbc_math.h` | `4cbe03013967fd9872c86677f4ff398994010706ffc6f77d39792f44bc8f2fdf` |
| `sbc/sbc_tables.h` | `fe57949c20000aa3cacc359887ea1ea0cb31146371beb2488f732bc6bdf17933` |
| `sbc/sbc_primitives.c` | `1badabd8eee20c25a1c0a684354e53e68e8901ca571493060965016ca9c0ec74` |
| `sbc/sbc_primitives.h` | `9cf89824889d2e0464d4135808ead9a7d70ee223d6e1b73e43969fa969a3951e` |

`COPYING.LIB`, `AUTHORS`, and every selected code file retain their upstream
notices. No optimized assembly, architecture-specific primitive, upstream tool,
application, or build-system implementation is included.
The pristine `COPYING.LIB` intentionally retains upstream whitespace (including
a whitespace-only line and terminal blank line); do not normalize it to silence
Git whitespace diagnostics. Its complete digest is checked above and by tests.

## Local patches

Apply the numbered patches from this directory with `patch -p1` to a copy of
`upstream/`; never edit the pristine files:

1. `0001-generic-c-only.patch` removes architecture headers and runtime CPU
   dispatch while retaining the upstream generic fixed-point algorithms.
2. `0002-caller-storage.patch` adds size/alignment queries and a caller-storage
   initializer/finalizer. `SBC_STATIC_ONLY` omits allocator-backed upstream entry
   points from the codec build. It does not change filter, quantizer, packing,
   or decoding algorithms.
3. `0003-freestanding-static.patch` replaces hosted C-library headers only for
   `SBC_STATIC_ONLY` with the project-owned minimal target declarations. It
   retains the generic codec algorithms and exposes no allocator or I/O API.

`tools/test_sbc.py` verifies every pristine hash, applies the patches in a
temporary directory, builds the wrapper, and exercises it. `--build-archive`
produces a separately linkable `libps5_bt_sbc.a` from the same verified inputs;
the normal acceptance run rebuilds a replacement archive and relinks the vector
consumer against it. The patched library remains separately rebuildable from
this source and these patches.

## Distribution and relinking

Any distributed combined executable must preserve prominent attribution and
the complete LGPL-2.1-or-later text, provide the complete selected library
source plus these local patches and build instructions, and use a valid LGPL
2.1 section 6 arrangement that permits recipients to replace and relink the
library. Application object materials and concrete working relinking steps are
therefore required before distributing a codec-enabled ELF. A URL alone is not
corresponding source. Only the explicit `ps5-sbc-selftest` variant links the
codec; its source package contains application objects and tested replacement
relink materials described in docs/SBC-TARGET.md.

This source selection does not establish patent clearance, Bluetooth
qualification, acoustic quality, interoperability, or PS5 execution.
