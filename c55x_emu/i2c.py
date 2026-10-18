"""I2C master of the VC5509A (I/O ports 0x3c00..0x3c0c), modeled at transaction level.

Bytes move instantly: XRDY/ARDY are set as soon as a byte is written, received bytes are
available at once (RRDY). Slaves implement begin_write(), end_write(bytes) and
read(count) -> bytes; an absent address answers NACK. Register sequences follow
src/drivers/iic.c (MDR 0x76a0 start/transmit, 0x5ea0 stop, 0x7c20 start/receive/stop).
"""

import collections

BASE = 0x3C00
OAR, IER, STR, CLKL, CLKH, CNT, DRR, SAR, DXR, MDR, PSC = 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0xC
STR_NACK, STR_ARDY, STR_RRDY, STR_XRDY, STR_BB = 1 << 1, 1 << 2, 1 << 3, 1 << 4, 1 << 12
MDR_STT, MDR_STP, MDR_MST, MDR_TRX, MDR_IRS = 1 << 13, 1 << 11, 1 << 10, 1 << 9, 1 << 5


class I2c:
    def __init__(self, bus):
        self.slaves = {}
        self.regs = {r: 0 for r in (OAR, IER, CLKL, CLKH, CNT, SAR, PSC)}
        self.mdr = 0
        self.nack = self.ardy = self.busy = False
        self.target = None              # slave of the open write message
        self.tx = []
        self.rx = []
        self.anomalies = collections.Counter()       # message -> count
        for r in range(0x0D):
            bus.hook_port(BASE + r, self.read, self.write)

    def _anomaly(self, text):
        self.anomalies[text] += 1

    def attach(self, addr, slave):
        self.slaves[addr] = slave

    def status(self):
        v = STR_XRDY
        v |= STR_NACK if self.nack else 0
        v |= STR_ARDY if self.ardy else 0
        v |= STR_RRDY if self.rx else 0
        v |= STR_BB if self.busy else 0
        return v

    def read(self, port):
        r = port - BASE
        if r == STR:
            return self.status()
        if r == MDR:
            return self.mdr
        if r == DRR:
            if not self.rx:
                self._anomaly("DRR read without data")
                return 0
            b = self.rx.pop(0)
            if not self.rx:
                self._release()
            return b
        return self.regs.get(r, 0)

    def write(self, port, value):
        r = port - BASE
        if r == MDR:
            self._control(value)
        elif r == DXR:
            if self.target is None:
                self._anomaly(f"DXR write 0x{value:02x} outside a write message")
            else:
                self.tx.append(value & 0xFF)
            self.ardy = True
        elif r == STR:
            if value & STR_NACK:
                self.nack = False
            if value & STR_ARDY:
                self.ardy = False
        elif r in self.regs:
            self.regs[r] = value
        else:
            self._anomaly(f"write 0x{value:04x} to register 0x{port:04x}")

    def _release(self):
        self.busy = False
        self.mdr &= ~(MDR_MST | MDR_STP | MDR_STT)

    def _control(self, v):
        self.mdr = v
        if not v & MDR_IRS:             # module in reset
            self.nack = self.ardy = self.busy = False
            self.target, self.tx, self.rx = None, [], []
            return
        if v & MDR_STT:
            self._start(v)
        elif v & MDR_STP:
            self._stop()

    def _start(self, v):
        if self.target is not None:     # repeated start: close the open message first
            self._stop()
        slave = self.slaves.get(self.regs[SAR] & 0x7F)
        self.busy = True
        if slave is None:
            self.nack = True
            return
        if v & MDR_TRX:
            self.target, self.tx = slave, []
            slave.begin_write()
            return
        count = self.regs[CNT] or 0x10000
        self.rx = list(slave.read(count))[:count]
        if not self.rx:
            self.nack = True
            self._release()

    def _stop(self):
        if self.target is not None:
            target, data = self.target, self.tx
            self.target, self.tx = None, []
            target.end_write(data)
        self._release()
