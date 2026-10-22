//! DSP features of compiled MP3 decoder code: fractional and saturating arithmetic (ST1
//! FRCT / SATD), rnd / saturate, mant / exp, dual multiplies sharing the Cmem operand,
//! circular mar() arithmetic, signed shift counts in Tx with SXMD = 0, 'previous || if
//! (cond) execute' for the next instruction, stores of a shifted accumulator, popboth into
//! an accumulator, the port qualifier on a read-modify-write and the reserved MMR 0x05
//! (from tests/test_c55x_emu_dsp.py). Programs: rust/fixtures/dsp.

mod common;

use c55x_core::core::Core;
use common::{message, plain, read, run, write};

fn machine(name: &str) -> Core {
    fixture!("dsp").core(&format!("t_{}", name))
}

mod mant_exp {
    use super::*;

    fn check(ac0: u64, mant: u64, exp: u32) {
        let mut cpu = machine("mant_exp");
        cpu.st.ac[0] = ac0;
        run(&mut cpu);
        assert_eq!((cpu.st.ac[1], cpu.st.t[1]), (mant, exp), "{:#x}", ac0);
    }

    #[test]
    fn spru375g_examples() {                            // SPRU375G 5-133, examples 1 and 2
        check(0x21_0A0A_0A0A, 0x00_4214_1414, 7);
        check(0x00_E804_0000, 0x00_7402_0000, 1);
    }

    #[test]
    fn zero() {
        check(0, 0, 0x8000);
    }
}

mod dual_multiply {
    use super::*;

    fn setup(name: &str) -> Core {
        let mut cpu = machine(name);
        for (addr, v) in [(0x1600, 3), (0x1610, 7), (0x1620, 2), (0x1621, 4)] {
            write(&mut cpu, addr, v);
        }
        run(&mut cpu);
        cpu
    }

    #[test]
    fn shared_coefficient_read_once() {
        let cpu = setup("dual");
        assert_eq!((cpu.st.ac[0], cpu.st.ac[1], cpu.st.xcdp, cpu.st.xar[0], cpu.st.xar[1]),
                   (6, 14, 0x1621, 0x1601, 0x1611));
    }

    #[test]
    fn fractional_mode_doubles_products() {
        let cpu = setup("dual_frct");
        assert_eq!((cpu.st.ac[0], cpu.st.ac[1]), (12, 28));
    }
}

mod saturation {
    use super::*;

    fn add_one(name: &str) -> u64 {
        let mut cpu = machine(name);
        cpu.st.ac[0] = 0x7FFF_FFFF;
        run(&mut cpu);
        cpu.st.ac[0]
    }

    #[test]
    fn satd_clamps_at_32_bits() {
        assert_eq!(add_one("add_one"), 0x8000_0000);
        assert_eq!(add_one("add_one_satd"), 0x7FFF_FFFF);
    }

    #[test]
    fn satd_with_m40_clamps_at_40_bits() {
        assert_eq!(add_one("add_one_satd_m40"), 0x8000_0000);
    }

    #[test]
    fn rnd_and_saturate() {
        let mut cpu = machine("rnd_and_saturate");
        cpu.st.ac[0] = 0x7FFF_C000;
        cpu.st.ac[2] = 0x1234_8000;
        run(&mut cpu);
        // SATR clears the 16 LSBs after saturating (SPRU374E 4.69 SATR example)
        assert_eq!((cpu.st.ac[1], cpu.st.ac[3]), (0x7FFF_0000, 0x1235_0000));
    }
}

mod execute_after_partner {                             // SPRU375G 5-160
    use super::*;

    fn run_with(name: &str) -> (u32, u32) {
        let mut cpu = machine(name);
        run(&mut cpu);
        (cpu.st.t[0], cpu.st.t[1])
    }

    #[test]
    fn partner_runs_and_next_is_conditional() {
        assert_eq!(run_with("after_partner_tc2_1"), (7, 5));
        assert_eq!(run_with("after_partner_tc2_0"), (0, 5));
    }
}

mod execute_d_unit {                                    // SPRU375G 5-163 / 5-164
    use super::*;

    fn run_with(name: &str) -> (u64, u64, u32) {
        let mut cpu = machine(name);
        write(&mut cpu, 0x1600, 0x20);
        write(&mut cpu, 0x1601, 0x21);
        run(&mut cpu);
        (cpu.st.ac[0], cpu.st.ac[1], cpu.st.xar[1])
    }

    #[test]
    fn next_data_operation_is_conditional() {
        assert_eq!(run_with("d_unit_tc1_1"), (5, 0, 0x1600));
        assert_eq!(run_with("d_unit_tc1_0"), (0, 0, 0x1600));
    }

    #[test]
    fn address_phase_always_runs() {                    // example 2: mar(*AR1+) runs anyway
        assert_eq!(run_with("d_unit_address_phase"), (0, 0x21, 0x1601));
    }
}

mod address_and_shift {
    use super::*;

    #[test]
    fn mar_arithmetic_is_circular() {
        let mut cpu = machine("mar_arithmetic_is_circular");
        *plain(&mut cpu, "bk03") = 4;
        run(&mut cpu);
        assert_eq!(cpu.st.xar[2], 0x010001);
    }

    #[test]
    fn shift_count_in_tx_is_signed_without_sxmd() {
        let mut cpu = machine("shift_count_in_tx_is_signed_without_sxmd");
        write(&mut cpu, 0x1602, 0x100);
        run(&mut cpu);
        assert_eq!(cpu.st.ac[0], 0x10);
    }

    #[test]
    fn store_of_shifted_accumulator() {
        let mut cpu = machine("store_of_shifted_accumulator");
        cpu.st.ac[0] = 0x1234_5678;
        run(&mut cpu);
        assert_eq!(read(&mut cpu, 0x1600), 0x1234);
    }

    #[test]
    fn popboth_sign_extends() {                         // deviates from SPRU375G 5-330, see O1
        let mut cpu = machine("popboth_sign_extends");
        cpu.st.ac[3] = 0x12_3456_789A;
        cpu.st.ac[2] = 0x00_FFFF_FF70;
        run(&mut cpu);
        assert_eq!((cpu.st.ac[3], cpu.st.ac[2]), (0x00_3456_789A, 0xFF_FFFF_FF70));
    }

    #[test]
    fn port_qualifier_covers_read_modify_write() {      // SPRU375G 5-327, cl55 mcbsp_start
        // the Python test hooked port 0x1234 from Python; here the port is a register of
        // the core's DMA model (channel 0 CSSA_L, 0x0c04), set to 0x10 by the program
        let mut cpu = machine("port_qualifier_covers_read_modify_write");
        write(&mut cpu, 0x0C04, 0xF0);                  // data memory at the same address stays untouched
        run(&mut cpu);
        let port = cpu.port_read(0x0C04).unwrap_or_else(|e| panic!("{}", message(e)));
        assert_eq!((port, read(&mut cpu, 0x0C04)), (0x11, 0xF0));
    }

    #[test]
    fn reserved_mmr_reads_zero() {                      // list search past its end, 0x81079c
        let mut cpu = machine("reserved_mmr_reads_zero");
        cpu.st.st3 = 0;
        run(&mut cpu);
        assert_eq!(cpu.st.ac[0], 0);
    }
}
