"""Findings of a review of the core against the TI manuals (SPRU374E mnemonic
instruction set, SPRU371F and C55x v3.x CPU reference guides). Every
expectation cites the manual section or worked example it comes from.
Instruction spellings were checked by assembling them with asm55 and reading
them back with dis55 -r -g."""

import unittest

from c55x_emu.testutil import machine, run

SATD, SXMD, FRCT, M40 = 9, 8, 6, 10


def bit(n, value=1):
    return f"bit(ST1, #{n}) = #{value}"


class SignedShiftSignTests(unittest.TestCase):
    """M40 = 0 signed shifts keep the sign of an in-range result: SPRU371F /
    v3.x 2.10.2.9 and 2.10.2.11 ('if SXMD = 1, bit 31 is copied into the
    accumulator's guard bits'); SPRU374E 4.70 ('performed on 40 bits')."""

    def test_negative_left_shift_without_satd(self):
        cpu, _ = machine("AC0 = #-4", "AC0 = AC0 << #1")
        run(cpu)
        self.assertEqual(cpu.ac[0], 0xFF_FFFF_FFF8)
        self.assertEqual(cpu.core.acov(), [0, 0, 0, 0])

    def test_negative_left_shift_with_satd(self):
        cpu, _ = machine(bit(SATD), "AC0 = #-4", "AC0 = AC0 << #1")
        run(cpu)
        self.assertEqual(cpu.ac[0], 0xFF_FFFF_FFF8)
        self.assertEqual(cpu.core.acov(), [0, 0, 0, 0])

    def test_shift_by_zero_sign_extends(self):
        for modes in ((), (SATD,)):
            with self.subTest(modes=modes):
                cpu, _ = machine(*(bit(b) for b in modes), "AC1 = AC0 << #0")
                cpu.ac[0] = 0x00_FFFF_FF00         # bit 31 set, guard bits clear
                run(cpu)
                self.assertEqual(cpu.ac[1], 0xFF_FFFF_FF00)
                self.assertEqual(cpu.core.acov(), [0, 0, 0, 0])

    def test_positive_tx_count_on_a_negative_value(self):
        cpu, _ = machine("T0 = #3", "AC0 = #-4", "AC1 = AC0 << T0")
        run(cpu)
        self.assertEqual(cpu.ac[1], 0xFF_FFFF_FFE0)
        self.assertEqual(cpu.core.acov(), [0, 0, 0, 0])

    def test_add_with_a_shifted_negative_accumulator(self):
        for modes in ((), (SATD,)):
            with self.subTest(modes=modes):
                cpu, _ = machine(*(bit(b) for b in modes), "AC0 = #-4", "AC1 = #100",
                                 "AC1 = AC1 + (AC0 << #2)")
                run(cpu)
                self.assertEqual(cpu.ac[1], 84)
                self.assertEqual(cpu.core.acov(), [0, 0, 0, 0])


