"""16-bit NOR flash on CE1 at data word 0x200000 (src/include/zen_flash.h).

Command set as used by flash_hw.c (SST / JEDEC): unlock 0xaa @ 0x5555, 0x55 @ 0x2aaa,
then at 0x5555: 0x90 software ID (word 0 = maker 0xbf, word 1 = device), 0x98 CFI query
(only word 0x27, the size exponent, is used by flash_cfi; 0x10..0x12 hold "QRY"), 0xf0 exit,
0xa0 program one word (bits can only be cleared), 0x80 + unlock + 0x10 @ 0x5555 chip
erase, 0x30 @ address sector erase (2048 words), 0x50 @ address block erase (32768
words). Commands are compared on the low byte (the firmware writes 0xaaaa, 0x9090 ...).
Program and erase complete at once, so the DQ6 toggle / DQ7 polling of the driver sees
the final data immediately.
The part is an SST39VF400A (marking on the board, docs/hw.md): device id 0x2780 (the
driver accepts 0x2780..0x2782), 256K words, so the chip ends at word 0x23FFFF; the default
flash layout ends at 0x230000. Whether the chip is mirrored above that in CE1 is not known
(not modeled).
"""

import collections

BASE, WORDS = 0x200000, 0x40000
MAKER, DEVICE = 0x00BF, 0x2780
SECTOR, BLOCK = 2048, 0x8000
UNLOCK = ((0x5555, 0xAA), (0x2AAA, 0x55))
CFI = {0x10: 0x51, 0x11: 0x52, 0x12: 0x59, 0x27: (2 * WORDS).bit_length() - 1}


class Nor:
    def __init__(self, bus, image=None):
        self.mem = list(image) if image is not None else [0xFFFF] * WORDS
        if len(self.mem) != WORDS:
            raise ValueError(f"NOR image must have {WORDS} words")
        self.seq = []                   # (offset, low byte) of the current command
        self.mode = "read"              # read, id, program
        self.erase_armed = False
        self.unknown = collections.Counter()
        self.programs = self.erases = 0
        bus.hook_data(BASE, BASE + WORDS, self.read, self.write)

    def read(self, word):
        off = word - BASE
        if self.mode == "id" and off in (0, 1):
            return MAKER if off == 0 else DEVICE
        if self.mode == "cfi" and off in CFI:
            return CFI[off]
        return self.mem[off]

    def write(self, word, value):
        off, low = word - BASE, value & 0xFF
        if self.mode == "program":
            self.mem[off] &= value
            self.programs += 1
            self.mode = "read"
            return
        if low == 0xF0 and len(self.seq) != 2:
            self.seq, self.mode, self.erase_armed = [], "read", False     # reset
            return
        self.seq.append((off, low))
        if len(self.seq) <= 2:
            if self.seq[-1] != UNLOCK[len(self.seq) - 1]:
                self._bad()
            return
        (addr, cmd), self.seq = self.seq[2], []
        if self.erase_armed:
            self.erase_armed = False
            if cmd == 0x10 and addr == 0x5555:
                self.mem = [0xFFFF] * WORDS
            elif cmd in (0x30, 0x50):
                size = SECTOR if cmd == 0x30 else BLOCK
                start = addr - addr % size
                self.mem[start:start + size] = [0xFFFF] * size
            else:
                return self._bad()
            self.erases += 1
            return
        if addr != 0x5555:
            return self._bad()
        if cmd == 0x90:
            self.mode = "id"
        elif cmd == 0x98:
            self.mode = "cfi"
        elif cmd == 0xF0:
            self.mode = "read"
        elif cmd == 0xA0:
            self.mode = "program"
        elif cmd == 0x80:
            self.erase_armed = True
        else:
            self._bad()

    def _bad(self):
        self.unknown[tuple(self.seq)] += 1
        self.seq, self.erase_armed = [], False

    def image_words(self):
        return list(self.mem)
