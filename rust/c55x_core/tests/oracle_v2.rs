//! Oracle v2 additions: K16 / K8 multiplier sign extension (SPRU374E 4.47/4.48 operand
//! rules) and repeat-body trace visibility (no value or cycle change), from
//! tests/test_c55x_emu_oracle_v2.py. Programs: rust/fixtures/oracle_v2. The runtime
//! listing (dis55 -qq -r -g) prints the negative constants as such (#-22118, #-7680),
//! where the Python tests used the 32-bit hex prints of dis55 -e (#0xffffa99a,
//! #0xffffe200); asm55 has no 32-bit constant load (AC1 = #0x10000 is preset).

mod common;

use std::collections::VecDeque;

use c55x_core::core::Core;
use common::{read, run, write};

fn machine(name: &str) -> Core {
    fixture!("oracle_v2").core(&format!("t_{}", name))
}

mod k16_constant {
    use super::*;

    #[test]
    fn mpyk_negative_print_sign_extends() {
        // a NEGATIVE K16 (-0x5666) sign extended into the multiplier, not +4294958490
        let mut cpu = machine("mpyk_negative_print_sign_extends");
        run(&mut cpu);
        assert_eq!(cpu.st.ac[0], (1 << 40) - (0x100 * 0x5666));
    }

    #[test]
    fn k16_constant_sign_extends() {
        // MACK: (Tx * #K16) with K16 = 0x8000 is -32768 in the 17-bit multiplier; the
        // product is negative, not +65536
        let mut cpu = machine("k16_constant_sign_extends");
        run(&mut cpu);
        assert_eq!(cpu.st.ac[0], (1 << 40) - (2 * 0x8000));
    }

    #[test]
    fn positive_k16_unchanged() {
        let mut cpu = machine("positive_k16_unchanged");
        run(&mut cpu);
        assert_eq!(cpu.st.ac[0], 3 * 0x7FFF);
    }

    #[test]
    fn pre_negated_constant_unchanged() {
        // a negative constant (0xffffe200 in the -e print) stays negative
        let mut cpu = machine("pre_negated_constant_unchanged");
        run(&mut cpu);
        assert_eq!(cpu.st.ac[0], (1 << 40) - (2 * 0x1e00));
    }
}

mod repeat_body_trace {
    use super::*;

    fn fill(value: u32, trace: bool) -> Core {
        let mut cpu = machine("fill");
        cpu.st.t[0] = value;
        cpu.st.xar[0] = 0x2000;                         // fill destination off the MMR page
        if trace {
            cpu.trace = Some((1024, VecDeque::new()));
        }
        run(&mut cpu);
        cpu
    }

    #[test]
    fn rpt_body_pcs_appear_in_the_trace() {
        let mut cpu = fill(0xBEEF, true);
        let trace: Vec<u32> = cpu.trace.take().unwrap().1.into_iter().collect();
        let body_pc = fixture!("oracle_v2").sym("b_fill");          // the repeated store
        assert_eq!(trace.iter().filter(|&&pc| pc == body_pc).count(), 4);   // CSR + 1 = 4 trips
        let repeat_pc = fixture!("oracle_v2").sym("r_fill");
        assert!(!trace[trace.len() - 3..].contains(&repeat_pc));    // marker only once
    }

    #[test]
    fn tracing_changes_no_value() {
        let once = |trace: bool| {
            let mut cpu = fill(0x1234, trace);
            cpu.trace = None;
            let mem: Vec<u32> = (0..4).map(|i| read(&mut cpu, 0x20000 + i)).collect();
            (cpu.st.ac, cpu.st.t, cpu.st.xar, mem, cpu.st.cycles)
        };
        assert_eq!(once(true), once(false));
    }
}

mod k8_constant {
    use super::*;

    #[test]
    fn mpyk_k8_negative() {
        // asm55 probe: MPYK #-128 encodes as 1e8040; sext16(0xff80) = -128 = sext8(0x80)
        let mut cpu = machine("mpyk_k8_negative");
        cpu.st.ac[1] = 0x10000;
        run(&mut cpu);
        assert_eq!(cpu.st.ac[0], (1 << 40) - 128);      // 1 * -128
    }

    #[test]
    fn macmk_smem_k8_negative() {
        // asm55 probe: MPYMK *AR3,#-83 encodes as f861ad20
        let mut cpu = machine("macmk_smem_k8_negative");
        write(&mut cpu, 0x1600, 0x0100);                // Smem = 0x0100
        run(&mut cpu);
        assert_eq!(cpu.st.ac[2], (1 << 40) - (0x100 * 83));    // 256 * -83
    }

    #[test]
    fn positive_k8_unchanged() {
        let mut cpu = machine("positive_k8_unchanged");
        write(&mut cpu, 0x1600, 0x0002);
        run(&mut cpu);
        assert_eq!(cpu.st.ac[2], 2 * 83);
    }
}
