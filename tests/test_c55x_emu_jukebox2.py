"""C55x emulator: the original Jukebox2 firmware boots through cinit, app_main, the
DSP/BIOS start and mcu_init (same MCU start-up as the reconstructed src/drivers/mcu_state.c)."""

import unittest

from c55x_emu.diskimage import MINIFS_SECTORS, UPDATE_DIR, synthetic_disk
from c55x_emu.loader import CODE_DIR, load_jukebox2


@unittest.skipUnless((CODE_DIR / "manifest.json").exists(), "Jukebox2 code blocks not exported")
class Jukebox2BootTests(unittest.TestCase):
    def test_boot_reaches_mcu_init(self):
        from c55x_emu.machine import Machine
        m = Machine(load_jukebox2())
        m.run(1_200_000)
        self.assertEqual(m.mcu.commands[:3], [(6, 9), (33, None), (36, 3)])
        self.assertGreater(m.cpu.interrupts_taken[4], 0)          # timer 0 via the BIOS dispatcher

    @unittest.skipUnless((UPDATE_DIR / "manifest.json").exists(), "official update not extracted")
    def test_cfs_format_with_the_original_code(self):          # slow: about 9M instructions
        from c55x_emu.cfsformat import format_cfs
        disk = synthetic_disk(MINIFS_SECTORS + 200_000)
        self.assertGreater(format_cfs(disk), 0)                   # raises unless flag == 0


if __name__ == "__main__":
    unittest.main()
