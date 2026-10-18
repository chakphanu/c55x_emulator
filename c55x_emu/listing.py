"""Decoded instructions from dis55 text: {address: Insn}.

Accepts dis55 output ('addr: hex  text', parallel parts separated by '_') and the project
listing (output/analysis/listing, 'addr  hex  text'). The last instruction of a
localrepeat/blockrepeat body comes from the encoding: the block opener is the first or
last part of its bundle; localrepeat '0100101E 1lllllll' (offset l7), blockrepeat
'0000111E llllllll llllllll' (offset l16); last body instruction = bundle end + offset.
Checked against the '}' markers of dis55 (they must agree when present).
"""

import re
import subprocess
from dataclasses import dataclass, field

from .paths import TI_BIN

LINE = re.compile(r"^([0-9a-f]{6,8}):?\s+([0-9a-f_]+)\s+(\S.*)$")
SECTION = re.compile(r"^(TEXT|DATA) section ")


@dataclass
class Insn:
    addr: int
    size: int
    text: str
    block_end: int = None           # localrepeat/blockrepeat: address of the last body insn
    exe: object = field(default=None, repr=False)


def disassemble(out_path):
    return subprocess.run([TI_BIN / "dis55", "-q", "-r", "-g", str(out_path)], check=True,
                          capture_output=True, text=True).stdout


def block_end(addr, raw, text):
    """Last body instruction of a repeat block opened by this bundle, else None."""
    parts = [p.strip() for p in text.split("||")]
    openers = [i for i, p in enumerate(parts) if p in ("localrepeat {", "blockrepeat {")]
    if not openers:
        return None
    local = parts[openers[0]] == "localrepeat {"
    n = 2 if local else 3
    b = raw[:n] if openers[0] == 0 else raw[-n:]
    end = addr + len(raw)
    if local and b[0] & 0xFE == 0x4A and b[1] & 0x80:
        return end + (b[1] & 0x7F)
    if not local and b[0] & 0xFE == 0x0E:
        return end + int.from_bytes(b[1:3], "big")
    raise ValueError(f"0x{addr:06x}: cannot locate the repeat opcode in {raw.hex()} '{text}'")


def parse(text, markers=True):
    """dis55 (TEXT sections) or project listing text -> {address: Insn}. markers=False
    ignores dis55 '}' lines (a window cut may separate a block from its end)."""
    insns, opened, last, in_text, dis_format = {}, [], None, True, None
    for line in text.splitlines():
        m = SECTION.match(line)
        if m:
            in_text = m[1] == "TEXT"
            continue
        if not in_text:
            continue
        s = line.strip()
        if s == "}":
            if dis_format is False or not markers:
                continue                    # project listing: ends come from the encoding
            if not opened:
                raise ValueError(f"unbalanced '}}' after 0x{last:06x}")
            start = opened.pop()
            if insns[start].block_end != last:
                raise ValueError(f"0x{start:06x}: encoded block end 0x{insns[start].block_end:06x}"
                                 f" differs from dis55 0x{last:06x}")
            continue
        m = LINE.match(s)
        if not m or m[3].endswith(":") or m[3].startswith((".byte", ".word")):
            continue
        addr = int(m[1], 16)
        raw = bytes.fromhex(m[2].replace("_", ""))
        insn = Insn(addr, len(raw), m[3].strip())
        insn.block_end = block_end(addr, raw, insn.text)
        insns[addr] = insn
        last = addr
        dis_format = s.split()[0].endswith(":")
        if insn.block_end is not None and dis_format and markers:
            opened.append(addr)             # dis55 format: a '}' marker will follow
    if opened:
        raise ValueError(f"unterminated repeat block at 0x{opened[-1]:06x}")
    return insns
