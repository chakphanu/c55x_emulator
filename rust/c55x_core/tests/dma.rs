//! The DMA controller of the core driven by small programs through I/O ports (from the
//! DmaTests of tests/test_c55x_emu_audio.py). Programs: rust/fixtures/dma.

mod common;

use c55x_core::core::Core;
use common::{read, run, write};

fn machine(name: &str) -> Core {
    fixture!("dma").core(&format!("t_{}", name))
}

#[test]
fn block_copy_raises_the_channel_interrupt() {
    let mut cpu = machine("block_copy_raises_the_channel_interrupt");
    for i in 0..4 {
        write(&mut cpu, 0x3000 + i, 0x100 + i);
    }
    run(&mut cpu);
    assert_eq!((0..4).map(|i| read(&mut cpu, 0x4000 + i)).collect::<Vec<_>>(), [0x100, 0x101, 0x102, 0x103]);
    assert!(cpu.st.ifr0 & (1 << 15) != 0);              // DMAC5 = interrupt 15
    assert_eq!([1, 2].map(|i| cpu.st.xar[i] & 0xFFFF), [0x20, 0]);     // BLOCK, read clears
    assert!(cpu.st.xar[3] & 0x80 == 0);                 // EN cleared at the block end
}

#[test]
fn double_index_interleaves_two_buffers() {             // audio_dma_program, SPRU587E
    let mut cpu = machine("double_index_interleaves_two_buffers");
    for i in 0..4 {
        write(&mut cpu, 0x3000 + i, 0x100 + i);         // left
        write(&mut cpu, 0x3100 + i, 0x200 + i);         // right
    }
    run(&mut cpu);
    assert_eq!((0..8).map(|i| read(&mut cpu, 0x4000 + i)).collect::<Vec<_>>(),
               [0x100, 0x200, 0x101, 0x201, 0x102, 0x202, 0x103, 0x203]);
}
