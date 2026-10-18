"""ATA hard disk on CE1 (src/include/zen_ide.h): task file at data words 0x280000..7,
alternate status / device control at 0x280806, INTRQ = CPU interrupt 3 (IFR0 bit 3).

Modeled (ATA/ATAPI-5 command protocol, LBA only): IDENTIFY DEVICE, READ/WRITE SECTORS
(0x20/0x21, 0x30/0x31), READ/WRITE MULTIPLE (0xc4/0xc5), SET MULTIPLE MODE (0xc6), SET
FEATURES (0xef), power and cache commands without data (accepted), software reset
(device control SRST). Data words are little endian (first disk byte in bits 7..0).
Each command step completes after 2000 cycles (BSY meanwhile); INTRQ follows the PIO
protocols: data-in once per DRQ block, data-out after each block, non-data at the end.
Unknown commands abort (ERR, ABRT) and are counted in 'unknown'.

The state machine runs in the Rust core (rust/c55x_core/src/ata.rs), so the firmware's
data word accesses (PIO and DMA) never leave it; the core reads and writes the disk
through disk.Disk, one call per sector. This module provides the IDENTIFY data and the
Python view (register access for tools and tests, counters).
"""

BASE, ALT = 0x280000, 0x280806
BSY, DRQ = 0x80, 0x08                   # status bits


def ata_string(text, words):
    raw = text.ljust(2 * words).encode("ascii")
    return [(raw[i] << 8) | raw[i + 1] for i in range(0, 2 * words, 2)]


def identify(disk, max_multiple=16):
    w = [0] * 256
    w[0] = 0x0040
    w[1], w[3], w[6] = 16383, 16, 63
    w[10:20] = ata_string("EMU0000000000001", 10)
    w[23:27] = ata_string("EMU 1.0", 4)
    w[27:47] = ata_string("C55X EMULATOR SYNTHETIC DISK", 20)
    w[47] = 0x8000 | max_multiple
    w[49] = 0x0200                      # LBA
    w[53] = 0x0006                      # words 64..70 and 88 valid
    w[60], w[61] = disk.sectors & 0xFFFF, disk.sectors >> 16
    w[64] = 0x0003                      # PIO 3 and 4
    w[80], w[82], w[83] = 0x003E, 0x4000, 0x4008     # ATA-1..5; APM supported
    return w


class Ata:
    def __init__(self, cpu, disk):
        self.cpu, self.disk = cpu, disk
        cpu.core.ata_attach(disk, disk.sectors, identify(disk))

    def read(self, word):
        """Register read as the CPU sees it (BASE..BASE + 7, ALT)."""
        return self.cpu.core.ata_io(word)

    def write(self, word, value):
        self.cpu.core.ata_io(word, value & 0xFFFF)

    @property
    def commands(self):
        """{command byte: count}"""
        return self.cpu.core.ata_stats()[0]

    @property
    def unknown(self):
        """{unmodeled access: count}"""
        return self.cpu.core.ata_stats()[1]
