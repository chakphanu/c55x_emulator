//! Instruction semantics on small programs (from tests/test_c55x_emu.py SemanticTests and
//! tests/test_c55x_emu_audio.py HiLoadTests). Programs: rust/fixtures/semantics.

mod common;

use c55x_core::core::Core;
use common::{read, read32, run, try_run, write, write32};

/// A core at the program t_NAME of the fixture.
fn machine(name: &str) -> Core {
    fixture!("semantics").core(&format!("t_{}", name))
}

#[test]
fn sign_and_zero_extension() {
    let mut cpu = machine("sign_and_zero_extension");
    write(&mut cpu, 0x1600, 0x8000);
    run(&mut cpu);
    assert_eq!(cpu.st.ac[0], 0xFFFFFF8000);
    assert_eq!(cpu.st.ac[1], 0x8000);
}

#[test]
fn long_constant_and_logical_shift() {
    // asm55 has no 'AC1 = AC0 >>> #1' (one register on both sides): the program uses
    // the two-register logical shift 'AC1 = AC0 <<< #-1'
    let mut cpu = machine("long_constant_and_logical_shift");
    run(&mut cpu);
    assert_eq!(cpu.st.ac[0], 0xFFFFFFFFFF);
    assert_eq!(cpu.st.ac[1], 0x7FFFFFFF);
}

#[test]
fn post_increment_of_long_access_steps_two_words() {
    let mut cpu = machine("post_increment_of_long_access_steps_two_words");
    write32(&mut cpu, 0x1600, 0x12345678);
    run(&mut cpu);
    assert_eq!((cpu.st.ac[0], cpu.st.xar[2]), (0x12345678, 0x1602));
}

#[test]
fn ar_write_keeps_the_high_part() {
    let mut cpu = machine("ar_write_keeps_the_high_part");
    run(&mut cpu);
    assert_eq!(cpu.st.xar[3], 0x010005);
}

#[test]
fn dual_push_pop_order() {
    let mut cpu = machine("dual_push_pop_order");
    run(&mut cpu);
    assert_eq!((cpu.st.t[0], cpu.st.t[1], cpu.st.xsp), (3, 2, 0x1000));
    assert_eq!((read(&mut cpu, 0x0FFE), read(&mut cpu, 0x0FFF)), (3, 2));
}

#[test]
fn call_and_return_restore_the_stacks() {
    let mut cpu = machine("call_and_return_restore_the_stacks");
    run(&mut cpu);
    assert_eq!((cpu.st.t[0], cpu.st.t[1], cpu.st.xsp, cpu.st.ssp, cpu.st.reta), (1, 7, 0x1000, 0x0800, 0));
}

#[test]
fn repeat_and_localrepeat_counts() {
    let mut cpu = machine("repeat_and_localrepeat_counts");
    run(&mut cpu);
    assert_eq!((cpu.st.t[0], cpu.st.t[1]), (8, 8));
}

#[test]
fn goto_out_of_a_loop_ends_it() {
    // asm55 needs two instructions in a localrepeat body: the second loop adds a nop
    let mut cpu = machine("goto_out_of_a_loop_ends_it");
    run(&mut cpu);
    assert_eq!((cpu.st.t[0], cpu.st.t[1], cpu.st.loop_active), (3, 5, [0, 0]));
}

#[test]
fn conditional_execute() {
    let mut cpu = machine("conditional_execute");
    run(&mut cpu);
    assert_eq!((cpu.st.t[1], cpu.st.t[2]), (128, 0));
}

#[test]
fn parallel_parts_read_before_writing() {
    let mut cpu = machine("parallel_parts_read_before_writing");
    run(&mut cpu);
    assert_eq!((cpu.st.ac[0], cpu.st.ac[1]), (0x10000, 6));
}

#[test]
fn parallel_mar_runs_in_the_address_phase() {
    let mut cpu = machine("parallel_mar_runs_in_the_address_phase");
    run(&mut cpu);
    assert_eq!((read32(&mut cpu, 0x1700), cpu.st.xar[3]), (0x160A, 0x160A));
}

#[test]
fn parallel_mar_leaves_the_partner_address_alone() {
    let mut cpu = machine("parallel_mar_leaves_the_partner_address_alone");
    write(&mut cpu, 0x1602, 7);
    write(&mut cpu, 0x1606, 9);
    run(&mut cpu);
    assert_eq!((cpu.st.xar[0] & 0xFFFF, cpu.st.xar[5]), (7, 0x1604));
}

#[test]
fn standalone_conditional_execute_skips_the_next_instruction() {
    // 'TC1 = #0' is spelled as asm55 / dis55 have it: bit(ST0, #13) = #0
    let mut cpu = machine("standalone_conditional_execute_skips_the_next_instruction");
    run(&mut cpu);
    assert_eq!((cpu.st.t[0], cpu.st.t[1]), (1, 3));
}

#[test]
fn multiply_uses_the_high_part_of_an_accumulator() {
    let mut cpu = machine("multiply_uses_the_high_part_of_an_accumulator");
    run(&mut cpu);
    assert_eq!(cpu.st.ac[1], 36);
}

#[test]
fn long_result_keeps_its_width() {
    let mut cpu = machine("long_result_keeps_its_width");
    write(&mut cpu, 0x1600, 0x449B);
    run(&mut cpu);
    assert_eq!(cpu.st.ac[0], 0x449B00);
}

#[test]
fn field_extract() {
    let mut cpu = machine("field_extract");
    run(&mut cpu);
    assert_eq!(cpu.st.ac[2], 0x123);
}

#[test]
fn unsupported_mode_raises() {
    let mut cpu = machine("unsupported_mode_raises");     // C54CM
    assert!(try_run(&mut cpu).is_err());
}

#[test]
fn hi_takes_the_sign_into_the_guard_bits() {             // SPRU375E 4.5.3
    // the low half is cleared: cl55 builds a long as HI(ACx) = hi; ACx |= lo
    let mut cpu = machine("hi_takes_the_sign_into_the_guard_bits");
    run(&mut cpu);
    assert_eq!((cpu.st.ac[0], cpu.st.ac[1]), (0x00030000, 0xFFFFFE0000));
}
