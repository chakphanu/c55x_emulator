"""Instruction cache over program memory: preloaded listings plus on-demand dis55 decoding.

An address without a decoded instruction is decoded from the current memory contents:
a WINDOW-byte slice is wrapped as '.byte' data at its address, assembled, linked and
disassembled with the TI tools (as scripts/c55x_listing.py does for the whole image).
Instructions that could be cut by the window end are dropped. Writes into a 256-byte
page holding decoded code (e.g. an overlay copied to SARAM) drop that page and the one
before it, so the new contents are decoded on the next fetch.
"""

import subprocess
import tempfile
from pathlib import Path

from .listing import TI_BIN, parse

WINDOW = 1024
MAX_INSN = 7                            # longest bundle in bytes (+1 margin)
MAX_CACHED_INSNS = 1 << 20             # 1M instructions: cap the code cache
MODE = "\t.mmregs\n\t.cpl_on\n\t.arms_on\n\t.c54cm_off\n"


def _run(cmd, cwd):
    r = subprocess.run([str(c) for c in cmd], cwd=cwd, capture_output=True, text=True,
                       errors="surrogateescape")
    if r.returncode != 0:
        raise RuntimeError(f"{' '.join(map(str, cmd))} failed:\n{r.stdout}\n{r.stderr}")
    return r.stdout


def dis55_window(data, addr, work):
    """dis55 text for raw bytes placed at program address addr."""
    lines = [MODE + '\t.sect ".text"']
    lines += ["\t.byte " + ",".join(f"0x{b:02x}" for b in data[i:i + 16]) for i in range(0, len(data), 16)]
    (work / "w.asm").write_text("\n".join(lines) + "\n")
    (work / "w.cmd").write_text(f"MEMORY {{ CODE (RX) : origin = 0x{addr:x}, length = 0x01000000 }}\n"
                                "SECTIONS { .text : {} > CODE }\n")
    _run([TI_BIN / "asm55", "w.asm", "w.obj"], work)
    _run([TI_BIN / "lnk55", "-q", "w.obj", "w.cmd", "-o", "w.out"], work)
    return _run([TI_BIN / "dis55", "-qq", "-r", "-g", "w.out"], work)


class CodeCache:
    def __init__(self, bus, insns=None, decode=True):
        self.bus = bus
        self.insns = {}
        self.pages = {}                 # page -> set of instruction addresses
        self.decode_enabled = decode
        self.decoded_windows = 0
        self._tmp = None
        self.dropped = None             # list of invalidated pages while a Rust core runs
        bus.code_pages = self.pages
        bus.invalidate_code = self.invalidate
        self.add(insns or {})

    def add(self, insns):
        for a, insn in insns.items():
            if len(self.insns) >= MAX_CACHED_INSNS:
                raise RuntimeError(
                    f"code cache full: {len(self.insns)} instructions "
                    f"(max {MAX_CACHED_INSNS}); possible runaway decode")
            self.insns[a] = insn
            self.pages.setdefault(a >> 8, set()).add(a)

    def get(self, pc):
        insn = self.insns.get(pc)
        if insn is None and self.decode_enabled:
            self.add(self.decode(pc))
            insn = self.insns.get(pc)
        return insn

    def decode(self, pc):
        if self._tmp is None:
            self._tmp = tempfile.TemporaryDirectory(prefix="c55x-emu-")
        data = bytes(self.bus.mem[pc:pc + WINDOW])
        text = dis55_window(data, pc, Path(self._tmp.name))
        self.decoded_windows += 1
        limit = pc + len(data) - MAX_INSN
        return {a: i for a, i in parse(text, markers=False).items() if a + i.size <= limit or a == pc}

    def invalidate(self, byte_addr):
        page = byte_addr >> 8
        if self.dropped is not None:
            self.dropped.extend((page - 1, page))
        for p in (page - 1, page):
            for a in self.pages.pop(p, ()):
                self.insns.pop(a, None)
