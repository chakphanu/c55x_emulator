"""Helpers shared by the emulator tests and the tests of the tools built on it."""

from .bus import Bus
from .cpu import Cpu
from .listing import parse as parse_listing


def machine(*lines):
    """CPU over a program at byte 0x20000, two bytes per instruction; idle loop at the end."""
    text, addr, openers = [], 0x20000, []
    for t in lines:
        if t == "}":
            start, index = openers.pop()     # encode 'localrepeat' with its body length
            text[index] = f"{start:06x}: 4a{0x80 | (addr - 2 - start - 2):02x} localrepeat {{"
            text.append(t)
            continue
        if t == "localrepeat {":
            openers.append((addr, len(text)))
        text.append(f"{addr:06x}: 0000 {t}")
        addr += 2
    end = addr
    text.append(f"{end:06x}: 0000 goto #0x{end:06x}")
    bus = Bus()
    cpu = Cpu(bus, parse_listing("\n".join(text)))
    cpu.pc, cpu.xsp, cpu.ssp = 0x20000, 0x1000, 0x0800
    return cpu, bus


def run(cpu):
    if not cpu.run(10_000):
        raise AssertionError("no idle loop reached")
