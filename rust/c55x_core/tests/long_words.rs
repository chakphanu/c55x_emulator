//! Addressed-word ordering of long transfers (from tests/test_c55x_emu_long_words.py).
//! TI SWPU073E section 3.3.2, p. 3-6, and section 6.5.3.1, p. 6-64: the addressed word is
//! the MSW; the paired LSW is at address XOR 1. Programs: rust/fixtures/long_words; asm55
//! has no uns(dbl(...)) load, so the loads run with SXMD = 0 (zero extension).

mod common;

use c55x_core::core::Core;
use common::{read, run, write};

fn machine(name: &str) -> Core {
    fixture!("long_words").core(&format!("t_{}", name))
}

#[test]
fn memory_load_both_parities_and_postincrement() {
    for parity in [0u32, 1] {
        let mut cpu = machine("memory_load");
        cpu.st.xar[2] = 0x2400 + parity;
        write(&mut cpu, 0x2400, 0x2468);
        write(&mut cpu, 0x2401, 0xB135);
        run(&mut cpu);
        assert_eq!(cpu.st.ac[2], [0x2468B135, 0xB1352468][parity as usize], "parity {}", parity);
        assert_eq!(cpu.st.xar[2], 0x2402 + parity);
    }
}

#[test]
fn memory_store_both_parities_and_postincrement() {
    for parity in [0u32, 1] {
        let mut cpu = machine("memory_store");
        cpu.st.xar[3] = 0x2600 + parity;
        cpu.st.ac[2] = 0x6ACE3579;
        run(&mut cpu);
        let expected = [(0x6ACE, 0x3579), (0x3579, 0x6ACE)][parity as usize];
        assert_eq!((read(&mut cpu, 0x2600), read(&mut cpu, 0x2601)), expected, "parity {}", parity);
        assert_eq!(cpu.st.xar[3], 0x2602 + parity);
    }
}

#[test]
fn mmr_load_both_parities() {
    for (name, expected) in [("mmr_load_l", 0x13576ACE), ("mmr_load_h", 0x6ACE1357)] {
        let mut cpu = machine(name);
        cpu.st.ac[0] = 0x3D6ACE1357;
        run(&mut cpu);
        assert_eq!(cpu.st.ac[2], expected, "{}", name);
    }
}

#[test]
fn mmr_store_both_parities() {
    for (name, expected) in [("mmr_store_l", 0x35796ACE), ("mmr_store_h", 0x6ACE3579)] {
        let mut cpu = machine(name);
        cpu.st.ac[0] = 0x3D00000000;
        cpu.st.ac[2] = 0x6ACE3579;
        run(&mut cpu);
        assert_eq!(cpu.st.ac[0] & 0xFFFFFFFF, expected, "{}", name);
    }
}
