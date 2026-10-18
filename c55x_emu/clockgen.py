"""Clock generators CLKMD (I/O 0x1c00) and USBDPLL (0x1e00): the PLLs lock at once.

Bits as used by set_cpu_clock (src/hw/clock.c, src/include/zen_hw.h, SPRU317K 3.8):
LOCK bit 0 (read only), PLL ENABLE bit 4, PLL MULT bits 11-7, PLL DIV bits 6-5,
BYPASS DIV bits 3-2. The emulator counts one cycle per instruction; the frequency (12 MHz
input, see clock.c) converts cycles to emulated seconds (Machine.seconds), on_change is
called before CLKMD changes. USBDPLL is used with
the same LOCK / PLL ENABLE bits by usb_pll_init (src/hw/chip.c); USB itself is not modeled.
"""

CLKMD, USBDPLL = 0x1C00, 0x1E00
LOCK, PLLENABLE = 1 << 0, 1 << 4
CLKIN_MHZ = 12


class ClockGen:
    def __init__(self, bus, port=CLKMD, on_change=None):
        self.clkmd = 0
        self.on_change = on_change
        bus.hook_port(port, self.read, self.write)

    def read(self, port):
        return (self.clkmd & ~LOCK) | (LOCK if self.clkmd & PLLENABLE else 0)

    def write(self, port, value):
        if self.on_change:
            self.on_change()
        self.clkmd = value & ~LOCK

    def mhz(self):
        if self.clkmd & PLLENABLE:
            return CLKIN_MHZ * ((self.clkmd >> 7) & 31) / (((self.clkmd >> 5) & 3) + 1)
        return CLKIN_MHZ / (((self.clkmd >> 2) & 3) + 1)
