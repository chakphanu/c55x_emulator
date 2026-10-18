"""Model of the MCU (I2C slave 7) as the main program uses it (src/drivers/mcu_*.c).

- Every received byte is its own message. Bytes 0..3 select a register (mcu_read_reg),
  5, 6 and 36 take one argument byte (mcu_send(2, ...)); everything else is a command.
- After each message the MCU raises interrupt 16 (IFR1 bit 0), the ready signal that
  mcu_send / mcu_read_reg poll for.
- A 2-byte read returns the selected register (high byte first), a 1-byte read the oldest
  queued event byte, 0x00 if none (mcu_event ignores class 0). Further queued events
  raise the interrupt again.
Meanings of commands, register bits and events are open (open questions M1..M4). Register
defaults: 1 = 0x0800 sets status bit 11, without which mcu_task_battery reports no battery
level (-1) and the battery query of mcu_op1 (request 0) never returns, so the boot stops
at the splash; 0 = 0xe664: high byte 230 (battery level, 209..243 = 0..100 percent),
low byte 100 (class 25, the normal class). Both are assumptions, not measured values.

Power: command 27 ends the firmware's suspend sequence (mcu_op0 op 5: ... 30, 27); the
MCU is then off and the DSP waits in power_down_idle. Only a Power key press is passed
on while off: it sets the wake reason in register 2 and raises the interrupt. The
firmware reads register 2 after resume command 54 (mcu_reg2_state) and suspends again at
once if it is 0; any of the decoded reasons keeps it on. WAKE_POWER_KEY (high byte bit 3,
state 1) is an assumption: which reason the real MCU reports for the key is not known.
Other events while off are ignored (logged).
"""

import collections

ADDR = 7
REGISTERS = (0, 1, 2, 3)
ARG_COMMANDS = (5, 6, 36)
CMD_SUSPEND = 27
POWER_PRESS = 0x10                      # controls.py: Power, class 1 (press)
WAKE_POWER_KEY = 0x0800
DEFAULT_REGS = {0: 0xE664, 1: 0x0800, 2: 0x0000, 3: 0x0000}
# Event classes as mcu_event (src/drivers/mcu_event.c) decodes them; key codes from
# g_mcu_keymap_9b (0x423e9b, classes 1..3) and g_mcu_keymap_ab (0x423eab, classes 4..6).
KEYMAP_9B = (17, 8, 13, 29, 5, 3, 7, 6, 11, 28, 12, 1, 12, 27, 8, 12)
KEYMAP_AB = (13, 4, 5, 3, 7, 6, 16, 22, 18, 26, 12, 12, 12, 12, 12, 12)
CLASSES = {1: "key, type 0", 2: "key, type 1", 3: "key, type 2", 4: "key B, type 0",
           5: "key B, type 1", 6: "key B, type 2", 7: "level (key 15)", 8: "level (key 15)",
           9: "level (key 14)", 10: "level (key 14)", 11: "status b11/0a", 12: "status b8/b9/b12",
           13: "status b14"}


def key_code(event):
    """Key code mcu_event assigns to an event byte, None for non-key classes."""
    cls = event >> 4
    if 1 <= cls <= 3:
        return KEYMAP_9B[event & 15]
    if 4 <= cls <= 6:
        return KEYMAP_AB[event & 15]
    return None


class Mcu:
    def __init__(self, raise_irq, regs=None):
        self.raise_irq = raise_irq
        self.regs = {**DEFAULT_REGS, **(regs or {})}
        self.selected = None
        self.pending_cmd = None
        self.events = collections.deque()
        self.log = collections.deque(maxlen=200)
        self.log_count = 0              # lines ever logged (the deque drops old ones)
        self.commands = []              # (command, argument or None)
        self.off = False                # after CMD_SUSPEND until a Power key press

    def _log(self, line):
        self.log.append(line)
        self.log_count += 1

    def begin_write(self):
        pass

    def end_write(self, data):
        for b in data:
            self._byte(b)
        self.raise_irq()

    def _byte(self, b):
        if self.pending_cmd is not None:
            self.commands.append((self.pending_cmd, b))
            self._log(f"cmd {self.pending_cmd} arg {b}")
            self.pending_cmd = None
        elif b in REGISTERS:
            self.selected = b
            self._log(f"select reg {b}")
        elif b in ARG_COMMANDS:
            self.pending_cmd = b
        else:
            self.commands.append((b, None))
            self._log(f"cmd {b}")
            if b == CMD_SUSPEND:
                self.off = True
                self.regs[2] = 0

    def read(self, count):
        if count == 2 and self.selected is not None:
            v = self.regs[self.selected]
            self._log(f"read reg {self.selected} = 0x{v:04x}")
            return [v >> 8, v & 0xFF]
        if count == 1:
            ev = self.events.popleft() if self.events else 0
            if ev:
                self._log(f"event 0x{ev:02x} read")
            if self.events:
                self.raise_irq()
            return [ev]
        self._log(f"unexpected read of {count} bytes")
        return [0] * count

    def inject(self, event):
        """Queue an event byte (high nibble class, low nibble detail) and signal it."""
        if self.off:
            if event & 0xFF != POWER_PRESS:
                self._log(f"event 0x{event & 0xFF:02x} ignored (off)")
                return
            self.off = False
            self.regs[2] = WAKE_POWER_KEY
            self._log("power key: wake")
        self.events.append(event & 0xFF)
        self._log(f"event 0x{event & 0xFF:02x} queued")
        self.raise_irq()
