"""Audio codec on I2C address 0x1a: TLV320AIC23B (src/drivers/aic23.c uses its register
map). A 2-byte message is (register << 9 | value) high byte first; register 15 resets.

The digital audio interface (TLV320AIC23B datasheet section 3.1.1): register 7 bit 6
master (the codec drives the bit clock and frame sync, as on the Zen Touch), bits 1-0
format (3 = DSP: frame sync, then the left and the right word), bits 3-2 input word
length (0 = 16 bit); register 9 bit 0 activates the interface. Register 8 selects the
rate; in USB mode (bit 0, MCLK 12 MHz, section 3.3.2.1) the DAC rate is 12 MHz divided by
USB_DIVISORS[SR3..SR0, BOSR] (44.1 kHz is really 44.118 kHz), bit 6 (CLKIN) halves it.
clock() -> the sample rate the McBSP is clocked with (rust/c55x_core/src/mcbsp.rs), 0
while the codec does not drive the clock. Normal mode (other MCLK) is not modeled.

Headphone output (registers 2 / 3, LHV / RHV): code 121 = 0 dB, 1 dB per step up to
127 = +6 dB, 48 and below mute; LRS (bit 8) writes both channels (aic23_hp_volume uses
it). gains() -> the linear left / right gain of the analog output, 0 while the DAC is soft
muted (register 5 bit 3), or the DAC or the output stage is powered down (register 6 bits
3 / 4). The emulator applies it to the live output (qt_audio.py); the McBSP stream itself
(and a WAV recording of it) is the digital signal before the codec.
"""

import collections

from .registers import EmulationError

ADDR = 0x1A
MCLK_USB = 12_000_000
# (SR3..SR0, BOSR) -> MCLK divisor of the DAC rate, USB mode table of the datasheet
USB_DIVISORS = {(0b0111, 0): 125, (0b1111, 1): 136, (0b0000, 0): 250, (0b1000, 1): 272,
                (0b0110, 0): 375, (0b1011, 1): 1496, (0b0011, 0): 1500, (0b0001, 0): 1500,
                (0b1001, 1): 1496, (0b0010, 0): 250, (0b1010, 1): 272}
FORMATS = {0: "right aligned", 1: "left aligned", 2: "I2S", 3: "DSP"}
# reset defaults (datasheet register map): headphone 0 dB, DAC soft mute on, ...
LRS, HP_0DB, HP_MUTE = 0x100, 121, 48   # headphone registers: both channels, 0 dB, mute code
DACM, PD_DAC, PD_OUT = 0x08, 0x08, 0x10  # register 5 DAC soft mute, register 6 power down bits
RESET_VALUES = {2: 0x079, 3: 0x079, 5: 0x008, 6: 0x007, 7: 0x001, 8: 0x020, 9: 0x000}


class Codec:
    def __init__(self):
        self.regs = dict(RESET_VALUES)
        self.log = collections.deque(maxlen=100)
        self.bad = collections.Counter()

    def begin_write(self):
        pass

    def end_write(self, data):
        if len(data) != 2:
            self.bad[f"{len(data)}-byte write"] += 1
            return
        word = (data[0] << 8) | data[1]
        reg, value = word >> 9, word & 0x1FF
        if reg == 15:
            self.regs = dict(RESET_VALUES)
        self.regs[reg] = value
        if reg in (2, 3) and value & LRS:
            self.regs[5 - reg] = value      # simultaneous update of the other channel
        self.log.append(f"reg {reg} = 0x{value:03x}")

    def read(self, count):
        return []                       # write-only device: NACK

    def sample_rate(self):
        r8 = self.regs[8]
        if not r8 & 1:
            raise EmulationError(f"AIC23 normal clock mode (register 8 = 0x{r8:03x}) not modeled")
        key = ((r8 >> 2) & 0xF, (r8 >> 1) & 1)
        if key not in USB_DIVISORS:
            raise EmulationError(f"AIC23 register 8 = 0x{r8:03x}: no USB-mode rate")
        rate = MCLK_USB / USB_DIVISORS[key]
        return rate / 2 if r8 & 0x40 else rate

    def clock(self):
        """Sample rate driven on the interface, 0 when inactive or slave."""
        r7 = self.regs[7]
        if not self.regs[9] & 1 or not r7 & 0x40:
            return 0
        if r7 & 3 != 3 or (r7 >> 2) & 3 != 0:
            raise EmulationError(f"AIC23 interface {FORMATS[r7 & 3]}, word length code "
                                 f"{(r7 >> 2) & 3} not modeled (DSP format, 16 bit only)")
        return self.sample_rate()

    def gains(self):
        """(left, right) linear gain of the headphone output."""
        r = self.regs
        if r[5] & DACM or r[6] & (PD_DAC | PD_OUT):
            return 0.0, 0.0
        return tuple(0.0 if r[n] & 0x7F <= HP_MUTE else 10 ** (((r[n] & 0x7F) - HP_0DB) / 20) for n in (2, 3))

    def describe(self):
        """Short state for the front ends."""
        r = self.regs
        if not r[9] & 1:
            return "codec inactive"
        muted = r[5] & 0x08
        out_off = r[6] & 0x10
        vol = r[2] & 0x7F
        return (f"{self.sample_rate() / 1000:.3f} kHz, headphone {vol - 121:+d} dB"
                + (", DAC muted" if muted else "") + (", output off" if out_off else ""))
