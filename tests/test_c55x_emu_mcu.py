"""C55x emulator: I2C master model, MCU model and Hello-MCU end to end."""

import unittest

from c55x_emu.paths import ROOT
from c55x_emu.bus import Bus
from c55x_emu.i2c import BASE, CNT, DRR, DXR, MDR, SAR, STR, STR_BB, STR_NACK, STR_RRDY, I2c
from c55x_emu.mcu import WAKE_POWER_KEY, Mcu, key_code

HELLO_MCU = ROOT / "build" / "hello_mcu" / "hello_mcu.out"


class Rig:
    def __init__(self):
        self.bus = Bus()
        self.i2c = I2c(self.bus)
        self.irqs = 0
        self.mcu = Mcu(self._irq, {1: 0x4123})
        self.i2c.attach(7, self.mcu)

    def _irq(self):
        self.irqs += 1

    def reg(self, r, v=None):
        if v is None:
            return self.bus.port_read(BASE + r)
        self.bus.port_write(BASE + r, v)
        return None

    def write_byte(self, addr, b):
        """Sequence of i2c_write (fw/common/i2c.c, src/drivers/iic.c)."""
        self.reg(SAR, addr)
        self.reg(MDR, 0x76A0)
        self.reg(DXR, b)
        st = self.reg(STR)
        self.reg(MDR, 0x5EA0)
        return st

    def read(self, addr, n):
        self.reg(SAR, addr)
        self.reg(CNT, n)
        self.reg(MDR, 0x7C20)
        out = []
        while self.reg(STR) & STR_RRDY:
            out.append(self.reg(DRR))
        return out


class I2cMcuTests(unittest.TestCase):
    def test_register_read(self):
        rig = Rig()
        rig.write_byte(7, 1)
        self.assertEqual(rig.irqs, 1)
        self.assertEqual(rig.read(7, 2), [0x41, 0x23])
        self.assertFalse(rig.reg(STR) & STR_BB)

    def test_command_with_argument(self):
        rig = Rig()
        rig.write_byte(7, 36)
        rig.write_byte(7, 3)
        rig.write_byte(7, 33)
        self.assertEqual(rig.mcu.commands, [(36, 3), (33, None)])

    def test_events_queue_and_empty_answer(self):
        rig = Rig()
        rig.mcu.inject(0x11)
        rig.mcu.inject(0x7A)
        self.assertEqual(rig.irqs, 2)
        self.assertEqual(rig.read(7, 1), [0x11])
        self.assertEqual(rig.irqs, 3)           # second event still pending
        self.assertEqual(rig.read(7, 1), [0x7A])
        self.assertEqual(rig.read(7, 1), [0x00])

    def test_absent_slave_nacks(self):
        rig = Rig()
        self.assertTrue(rig.write_byte(0x50, 1) & STR_NACK)
        self.assertEqual(rig.irqs, 0)

    def test_key_codes(self):
        self.assertEqual((key_code(0x11), key_code(0x40), key_code(0x7A)), (8, 13, None))


@unittest.skipUnless(HELLO_MCU.exists(), "build Hello-MCU first: fw/build.sh hello_mcu")
class McuPowerTests(unittest.TestCase):
    def test_only_power_wakes_after_suspend(self):
        irqs = []
        mcu = Mcu(lambda: irqs.append(1))
        mcu.end_write([30, 27])                     # firmware suspend sequence
        self.assertTrue(mcu.off)
        n = len(irqs)
        mcu.inject(0x11)                            # OK: ignored while off
        self.assertEqual((list(mcu.events), len(irqs)), ([], n))
        mcu.inject(0x10)                            # Power press
        self.assertEqual((mcu.off, mcu.regs[2], list(mcu.events)), (False, WAKE_POWER_KEY, [0x10]))
        self.assertEqual(len(irqs), n + 1)


class HelloMcuTests(unittest.TestCase):
    def test_start_up_and_event_display(self):
        from c55x_emu.machine import Machine
        m = Machine(HELLO_MCU, {1: 0x0100})
        m.run(300_000)
        self.assertEqual(m.mcu.commands, [(6, 9), (33, None), (36, 3)])
        before = [row[:] for row in m.lcd.rows()]
        m.mcu.inject(0x11)
        m.run(300_000)
        after = m.lcd.rows()
        self.assertFalse(m.mcu.events)
        self.assertNotEqual(before[32:48], after[32:48])    # first event line (page 4) drawn
        self.assertFalse(m.i2c.anomalies)
        self.assertGreaterEqual(m.cpu.interrupts_taken[16], 1)     # via fw_mcu_isr (INT1)

    def test_timer_tick_interrupt(self):
        from c55x_emu.machine import Machine
        m = Machine(HELLO_MCU)
        m.run(1_100_000)                                    # one tick = 1,000,000 clocks
        self.assertEqual(m.cpu.interrupts_taken[4], 1)
        self.assertEqual(m.timers[0].expired, 1)


if __name__ == "__main__":
    unittest.main()
