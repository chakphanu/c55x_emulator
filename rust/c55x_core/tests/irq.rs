//! Interrupt dispatch (SPRU371F 4.4.2, 5.2), from the DispatchTests of
//! tests/test_c55x_emu_irq.py. Program: rust/fixtures/irq (main loop t_main, service
//! routines t_isr4 / t_isr16); the tests write the vectors into the IVPD / IVPH pages.

mod common;

use c55x_core::core::{Core, MEM_BYTES};
use c55x_core::icache::PcFilter;
use common::{message, read, step};

const ST1_INTM: u32 = 1 << 11;

fn sym(name: &str) -> u32 {
    fixture!("irq").sym(name)
}

/// The address after the instruction at addr.
fn next(addr: u32) -> u32 {
    addr + fixture!("irq").insns[&addr].size
}

/// SPRU371F 5.2: IVPD (or IVPH for 16..23) as bits 23..8, n * 8 as bits 7..0.
fn vector(cpu: &Core, n: u32) -> u32 {
    let base = if (16..=23).contains(&n) { cpu.st.ivph } else { cpu.st.ivpd };
    ((base << 8) | (n << 3)) & 0xFFFFFF
}

fn isr_address(cpu: &Core, n: u32) -> u32 {
    let v = vector(cpu, n);
    (1..4).fold(0, |a, i| (a << 8) | cpu.mem_byte(v + i) as u32)
}

/// Vector format of SPRU371F 5.2: byte 0 ignored, bytes 1..3 ISR address.
fn set_vector(cpu: &mut Core, n: u32, isr: u32) {
    let v = vector(cpu, n);
    let mut bytes = vec![0xEA, (isr >> 16) as u8, (isr >> 8) as u8, isr as u8];
    bytes.extend([0x5E, 0x80, 0x5F, 0x80]);
    assert!((v as usize) + bytes.len() <= MEM_BYTES);
    cpu.load_bytes(v, &bytes);
}

/// The CPU of the Python setUp: SP 0x1000, SSP 0x0800, INTM = 0, IVPD 0x0300, IVPH 0x0400,
/// IV4 and IV16 set.
fn machine() -> Core {
    let mut cpu = fixture!("irq").core("t_main");
    cpu.st.st1 &= !ST1_INTM;
    cpu.st.ivpd = 0x0300;
    cpu.st.ivph = 0x0400;
    set_vector(&mut cpu, 4, sym("t_isr4"));             // IV4 in the IVPD page
    set_vector(&mut cpu, 16, sym("t_isr16"));           // IV16 in the IVPH page
    cpu
}

#[test]
fn vector_addresses() {
    let cpu = machine();
    assert_eq!((vector(&cpu, 4), vector(&cpu, 16), vector(&cpu, 24)), (0x30020, 0x40080, 0x300C0));
    assert_eq!((isr_address(&cpu, 4), isr_address(&cpu, 16)), (sym("t_isr4"), sym("t_isr16")));
}

#[test]
fn context_is_saved_and_restored() {
    let (main, isr4) = (sym("t_main"), sym("t_isr4"));
    let mut cpu = machine();
    step(&mut cpu);                                     // T0 = #1
    cpu.st.ier0 = 1 << 4;
    cpu.st.tc1 = 1;
    cpu.st.ifr0 |= 1 << 4;                              // raise_interrupt(4)
    let st1 = cpu.st.st1;
    step(&mut cpu);                                     // taken, then T2 = #7 runs
    assert_eq!((cpu.st.pc, cpu.st.ifr0, cpu.st.reta), (next(isr4), 0, next(main)));
    assert!(cpu.st.st1 & ST1_INTM != 0);
    let st2 = cpu.st.st2;
    assert_eq!([1, 2, 3].map(|k| read(&mut cpu, 0x1000 - k)), [st2 & !(1 << 12), st1, 0]);
    assert_eq!(read(&mut cpu, 0x07FF) >> 13 & 1, 1);    // ST0_55 with TC1
    assert_eq!((cpu.st.xsp, cpu.st.ssp), (0x0FFD, 0x07FD));
    step(&mut cpu);                                     // return_int
    assert_eq!((cpu.st.pc, cpu.st.xsp, cpu.st.ssp, cpu.st.st1, cpu.st.t[2]), (next(main), 0x1000, 0x0800, st1, 7));
    step(&mut cpu);
    assert_eq!(cpu.st.t[1], 2);
}

#[test]
fn priority_and_masking() {
    let mut cpu = machine();
    cpu.st.ier0 = 1 << 4;
    cpu.st.ier1 = 1;
    cpu.st.ifr0 |= 1 << 4;
    cpu.st.ifr1 |= 1;                                   // raise_interrupt(16)
    cpu.st.st1 |= ST1_INTM;
    step(&mut cpu);
    assert_eq!(cpu.st.pc, next(sym("t_main")));         // INTM = 1: nothing taken
    cpu.st.st1 &= !ST1_INTM;
    step(&mut cpu);
    assert_eq!(cpu.st.pc, next(sym("t_isr16")));        // IV16 has priority 5, IV4 priority 7
    assert_eq!((cpu.st.ifr0, cpu.st.ifr1), (1 << 4, 0));
}

#[test]
fn disabled_interrupt_stays_pending() {
    let mut cpu = machine();
    cpu.st.ifr0 |= 1 << 4;
    cpu.run(10, &PcFilter::default(), None).map_err(message).unwrap();
    assert_eq!((cpu.st.ifr0, cpu.st.t[2]), (1 << 4, 0));
}