class OverflowAttributionTests(unittest.TestCase):
    """Each operation of a bundle reports overflow on its own destination
    (SPRU374E 4.27, dual MAC: 'If an overflow is detected, the destination
    accumulator overflow status bit is set'); stores report nothing (4.6.x
    'Affects none')."""

    def dual_mac(self, ac0, ac1, x, y):
        cpu, bus = machine("XAR1 = mar(*(#01600h))", "XAR2 = mar(*(#01601h))",
                           "XCDP = mar(*(#01602h))",
                           "AC0 = AC0 + (*AR1 * coef(*CDP)), AC1 = AC1 + (*AR2 * coef(*CDP))")
        bus.write(0x1600, x)
        bus.write(0x1601, y)
        bus.write(0x1602, 0x7FFF)
        cpu.ac[0], cpu.ac[1] = ac0, ac1
        run(cpu)
        return cpu

    def test_dual_mac_second_destination_overflows(self):
        cpu = self.dual_mac(0, 0x7FFF_0000, 1, 0x7FFF)
        self.assertEqual(cpu.ac[1], 0x7FFF_0000 + 0x7FFF * 0x7FFF)
        self.assertEqual(cpu.core.acov(), [0, 1, 0, 0])

    def test_dual_mac_first_destination_overflows(self):
        cpu = self.dual_mac(0x7FFF_0000, 0, 0x7FFF, 1)
        self.assertEqual(cpu.core.acov(), [1, 0, 0, 0])

    def test_dual_mac_both_destinations_overflow(self):
        cpu = self.dual_mac(0x7FFF_0000, 0x7FFF_0000, 0x7FFF, 0x7FFF)
        self.assertEqual(cpu.core.acov(), [1, 1, 0, 0])

    def store_bundle(self, ac1, smem, ac2, t2):
        cpu, bus = machine("XAR0 = mar(*(#01600h))", "XAR1 = mar(*(#01610h))", f"T2 = #{t2}",
                           "AC3 = AC1 + (*AR0 << #16) || *AR1 = HI(AC2 << T2)")
        bus.write(0x1600, smem)
        cpu.ac[1], cpu.ac[2] = ac1, ac2
        run(cpu)
        return cpu, bus

    def test_store_shift_overflow_is_not_reported_on_the_partner(self):
        cpu, _ = self.store_bundle(0, 1, 0x4000_0000, 2)
        self.assertEqual(cpu.core.acov(), [0, 0, 0, 0])

    def test_store_does_not_erase_the_partner_overflow(self):
        cpu, _ = self.store_bundle(0x7FFF_0000, 0x7FFF, 0x1234_0000, 0)
        self.assertEqual(cpu.core.acov(), [0, 0, 0, 1])


class StoreShiftTests(unittest.TestCase):
    """Accumulator stores shift on 40 bits according to SXMD only; M40 and SATD
    are not involved and no ACOV is set (SPRU374E 4.6.6-4.6.10, 'Affected by
    [C54CM,] [RDM,] SXMD / Affects none'). saturate() clamps the shifted value
    to 00 7FFF FFFF / FF 8000 0000 (4.6.11-4.6.13)."""

    def store(self, insn, acc, value, *lines):
        cpu, bus = machine(*lines, "XAR0 = mar(*(#01600h))", "T1 = #1", insn)
        cpu.ac[acc] = value
        run(cpu)
        return cpu, bus.read(0x1600)

    def test_right_shift_brings_guard_bits_down(self):
        _, word = self.store("*AR0 = HI(AC0 << #-8)", 0, 0x01_0000_0000)
        self.assertEqual(word, 0x0100)

    def test_store_shift_does_not_saturate_with_satd(self):
        cpu, word = self.store("*AR0 = HI(AC0 << #2)", 0, 0x00_4000_0000, bit(SATD))
        self.assertEqual(word, 0x0000)                 # bits 31-16 of 01 0000 0000
        self.assertEqual(cpu.core.acov(), [0, 0, 0, 0])

    def test_saturate_keyword_clamps_the_shifted_value(self):
        cases = ((0x00_8000_0000, 0x7FFF),            # 40-bit 01 0000 0000 -> 7FFF FFFF
                 (0xFF_BFFF_FFFF, 0x8000),            # -0x80000002 -> FF 8000 0000
                 (0xFF_FFFF_FFFC, 0xFFFF))            # -8 is in range
        for value, expected in cases:
            with self.subTest(value=hex(value)):
                cpu, word = self.store("*AR0 = HI(saturate(AC1 << T1))", 1, value)
                self.assertEqual(word, expected)
                self.assertEqual(cpu.core.acov(), [0, 0, 0, 0])


class ProductSignTests(unittest.TestCase):
    """The 32-bit product is sign extended to 40 bits (SPRU374E 4.47.1 / 4.48.2)
    whatever the operand form. The value for 8000h x 8000h with FRCT = 1 and
    SMUL = 0 follows the manual text and is a hardware-validation item."""

    def test_smem_and_accumulator_forms_agree(self):
        cpu, bus = machine(bit(FRCT), "T1 = #-32768", "XAR1 = mar(*(#01600h))",
                           "AC0 = T1 * *AR1", "AC1 = AC1 * T1")
        bus.write(0x1600, 0x8000)
        cpu.ac[1] = 0xFF_8000_0000                     # ACx(32-16) = -8000h
        run(cpu)
        self.assertEqual(cpu.ac[0], cpu.ac[1])
        self.assertEqual(cpu.ac[0], 0xFF_8000_0000)
        self.assertEqual(cpu.core.acov(), [0, 0, 0, 0])


