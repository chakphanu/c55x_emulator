//! A dropped core frees its memory (from tests/test_c55x_emu_gc.py, where a Python
//! reference cycle once leaked 16 MiB per machine). The cores touch all of their memory,
//! so a leak would show in the resident set. The allocator may keep freed memory for
//! reuse instead of returning it, so the tests check that dropped memory is reused.

mod common;

use std::sync::Mutex;

use c55x_core::core::{Core, MEM_BYTES};
use common::run;

/// The two tests measure the resident set: never at the same time.
static RSS: Mutex<()> = Mutex::new(());

fn rss_mib() -> u64 {
    let status = std::fs::read_to_string("/proc/self/status").unwrap();
    let line = status.lines().find(|l| l.starts_with("VmRSS")).unwrap();
    line.split_whitespace().nth(1).unwrap().parse::<u64>().unwrap() / 1024
}

fn build() -> Core {
    let mut cpu = fixture!("acov").core("t_flags_are_clear_after_reset");     // AC0 = #0
    let page = [0x5Au8; 1 << 16];
    for addr in (0..MEM_BYTES).step_by(page.len()) {
        cpu.load_bytes(addr as u32, &page);
    }
    cpu.st.pc = fixture!("acov").sym("t_flags_are_clear_after_reset");
    run(&mut cpu);
    cpu
}

#[test]
fn dropped_cores_are_collected() {
    let _one = RSS.lock().unwrap_or_else(|e| e.into_inner());
    drop(build());
    let base = rss_mib();
    let cores: Vec<Core> = (0..5).map(|_| build()).collect();
    let five = rss_mib();
    assert!(five >= base + 5 * 16 - 16, "the cores did not touch their memory");
    drop(cores);
    let again: Vec<Core> = (0..5).map(|_| build()).collect();
    assert!(rss_mib() < five + 16, "5 dropped cores were not freed ({} MiB more)", rss_mib() - five);
    drop(again);
}

#[test]
fn rss_does_not_grow_with_the_number_of_cores() {
    let _one = RSS.lock().unwrap_or_else(|e| e.into_inner());
    drop(build());
    let base = rss_mib();
    for _ in 0..40 {                                    // 40 x 16 MiB would be 640 MiB if leaked
        drop(build());
    }
    assert!(rss_mib().saturating_sub(base) < 160);
}
