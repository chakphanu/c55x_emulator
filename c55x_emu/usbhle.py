"""USB connection to an MTP host, emulated above the hardware (Jukebox2 only).

The USB controller (CE1 data 0x282000) is not identified, so the usb0 driver entries
are replaced by Python (machine.hle) and a scripted host talks to the firmware's own MTP
responder (src/mtp/, src/drivers/usb.c):

  usb_op1 (0x80e27b) op 0: receive req.len bytes into req.ptr (returns 1; 0 when the
          host has nothing more to send, the responder then ends the connection)
          op 9: file data of SendObject: req.handle points to the absolute LBAs of the
          8192-byte blocks (one long each), the bytes go straight to the disk as the
          firmware's handler (0x0338f6, USB FIFO -> ATA) writes them; returns 0 (done)
  usb_op2 (0x80e176) op 1/2: send req.len bytes from req.ptr (returns 1)
          op 4: g_usb_mode = *req.ptr; op 5/6/7/8/10: state changes, nothing to do here
          op 9: file data of GetObject from the blocks whose LBAs req.handle points to
  struct mtp_xfer: op @0, ptr @2 (long), len @4 (long), handle @6 (long), fd @8

USB data words hold two bytes, the first in bits 15..8; the ATA model stores the low
byte first, so file data lands on the disk with the bytes of each pair swapped.
Plugging in is reported by the MCU (mcu_event class 12, mcu_status_notify): event 0xc4
sets status b8 (sysevent 5, argument 1: xferserver mask 96 -> 168), event 0xc2 sets b9
(sysevent 8, argument 1): the xferserver task (func_0087fafc) then opens "usb0" and
wakes the MTP worker; argument 0 (events 0xc5 / 0xc3) ends the connection. Register 1
bits 8 / 9 follow the events. That b8 / b9 mean USB power / USB connected is inferred
from this chain (sysevent 3 opens the 1394 transport instead), not measured. Once usb0 is
set up (about 16 s after the boot) it takes the link event itself through g_mcu_event_cb
(0x80e0cc) and waits for the controller; that callback is replaced to let the event
through (link_cb).
Containers on the wire are little endian (MTP 1.0 / PTP).
"""

import struct

from .disk import SECTOR
from .registers import EmulationError

USB_OP1, USB_OP2 = 0x80E27B, 0x80E176
USB_LINK_CB = 0x80E0CC              # the usb0 driver's MCU status callback (g_mcu_event_cb)
USB_MODE = 0x420DF3                 # g_usb_mode
# (register 1 bit, event when set, event when cleared) of the MCU status, mcu_event class 12
MCU_USB_POWER = (0x0100, 0xC4, 0xC5)       # b8
MCU_USB_LINK = (0x0200, 0xC2, 0xC3)        # b9
BLOCK = 8192                        # bytes per LBA list entry (func_00812c49)
STORAGE = 0x00010001
FORMAT_MP3 = 0x3009
OP_OPENSESSION, OP_CLOSESESSION = 0x1002, 0x1003
OP_SENDOBJECTINFO, OP_SENDOBJECT = 0x100C, 0x100D
RESPONSE_OK = 0x2001
TYPE_COMMAND, TYPE_DATA, TYPE_RESPONSE = 1, 2, 3


def container(kind, code, tid, params=(), payload=b""):
    if params and payload:
        raise ValueError("a container has parameters or a payload")
    body = struct.pack(f"<{len(params)}I", *params) + payload
    return struct.pack("<IHHI", 12 + len(body), kind, code, tid) + body


