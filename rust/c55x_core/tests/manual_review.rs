//! Findings of a review of the core against the TI manuals (SPRU374E mnemonic instruction
//! set, SPRU371F and C55x v3.x CPU reference guides), from
//! tests/test_c55x_emu_manual_review.py. Every expectation cites the manual section or
//! worked example it comes from. Programs: rust/fixtures/manual_review (the spellings
//! asm55 assembles and dis55 -qq -r -g prints back); AC0 = 0x7FFFFFFF is preset where the
//! Python tests loaded it.

mod common;

use c55x_core::core::Core;
use common::{read, run, write};

fn machine(name: &str) -> Core {
    fixture!("manual_review").core(&format!("t_{}", name))
}

/// M40 = 0 signed shifts keep the sign of an in-range result: SPRU371F / v3.x 2.10.2.9
/// and 2.10.2.11 ('if SXMD = 1, bit 31 is copied into the accumulator's guard bits');
/// SPRU374E 4.70 ('performed on 40 bits').
mod signed_shift_sign {
    use super::*;

    #[test]
    fn negative_left_shift_without_satd() {
        let mut cpu = machine("neg_left_shift");
        run(&mut cpu);
        assert_eq!(cpu.st.ac[0], 0xFF_FFFF_FFF8);
        assert_eq!(cpu.st.acov, [0, 0, 0, 0]);
    }

    #[test]
    fn negative_left_shift_with_satd() {
        let mut cpu = machine("neg_left_shift_satd");
        run(&mut cpu);
        assert_eq!(cpu.st.ac[0], 0xFF_FFFF_FFF8);
        assert_eq!(cpu.st.acov, [0, 0, 0, 0]);
    }

    #[test]
    fn shift_by_zero_sign_extends() {
        for name in ["shift_by_zero", "shift_by_zero_satd"] {
            let mut cpu = machine(name);
            cpu.st.ac[0] = 0x00_FFFF_FF00;              // bit 31 set, guard bits clear
            run(&mut cpu);
            assert_eq!(cpu.st.ac[1], 0xFF_FFFF_FF00, "{}", name);
            assert_eq!(cpu.st.acov, [0, 0, 0, 0], "{}", name);
        }
    }

    #[test]
    fn positive_tx_count_on_a_negative_value() {
        let mut cpu = machine("positive_tx_count");
        run(&mut cpu);
        assert_eq!(cpu.st.ac[1], 0xFF_FFFF_FFE0);
        assert_eq!(cpu.st.acov, [0, 0, 0, 0]);
    }

    #[test]
    fn add_with_a_shifted_negative_accumulator() {
        for name in ["add_shifted", "add_shifted_satd"] {
            let mut cpu = machine(name);
            run(&mut cpu);
            assert_eq!(cpu.st.ac[1], 84, "{}", name);
            assert_eq!(cpu.st.acov, [0, 0, 0, 0], "{}", name);
        }
    }
}

/// Each operation of a bundle reports overflow on its own destination (SPRU374E 4.27,
/// dual MAC: 'If an overflow is detected, the destination accumulator overflow status bit
/// is set'); stores report nothing (4.6.x 'Affects none').
mod overflow_attribution {
    use super::*;

    fn dual_mac(ac0: u64, ac1: u64, x: u32, y: u32) -> Core {
        let mut cpu = machine("dual_mac");
        write(&mut cpu, 0x1600, x);
        write(&mut cpu, 0x1601, y);
        write(&mut cpu, 0x1602, 0x7FFF);
        cpu.st.ac[0] = ac0;
        cpu.st.ac[1] = ac1;
        run(&mut cpu);
        cpu
    }

    #[test]
    fn dual_mac_second_destination_overflows() {
        let cpu = dual_mac(0, 0x7FFF_0000, 1, 0x7FFF);
        assert_eq!(cpu.st.ac[1], 0x7FFF_0000 + 0x7FFF * 0x7FFF);
        assert_eq!(cpu.st.acov, [0, 1, 0, 0]);
    }

