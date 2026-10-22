"""C55x emulator audio path: McBSP0 transmitter of the Rust core (rust/c55x_core/src/
mcbsp.rs) driven by small programs through I/O ports (DMA: rust/c55x_core/tests/dma.rs).
HI(ACx) loads: rust/c55x_core/tests/semantics.rs."""

import struct
import unittest

from c55x_emu.testutil import machine, run

DMA0, DMA5 = 0x0C00, 0x0C00 + 5 * 0x20
MCBSP0 = 0x2800
CSDP, CCR, CICR, CSR, CSSA_L, CSSA_U, CDSA_L, CDSA_U, CEN, CFN, CFI, CEI = range(12)


def port_writes(base, pairs):
    return [f"*port(#0x{base + r:04x}) = #{v}" for r, v in pairs]


class McBspStreamTests(unittest.TestCase):
    FRAMES = 8                                               # DMA block: 8 stereo frames

    def stream(self):
        """McBSP0 like mcbsp_config(0, 1, 1, 2) (16 bit, 2 words per frame, external clock),
        DMA channel 0 on XEVT0 with auto-init and repeat over a table of FRAMES frames."""
        prog = ["bit(ST1, #11) = #0"]
        prog += port_writes(MCBSP0, ((9, 0x140), (8, 0), (0x12, 0x4003), (4, 0x100)))
        prog += port_writes(DMA0, ((CSDP, 0x601), (CICR, 0x20), (CSSA_L, 0x6000), (CDSA_L, 2 * (MCBSP0 + 3)),
                                   (CEN, 2), (CFN, self.FRAMES), (CCR, 0x80 | 2 | 0x100 | 0x200 | 1 << 12)))
        prog += [f"*port(#0x{MCBSP0 + 4:04x}) = #{0x101}"]      # XRST: transmitter on
        cpu, bus = machine(*prog)
        cpu.audio_clock = lambda: (44100.0, 108.0)
        table = [(i * 1000 - 8000) & 0xFFFF for i in range(2 * self.FRAMES)]
        for i, v in enumerate(table):
            bus.write(0x3000 + i, v)
        cpu.run(10_000, until=108_000_000 // 100)         # 10 ms
        return cpu, table

    def test_samples_follow_the_table(self):
        cpu, table = self.stream()
        (rate, data), = cpu.core.take_audio()
        samples = [v & 0xFFFF for v in struct.unpack(f"<{len(data) // 2}h", data)]
        self.assertEqual(rate, 44100.0)
        self.assertGreater(len(samples), 400 * 2)                # ~441 frames in 10 ms, batched
        self.assertEqual(samples, [table[i % len(table)] for i in range(len(samples))])
        self.assertEqual(cpu.core.stats()["mcbsp_underruns"], [0, 0])

    def test_block_interrupts_and_rate(self):
        cpu, _ = self.stream()
        blocks = cpu.core.stats()["dma_blocks"][0]
        self.assertTrue(cpu.ifr1 & (1 << 2))                     # DMAC0 = interrupt 18
        self.assertAlmostEqual(blocks, 441 / self.FRAMES, delta=3)

    def test_no_clock_no_samples(self):
        prog = port_writes(MCBSP0, ((9, 0x140), (0x12, 0x4003))) + [f"*port(#0x{MCBSP0 + 4:04x}) = #1"]
        cpu, _ = machine("bit(ST1, #11) = #0", *prog)
        cpu.audio_clock = lambda: (0, 108.0)                     # codec inactive
        cpu.run(10_000, until=1_080_000)
        self.assertEqual(cpu.core.take_audio(), [])


if __name__ == "__main__":
    unittest.main()
