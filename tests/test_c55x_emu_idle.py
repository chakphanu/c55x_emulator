"""C55x emulator: the idle instruction with the idle configuration (SPRU317K, SPRU375G)."""

import unittest

from c55x_emu.idlectl import IdleControl
from c55x_emu.testutil import machine


def idle_machine(icr):
    cpu, bus = machine("XAR3 = mar(*(#00001h))", f"*AR3 = #{icr} || writeport()", "idle",
                       "AC0 = #5")
    cpu.idle_ctl = IdleControl(bus)
    cpu.ier0 = 1 << 4                                # timer 0; INTM = 1 after reset
    cpu.schedule(1000, lambda: cpu.raise_interrupt(4))
    return cpu


class IdleTests(unittest.TestCase):
    def test_cpu_domain_active_runs_on(self):
        cpu = idle_machine(0x2A)                     # EMIF, peripherals, DMA idle
        cpu.run(4)
        self.assertEqual((cpu.ac[0], cpu.in_idle), (5, False))
        self.assertLess(cpu.cycles, 1000)

    def test_enabled_interrupt_wakes_with_intm_set(self):
        cpu = idle_machine(0x01)
        cpu.run(10)
        self.assertEqual((cpu.ac[0], cpu.in_idle), (5, False))
        self.assertGreaterEqual(cpu.cycles, 1000)
        self.assertEqual(cpu.idle_ctl.istr, 0x01)

    def test_clock_stopped_waits_for_an_external_interrupt(self):
        cpu = idle_machine(0x3F)
        self.assertTrue(cpu.run(10))
        self.assertTrue(cpu.in_idle)
        self.assertLess(cpu.cycles, 1000)               # timers do not run
        cpu.raise_interrupt(4)                          # e.g. the MCU line in the board
        cpu.run(3)
        self.assertEqual((cpu.ac[0], cpu.in_idle), (5, False))

    def test_interrupt_ends_idle_and_returns_after_it(self):   # power_down_idle wake-up
        cpu, bus = machine("XAR3 = mar(*(#00001h))", "*AR3 = #63 || writeport()",
                           "bit(ST1, #11) = #0", "idle", "AC0 = #5", "goto #0x02000a",
                           "AC1 = #7", "return_int")
        cpu.idle_ctl = IdleControl(bus)
        cpu.ier0, cpu.ivpd = 1 << 4, 0x0200
        bus.mem[0x20020:0x20028] = bytes([0, 0x02, 0x00, 0x0C, 0x20, 0x20, 0x20, 0x20])
        self.assertTrue(cpu.run(10))                    # powered down, clock stopped
        self.assertTrue(cpu.in_idle)
        cpu.raise_interrupt(4)                          # external wake-up (MCU in the board)
        cpu.run(20)
        self.assertEqual((cpu.ac[1], cpu.ac[0], cpu.in_idle, cpu.pc), (7, 5, False, 0x2000A))


if __name__ == "__main__":
    unittest.main()