    #[test]
    fn dual_mac_first_destination_overflows() {
        let cpu = dual_mac(0x7FFF_0000, 0, 0x7FFF, 1);
        assert_eq!(cpu.st.acov, [1, 0, 0, 0]);
    }

    #[test]
    fn dual_mac_both_destinations_overflow() {
        let cpu = dual_mac(0x7FFF_0000, 0x7FFF_0000, 0x7FFF, 0x7FFF);
        assert_eq!(cpu.st.acov, [1, 1, 0, 0]);
    }

    fn store_bundle(name: &str, ac1: u64, smem: u32, ac2: u64) -> Core {
        let mut cpu = machine(name);
        write(&mut cpu, 0x1600, smem);
        cpu.st.ac[1] = ac1;
        cpu.st.ac[2] = ac2;
        run(&mut cpu);
        cpu
    }

    #[test]
    fn store_shift_overflow_is_not_reported_on_the_partner() {
        let cpu = store_bundle("store_bundle_t2_2", 0, 1, 0x4000_0000);
        assert_eq!(cpu.st.acov, [0, 0, 0, 0]);
    }

    #[test]
    fn store_does_not_erase_the_partner_overflow() {
        let cpu = store_bundle("store_bundle_t2_0", 0x7FFF_0000, 0x7FFF, 0x1234_0000);
        assert_eq!(cpu.st.acov, [0, 0, 0, 1]);
    }
}

/// Accumulator stores shift on 40 bits according to SXMD only; M40 and SATD are not
/// involved and no ACOV is set (SPRU374E 4.6.6-4.6.10, 'Affected by [C54CM,] [RDM,] SXMD /
/// Affects none'). saturate() clamps the shifted value to 00 7FFF FFFF / FF 8000 0000
/// (4.6.11-4.6.13).
mod store_shift {
    use super::*;

    fn store(name: &str, acc: usize, value: u64) -> (Core, u32) {
        let mut cpu = machine(name);
        cpu.st.ac[acc] = value;
        run(&mut cpu);
        let word = read(&mut cpu, 0x1600);
        (cpu, word)
    }

    #[test]
    fn right_shift_brings_guard_bits_down() {
        let (_, word) = store("store_right", 0, 0x01_0000_0000);
        assert_eq!(word, 0x0100);
    }

    #[test]
    fn store_shift_does_not_saturate_with_satd() {
        let (cpu, word) = store("store_satd", 0, 0x00_4000_0000);
        assert_eq!(word, 0x0000);                       // bits 31-16 of 01 0000 0000
        assert_eq!(cpu.st.acov, [0, 0, 0, 0]);
    }

    #[test]
    fn saturate_keyword_clamps_the_shifted_value() {
        let cases = [(0x00_8000_0000, 0x7FFF),          // 40-bit 01 0000 0000 -> 7FFF FFFF
                     (0xFF_BFFF_FFFF, 0x8000),          // -0x80000002 -> FF 8000 0000
                     (0xFF_FFFF_FFFC, 0xFFFF)];         // -8 is in range
        for (value, expected) in cases {
            let (cpu, word) = store("store_saturate", 1, value);
            assert_eq!(word, expected, "{:#x}", value);
            assert_eq!(cpu.st.acov, [0, 0, 0, 0], "{:#x}", value);
        }
    }
}

/// The 32-bit product is sign extended to 40 bits (SPRU374E 4.47.1 / 4.48.2) whatever the
/// operand form. The value for 8000h x 8000h with FRCT = 1 and SMUL = 0 follows the manual
/// text and is a hardware-validation item.
mod product_sign {
    use super::*;

    #[test]
    fn smem_and_accumulator_forms_agree() {
        let mut cpu = machine("smem_and_accumulator_forms_agree");
        write(&mut cpu, 0x1600, 0x8000);
        cpu.st.ac[1] = 0xFF_8000_0000;                  // ACx(32-16) = -8000h
        run(&mut cpu);
        assert_eq!(cpu.st.ac[0], cpu.st.ac[1]);
        assert_eq!(cpu.st.ac[0], 0xFF_8000_0000);
        assert_eq!(cpu.st.acov, [0, 0, 0, 0]);
    }
}

