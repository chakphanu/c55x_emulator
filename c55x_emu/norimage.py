"""Synthetic NOR flash contents as an official update leaves them (zen_flash.h).

The area table ("TSIG" header + entries of 16 words, docs/2026-10-09-update_and_flash)
uses the default layout of flash_format (data word 0x423df6): BOOT 0x200000 (32 KiB),
CONF 0x204000 (16 KiB), TOC0 0x206000 (8 KiB, the table itself), PFM1 0x207000 (8 KiB),
RESC 0x208000 (320 KiB). BOOT gets FBOOT and RESC gets FRESC from the extracted update,
packed big endian per word (as the update parser's data words), with length and 16-bit
word sum filled in like flash_write does; CONF and PFM1 stay empty (length 0, erased).
"""

import struct

from .diskimage import UPDATE_DIR
from .nor import BASE, WORDS

TABLE = 0x206000
LAYOUT = (("BOOT", 0x200000, 0x8000), ("CONF", 0x204000, 0x4000), ("TOC0", 0x206000, 0x2000),
          ("PFM1", 0x207000, 0x2000), ("RESC", 0x208000, 0x50000))


def words_of(data):
    if len(data) % 2:
        data += b"\xff"
    return list(struct.unpack(f">{len(data) // 2}H", data))


def fourcc(name):
    return struct.unpack(">2H", name.encode("ascii"))


def entry(name, addr, length, alloc, total):
    return [*fourcc(name), addr >> 16, addr & 0xFFFF, length >> 16, length & 0xFFFF,
            alloc >> 16, alloc & 0xFFFF, 0, total & 0xFFFF, 0, 0, 0, 0, 0, 0]


def build(contents):
    """contents: {area name: bytes} -> list of WORDS flash words."""
    mem = [0xFFFF] * WORDS
    table = entry("TSIG", TABLE, 0, 0, 0)
    for name, addr, alloc in LAYOUT:
        data = contents.get(name, b"")
        if len(data) > alloc:
            raise ValueError(f"{name}: {len(data)} bytes exceed {alloc}")
        w = words_of(data)
        mem[addr - BASE:addr - BASE + len(w)] = w
        table += entry(name, addr, len(data), alloc, sum(w) & 0xFFFF if data else 0)
    mem[TABLE - BASE:TABLE - BASE + len(table)] = table
    return mem


def synthetic_nor(update_dir=UPDATE_DIR):
    if not (update_dir / "FBOOT").exists():
        return build({})
    return build({"BOOT": (update_dir / "FBOOT").read_bytes(),
                  "RESC": (update_dir / "FRESC").read_bytes()})
