"""C55x emulator: ATA disk, synthetic disk image (partition table, minifs). DMA:
test_c55x_emu_audio.py."""

import struct
import unittest

from c55x_emu.ata import ALT, BASE, BSY, DRQ, Ata
from c55x_emu.bus import Bus
from c55x_emu.cpu import Cpu
from c55x_emu.disk import SECTOR, Disk
from c55x_emu.diskimage import MAGIC, MINIFS_SECTORS, partition_table, synthetic_disk
from c55x_emu import minifs_image


def rig(sectors=4096):
    cpu = Cpu(Bus(), {})
    disk = Disk(sectors)
    return cpu, disk, Ata(cpu, disk)


def settle(cpu, cycles=5000):
    cpu.core.advance(cycles)


class AtaTests(unittest.TestCase):
    def test_identify_reports_the_capacity(self):
        cpu, disk, ata = rig()
        ata.write(BASE + 7, 0xEC)
        self.assertTrue(ata.read(BASE + 7) & BSY)
        settle(cpu)
        self.assertTrue(ata.read(BASE + 7) & DRQ)
        self.assertTrue(cpu.ifr0 & (1 << 3))                    # INTRQ = interrupt 3
        words = [ata.read(BASE) for _ in range(256)]
        self.assertEqual((words[60], words[61]), (4096, 0))
        self.assertFalse(ata.read(BASE + 7) & DRQ)

    def test_write_then_read_multiple(self):
        cpu, disk, ata = rig()
        ata.write(BASE + 2, 2)
        ata.write(BASE + 7, 0xC6)                               # SET MULTIPLE 2
        settle(cpu)
        for reg, v in ((2, 3), (3, 10), (4, 0), (5, 0), (6, 0xE0)):
            ata.write(BASE + reg, v)
        ata.write(BASE + 7, 0xC5)                               # WRITE MULTIPLE 3 sectors
        settle(cpu)
        for i in range(3 * 256):
            self.assertTrue(ata.read(ALT) & DRQ or i % 512 == 0)
            ata.write(BASE, i)
            if i % 512 == 511:
                settle(cpu)
        settle(cpu)
        self.assertEqual(disk.read(10)[:4], bytes([0, 0, 1, 0]))   # word 0, word 1, low first
        self.assertEqual(disk.read(12)[:2], (512).to_bytes(2, "little"))
        ata.write(BASE + 7, 0xC4)                               # READ MULTIPLE
        settle(cpu)
        got = []
        while len(got) < 3 * 256:
            if not ata.read(ALT) & DRQ:
                settle(cpu)
                continue
            got.append(ata.read(BASE))
        self.assertEqual(got, list(range(3 * 256)))

    def test_unknown_command_aborts(self):
        cpu, disk, ata = rig()
        ata.write(BASE + 7, 0x99)
        settle(cpu)
        self.assertEqual((ata.read(BASE + 7) & 1, ata.read(BASE + 1)), (1, 0x04))


class ImageTests(unittest.TestCase):
    def test_partition_table_layout(self):
        raw = partition_table(100_000, [("minifs", 40416), ("cfs", None)])
        words = struct.unpack("<256H", raw)
        self.assertEqual((words[0] << 16 | words[1], words[3], words[4] << 16 | words[5]),
                         (MAGIC, 512, 100_000))
        self.assertEqual(words[8:12], (0, 40416, 0, 1))
        self.assertEqual("".join(chr(w) for w in words[12:18]), "minifs")
        self.assertEqual((words[20] << 16 | words[21], words[23]), (100_000 - 40417, 40417))

    def test_minifs_file_round_trip(self):
        disk = Disk(MINIFS_SECTORS + 10)
        data = bytes(range(256)) * 20 + b"x"
        minifs_image.write(disk, 1, [("a.bin", data)], MINIFS_SECTORS * SECTOR)

        def mem_bytes(off, n):              # memory order = swapped disk pairs
            raw = b"".join(disk.read(1 + s) for s in range(off // SECTOR, (off + n) // SECTOR + 1))
            return minifs_image.swap_pairs(raw[off % SECTOR:off % SECTOR + n + (n & 1)])[:n]
        entry = struct.unpack(">16H4L", mem_bytes(minifs_image.DIR, 48))
        self.assertEqual("".join(chr(w) for w in entry[:5]), "a.bin")
        self.assertEqual(entry[16:], (1, len(data), 0, 0))
        clusters = struct.unpack(">L4H", mem_bytes(minifs_image.MAPS, 12))[1:]
        self.assertEqual(clusters, (0, 1, 0xFFFF, 0))
        self.assertEqual(mem_bytes(minifs_image.DATA, len(data)), data)
        self.assertEqual(struct.unpack(">L", mem_bytes(minifs_image.BITMAP, 4))[0], 3)

    def test_synthetic_disk_without_files(self):
        disk = synthetic_disk(MINIFS_SECTORS + 100, files=[])
        self.assertEqual(struct.unpack("<2H", disk.read(0)[:4]), (MAGIC >> 16, MAGIC & 0xFFFF))
        self.assertEqual(disk.read(1), bytes(SECTOR))         # empty minifs: first long 0


if __name__ == "__main__":
    unittest.main()