/// uns() multiplier operands give an unsigned product that is not sign extended. Values of
/// the SPRU374E 4.27 example AMAR *AR2+ :: MAC uns(*AR1), uns(*CDP), AC0 >> #16: EF00h x
/// A067h = 95C0 2900h, plus AC0 >> 16, overflows bit 31 (M40 = 0, FRCT = 0, SATD = 0). The
/// single MAC with the same operands is used: MACM uns(*AR1), uns(*AR3), AC0 >> #16.
mod unsigned_product {
    use super::*;

    #[test]
    fn unsigned_mac_example() {
        let mut cpu = machine("unsigned_mac_example");
        write(&mut cpu, 0x1600, 0xEF00);
        write(&mut cpu, 0x1602, 0xA067);
        cpu.st.ac[0] = 0x00_6900_0000;
        cpu.st.ac[1] = 0x00_0023_0000;
        run(&mut cpu);
        assert_eq!((cpu.st.ac[0], cpu.st.ac[1]), (0x00_95C0_9200, 0x00_0023_0000));
        assert_eq!((cpu.st.acov, cpu.st.carry), ([1, 0, 0, 0], 0));
    }
}

/// With SXMD = 0, 16-bit MAC input operands are still sign extended to 17 bits (SPRU371F
/// 2.10.2.11) and uns() operands stay zero extended; a byte loaded into a T/AR register is
/// sign extended to 16 bits regardless of SXMD, while an accumulator destination follows
/// SXMD (SPRU374E 4.4.5 / 4.4.6).
mod sxmd_independent_operand {
    use super::*;

    fn run_with(name: &str, words: &[u32], t1: u32, ac0: u64) -> Core {
        let mut cpu = machine(name);
        for (i, &w) in words.iter().enumerate() {
            write(&mut cpu, 0x1600 + i as u32, w);
        }
        cpu.st.t[1] = t1;
        cpu.st.ac[0] = ac0;
        run(&mut cpu);
        cpu
    }

    #[test]
    fn multiply_operands_ignore_sxmd() {
        let cpu = run_with("sx_mpy", &[0xFFFF], 0xFFFF, 0);
        assert_eq!(cpu.st.ac[1], 0x00_0000_0001);
        assert_eq!(cpu.st.acov, [0, 0, 0, 0]);
    }

    #[test]
    fn mac_operands_ignore_sxmd() {
        let cpu = run_with("sx_mac", &[0xFFFF], 0xFFFF, 0x10);
        assert_eq!(cpu.st.ac[0], 0x00_0000_0011);
        assert_eq!(cpu.st.acov, [0, 0, 0, 0]);
    }

    #[test]
    fn unsigned_operands_stay_zero_extended() {
        let cpu = run_with("sx_uns", &[0xFFFF, 0xFFFF], 0, 0);
        assert_eq!(cpu.st.ac[1], 0x00_FFFE_0001);
    }

    #[test]
    fn byte_load_to_ta_register_ignores_sxmd() {
        let cpu = run_with("sx_t_low", &[0x0080], 0, 0);
        assert_eq!(cpu.st.t[0], 0xFF80);
        let cpu = run_with("sx_ar_high", &[0x8000], 0, 0);
        assert_eq!(cpu.st.xar[2] & 0xFFFF, 0xFF80);
        let cpu = run_with("sx_t_uns_low", &[0x0080], 0, 0);
        assert_eq!(cpu.st.t[0], 0x0080);
    }

    #[test]
    fn byte_load_to_accumulator_follows_sxmd() {
        let cpu = run_with("sx_ac_low", &[0x0080], 0, 0);
        assert_eq!(cpu.st.ac[0], 0x00_0000_0080);
    }
}

mod status_flag {
    use super::*;

