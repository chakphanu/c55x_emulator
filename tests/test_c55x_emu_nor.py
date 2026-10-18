"""C55x emulator: NOR flash command set and the synthetic flash image."""

import unittest

from c55x_emu.bus import Bus
from c55x_emu.nor import BASE, DEVICE, MAKER, SECTOR, WORDS, Nor
from c55x_emu import norimage


def command(bus, cmd, addr=0x5555):
    bus.write(BASE + 0x5555, 0xAAAA)
    bus.write(BASE + 0x2AAA, 0x5555)
    bus.write(BASE + addr, cmd)


class NorTests(unittest.TestCase):
    def setUp(self):
        self.bus = Bus()
        self.nor = Nor(self.bus)

    def test_software_id_and_exit(self):
        command(self.bus, 0x9090)
        self.assertEqual((self.bus.read(BASE), self.bus.read(BASE + 1)), (MAKER, DEVICE))
        command(self.bus, 0xF0F0)
        self.assertEqual(self.bus.read(BASE), 0xFFFF)

    def test_cfi_size_exponent(self):
        command(self.bus, 0x9898)
        self.assertEqual(1 << self.bus.read(BASE + 0x27), 2 * WORDS)
        self.bus.write(BASE, 0xF0)                              # plain reset
        self.assertEqual(self.bus.read(BASE + 0x27), 0xFFFF)

    def test_program_only_clears_bits(self):
        command(self.bus, 0xA0A0)
        self.bus.write(BASE + 0x100, 0x1234)
        command(self.bus, 0xA0A0)
        self.bus.write(BASE + 0x100, 0xFF00)
        self.assertEqual(self.bus.read(BASE + 0x100), 0x1200)
        self.assertEqual(self.nor.programs, 2)

    def test_sector_erase(self):
        for off in (SECTOR - 1, SECTOR, 2 * SECTOR):
            command(self.bus, 0xA0A0)
            self.bus.write(BASE + off, 0)
        command(self.bus, 0x8080)
        command(self.bus, 0x3030, SECTOR + 5)
        self.assertEqual([self.bus.read(BASE + o) for o in (SECTOR - 1, SECTOR, 2 * SECTOR)],
                         [0, 0xFFFF, 0])

    def test_bad_unlock_is_counted(self):
        self.bus.write(BASE + 0x5555, 0xAAAA)
        self.bus.write(BASE + 0x1234, 0x5555)
        self.assertEqual(sum(self.nor.unknown.values()), 1)


class NorImageTests(unittest.TestCase):
    def test_table_and_checksum(self):
        data = b"\x12\x34\x56"
        mem = norimage.build({"BOOT": data})
        table = norimage.TABLE - BASE
        self.assertEqual(mem[table:table + 2], list(norimage.fourcc("TSIG")))
        boot = mem[table + 16:table + 32]
        self.assertEqual(boot[:2], list(norimage.fourcc("BOOT")))
        self.assertEqual((boot[4] << 16 | boot[5], boot[9]), (3, (0x1234 + 0x56FF) & 0xFFFF))
        self.assertEqual(mem[:2], [0x1234, 0x56FF])

    def test_oversized_area_raises(self):
        with self.assertRaises(ValueError):
            norimage.build({"CONF": bytes(0x4001)})


if __name__ == "__main__":
    unittest.main()
