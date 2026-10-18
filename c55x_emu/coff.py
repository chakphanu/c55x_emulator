"""TI-COFF version 2 loader for C55x executables (headers little endian, raw data big endian).

Section addresses and sizes are byte units (checked against ofd55 for .text and .const).
"""

import struct

FILE_HDR = struct.Struct("<HHIIIHHH")       # magic, nscns, time, symptr, nsyms, opthdr, flags, target
OPT_HDR = struct.Struct("<HHIIIIII")        # magic, vstamp, tsize, dsize, bsize, entry, text, data
SCN_HDR = struct.Struct("<8sIIIIIIIIIHH")   # name, paddr, vaddr, size, scnptr, relptr, lnnoptr,
                                            # nreloc, nlnno, flags, reserved, page
SYM = struct.Struct("<8sIhHbb")             # name, value, section, type, class, numaux
COFF2_MAGIC = 0x00C2
C55X_TARGET = 0x009C
STYP_NOLOAD = 0x02
STYP_BSS = 0x80
STYP_COPY = 0x10


def _name(raw, strings):
    if raw[:4] == b"\0\0\0\0":
        off = struct.unpack_from("<I", raw, 4)[0]
        return strings[off:strings.index(b"\0", off)].decode()
    return raw.rstrip(b"\0").decode()


def load(data):
    """-> {"entry", "sections": [(name, byte address, bytes)], "symbols": {name: value}
    (the first of equal names), "symbol_list": [(name, value)] (all)}."""
    magic, nscns, _, symptr, nsyms, opthdr, _, target = FILE_HDR.unpack_from(data, 0)
    if magic != COFF2_MAGIC or target != C55X_TARGET:
        raise ValueError(f"not a C55x TI-COFF2 file (magic 0x{magic:x}, target 0x{target:x})")
    if opthdr != OPT_HDR.size:
        raise ValueError("executable without optional header")
    entry = OPT_HDR.unpack_from(data, FILE_HDR.size)[5]
    strings = data[symptr + nsyms * SYM.size:]
    sections = []
    off = FILE_HDR.size + opthdr
    for i in range(nscns):
        raw, paddr, _, size, scnptr, *_rest = SCN_HDR.unpack_from(data, off + i * SCN_HDR.size)
        flags = _rest[4]
        if scnptr == 0 or flags & (STYP_NOLOAD | STYP_BSS | STYP_COPY):
            continue
        sections.append((_name(raw, strings), paddr, data[scnptr:scnptr + size]))
    symbols, symbol_list, i = {}, [], 0
    while i < nsyms:
        raw, value, scn, _, _, numaux = SYM.unpack_from(data, symptr + i * SYM.size)
        if scn > 0:
            symbols.setdefault(_name(raw, strings), value)
            symbol_list.append((_name(raw, strings), value))
        i += 1 + numaux
    return {"entry": entry, "sections": sections, "symbols": symbols, "symbol_list": symbol_list}