class UnsignedProductTests(unittest.TestCase):
    """uns() multiplier operands give an unsigned product that is not sign
    extended. Values of the SPRU374E 4.27 example AMAR *AR2+ :: MAC uns(*AR1),
    uns(*CDP), AC0 >> #16: EF00h x A067h = 95C0 2900h, plus AC0 >> 16, overflows
    bit 31 (M40 = 0, FRCT = 0, SATD = 0). The AMAR-paired form is not accepted
    by the test machine, so the single MAC with the same operands is used:
    MACM uns(*AR1), uns(*AR3), AC0 >> #16, written as dis55 -r -g prints it."""

    def test_unsigned_mac_example(self):
        cpu, bus = machine("XAR1 = mar(*(#01600h))", "XAR3 = mar(*(#01602h))",
                           "AC0 = (AC0 >> #16) + (uns(*AR1) * uns(*AR3))")
        bus.write(0x1600, 0xEF00)
        bus.write(0x1602, 0xA067)
        cpu.ac[0], cpu.ac[1] = 0x00_6900_0000, 0x00_0023_0000
        run(cpu)
        self.assertEqual((cpu.ac[0], cpu.ac[1]), (0x00_95C0_9200, 0x00_0023_0000))
        self.assertEqual((cpu.core.acov(), cpu.carry), ([1, 0, 0, 0], 0))


class SxmdIndependentOperandTests(unittest.TestCase):
    """With SXMD = 0, 16-bit MAC input operands are still sign extended to 17
    bits (SPRU371F 2.10.2.11) and uns() operands stay zero extended; a byte
    loaded into a T/AR register is sign extended to 16 bits regardless of SXMD,
    while an accumulator destination follows SXMD (SPRU374E 4.4.5 / 4.4.6)."""

    def run_with(self, insn, words, t1=0, ac0=0):
        cpu, bus = machine(bit(SXMD, 0), "XAR0 = mar(*(#01600h))", "XAR1 = mar(*(#01600h))",
                           "XAR2 = mar(*(#01601h))", insn)
        for i, w in enumerate(words):
            bus.write(0x1600 + i, w)
        cpu.t[1], cpu.ac[0] = t1, ac0
        run(cpu)
        return cpu

    def test_multiply_operands_ignore_sxmd(self):
        cpu = self.run_with("AC1 = T1 * *AR1", [0xFFFF], t1=0xFFFF)
        self.assertEqual(cpu.ac[1], 0x00_0000_0001)
        self.assertEqual(cpu.core.acov(), [0, 0, 0, 0])

    def test_mac_operands_ignore_sxmd(self):
        cpu = self.run_with("AC0 = AC0 + (T1 * *AR1)", [0xFFFF], t1=0xFFFF, ac0=0x10)
        self.assertEqual(cpu.ac[0], 0x00_0000_0011)
        self.assertEqual(cpu.core.acov(), [0, 0, 0, 0])

    def test_unsigned_operands_stay_zero_extended(self):
        cpu = self.run_with("AC1 = uns(*AR1) * uns(*AR2)", [0xFFFF, 0xFFFF])
        self.assertEqual(cpu.ac[1], 0x00_FFFE_0001)

    def test_byte_load_to_ta_register_ignores_sxmd(self):
        cpu = self.run_with("T0 = low_byte(*AR0)", [0x0080])
        self.assertEqual(cpu.t[0], 0xFF80)
        cpu = self.run_with("AR2 = high_byte(*AR0)", [0x8000])
        self.assertEqual(cpu.xar[2] & 0xFFFF, 0xFF80)
        cpu = self.run_with("T0 = uns(low_byte(*AR0))", [0x0080])
        self.assertEqual(cpu.t[0], 0x0080)

    def test_byte_load_to_accumulator_follows_sxmd(self):
        cpu = self.run_with("AC0 = low_byte(*AR0)", [0x0080])
        self.assertEqual(cpu.ac[0], 0x00_0000_0080)


