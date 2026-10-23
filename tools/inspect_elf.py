#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Fail closed on the minimal bootstrap's loader-relevant ELF properties."""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import struct


def inspect_bytes(data):
    def require(condition, message):
        if not condition:
            raise ValueError(message)

    def unpack(fmt, offset):
        size = struct.calcsize(fmt)
        require(0 <= offset <= len(data) - size, "truncated ELF structure")
        return struct.unpack_from(fmt, data, offset)

    def region(offset, size):
        require(0 <= offset <= len(data) and 0 <= size <= len(data) - offset,
                "ELF range exceeds file")
        return data[offset:offset + size]

    def string(table, offset):
        require(0 <= offset < len(table), "invalid string offset")
        end = table.find(b"\0", offset)
        require(end >= 0, "unterminated ELF string")
        return table[offset:end].decode("ascii")

    require(data[:7] == b"\x7fELF\x02\x01\x01", "expected little-endian ELF64")
    kind, machine, version, entry, phoff, shoff, _, ehsize, phsize, phnum, shsize, shnum, shstr = unpack("<HHIQQQIHHHHHH", 16)
    require((kind, machine, version, ehsize, phsize, shsize) == (3, 62, 1, 64, 56, 64),
            "expected x86-64 ET_DYN with standard headers")
    require(phnum > 0 and shnum > 0 and shstr < shnum, "missing ELF tables")
    region(phoff, phnum * phsize)
    region(shoff, shnum * shsize)
    programs = [unpack("<IIQQQQQQ", phoff + i * phsize) for i in range(phnum)]
    sections = [unpack("<IIQQQQIIQQ", shoff + i * shsize) for i in range(shnum)]
    loads = []
    for ptype, flags, offset, address, _, filesz, memsz, alignment in programs:
        region(offset, filesz)
        require(ptype not in (3, 7), "interpreter or TLS segment is forbidden")
        if ptype == 1:
            require(flags in (4, 5, 6), "unexpected or writable/executable PT_LOAD")
            require(filesz <= memsz and alignment == 0x4000,
                    "PT_LOAD must use 16 KiB alignment and filesz <= memsz")
            require(address % alignment == offset % alignment == 0,
                    "PT_LOAD must start on a 16 KiB boundary")
            loads.append({"offset": offset, "vaddr": address, "filesz": filesz,
                          "memsz": memsz, "flags": flags, "align": alignment})
    require(loads and min(p["vaddr"] for p in loads) == 0,
            "audited loader requires image base zero")
    require(any(p["flags"] & 1 and p["vaddr"] <= entry < p["vaddr"] + p["memsz"]
                for p in loads), "entry is outside executable PT_LOAD")
    names = region(sections[shstr][4], sections[shstr][5])
    symbols, undefined, relocations = {}, [], Counter()
    for section in sections:
        name, stype, flags, _, offset, size, link, _, _, entsize = section
        label = string(names, name)
        require(not flags & 0x400, "TLS section is forbidden")
        require(not (label in (".init_array", ".fini_array", ".preinit_array") and size),
                "constructors/destructors are unsupported")
        if flags & 2 and size:
            require(any(p["vaddr"] <= section[3] and section[3] + size <= p["vaddr"] + p["memsz"]
                        for p in loads), "allocated section is outside PT_LOAD")
        if stype != 8:
            body = region(offset, size)
        if stype in (2, 11):
            require(entsize == 24 and size % 24 == 0 and link < shnum,
                    "invalid symbol table")
            strings = region(sections[link][4], sections[link][5])
            for pos in range(offset, offset + size, 24):
                key, info, _, index, value, _ = unpack("<IBBHQQ", pos)
                symbol = string(strings, key)
                if symbol:
                    if index == 0 and info >> 4 != 0:
                        undefined.append(symbol)
                    elif index:
                        symbols[symbol] = value
        elif stype == 6:
            require(entsize == 16 and size % 16 == 0, "invalid dynamic table")
            for pos in range(offset, offset + size, 16):
                tag, _ = unpack("<qQ", pos)
                require(tag not in (1, 17, 18, 19, 35, 36, 37),
                        "DT_NEEDED, REL or RELR is forbidden")
        elif stype == 4:
            require(entsize == 24 and size % 24 == 0, "invalid RELA table")
            for pos in range(offset, offset + size, 24):
                target, info, addend = unpack("<QQq", pos)
                require(info == 8, "only symbol-free R_X86_64_RELATIVE is supported")
                require(any(p["vaddr"] <= target and target + 8 <= p["vaddr"] + p["memsz"]
                            for p in loads), "relocation target outside PT_LOAD")
                require(0 <= addend <= max(p["vaddr"] + p["memsz"] for p in loads),
                        "relative relocation points outside image")
                relocations["R_X86_64_RELATIVE"] += 1
        else:
            require(stype not in (9, 19), f"unsupported relocation section {label}")
    require(not undefined, f"undefined imports: {sorted(set(undefined))}")
    required = ("_start", "main", "ps5_rust_probe", "ps5_platform_log",
                "ps5_platform_abort", "ps5_bootstrap_lookup", "__crt_syscall", "__crt_syscall_init")
    require(all(key in symbols for key in required), "missing required bootstrap/Rust symbols")
    require(symbols["_start"] == entry, "entry differs from _start")
    forbidden = [s for s in symbols if s.startswith("kernel_") or
                 s in ("__patch_init", "__kernel_init", "__crt_start", "__rtld_init")]
    require(not forbidden, f"forbidden stock runtime symbols: {forbidden}")
    return {"sha256": hashlib.sha256(data).hexdigest(), "size": len(data),
            "type": "ET_DYN", "machine": "x86-64", "entry": entry,
            "segments": loads, "imports": [], "relocations": dict(relocations),
            "required_symbols": {name: symbols[name] for name in required},
            "hardware": "UNTESTED; structural inspection is not runtime approval"}


