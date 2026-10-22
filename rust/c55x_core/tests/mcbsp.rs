//! McBSP0 transmitting an audio stream fed by DMA (from the McBspStreamTests of
//! tests/test_c55x_emu_audio.py). Program: rust/fixtures/mcbsp. The audio clock the
//! Python tests set as a callable is plain data on the native host (set_audio_clock).

mod common;

use c55x_core::core::Core;
use c55x_core::native::NativeHost;
use common::{try_run_until, write};

const FRAMES: usize = 8;                                // DMA block: 8 stereo frames

fn machine(name: &str, rate_hz: f64) -> Core {
    let mut cpu = fixture!("mcbsp").core(&format!("t_{}", name));
    NativeHost::of(&mut cpu).unwrap().set_audio_clock(rate_hz, 108.0);
    cpu
}

/// The samples McBSP0 sent since the last call (Core.take_audio).
fn take_audio(cpu: &mut Core) -> Vec<(f64, Vec<i16>)> {
    cpu.audio_samples = 0;
    std::mem::take(&mut cpu.audio)
}

/// McBSP0 like mcbsp_config(0, 1, 1, 2) (16 bit, 2 words per frame, external clock), DMA
/// channel 0 on XEVT0 with auto-init and repeat over a table of FRAMES frames; 10 ms.
fn stream() -> (Core, Vec<u32>) {
    let mut cpu = machine("stream", 44100.0);
    let table: Vec<u32> = (0..2 * FRAMES as i64).map(|i| ((i * 1000 - 8000) & 0xFFFF) as u32).collect();
    for (i, &v) in table.iter().enumerate() {
        write(&mut cpu, 0x3000 + i as u32, v);
    }
    try_run_until(&mut cpu, 10_000, 108_000_000 / 100).unwrap();
    (cpu, table)
}

#[test]
fn samples_follow_the_table() {
    let (mut cpu, table) = stream();
    let audio = take_audio(&mut cpu);
    assert_eq!(audio.len(), 1);
    let (rate, data) = &audio[0];
    let samples: Vec<u32> = data.iter().map(|&v| v as u16 as u32).collect();
    assert_eq!(*rate, 44100.0);
    assert!(samples.len() > 400 * 2, "{} samples", samples.len());    // ~441 frames in 10 ms, batched
    assert!(samples.iter().enumerate().all(|(i, &s)| s == table[i % table.len()]));
    assert_eq!(cpu.mcbsp.iter().map(|m| m.underruns).collect::<Vec<_>>(), [0, 0]);
}

#[test]
fn block_interrupts_and_rate() {
    let (cpu, _) = stream();
    let blocks = cpu.dma.blocks[0] as f64;
    assert!(cpu.st.ifr1 & (1 << 2) != 0);               // DMAC0 = interrupt 18
    assert!((blocks - 441.0 / FRAMES as f64).abs() <= 3.0, "{} blocks", blocks);
}

#[test]
fn no_clock_no_samples() {
    let mut cpu = machine("no_clock_no_samples", 0.0);  // codec inactive
    try_run_until(&mut cpu, 10_000, 1_080_000).unwrap();
    assert!(take_audio(&mut cpu).is_empty());
}
