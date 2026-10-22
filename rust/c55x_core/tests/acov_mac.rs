//! MAC/MPY overflow reporting per SPRU374E 4.47/4.48 and the M40 mode definition of
//! SPRU371F / C55x v3.x CPU 2.10.2.9: the accumulate checks overflow at the M40-selected
//! boundary, reports the destination ACOVy and saturates per SATD; multiplication overflow
//! detection depends on SMUL (from tests/test_c55x_emu_acov_mac.py). Programs:
//! rust/fixtures/acov_mac. asm55 has no 32-bit constant load (the tests preset AC0) and
//! no MAS with a constant (the negative case multiplies by *AR3 = 0x7FFF).

mod common;

use c55x_core::core::Core;
use common::{run, step, write};

fn machine(name: &str) -> Core {
    fixture!("acov_mac").core(&format!("t_{}", name))
}

#[test]
fn m40_off_accumulate_overflow_reports_and_saturates() {
    let mut cpu = machine("m40_off_accumulate_overflow_1");
    cpu.st.ac[0] = 0x7FFFFFFF;
    run(&mut cpu);
    // product 0x3FFF0001 + 0x7FFFFFFF leaves the signed 32-bit range
    assert_eq!(cpu.st.ac[0], 0x7FFFFFFF + 0x7FFF * 0x7FFF);
    assert_eq!(cpu.st.acov, [1, 0, 0, 0]);
    let mut cpu = machine("m40_off_accumulate_overflow_2");
    cpu.st.ac[0] = 0x7FFFFFFF;
    run(&mut cpu);
    assert_eq!(cpu.st.ac[0], 0x7FFFFFFF);
    assert_eq!(cpu.st.acov, [1, 0, 0, 0]);
}

#[test]
fn m40_off_negative_overflow_saturates() {
    let mut cpu = machine("m40_off_negative_overflow_saturates");
    cpu.st.ac[0] = 0xFF80000001;                        // -0x7FFFFFFF
    write(&mut cpu, 0x1600, 0x7FFF);
    run(&mut cpu);
    assert_eq!(cpu.st.ac[0], 0xFF80000000);
    assert_eq!(cpu.st.acov, [1, 0, 0, 0]);
}

#[test]
fn m40_on_accumulate_uses_40_bits() {
    let mut cpu = machine("m40_on_accumulate_uses_40_bits");
    cpu.st.ac[0] = 0x7FFFFFFF;
    run(&mut cpu);
    assert_eq!(cpu.st.ac[0], 0x7FFFFFFF + 0x7FFF * 0x7FFF);
    assert_eq!(cpu.st.acov, [0, 0, 0, 0]);
}

#[test]
fn in_range_product_and_sum_report_nothing() {
    let mut cpu = machine("in_range_product_and_sum_report_nothing");
    run(&mut cpu);
    assert_eq!(cpu.st.ac[0], 0x1000 + 0x40 * 0x40);
    assert_eq!(cpu.st.acov, [0, 0, 0, 0]);
}

#[test]
fn smul_reports_the_multiplier_overflow() {
    // FRCT = 1, SMUL = 1, SATD = 1: 0x8000 x 0x8000 saturates the product to
    // 0x7FFF FFFF and reports
    let mut cpu = machine("smul_reports_the_multiplier_overflow");
    run(&mut cpu);
    assert_eq!((cpu.st.ac[0], cpu.st.acov[0]), (0x7FFFFFFF, 1));
}

#[test]
fn unsigned_product_reports_nothing() {
    let mut cpu = machine("unsigned_product_reports_nothing");
    write(&mut cpu, 0x1600, 0xFFFF);
    run(&mut cpu);
    assert_eq!(cpu.st.ac[0], 0xFFFE0001);
    assert_eq!(cpu.st.acov, [0, 0, 0, 0]);
}

#[test]
fn step_and_run_agree_on_mac() {
    let mut a = machine("step_and_run_agree_on_mac");
    a.st.ac[0] = 0x7FFFFFFF;
    run(&mut a);
    let mut b = machine("step_and_run_agree_on_mac");
    b.st.ac[0] = 0x7FFFFFFF;
    for _ in 0..3 {
        step(&mut b);
    }
    assert_eq!(a.st.ac, b.st.ac);
}
