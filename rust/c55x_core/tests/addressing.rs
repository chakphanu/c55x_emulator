//! Circular addressing, the ARMS dependent short operands, the 32-bit stack transfers the
//! DSP/BIOS dispatcher uses to save the circular buffer registers, the M40 computation
//! mode and CARRY/Borrow for 64-bit add and subtract (from
//! tests/test_c55x_emu_addressing.py). Programs: rust/fixtures/addressing. asm55 has no
//! 'TC1 = AC0 == #0' (the programs compare with AC1 = 0) and no 'T0 * T1' MAC (the
//! carry test multiplies AC1 * T1).

mod common;

use c55x_core::core::Core;
use common::{plain, read, read32, run, try_run, write, write32};

fn machine(name: &str) -> Core {
    fixture!("addressing").core(&format!("t_{}", name))
}

mod circular {
    use super::*;

    #[test]
    fn index_wraps_in_the_buffer() {
        let mut cpu = machine("index_wraps_in_the_buffer");
        *plain(&mut cpu, "bk03") = 4;
        *plain(&mut cpu, "bsa23") = 0x1600;
        for i in 0..4 {
            write(&mut cpu, 0x011600 + i, 10 + i);
        }
        run(&mut cpu);
        assert_eq!(cpu.st.ac.map(|a| a & 0xFFFF), [13, 12, 13, 11]);
        assert_eq!(cpu.st.xar[2], 0x010003);
    }

    #[test]
    fn linear_pointer_ignores_the_buffer() {
        let mut cpu = machine("linear_pointer_ignores_the_buffer");
        *plain(&mut cpu, "bk03") = 4;
        *plain(&mut cpu, "bsa23") = 0x100;
        write(&mut cpu, 0x1600, 7);
        run(&mut cpu);
        assert_eq!((cpu.st.ac[0], cpu.st.xar[3]), (7, 0x1601));
    }

    #[test]
    fn zero_buffer_size_raises() {
        let mut cpu = machine("zero_buffer_size_raises");
        assert!(try_run(&mut cpu).is_err());
    }
}

mod short_operand {
    use super::*;

    #[test]
    fn dsp_mode_uses_t1() {                             // SPRU375G table 6-2
        let mut cpu = machine("dsp_mode_uses_t1");
        for i in 0..6 {
            write(&mut cpu, 0x1600 + i, 0x100 + i);
        }
        run(&mut cpu);
        assert_eq!([0, 1, 2].map(|i| cpu.st.ac[i] & 0xFFFF), [0x102, 0x100, 0x105]);
        assert_eq!(cpu.st.xar[1], 0x1602);
    }
}

mod long_stack {
    use super::*;

    #[test]
    fn mmr_block_save_and_restore() {                   // BIOS dispatcher 0x03bbbe / 0x03bce0
        let mut cpu = machine("mmr_block_save_and_restore");
        run(&mut cpu);
        assert_eq!((*plain(&mut cpu, "bk47"), *plain(&mut cpu, "bsa45"), cpu.st.xsp), (256, 99, 0x1000));
    }

    #[test]
    fn long_push_order() {
        let mut cpu = machine("long_push_order");
        write32(&mut cpu, 0x1600, 0x12345678);
        run(&mut cpu);
        assert_eq!((cpu.st.xsp, read(&mut cpu, 0xFFE), read(&mut cpu, 0xFFF)), (0xFFE, 0x1234, 0x5678));
    }
}

mod m40 {
    use super::*;

    #[test]
    fn m40_off_keeps_32_bit_semantics() {
        let mut cpu = machine("m40_off_keeps_32_bit_semantics");
        run(&mut cpu);
        assert_eq!((cpu.st.ac[0], cpu.st.tc1, cpu.st.acov), (0, 1, [1, 0, 0, 0]));    // bit 32 is shifted out
    }

    #[test]
    fn m40_on_uses_the_guard_bits() {
        let mut cpu = machine("m40_on_uses_the_guard_bits");
        run(&mut cpu);
        assert_eq!((cpu.st.tc1, cpu.st.ac[1], cpu.st.ac[2]), (0, 0x10000, 0x10000));
    }

    #[test]
    fn sign_extended_low_byte() {                       // gfx_draw_char 0x80022f: glyph bearing
        let mut cpu = machine("sign_extended_low_byte");
        write(&mut cpu, 0x1600, 0x0600);
        write(&mut cpu, 0x1601, 0x06FE);
        run(&mut cpu);
        assert_eq!((cpu.st.ac[1], cpu.st.ac[2]), (0, (1 << 40) - 2));
    }
}

mod carry {
    use super::*;

    /// 64-bit a op b as the firmware does it: low longs first, then the high words.
    fn long_op(program: &str, a: u64, b: u64) -> u64 {
        let mut cpu = machine(program);
        for (base, v) in [(0x1600, a), (0x1610, b)] {
            write32(&mut cpu, base, v as u32);
            write32(&mut cpu, base + 2, (v >> 32) as u32);
        }
        run(&mut cpu);
        ((read32(&mut cpu, 0x1622) as u64) << 32) | read32(&mut cpu, 0x1620) as u64
    }

    #[test]
    fn _64_bit_add() {
        for (a, b) in [(0xFFFFFFFF, 1), (0x1234_8000_FFFF_FFFF, 0x0000_8000_0000_0001), (5, 7)] {
            assert_eq!(long_op("long_add", a, b), a.wrapping_add(b), "{:#x}", a);
        }
    }

    #[test]
    fn _64_bit_subtract() {
        for (a, b) in [(0x1_0000_0000, 1), (0x1234_0000_0000_0000, 0x0000_0001_0000_0001), (9, 7)] {
            assert_eq!(long_op("long_subtract", a, b), a.wrapping_sub(b), "{:#x}", a);
        }
    }

    #[test]
    fn product_leaves_carry() {
        let mut cpu = machine("product_leaves_carry");
        run(&mut cpu);
        assert_eq!(cpu.st.carry, 0);                    // borrow from the subtraction
    }
}

mod bit_field {
    use super::*;

    #[test]
    fn band_sets_tc_on_any_bit() {                      // gfx_clear_rect 0x801a30: TC1 = y0 & 7
        let mut cpu = machine("band_sets_tc_on_any_bit");
        write(&mut cpu, 0x1600, 6);
        run(&mut cpu);
        assert_eq!((cpu.st.tc1, cpu.st.tc2), (1, 0));
    }
}

mod mmr_load {
    use super::*;

    #[test]
    fn mmr_load_sign_extends_like_memory() {            // a MMR access is an Smem operand
        let mut cpu = machine("mmr_load_sign_extends_like_memory");
        run(&mut cpu);
        assert_eq!((cpu.st.ac[0], cpu.st.ac[1]), (0xFFFFFFFFFE, 0xFFFE));
    }
}