class StatusFlagTests(unittest.TestCase):
    def test_acov_is_part_of_st0(self):
        # SPRU371F 2.10.1.1: ACOV0 is bit 10 of ST0_55, read/write
        cpu, _ = machine("AC0 = #0x7FFFFFFF", "AC0 = AC0 + #1", "T0 = @ST0_L || mmap()")
        run(cpu)
        self.assertEqual((cpu.t[0] >> 10) & 1, 1)

    def test_bclr_acov0_clears_the_flag(self):
        cpu, _ = machine("AC0 = #0x7FFFFFFF", "AC0 = AC0 + #1", "bit(ST0, #10) = #0")
        run(cpu)
        self.assertEqual(cpu.core.acov(), [0, 0, 0, 0])

    def test_accumulator_move_reports_overflow(self):
        # SPRU374E 4.5.1 example: MOV AC0, AC1 with M40 = 0, SATD = 0
        cpu, _ = machine("AC1 = AC0")
        cpu.ac[0], cpu.ac[1] = 0x01_E500_0030, 0x00_2800_0200
        run(cpu)
        self.assertEqual(cpu.ac[1], 0x01_E500_0030)
        self.assertEqual(cpu.core.acov(), [0, 1, 0, 0])

    def test_sat_reports_overflow(self):
        # SPRU374E 4.69 example: SAT AC0, AC1
        cpu, _ = machine("AC1 = saturate(AC0)")
        cpu.ac[0] = 0xEF_0FF0_8023
        run(cpu)
        self.assertEqual(cpu.ac[1], 0xFF_8000_0000)
        self.assertEqual(cpu.core.acov(), [0, 1, 0, 0])

    def test_satr_clears_the_low_word_after_saturation(self):
        # SPRU374E 4.69 example: SATR AC0, AC1 with RDM = 0
        cpu, _ = machine("AC1 = saturate(rnd(AC0))")
        cpu.ac[0] = 0x00_7FFF_8000
        run(cpu)
        self.assertEqual(cpu.ac[1], 0x00_7FFF_0000)
        self.assertEqual(cpu.core.acov(), [0, 1, 0, 0])

    def test_subc_reports_overflow_without_saturation(self):
        # SPRU374E 4.25 example: SUBC *AR1, AC0, AC1 with SXMD = 0
        cpu, bus = machine(bit(SXMD, 0), "XAR1 = mar(*(#0300h))", "subc(*AR1, AC0, AC1)")
        bus.write(0x300, 0x200)
        cpu.ac[0] = 0x23_4300_0000
        run(cpu)
        self.assertEqual((cpu.ac[1], cpu.carry), (0x46_8400_0001, 1))
        self.assertEqual(cpu.core.acov(), [0, 1, 0, 0])

    def test_add_k16_to_memory_saturates_and_reports_acov0(self):
        # SPRU374E 4.7.16 ADD K16, Smem: overflow at bit 31 sets ACOV0,
        # SATD saturates the stored word to 7FFFh / 8000h
        cpu, bus = machine(bit(SATD), "XAR0 = mar(*(#01600h))", "*AR0 = *AR0 + #1")
        bus.write(0x1600, 0x7FFF)
        run(cpu)
        self.assertEqual(bus.read(0x1600), 0x7FFF)
        self.assertEqual(cpu.core.acov(), [1, 0, 0, 0])

    def test_add_k16_carry_and_one_address_update(self):
        # SPRU374E 4.7.16: CARRY at bit 31; the Smem address is evaluated once
        cpu, bus = machine("XAR0 = mar(*(#01600h))", "*AR0+ = *AR0+ + #1")
        bus.write(0x1600, 0xFFFF)
        bus.write(0x1601, 0x1234)
        run(cpu)
        self.assertEqual((bus.read(0x1600), bus.read(0x1601)), (0x0000, 0x1234))
        self.assertEqual((cpu.carry, cpu.core.acov()), (1, [0, 0, 0, 0]))
        self.assertEqual(cpu.xar[0], 0x1601)

    def test_add_k16_negative_constant_saturates_to_8000h(self):
        # SPRU374E 4.7.16: "- #k" is ADD #-k, Smem (asm55 / dis55)
        cpu, bus = machine(bit(SATD), "XAR0 = mar(*(#01600h))", "*AR0 = *AR0 - #1")
        bus.write(0x1600, 0x8000)
        run(cpu)
        self.assertEqual(bus.read(0x1600), 0x8000)
        self.assertEqual(cpu.core.acov(), [1, 0, 0, 0])

    def test_accumulator_move_saturates_and_sets_st0_bit_9(self):
        # SPRU374E 4.5.1 with SATD = 1; ACOV1 is bit 9 of ST0_55 (SPRU371F 2.10.1.1)
        cpu, _ = machine(bit(SATD), "AC1 = AC0", "T0 = @ST0_L || mmap()")
        cpu.ac[0] = 0x01_E500_0030
        run(cpu)
        self.assertEqual(cpu.ac[1], 0x00_7FFF_FFFF)
        self.assertEqual(cpu.core.acov(), [0, 1, 0, 0])
        self.assertEqual((cpu.t[0] >> 9) & 1, 1)

    def test_accumulator_move_with_m40_does_not_overflow(self):
        # SPRU374E 4.5.1: overflow is detected according to M40
        cpu, _ = machine(bit(M40), "AC1 = AC0")
        cpu.ac[0] = 0x01_E500_0030
        run(cpu)
        self.assertEqual(cpu.ac[1], 0x01_E500_0030)
        self.assertEqual(cpu.core.acov(), [0, 0, 0, 0])

    def test_repeated_subc_example(self):
        # SPRU374E 4.25 second example: repeat(CSR) SUBC *AR1, AC1 with CSR = 1,
        # two steps with divisor 0100h << 15 = 80 0000h:
        #   0746 0000 - 80 0000 = 06C6 0000 -> 0D8C 0001
        #   0D8C 0001 - 80 0000 = 0D0C 0001 -> 1A18 0003
        # SPRU374E (and SWPU067E) print 00 1A18 0007: that low word contradicts
        # the 4.25 algorithm and the printed high word (three steps would give
        # 3330 0007), so this test expects the computed value.
        cpu, bus = machine("XAR1 = mar(*(#0200h))", "CSR = #1", "repeat(CSR)",
                           "subc(*AR1, AC1, AC1)")
        bus.write(0x200, 0x0100)
        cpu.ac[1] = 0x00_0746_0000
        run(cpu)
        self.assertEqual((cpu.ac[1], cpu.carry), (0x00_1A18_0003, 1))
        self.assertEqual(cpu.core.acov(), [0, 0, 0, 0])