    // SPRU371F 2.10.1.1: ACOV0 is bit 10 of ST0_55, read/write
    #[test]
    fn acov_is_part_of_st0() {
        let mut cpu = machine("acov_is_part_of_st0");
        cpu.st.ac[0] = 0x7FFFFFFF;
        run(&mut cpu);
        assert_eq!((cpu.st.t[0] >> 10) & 1, 1);
    }

    #[test]
    fn bclr_acov0_clears_the_flag() {
        let mut cpu = machine("bclr_acov0_clears_the_flag");
        cpu.st.ac[0] = 0x7FFFFFFF;
        run(&mut cpu);
        assert_eq!(cpu.st.acov, [0, 0, 0, 0]);
    }

    // SPRU374E 4.5.1 example: MOV AC0, AC1 with M40 = 0, SATD = 0
    #[test]
    fn accumulator_move_reports_overflow() {
        let mut cpu = machine("accumulator_move");
        cpu.st.ac[0] = 0x01_E500_0030;
        cpu.st.ac[1] = 0x00_2800_0200;
        run(&mut cpu);
        assert_eq!(cpu.st.ac[1], 0x01_E500_0030);
        assert_eq!(cpu.st.acov, [0, 1, 0, 0]);
    }

    // SPRU374E 4.69 example: SAT AC0, AC1
    #[test]
    fn sat_reports_overflow() {
        let mut cpu = machine("sat_reports_overflow");
        cpu.st.ac[0] = 0xEF_0FF0_8023;
        run(&mut cpu);
        assert_eq!(cpu.st.ac[1], 0xFF_8000_0000);
        assert_eq!(cpu.st.acov, [0, 1, 0, 0]);
    }

    // SPRU374E 4.69 example: SATR AC0, AC1 with RDM = 0
    #[test]
    fn satr_clears_the_low_word_after_saturation() {
        let mut cpu = machine("satr_clears_the_low_word_after_saturation");
        cpu.st.ac[0] = 0x00_7FFF_8000;
        run(&mut cpu);
        assert_eq!(cpu.st.ac[1], 0x00_7FFF_0000);
        assert_eq!(cpu.st.acov, [0, 1, 0, 0]);
    }

    // SPRU374E 4.25 example: SUBC *AR1, AC0, AC1 with SXMD = 0
    #[test]
    fn subc_reports_overflow_without_saturation() {
        let mut cpu = machine("subc_reports_overflow_without_saturation");
        write(&mut cpu, 0x300, 0x200);
        cpu.st.ac[0] = 0x23_4300_0000;
        run(&mut cpu);
        assert_eq!((cpu.st.ac[1], cpu.st.carry), (0x46_8400_0001, 1));
        assert_eq!(cpu.st.acov, [0, 1, 0, 0]);
    }

    // SPRU374E 4.7.16 ADD K16, Smem: overflow at bit 31 sets ACOV0, SATD saturates
    // the stored word to 7FFFh / 8000h
    #[test]
    fn add_k16_to_memory_saturates_and_reports_acov0() {
        let mut cpu = machine("add_k16_to_memory_saturates_and_reports_acov0");
        write(&mut cpu, 0x1600, 0x7FFF);
        run(&mut cpu);
        assert_eq!(read(&mut cpu, 0x1600), 0x7FFF);
        assert_eq!(cpu.st.acov, [1, 0, 0, 0]);
    }

    // SPRU374E 4.7.16: CARRY at bit 31; the Smem address is evaluated once
    #[test]
    fn add_k16_carry_and_one_address_update() {
        let mut cpu = machine("add_k16_carry_and_one_address_update");
        write(&mut cpu, 0x1600, 0xFFFF);
        write(&mut cpu, 0x1601, 0x1234);
        run(&mut cpu);
        assert_eq!((read(&mut cpu, 0x1600), read(&mut cpu, 0x1601)), (0x0000, 0x1234));
        assert_eq!((cpu.st.carry, cpu.st.acov), (1, [0, 0, 0, 0]));
        assert_eq!(cpu.st.xar[0], 0x1601);
    }

