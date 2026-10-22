//! SFTL (algebraic <<< and >>>) pinned to SPRU374E 4.37: the shift works on the
//! M40-selected width, the destination guard bits are cleared when M40 = 0, CARRY holds
//! the shifted-out bit and is cleared on a zero count (from tests/test_c55x_emu_sftl.py).
//! Programs: rust/fixtures/sftl.

mod common;

use c55x_core::core::Core;
use common::run;

fn machine(name: &str) -> Core {
    fixture!("sftl").core(&format!("t_{}", name))
}

#[test]
fn manual_example_4_37_3() {
    // SFTL AC0, T0, AC1 with T0 = -6, M40 = 0: the manual's worked example expects
    // AC1 = 00 02C0 0048 (guard bits cleared)
    let mut cpu = machine("register_count");
    cpu.st.ac[0] = 0x5FB0001234;
    cpu.st.t[0] = 0xFFFA;
    run(&mut cpu);
    assert_eq!(cpu.st.ac[1], 0x02C00048);
    assert_eq!(cpu.st.carry, 1);
}

#[test]
fn right_by_one_clears_the_guards() {
    let mut cpu = machine("right_by_one_clears_the_guards");
    cpu.st.ac[3] = 0xFF80000001;
    run(&mut cpu);
    assert_eq!(cpu.st.ac[3], 0x40000000);
    assert_eq!(cpu.st.carry, 1);
}

#[test]
fn register_destination_shifts_16_bits() {
    let mut cpu = machine("register_destination_shifts_16_bits");
    cpu.st.t[2] = 0x0003;
    run(&mut cpu);
    assert_eq!(cpu.st.t[2], 1);
    assert_eq!(cpu.st.carry, 1);
}

#[test]
fn zero_count_clears_carry() {
    let mut cpu = machine("register_count");
    cpu.st.ac[0] = 0x5FB0001234;
    cpu.st.t[0] = 0;
    cpu.st.carry = 1;
    run(&mut cpu);
    assert_eq!(cpu.st.ac[1], 0xB0001234);               // guards still cleared
    assert_eq!(cpu.st.carry, 0);
}