class BitCountTests(unittest.TestCase):
    """BCNT ACx, ACy, TCx, Tx (SPRU374E 4.9): the 1 bits of ACx AND ACy are
    counted into Tx; TCx is set when the count is odd, cleared when even."""

    def count(self, ac1, ac2, tc1):
        cpu, _ = machine(f"bit(ST0, #13) = #{tc1}", "T1 = count(AC1, AC2, TC1)")
        cpu.ac[1], cpu.ac[2] = ac1, ac2
        run(cpu)
        return cpu.t[1], cpu.tc1

    def test_manual_example(self):
        self.assertEqual(self.count(0x7E_2355_4FC0, 0x0F_E340_5678, 0), (0x000B, 1))

    def test_even_count_clears_tcx(self):
        self.assertEqual(self.count(0x80_0000_0003, 0x80_0000_0001, 1), (2, 0))


class DualArithmeticTests(unittest.TestCase):
    """Dual 16-bit ADD / SUB with dual(Lmem) (SPRU374E 4.26.3-4.26.5): the
    high part uses ACx(39-16) and the most significant word, the low part
    ACx(15-0) and the least significant word (Lmem + 1 at an even address,
    Lmem - 1 at an odd one). Overflow is detected per part (bit 15 low, bit 31
    high) and either one sets ACOVy; SATD saturates each part on its own
    (7FFFh / 8000h, 00 7FFFh / FF 8000h); CARRY is the carry of the high part,
    extracted at bit 31."""

    ADD = "HI(AC1) = HI(*AR0) + HI(AC0), LO(AC1) = LO(*AR0) + LO(AC0)"
    SUB = "HI(AC1) = HI(AC0) - HI(*AR0), LO(AC1) = LO(AC0) - LO(*AR0)"

    def run_dual(self, insn, words, ac0, *lines, address=0x1600, carry=0):
        cpu, bus = machine(*lines, f"XAR0 = mar(*(#0{address:x}h))", insn)
        for i, w in enumerate(words):
            bus.write(0x1600 + i, w)
        cpu.ac[0], cpu.carry = ac0, carry
        run(cpu)
        return cpu

    def test_add_at_even_address(self):
        cpu = self.run_dual(self.ADD, [0x1234, 0x0001], 0x00_0001_0002)
        self.assertEqual((cpu.ac[1], cpu.carry, cpu.core.acov()), (0x00_1235_0003, 0, [0, 0, 0, 0]))

    def test_add_at_odd_address_takes_the_previous_word(self):
        cpu = self.run_dual(self.ADD, [0x0001, 0x1234], 0x00_0001_0002, address=0x1601)
        self.assertEqual(cpu.ac[1], 0x00_1235_0003)

    @unittest.expectedFailure       # not fixed yet (F9)
    def test_low_part_overflow_sets_acov(self):
        for satd, low in ((0, 0x8000), (1, 0x7FFF)):
            with self.subTest(satd=satd):
                cpu = self.run_dual(self.ADD, [0x0000, 0x7FFF], 0x00_0000_0001, bit(SATD, satd))
                self.assertEqual(cpu.ac[1], low)
                self.assertEqual(cpu.core.acov(), [0, 1, 0, 0])

    @unittest.expectedFailure       # not fixed yet (F9)
    def test_high_part_overflow_sets_acov(self):
        for satd, high in ((0, 0x00_8000), (1, 0x00_7FFF)):
            with self.subTest(satd=satd):
                cpu = self.run_dual(self.ADD, [0x7FFF, 0x0000], 0x00_0001_0000, bit(SATD, satd))
                self.assertEqual(cpu.ac[1], high << 16)
                self.assertEqual(cpu.core.acov(), [0, 1, 0, 0])

    def test_high_part_carry(self):
        cpu = self.run_dual(self.ADD, [0xFFFF, 0x0000], 0x00_0001_0000)
        self.assertEqual((cpu.ac[1], cpu.carry, cpu.core.acov()), (0, 1, [0, 0, 0, 0]))

    def test_subtract_reports_the_high_part_borrow(self):
        cpu = self.run_dual(self.SUB, [0x1234, 0x0001], 0x00_0001_0002, carry=1)
        self.assertEqual((cpu.ac[1], cpu.carry), (0xFF_EDCD_0001, 0))
        cpu = self.run_dual(self.SUB, [0x0001, 0x0001], 0x00_1234_0002)
        self.assertEqual((cpu.ac[1], cpu.carry), (0x00_1233_0001, 1))