    // SPRU374E 4.7.16: "- #k" is ADD #-k, Smem (asm55 / dis55)
    #[test]
    fn add_k16_negative_constant_saturates_to_8000h() {
        let mut cpu = machine("add_k16_negative_constant_saturates_to_8000h");
        write(&mut cpu, 0x1600, 0x8000);
        run(&mut cpu);
        assert_eq!(read(&mut cpu, 0x1600), 0x8000);
        assert_eq!(cpu.st.acov, [1, 0, 0, 0]);
    }

    // SPRU374E 4.5.1 with SATD = 1; ACOV1 is bit 9 of ST0_55 (SPRU371F 2.10.1.1)
    #[test]
    fn accumulator_move_saturates_and_sets_st0_bit_9() {
        let mut cpu = machine("accumulator_move_saturates_and_sets_st0_bit_9");
        cpu.st.ac[0] = 0x01_E500_0030;
        run(&mut cpu);
        assert_eq!(cpu.st.ac[1], 0x00_7FFF_FFFF);
        assert_eq!(cpu.st.acov, [0, 1, 0, 0]);
        assert_eq!((cpu.st.t[0] >> 9) & 1, 1);
    }

    // SPRU374E 4.5.1: overflow is detected according to M40
    #[test]
    fn accumulator_move_with_m40_does_not_overflow() {
        let mut cpu = machine("accumulator_move_with_m40_does_not_overflow");
        cpu.st.ac[0] = 0x01_E500_0030;
        run(&mut cpu);
        assert_eq!(cpu.st.ac[1], 0x01_E500_0030);
        assert_eq!(cpu.st.acov, [0, 0, 0, 0]);
    }

    // SPRU374E 4.25 second example: repeat(CSR) SUBC *AR1, AC1 with CSR = 1, two steps
    // with divisor 0100h << 15 = 80 0000h:
    //   0746 0000 - 80 0000 = 06C6 0000 -> 0D8C 0001
    //   0D8C 0001 - 80 0000 = 0D0C 0001 -> 1A18 0003
    // SPRU374E (and SWPU067E) print 00 1A18 0007: that low word contradicts the 4.25
    // algorithm and the printed high word (three steps would give 3330 0007), so this
    // test expects the computed value.
    #[test]
    fn repeated_subc_example() {
        let mut cpu = machine("repeated_subc_example");
        write(&mut cpu, 0x200, 0x0100);
        cpu.st.ac[1] = 0x00_0746_0000;
        run(&mut cpu);
        assert_eq!((cpu.st.ac[1], cpu.st.carry), (0x00_1A18_0003, 1));
        assert_eq!(cpu.st.acov, [0, 0, 0, 0]);
    }
}

/// BCNT ACx, ACy, TCx, Tx (SPRU374E 4.9): the 1 bits of ACx AND ACy are counted into Tx;
/// TCx is set when the count is odd, cleared when even.
mod bit_count {
    use super::*;

    fn count(name: &str, ac1: u64, ac2: u64) -> (u32, u32) {
        let mut cpu = machine(name);
        cpu.st.ac[1] = ac1;
        cpu.st.ac[2] = ac2;
        run(&mut cpu);
        (cpu.st.t[1], cpu.st.tc1)
    }

    #[test]
    fn manual_example() {
        assert_eq!(count("count_tc1_0", 0x7E_2355_4FC0, 0x0F_E340_5678), (0x000B, 1));
    }

    #[test]
    fn even_count_clears_tcx() {
        assert_eq!(count("count_tc1_1", 0x80_0000_0003, 0x80_0000_0001), (2, 0));
    }
}

/// Dual 16-bit ADD / SUB with dual(Lmem) (SPRU374E 4.26.3-4.26.5): the high part uses
/// ACx(39-16) and the most significant word, the low part ACx(15-0) and the least
/// significant word (Lmem + 1 at an even address, Lmem - 1 at an odd one). Overflow is
/// detected per part (bit 15 low, bit 31 high) and either one sets ACOVy; SATD saturates
/// each part on its own (7FFFh / 8000h, 00 7FFFh / FF 8000h); CARRY is the carry of the
/// high part, extracted at bit 31.
mod dual_arithmetic {
    use super::*;

