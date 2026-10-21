"""Oracle v2 additions: K16 multiplier sign extension (SPRU374E 4.47/4.48
operand rules) and repeat-body trace visibility (no value or cycle change)."""

import unittest
from tests.test_c55x_emu_long_words import machine, run


class K16ConstantTests(unittest.TestCase):
    def test_mpyk_negative_print_sign_extends(self):
        # the real-code form: AC0 = AC0 * #0xffffa99a is a NEGATIVE K16
        # (-0x5666) sign extended into the multiplier, not +4294958490
        cpu, _ = machine("AC0 = #0x100 << #16", "AC0 = AC0 * #0xffffa99a")
        run(cpu)
        self.assertEqual(cpu.ac[0], (1 << 40) - (0x100 * 0x5666))

    def test_k16_constant_sign_extends(self):
        # MACK: (Tx * #K16) with K16 = 0x8000 is -32768 in the 17-bit
        # multiplier; the product is negative, not +65536
        cpu, _ = machine("T1 = #2", "AC0 = AC1 + (T1 * #0x8000)")
        run(cpu)
        self.assertEqual(cpu.ac[0], (1 << 40) - (2 * 0x8000))

    def test_positive_k16_unchanged(self):
        cpu, _ = machine("T1 = #3", "AC0 = AC1 + (T1 * #0x7FFF)")
        run(cpu)
        self.assertEqual(cpu.ac[0], 3 * 0x7FFF)

    def test_pre_negated_constant_unchanged(self):
        # a constant printed as 0xffffe200 is already negative and stays so
        cpu, _ = machine("T1 = #2", "AC0 = AC1 + (T1 * #0xffffe200)")
        run(cpu)
        self.assertEqual(cpu.ac[0], (1 << 40) - (2 * 0x1e00))


class RepeatBodyTraceTests(unittest.TestCase):
    def _fill_lines(self):
        return ("T1 = #4", "AR1 = T1 - #1", "CSR = AR1",
                "XAR3 = mar(*AR0) || repeat(CSR)", "*AR3+ = T0")

    def test_rpt_body_pcs_appear_in_the_trace(self):
        cpu, bus = machine(*self._fill_lines())
        cpu.t[0] = 0xBEEF
        cpu.xar[0] = 0x2000      # fill destination off the MMR page
        cpu.core.set_trace(1024)
        run(cpu)
        trace = cpu.core.trace()
        body_pc = 0x20000 + 2 * 4          # the repeated store
        self.assertEqual(trace.count(body_pc), 4)   # CSR + 1 = 4 trips
        cpu.core.set_trace(0)
        self.assertNotIn(0x20000 + 2 * 3, trace[-3:])   # marker only once

    def test_tracing_changes_no_value(self):
        def once(trace_on):
            cpu, bus = machine(*self._fill_lines())
            cpu.t[0] = 0x1234
            cpu.xar[0] = 0x2000
            if trace_on:
                cpu.core.set_trace(1024)
            run(cpu)
            if trace_on:
                cpu.core.set_trace(0)
            return [cpu.ac, cpu.t, cpu.xar], [bus.read(0x20000 + i) for i in range(4)], cpu.cycles
        self.assertEqual(once(True), once(False))


if __name__ == "__main__":
    unittest.main()

class K8ConstantTests(unittest.TestCase):
    def test_mpyk_k8_negative(self):
        # asm55 probe: MPYK #-128 encodes as 1e8040; dis55 -e -r prints
        # AC0 = AC1 * #0xffffff80. sext16(0xff80) = -128 = sext8(0x80).
        cpu, _ = machine("AC1 = #0x10000", "AC0 = AC1 * #0xffffff80")
        run(cpu)
        self.assertEqual(cpu.ac[0], (1 << 40) - (1 * 128))  # 1 * -128

    def test_macmk_smem_k8_negative(self):
        # asm55 probe: MPYMK *AR3,#-83 encodes as f861ad20
        cpu, bus = machine("XAR3 = mar(*(#01600h))", "AC2 = *AR3 * #0xffffffad")
        bus.write(0x1600, 0x0100)         # Smem = 0x0100
        run(cpu)
        self.assertEqual(cpu.ac[2], (1 << 40) - (0x100 * 83))  # 256 * -83

    def test_positive_k8_unchanged(self):
        cpu, bus = machine("XAR3 = mar(*(#01600h))", "AC2 = *AR3 * #0x53")
        bus.write(0x1600, 0x0002)
        run(cpu)
        self.assertEqual(cpu.ac[2], 2 * 83)