def self_test(data):
    inspect_bytes(data)
    cases = [b"", data[:63], b"NOPE" + data[4:], data[:-1]]
    for index, replacement in ((16, b"\x02\0"), (18, b"\xb7\0")):
        cases.append(data[:index] + replacement + data[index + len(replacement):])
    phoff = struct.unpack_from("<Q", data, 32)[0]
    modified = bytearray(data)
    struct.pack_into("<I", modified, phoff, 7)
    cases.append(modified)
    modified = bytearray(data)
    struct.pack_into("<I", modified, phoff + 4, 7)
    cases.append(modified)
    modified = bytearray(data)
    struct.pack_into("<Q", modified, phoff + 48, 0x1000)
    cases.append(modified)
    shoff = struct.unpack_from("<Q", data, 40)[0]
    shnum = struct.unpack_from("<H", data, 60)[0]
    for index in range(shnum):
        section_offset = shoff + 64 * index
        section = struct.unpack_from("<IIQQQQIIQQ", data, section_offset)
        stype, offset, size = section[1], section[4], section[5]
        if stype == 4 and size:
            modified = bytearray(data)
            struct.pack_into("<Q", modified, offset + 8, 7)
            cases.append(modified)
        if stype == 6:
            modified = bytearray(data)
            struct.pack_into("<q", modified, offset, 1)
            cases.append(modified)
        if stype == 2:
            for pos in range(offset, offset + size, 24):
                _, info, _, symbol_index, _, _ = struct.unpack_from("<IBBHQQ", data, pos)
                if info >> 4 == 1 and symbol_index:
                    modified = bytearray(data)
                    struct.pack_into("<H", modified, pos + 6, 0)
                    cases.append(modified)
                    break
    for case in cases:
        try:
            inspect_bytes(case)
        except (ValueError, UnicodeDecodeError):
            continue
        raise AssertionError("malformed ELF accepted")
    print(f"PASS ELF inspection and {len(cases)} malformed-input cases")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("elf", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    try:
        data = args.elf.read_bytes()
        if args.self_test:
            self_test(data)
        else:
            print(json.dumps(inspect_bytes(data), indent=2))
    except (OSError, ValueError, UnicodeDecodeError) as error:
        parser.exit(1, f"ELF inspection failed: {error}\n")
