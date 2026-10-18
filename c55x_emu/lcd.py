"""LCD controller model (ST7571-like command set, see src/include/zen_lcd.h).

160 columns x 13 pages of 8 rows; a data byte sets 8 vertical pixels (bit 0 = top) at the
current page/column, then the column advances. The panel shows rows 0..103. Orientation
(ADC/SHL reverse) is taken as the firmware's own coordinates: column 0 left, page 0 top
(assumption, to be checked against the original firmware's screens).
Word addresses: command 0x281000, data 0x281001, status 0x283800 (reads 0: never busy).
"""

WIDTH, HEIGHT, PAGES = 160, 104, 13
CMD, DATA, STATUS = 0x281000, 0x281001, 0x283800
TWO_BYTE = {0x48, 0x81}                 # partial duty, contrast: one argument byte follows


class Lcd:
    def __init__(self, bus):
        self.ram = [bytearray(WIDTH) for _ in range(PAGES)]
        self.page = self.column = 0
        self.on = False
        self.contrast = None
        self.pending = None
        self.unknown = []
        self.version = 0                # incremented on every visible change
        bus.hook_data(CMD, DATA + 1, self._read, self._write)
        bus.hook_data(STATUS, STATUS + 1, self._read, self._write_status)

    def _read(self, word):
        return 0

    def _write_status(self, word, value):
        self.unknown.append(("status-write", value))

    def _write(self, word, value):
        if word == DATA:
            if self.page < PAGES and self.column < WIDTH:
                self.ram[self.page][self.column] = value & 0xFF
                self.version += 1
            self.column += 1
            return
        self.command(value & 0xFF)

    def command(self, c):
        if self.pending is not None:
            if self.pending == 0x81:
                self.contrast = c
            self.pending = None
            return
        if c in TWO_BYTE:
            self.pending = c
        elif c & 0xF0 == 0xB0:
            self.page = c & 0x0F
        elif c & 0xF0 == 0x00:
            self.column = (self.column & 0xF0) | (c & 0x0F)
        elif c & 0xF0 == 0x10:
            self.column = (self.column & 0x0F) | ((c & 0x0F) << 4)
        elif c in (0xAE, 0xAF):
            self.on = c == 0xAF
            self.version += 1
        elif c in (0xA1, 0xC8, 0xAB, 0x66, 0xA9, 0xE1) or c & 0xF8 in (0x20, 0x28, 0x50):
            pass                        # orientation, oscillator, power: no visible effect here
        else:
            self.unknown.append(("command", c))

    def pixel(self, x, y):
        return (self.ram[y >> 3][x] >> (y & 7)) & 1

    def rows(self):
        """HEIGHT lists of WIDTH pixels (1 = dark); all 0 while the display is off."""
        if not self.on:
            return [[0] * WIDTH for _ in range(HEIGHT)]
        return [[self.pixel(x, y) for x in range(WIDTH)] for y in range(HEIGHT)]
