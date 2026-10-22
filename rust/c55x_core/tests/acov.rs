//! Accumulator overflow flags (ACOV0..ACOV3) on shift counts and the M40 shifter width
//! (from tests/test_c55x_emu_acov.py). Programs: rust/fixtures/acov. asm55 has no
//! 'ACy = ACx >> Tx' or 32-bit constant loads: the programs shift by a negative Tx /
//! '<< #-k' and the tests preset the accumulator values.

mod common;

use c55x_core::core::Core;
use common::{run, step};

fn machine(name: &str) -> Core {
    fixture!("acov").core(&format!("t_{}", name))
}

/// SPRU374E 4.70.3: shifting an accumulator by a Tx content outside -32..+31 saturates
/// the count to that bound and reports an overflow on the destination accumulator
/// (sticky per-accumulator flag, clear at reset).
mod acov {
    use super::*;

    #[test]
    fn flags_are_clear_after_reset() {
        let cpu = machine("flags_are_clear_after_reset");
        assert_eq!(cpu.st.acov, [0, 0, 0, 0]);
    }

    #[test]
    fn in_range_count_leaves_the_flags_clear() {
        for (name, value) in [("in_range_count_5", 0x1234), ("in_range_count_m4", 0x5678)] {
            let mut cpu = machine(name);
            cpu.st.ac[0] = value;
            run(&mut cpu);
            assert_eq!(cpu.st.acov, [0, 0, 0, 0], "{}", name);
        }
    }

    #[test]
    fn count_above_31_sets_the_destination_flag() {
        let mut cpu = machine("count_above_31_sets_the_destination_flag");
        cpu.st.ac[0] = 0x01;
        run(&mut cpu);
        assert_eq!(cpu.st.acov, [0, 1, 0, 0]);          // sticky, source flag clear
    }

    #[test]
    fn count_below_minus_32_sets_the_destination_flag() {
        let mut cpu = machine("count_below_minus_32_sets_the_destination_flag");
        cpu.st.ac[0] = 0x1234;
        run(&mut cpu);
        assert_eq!(cpu.st.acov, [0, 0, 1, 0]);
    }

    #[test]
    fn clamped_count_result_is_confined_to_the_working_width() {
        // SPRU374E 4.70.3 with M40 = 0 (default): the shifter works in the 32-bit domain.
        // Count 40 saturates to 31: 1 << 31 leaves the signed 32-bit range, the
        // destination flag is set and the truncated result (guard bits clear) is stored.
        // Count -33 saturates to -32: 0x1234 >> 32 is 0.
        for (name, value, expected) in [("clamped_left", 0x01, 0x80000000), ("clamped_right", 0x1234, 0x0)] {
            let mut cpu = machine(name);
            cpu.st.ac[0] = value;
            run(&mut cpu);
            assert_eq!(cpu.st.ac[1], expected, "{}", name);
            assert_eq!(cpu.st.acov, [0, 1, 0, 0], "{}", name);
        }
    }
}

/// M40 selects the shifter working width (SPRU374E 4.70.3).
mod shift_m40 {
    use super::*;

    #[test]
    fn m40_off_extracts_bits_shifted_past_31() {
        // SPRU371F / v3.x CPU 2.10.2.9: with M40 = 0 shifts work on 32-bit values and
        // left shifts extract bits shifted out past bit 31; overflow is still reported
        // at the boundary.
        let mut cpu = machine("m40_off_extracts_bits_shifted_past_31");
        cpu.st.ac[0] = 0x10000;
        run(&mut cpu);
        assert_eq!(cpu.st.ac[0], 0);
        assert_eq!(cpu.st.acov, [1, 0, 0, 0]);
    }

    #[test]
    fn m40_off_with_satd_saturates() {
        let mut cpu = machine("m40_off_with_satd_saturates_1");
        cpu.st.ac[0] = 0x10000;
        run(&mut cpu);
        assert_eq!(cpu.st.ac[0], 0x7FFFFFFF);
        assert_eq!(cpu.st.acov, [1, 0, 0, 0]);
        let mut cpu = machine("m40_off_with_satd_saturates_2");
        run(&mut cpu);
        assert_eq!(cpu.st.ac[0], 0xFF80000000);
        assert_eq!(cpu.st.acov, [1, 0, 0, 0]);
    }

    #[test]
    fn m40_on_keeps_the_guard_bits() {
        let mut cpu = machine("m40_on_keeps_the_guard_bits");
        cpu.st.ac[0] = 0x10000;
        run(&mut cpu);
        assert_eq!(cpu.st.ac[0], 0x100000000);
        assert_eq!(cpu.st.acov, [0, 0, 0, 0]);
    }

    #[test]
    fn step_and_run_agree_on_immediate_shifts() {
        // the interpreter (single-step) and the compiled engine must produce the same
        // values for every shift form
        let mut a = machine("step_and_run_agree_on_immediate_shifts");
        run(&mut a);
        let mut b = machine("step_and_run_agree_on_immediate_shifts");
        for _ in 0..6 {
            step(&mut b);
        }
        assert_eq!(a.st.ac, b.st.ac);
    }
}