    fn run_dual(name: &str, words: [u32; 2], ac0: u64, carry: u32) -> Core {
        let mut cpu = machine(name);
        write(&mut cpu, 0x1600, words[0]);
        write(&mut cpu, 0x1601, words[1]);
        cpu.st.ac[0] = ac0;
        cpu.st.carry = carry;
        run(&mut cpu);
        cpu
    }

    #[test]
    fn add_at_even_address() {
        let cpu = run_dual("dual_add_even", [0x1234, 0x0001], 0x00_0001_0002, 0);
        assert_eq!((cpu.st.ac[1], cpu.st.carry, cpu.st.acov), (0x00_1235_0003, 0, [0, 0, 0, 0]));
    }

    #[test]
    fn add_at_odd_address_takes_the_previous_word() {
        let cpu = run_dual("dual_add_odd", [0x0001, 0x1234], 0x00_0001_0002, 0);
        assert_eq!(cpu.st.ac[1], 0x00_1235_0003);
    }

    #[test]
    fn low_part_overflow_sets_acov() {
        for (name, low) in [("dual_add_satd_0", 0x8000), ("dual_add_satd_1", 0x7FFF)] {
            let cpu = run_dual(name, [0x0000, 0x7FFF], 0x00_0000_0001, 0);
            assert_eq!(cpu.st.ac[1], low, "{}", name);
            assert_eq!(cpu.st.acov, [0, 1, 0, 0], "{}", name);
        }
    }

    #[test]
    fn high_part_overflow_sets_acov() {
        for (name, high) in [("dual_add_satd_0", 0x00_8000u64), ("dual_add_satd_1", 0x00_7FFF)] {
            let cpu = run_dual(name, [0x7FFF, 0x0000], 0x00_0001_0000, 0);
            assert_eq!(cpu.st.ac[1], high << 16, "{}", name);
            assert_eq!(cpu.st.acov, [0, 1, 0, 0], "{}", name);
        }
    }

    #[test]
    fn high_part_carry() {
        let cpu = run_dual("dual_add_even", [0xFFFF, 0x0000], 0x00_0001_0000, 0);
        assert_eq!((cpu.st.ac[1], cpu.st.carry, cpu.st.acov), (0, 1, [0, 0, 0, 0]));
    }

    #[test]
    fn subtract_reports_the_high_part_borrow() {
        let cpu = run_dual("dual_sub", [0x1234, 0x0001], 0x00_0001_0002, 1);
        assert_eq!((cpu.st.ac[1], cpu.st.carry), (0xFF_EDCD_0001, 0));
        let cpu = run_dual("dual_sub", [0x0001, 0x0001], 0x00_1234_0002, 0);
        assert_eq!((cpu.st.ac[1], cpu.st.carry), (0x00_1233_0001, 1));
    }
}

/// RPTCC k8, cond (SPRU374E 4.61): while the condition holds the next instruction runs
/// k8 + 1 times. After a completed loop RPTC reads 0000 (the 4.61 example); after an early
/// exit RPTC(7-0) holds the number of iterations not performed (the high byte is not shown
/// in the manual and is not asserted). The body is a SUBC, which leaves TC1 unchanged.
mod conditional_repeat {
    use super::*;

    fn repeat(name: &str) -> Core {
        let mut cpu = machine(name);
        write(&mut cpu, 0x200, 0x0100);
        cpu.st.ac[1] = 0x00_0746_0000;
        run(&mut cpu);
        cpu
    }

    #[test]
    fn true_condition_runs_k8_plus_1_times() {
        let cpu = repeat("rptcc_tc1_1");
        assert_eq!((cpu.st.ac[1], cpu.st.carry, cpu.st.t[0]), (0x00_6560_000F, 1, 0x0000));
    }