def ptp_string(text):
    if not text:
        return b"\x00"
    chars = text.encode("utf-16-le") + b"\x00\x00"
    if len(chars) // 2 > 255:
        raise ValueError(f"name too long for an MTP string: {text}")
    return bytes([len(chars) // 2]) + chars


def object_info(name, size, fmt=FORMAT_MP3):
    """ObjectInfo dataset (MTP 1.0 5.3.1): storage, format, protection, size, 40 bytes of
    thumbnail/image/parent/association/sequence fields, name, two dates, keywords."""
    return (struct.pack("<IHHI", STORAGE, fmt, 0, size) + bytes(40)
            + ptp_string(name) + ptp_string("") * 3)


def swap_pairs(data):
    out = bytearray(data)
    out[0::2], out[1::2] = data[1::2], data[0::2]
    return bytes(out)


class MtpHost:
    """Scripted host: OpenSession, SendObjectInfo + SendObject per file, CloseSession."""

    def __init__(self, files):
        """files: [(name, bytes)]"""
        self.rx = bytearray()           # host -> device, not yet received
        self.tx = bytearray()           # device -> host
        self.file_ranges = []           # rx offsets of file payloads still to come
        self.blocks_written = 0
        tid = 0
        self._put(container(TYPE_COMMAND, OP_OPENSESSION, tid, (1,)))
        for name, data in files:
            tid += 1
            self._put(container(TYPE_COMMAND, OP_SENDOBJECTINFO, tid, (STORAGE, 0xFFFFFFFF)))
            self._put(container(TYPE_DATA, OP_SENDOBJECTINFO, tid, payload=object_info(name, len(data))))
            tid += 1
            self._put(container(TYPE_COMMAND, OP_SENDOBJECT, tid))
            self._put(container(TYPE_DATA, OP_SENDOBJECT, tid, payload=data))
        tid += 1
        self._put(container(TYPE_COMMAND, OP_CLOSESESSION, tid))
        self.expected = tid + 1         # responses

    def _put(self, data):
        self.rx += data

    def install(self, m):
        """Replace the usb0 entries, also their C versions when the program has them (the
        hybrid: C callers call the C address), and the driver's MCU status callback."""
        for name, addr, handler in (("_usb_op1", USB_OP1, self.op1), ("_usb_op2", USB_OP2, self.op2)):
            m.hle[addr] = handler
            c = m.program.symbols.get(name)
            if c is not None:
                m.hle[c] = handler
        m.hle[USB_LINK_CB] = self.link_cb

    def link_cb(self, m):
        """usb0 registers this as g_mcu_event_cb once it is set up (about 16 s after the
        boot). For the link event (type 8) it wakes the driver task, which would talk to
        the USB controller, and keeps the event; here the event goes on to the MTP server
        as before the driver was set up (mcu_status_notify posts it on 0)."""
        self._ret(m, 0)

    @staticmethod
    def _req(m):
        bus, req = m.bus, m.cpu.xar[0]
        return bus, req, bus.read(req), bus.read32(req + 2), bus.read32(req + 4)

    @staticmethod
    def _ret(m, value):
        m.cpu.t[0] = value
        m.cpu.core.ret()

    def op1(self, m):
        bus, req, op, ptr, length = self._req(m)
        if op == 9:
            return self._ret(m, self._recv_file(m, bus, req, length))
        if op != 0:
            raise EmulationError(f"usb_op1: request {op} not emulated")
        if not self.rx:
            self._refill()
        if not self.rx:
            return self._ret(m, 0)
        if length > len(self.rx):
            raise EmulationError(f"usb_op1: device wants {length} bytes, host has {len(self.rx)}")
        data, self.rx = bytes(self.rx[:length]), self.rx[length:]
        for i in range(0, length - 1, 2):
            bus.write(ptr + i // 2, data[i] << 8 | data[i + 1])
        if length & 1:
            w = ptr + length // 2
            bus.write(w, data[-1] << 8 | (bus.read(w) & 0xFF))
        self._ret(m, 1)

    def op2(self, m):
        bus, req, op, ptr, length = self._req(m)
        if op in (1, 2):
            for i in range(length):
                w = bus.read(ptr + i // 2)
                self.tx.append(w >> 8 if i % 2 == 0 else w & 0xFF)
            return self._ret(m, 1)
        if op == 4:
            bus.write(USB_MODE, bus.read(ptr))
            return self._ret(m, 0)
        if op in (5, 6, 7, 8, 10):
            return self._ret(m, 0)
        if op == 9:
            return self._ret(m, self._send_file(m, bus, req, length))
        raise EmulationError(f"usb_op2: request {op} not emulated")

    def _refill(self):
        """More host -> device data when the device asks and nothing is left (MtpScript)."""

    def _send_file(self, m, bus, req, length):
        """op 9 of usb_op2 (GetObject): the file data from the disk blocks whose LBAs
        req.handle points to, the way op 9 of usb_op1 stores them."""
        lbas = bus.read32(req + 6)
        for i in range(0, length, BLOCK):
            lba = bus.read32(lbas + (i // BLOCK) * 2)
            n = min(BLOCK, length - i)
            data = b"".join(swap_pairs(m.disk.read(lba + s)) for s in range(-(-n // SECTOR)))
            self.tx += data[:n]
        bus.write32(req + 4, 0)             # nothing left (mtp_op_getobject sends the rest)
        return 0                            # 0: done (func_0087d547 turns 1 into an error)

    def _recv_file(self, m, bus, req, length):
        if length > len(self.rx):
            raise EmulationError(f"op 9: {length} bytes wanted, host has {len(self.rx)}")
        data, self.rx = bytes(self.rx[:length]), self.rx[length:]
        lbas = bus.read32(req + 6)
        for i in range(0, length, BLOCK):
            lba = bus.read32(lbas + (i // BLOCK) * 2)
            chunk = data[i:i + BLOCK]
            chunk += bytes(-len(chunk) % SECTOR)
            for s in range(0, len(chunk), SECTOR):
                m.disk.write(lba + s // SECTOR, swap_pairs(chunk[s:s + SECTOR]))
            self.blocks_written += 1
        return 0

    def responses(self):
        """-> [(code, tid, params)] of the responses received so far."""
        out, buf = [], bytes(self.tx)
        while len(buf) >= 12:
            length, kind, code, tid = struct.unpack_from("<IHHI", buf)
            if length == 0xFFFFFFFF or length < 12 or length > len(buf):
                raise EmulationError(f"device sent a container of length 0x{length:x}")
            if kind == TYPE_RESPONSE:
                out.append((code, tid, struct.unpack_from(f"<{(length - 12) // 4}I", buf, 12)))
            buf = buf[length:]
        return out

    def done(self):
        return len(self.responses()) >= self.expected


def mcu_status(m, bit, on):
    """Change an MCU status bit (MCU_USB_POWER / MCU_USB_LINK) and report it."""
    mask, ev_on, ev_off = bit
    if on:
        m.mcu.regs[1] |= mask
    else:
        m.mcu.regs[1] &= ~mask
    m.mcu.inject(ev_on if on else ev_off)


def usb_cable(m, plugged):
    """USB cable in / out: power and link status (see the module comment)."""
    for bit in ((MCU_USB_POWER, MCU_USB_LINK) if plugged else (MCU_USB_LINK, MCU_USB_POWER)):
        mcu_status(m, bit, plugged)
