#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Inspect every non-index ar member without requiring host support for ELF."""
from pathlib import Path
import struct
import sys


def inspect(path):
    data = path.read_bytes()
    if data[:8] != b"!<arch>\n":
        raise ValueError("expected ordinary ar archive")
    offset, objects = 8, 0
    while offset < len(data):
        header = data[offset:offset + 60]
        if len(header) != 60 or header[58:] != b"`\n":
            raise ValueError("invalid ar header")
        size = int(header[48:58].strip())
        name = header[:16].strip()
        body = data[offset + 60:offset + 60 + size]
        if len(body) != size:
            raise ValueError("truncated archive member")
        if name not in [b"/", b"//", b"/SYM64/"]:
            if body[:7] != b"\x7fELF\x02\x01\x01" or len(body) < 64:
                raise ValueError(f"expected little-endian ELF64 object: {name!r}")
            kind, machine = struct.unpack_from("<HH", body, 16)
            if kind != 1 or machine != 62:
                raise ValueError(f"expected x86-64 relocatable object: {name!r}")
            objects += 1
        offset += 60 + size + (size % 2)
    if not objects or offset != len(data):
        raise ValueError("empty or misaligned archive")
    print(f"PASS {objects} little-endian ELF64 x86-64 relocatable objects")
    print("This confirms archive format, not PS5 runtime/loader compatibility.")


if __name__ == "__main__":
    inspect(Path(sys.argv[1]))