    #[test]
    fn false_condition_runs_no_iteration() {
        let cpu = repeat("rptcc_tc1_0");
        // the 4.61 table gives RPTC(7-0) only: k8 + 1 iterations not performed
        assert_eq!((cpu.st.ac[1], cpu.st.t[0] & 0xFF), (0x00_0746_0000, 0x04));
    }

    #[test]
    fn negated_condition() {
        let cpu = repeat("rptcc_not_tc1_0");
        assert_eq!((cpu.st.ac[1], cpu.st.t[0]), (0x00_6560_000F, 0x0000));
    }
}

/// XCCPART (SPRU374E 4.28.2): the condition controls the execute phase of the conditional
/// instruction(s); operations of the address phase (pointer updates) are always executed.
mod execute_conditionally {
    use super::*;

    // SPRU374E 4.28.2 examples, XCCPART AR0 != #0 then ADD *AR2+, AC0; the example's
    // 00 0000 EF0C shows EF00h zero extended, so SXMD = 0
    #[test]
    fn manual_examples() {
        for (name, ac0) in [("xcc_ar0_3000", 0x00_0000_EF0C), ("xcc_ar0_0", 0x00_0000_000C)] {
            let mut cpu = machine(name);
            write(&mut cpu, 0x405, 0xEF00);
            cpu.st.ac[0] = 0x00_0000_000C;
            run(&mut cpu);
            assert_eq!((cpu.st.ac[0], cpu.st.xar[2]), (ac0, 0x0406), "{}", name);
        }
    }

    // previous_instruction || XCCPART cond: the partner runs unconditionally
    #[test]
    fn after_a_partner() {
        for (name, ac0) in [("xcc_partner_tc2_0", 0x0C), ("xcc_partner_tc2_1", 0x0D)] {
            let mut cpu = machine(name);
            write(&mut cpu, 0x1600, 1);
            cpu.st.ac[0] = 0x0C;
            run(&mut cpu);
            assert_eq!((cpu.st.t[1], cpu.st.ac[0], cpu.st.xar[2]), (5, ac0, 0x1601), "{}", name);
        }
    }

    #[test]
    fn parallel_store_keeps_the_pointer_update() {
        for (name, word) in [("xcc_store_tc1_0", 0x0000), ("xcc_store_tc1_1", 0x0009)] {
            let mut cpu = machine(name);
            run(&mut cpu);
            assert_eq!((read(&mut cpu, 0x1600), cpu.st.xar[0]), (word, 0x1601), "{}", name);
        }
    }
}

/// SQDST Xmem, Ymem, ACx, ACy (SPRU374E 4.77): ACy = ACy + ACx(32-16)^2 in parallel with
/// ACx = (Xmem << #16) - (Ymem << #16); CARRY is the complement of the borrow of that
/// subtraction.
mod square_distance {
    use super::*;

    fn sqdst(xmem: u32, ymem: u32, ac0: u64, carry: u32) -> Core {
        let mut cpu = machine("sqdst");
        write(&mut cpu, 0x1600, xmem);
        write(&mut cpu, 0x1601, ymem);
        cpu.st.ac[0] = ac0;
        cpu.st.ac[1] = 0;
        cpu.st.carry = carry;
        run(&mut cpu);
        cpu
    }

    #[test]
    fn manual_example() {
        let cpu = sqdst(0x0055, 0x00AA, 0xFF_ABCD_0000, 0);
        assert_eq!((cpu.st.ac[0], cpu.st.ac[1]), (0xFF_FFAB_0000, 0x00_1BB1_8229));
        assert_eq!((cpu.st.carry, cpu.st.acov), (0, [0, 0, 0, 0]));
    }

    #[test]
    fn no_borrow_sets_carry() {
        let cpu = sqdst(0x00AA, 0x0055, 0, 0);
        assert_eq!((cpu.st.ac[0], cpu.st.carry), (0x00_0055_0000, 1));
    }
}
