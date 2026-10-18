"""C55x emulator audio path: DMA controller and McBSP0 transmitter of the Rust core
(rust/c55x_core/src/dma.rs, mcbsp.rs) driven by small programs through I/O ports, the
AIC23 register decoding (codec.py) and fw/hello_audio end to end."""

import struct
import unittest

from c55x_emu.paths import ROOT
from c55x_emu.codec import Codec
from c55x_emu.registers import EmulationError
from c55x_emu.testutil import machine, run

HELLO_AUDIO = ROOT / "build" / "hello_audio" / "hello_audio.out"

DMA0, DMA5 = 0x0C00, 0x0C00 + 5 * 0x20
MCBSP0 = 0x2800
CSDP, CCR, CICR, CSR, CSSA_L, CSSA_U, CDSA_L, CDSA_U, CEN, CFN, CFI, CEI = range(12)


def port_writes(base, pairs):
    return [f"*port(#0x{base + r:04x}) = #{v}" for r, v in pairs]


class DmaTests(unittest.TestCase):
    def test_block_copy_raises_the_channel_interrupt(self):
        prog = port_writes(DMA5, ((CSDP, 1 | 1 << 2 | 1 << 9), (CICR, 0x20), (CSSA_L, 0x6000),
                                  (CSSA_U, 0), (CDSA_L, 0x8000), (CDSA_U, 0), (CEN, 4), (CFN, 1),
                                  (CCR, 0x80 | 1 << 12 | 1 << 14)))  # EN, post increment both
        prog += [f"AR1 = *port(#0x{DMA5 + CSR:04x})", f"AR2 = *port(#0x{DMA5 + CSR:04x})",
                 f"AR3 = *port(#0x{DMA5 + CCR:04x})"]
        cpu, bus = machine(*prog)
        for i in range(4):
            bus.write(0x3000 + i, 0x100 + i)
        run(cpu)
        self.assertEqual([bus.read(0x4000 + i) for i in range(4)], [0x100, 0x101, 0x102, 0x103])
        self.assertTrue(cpu.ifr0 & (1 << 15))                    # DMAC5 = interrupt 15
        self.assertEqual([cpu.xar[i] & 0xFFFF for i in (1, 2)], [0x20, 0])   # BLOCK, read clears
        self.assertFalse(cpu.xar[3] & 0x80)                      # EN cleared at the block end

    def test_double_index_interleaves_two_buffers(self):     # audio_dma_program, SPRU587E
        diff = 0x200                                         # right - left buffer, bytes
        prog = port_writes(DMA5, ((CSDP, 1 | 1 << 2 | 1 << 9), (CSSA_L, 0x6000), (CDSA_L, 0x8000),
                                  (CEN, 2), (CFN, 4), (CEI, diff - 1), (CFI, (1 - diff) & 0xFFFF),
                                  (CCR, 0x80 | 3 << 12 | 1 << 14)))
        cpu, bus = machine(*prog)
        for i in range(4):
            bus.write(0x3000 + i, 0x100 + i)                 # left
            bus.write(0x3100 + i, 0x200 + i)                 # right
        run(cpu)
        self.assertEqual([bus.read(0x4000 + i) for i in range(8)],
                         [0x100, 0x200, 0x101, 0x201, 0x102, 0x202, 0x103, 0x203])


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


class HiLoadTests(unittest.TestCase):
    def test_hi_takes_the_sign_into_the_guard_bits(self):   # SPRU375E 4.5.3
        # the low half is cleared: cl55 builds a long as HI(ACx) = hi; ACx |= lo
        cpu, _ = machine("AC0 = #-5", "AR1 = #3", "HI(AC0) = AR1", "AC1 = #5", "AR2 = #-2",
                         "HI(AC1) = AR2")
        run(cpu)
        self.assertEqual((cpu.ac[0], cpu.ac[1]), (0x00030000, 0xFFFFFE0000))


def codec(*writes):
    c = Codec()
    for reg, value in writes:
        word = reg << 9 | value
        c.end_write([word >> 8, word & 0xFF])
    return c


class CodecTests(unittest.TestCase):
    def test_usb_mode_rates(self):                       # aic23_set_rate values
        for r8, hz in ((0x023, 12e6 / 272), (0x001, 48000), (0x019, 32000), (0x00D, 8000),
                       (0x01D, 96000), (0x063, 6e6 / 272)):
            self.assertAlmostEqual(codec((8, r8)).sample_rate(), hz, msg=hex(r8))

    def test_clock_needs_master_and_active(self):
        self.assertEqual(codec((7, 0x043), (8, 0x023)).clock(), 0)          # inactive
        self.assertEqual(codec((7, 0x003), (8, 0x023), (9, 1)).clock(), 0)  # slave
        self.assertAlmostEqual(codec((7, 0x043), (8, 0x023), (9, 1)).clock(), 12e6 / 272)
        with self.assertRaises(EmulationError):
            codec((7, 0x042), (8, 0x023), (9, 1)).clock()                 # I2S not modeled

    def test_reset(self):
        c = codec((9, 1), (15, 0))
        self.assertEqual(c.describe(), "codec inactive")

    def test_headphone_gain(self):                      # aic23_hp_volume: reg 2 with LRS
        playing = ((5, 0x000), (6, 0x067))              # unmuted, DAC and output on
        self.assertEqual(codec(*playing, (2, 0x179)).gains(), (1.0, 1.0))        # 0 dB, both
        left, right = codec(*playing, (2, 0x1E2)).gains()                        # 98: -23 dB
        self.assertAlmostEqual(left, 10 ** (-23 / 20))
        self.assertEqual(left, right)
        self.assertEqual(codec(*playing, (2, 0x050)).gains()[0], 10 ** (-41 / 20))   # no LRS: left only
        self.assertEqual(codec(*playing, (2, 0x050)).gains()[1], 1.0)            # reset value 0x079
        self.assertEqual(codec(*playing, (2, 0x130)).gains(), (0.0, 0.0))        # 48: mute
        self.assertEqual(codec((5, 0x008), (6, 0x067)).gains(), (0.0, 0.0))      # soft mute
        self.assertEqual(codec((5, 0x000), (6, 0x077)).gains(), (0.0, 0.0))      # output off


@unittest.skipUnless(HELLO_AUDIO.exists(), "fw/build.sh hello_audio not run")
class HelloAudioTests(unittest.TestCase):
    def test_one_kilohertz_tone(self):
        from c55x_emu.machine import Machine
        m = Machine(str(HELLO_AUDIO))
        segments = []

        class Sink:
            def feed(self, s):
                segments.extend(s)
        m.audio_out = Sink()
        m.run(10**12, until_seconds=0.6)
        (rate, data), = segments
        s = struct.unpack(f"<{len(data) // 2}h", data)
        left, right = s[0::2], s[1::2]
        part = left[int(rate * 0.1):int(rate * 0.5)]                  # 0.4 s
        crossings = sum(1 for a, b in zip(part, part[1:]) if a < 0 <= b)
        self.assertAlmostEqual(rate, 12e6 / 272)
        self.assertAlmostEqual(crossings, 400, delta=2)            # 1000.4 Hz
        self.assertEqual(right, tuple(-v for v in left))
        self.assertAlmostEqual(max(left), 8000, delta=100)
        self.assertEqual(m.cpu.core.stats()["mcbsp_underruns"][0], 0)


if __name__ == "__main__":
    unittest.main()