class ConditionalRepeatTests(unittest.TestCase):
    """RPTCC k8, cond (SPRU374E 4.61): while the condition holds the next
    instruction runs k8 + 1 times; RPTC(15-8) holds the condition code and
    RPTC(7-0) the number of iterations not performed (0 when the condition
    never fails). The body is a SUBC, which leaves TC1 unchanged."""

    def repeat(self, cond, tc1):
        cpu, bus = machine("XAR1 = mar(*(#0200h))", f"bit(ST0, #13) = #{tc1}",
                           f"while ({cond} && (RPTC < #3)) repeat", "subc(*AR1, AC1, AC1)",
                           "T0 = RPTC")
        bus.write(0x200, 0x0100)
        cpu.ac[1] = 0x00_0746_0000
        run(cpu)
        return cpu

    def test_true_condition_runs_k8_plus_1_times(self):
        cpu = self.repeat("TC1", 1)
        self.assertEqual((cpu.ac[1], cpu.carry, cpu.t[0]), (0x00_6560_000F, 1, 0x6400))

    def test_false_condition_runs_no_iteration(self):
        cpu = self.repeat("TC1", 0)
        self.assertEqual((cpu.ac[1], cpu.t[0]), (0x00_0746_0000, 0x6404))

    def test_negated_condition(self):
        cpu = self.repeat("!TC1", 0)
        self.assertEqual((cpu.ac[1], cpu.t[0]), (0x00_6560_000F, 0x7400))


class ExecuteConditionallyTests(unittest.TestCase):
    """XCCPART (SPRU374E 4.28.2): the condition controls the execute phase of
    the conditional instruction(s); operations of the address phase (pointer
    updates) are always executed."""

    @unittest.expectedFailure       # not fixed yet (XCCPART address phase)
    def test_manual_examples(self):
        # SPRU374E 4.28.2 examples, XCCPART AR0 != #0 then ADD *AR2+, AC0; the
        # example's 00 0000 EF0C shows EF00h zero extended, so SXMD = 0
        for ar0, ac0 in ((0x3000, 0x00_0000_EF0C), (0x0000, 0x00_0000_000C)):
            with self.subTest(ar0=ar0):
                cpu, bus = machine(bit(SXMD, 0), f"XAR0 = mar(*(#0{ar0:x}h))", "XAR2 = mar(*(#0405h))",
                                   "if (AR0 != #0) execute(D_Unit)", "AC0 = AC0 + *AR2+")
                bus.write(0x405, 0xEF00)
                cpu.ac[0] = 0x00_0000_000C
                run(cpu)
                self.assertEqual((cpu.ac[0], cpu.xar[2]), (ac0, 0x0406))

    @unittest.expectedFailure       # not fixed yet (XCCPART address phase)
    def test_after_a_partner(self):
        # previous_instruction || XCCPART cond: the partner runs unconditionally
        for tc2, ac0 in ((0, 0x0C), (1, 0x0D)):
            with self.subTest(tc2=tc2):
                cpu, bus = machine(f"bit(ST0, #12) = #{tc2}", "XAR2 = mar(*(#01600h))",
                                   "T1 = #5 || if (TC2) execute(D_Unit)", "AC0 = AC0 + *AR2+")
                bus.write(0x1600, 1)
                cpu.ac[0] = 0x0C
                run(cpu)
                self.assertEqual((cpu.t[1], cpu.ac[0], cpu.xar[2]), (5, ac0, 0x1601))

    def test_parallel_store_keeps_the_pointer_update(self):
        for tc1, word in ((0, 0x0000), (1, 0x0009)):
            with self.subTest(tc1=tc1):
                cpu, bus = machine(f"bit(ST0, #13) = #{tc1}", "XAR0 = mar(*(#01600h))", "T0 = #9",
                                   "if (TC1) execute(D_Unit) || *AR0+ = T0")
                run(cpu)
                self.assertEqual((bus.read(0x1600), cpu.xar[0]), (word, 0x1601))


class SquareDistanceTests(unittest.TestCase):
    """SQDST Xmem, Ymem, ACx, ACy (SPRU374E 4.77): ACy = ACy + ACx(32-16)^2 in
    parallel with ACx = (Xmem << #16) - (Ymem << #16); CARRY is the complement
    of the borrow of that subtraction."""

    def sqdst(self, xmem, ymem, ac0, carry=0):
        cpu, bus = machine("XAR0 = mar(*(#01600h))", "XAR1 = mar(*(#01601h))",
                           "sqdst(*AR0,*AR1,AC0,AC1)")
        bus.write(0x1600, xmem)
        bus.write(0x1601, ymem)
        cpu.ac[0], cpu.ac[1], cpu.carry = ac0, 0, carry
        run(cpu)
        return cpu

    def test_manual_example(self):
        cpu = self.sqdst(0x0055, 0x00AA, 0xFF_ABCD_0000)
        self.assertEqual((cpu.ac[0], cpu.ac[1]), (0xFF_FFAB_0000, 0x00_1BB1_8229))
        self.assertEqual((cpu.carry, cpu.core.acov()), (0, [0, 0, 0, 0]))

    def test_no_borrow_sets_carry(self):
        cpu = self.sqdst(0x00AA, 0x0055, 0, carry=0)
        self.assertEqual((cpu.ac[0], cpu.carry), (0x00_0055_0000, 1))


if __name__ == "__main__":
    unittest.main()
