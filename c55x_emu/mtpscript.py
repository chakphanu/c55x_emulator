"""An MTP conversation in steps, for tests of the firmware's MTP responder (usbhle.py).

Each step is a function of the results so far that returns the next transactions
(opcode, parameters, data sent by the host or None). The host builds the next step when
the device asks for more and the previous transactions are answered, so a step can use
the handles of an earlier one. The device side of the conversation (every byte it sends)
is the result: the hybrid check compares it with the original firmware's.

session_steps() is the scenario of the hybrid check: device and storage information, the
objects on the disk and their properties, a new file sent, renamed (SetObjectPropValue and
SetObjectPropList), given references, read back and deleted; a second file created with
SendObjectPropList; GetObjectPropList for one object, all objects and the root folder;
error cases; the friendly name set and reset; ResetDevice (ends the session: the device
answers no command after it).
"""
import hashlib
import struct

from .registers import EmulationError
from .usbhle import (FORMAT_MP3, OP_OPENSESSION, OP_SENDOBJECT, OP_SENDOBJECTINFO,
                     STORAGE, TYPE_COMMAND, TYPE_DATA, TYPE_RESPONSE, MtpHost, container, object_info,
                     ptp_string)

OP_GETDEVICEINFO, OP_GETSTORAGEIDS, OP_GETSTORAGEINFO = 0x1001, 0x1004, 0x1005
OP_GETOBJECTHANDLES, OP_GETOBJECTINFO, OP_GETOBJECT, OP_DELETEOBJECT = 0x1007, 0x1008, 0x1009, 0x100B
OP_GETDEVICEPROPDESC, OP_GETDEVICEPROPVALUE = 0x1014, 0x1015
OP_GETOBJECTPROPSSUPPORTED, OP_GETOBJECTPROPDESC = 0x9801, 0x9802
OP_GETOBJECTPROPVALUE, OP_SETOBJECTPROPVALUE, OP_GETOBJECTPROPLIST = 0x9803, 0x9804, 0x9805
OP_SETOBJECTPROPLIST, OP_GETINTERDEPENDENTPROPDESC, OP_SENDOBJECTPROPLIST = 0x9806, 0x9807, 0x9808
OP_GETOBJECTREFERENCES, OP_SETOBJECTREFERENCES = 0x9810, 0x9811
OP_RESETDEVICE, OP_FORMATSTORE = 0x1010, 0x100F
OP_SETDEVICEPROPVALUE, OP_RESETDEVICEPROPVALUE = 0x1016, 0x1017
OP_COPYOBJECT = 0x101A
PROP_NAME, PROP_FILENAME = 0xDC44, 0xDC07
TYPE_STR = 0xFFFF
DEVPROP_FRIENDLY_NAME = 0xD402
ALL = 0xFFFFFFFF


class MtpScript(MtpHost):
    """Host that sends the transactions of steps one step at a time."""

    def __init__(self, steps):
        self.rx, self.tx = bytearray(), bytearray()
        self.blocks_written = 0
        self.steps = list(steps)
        self.sent = []                  # [(tid, opcode, params)] in order
        self.expected = 0

    def _refill(self):
        if not self.steps or len(self.transactions()) < len(self.sent):
            return
        for op, params, payload in self.steps.pop(0)(self.transactions()):
            tid = len(self.sent)
            self.sent.append((tid, op, tuple(params)))
            self._put(container(TYPE_COMMAND, op, tid, params))
            if payload is not None:
                self._put(container(TYPE_DATA, op, tid, payload=payload))
        self.expected = len(self.sent)

    def transactions(self):
        """-> [{"op", "params", "code", "rparams", "data"}] of the answered transactions."""
        data, out, buf = {}, [], bytes(self.tx)
        ops = {tid: (op, params) for tid, op, params in self.sent}
        while len(buf) >= 12:
            length, kind, code, tid = struct.unpack_from("<IHHI", buf)
            if length == 0xFFFFFFFF and kind == TYPE_DATA:
                length = stream_end(buf, tid)   # data of unknown length: up to the response
            if length is None or length < 12 or length > len(buf):
                break
            if kind == TYPE_DATA:
                data[tid] = buf[12:length]
            elif kind == TYPE_RESPONSE:
                op, params = ops.get(tid, (None, ()))
                out.append({"tid": tid, "op": op, "params": params, "code": code,
                            "rparams": struct.unpack_from(f"<{(length - 12) // 4}I", buf, 12),
                            "data": data.pop(tid, b"")})
            buf = buf[length:]
        return out

    def done(self):
        return not self.steps and len(self.transactions()) >= len(self.sent) > 0


def stream_end(buf, tid):
    """End of a data container of unknown length (0xffffffff, GetObjectPropList): where the
    response of the same transaction starts; None while it has not come."""
    at = 12
    while True:
        at = buf.find(struct.pack("<HHI", TYPE_RESPONSE, 0, tid)[:2], at)
        if at < 4:
            return None
        start = at - 4
        length, kind, code, t = struct.unpack_from("<IHHI", buf, start) if start + 12 <= len(buf) else (0, 0, 0, -1)
        if kind == TYPE_RESPONSE and t == tid and 12 <= length <= 32 and (length - 12) % 4 == 0:
            return start
        at += 1


def u32_array(data):
    """MTP array of UINT32 (count, then the elements)."""
    if len(data) < 4:
        return []
    n = struct.unpack_from("<I", data)[0]
    return list(struct.unpack_from(f"<{n}I", data, 4)) if len(data) >= 4 + 4 * n else []


def test_mp3(frames=40):
    """Small MPEG-1 layer III file: 128 kbit/s, 44.1 kHz frames of silence."""
    return (b"\xff\xfb\x90\x64" + bytes(413)) * frames


def prop_list(elements):
    """ObjectPropList dataset: [(handle, property code, datatype, value bytes)]."""
    return struct.pack("<I", len(elements)) + b"".join(
        struct.pack("<IHH", h, code, kind) + value for h, code, kind, value in elements)


def session_steps(new_name="zen_mtp_test.mp3", title="Zen MTP test", list_name="zen_mtp_list.mp3"):
    def last(results, op):
        return next((r for r in reversed(results) if r["op"] == op), None)

    def handles(results):
        r = last(results, OP_GETOBJECTHANDLES)
        return u32_array(r["data"]) if r else []

    def new_handle(results, op=OP_SENDOBJECTINFO):
        r = last(results, op)
        return r["rparams"][2] if r and len(r["rparams"]) > 2 else 0

    data = test_mp3()
    return [
        lambda r: [(OP_OPENSESSION, (1,), None)],
        lambda r: [(OP_GETDEVICEINFO, (), None), (OP_GETSTORAGEIDS, (), None)],
        lambda r: [(OP_GETSTORAGEINFO, (STORAGE,), None), (OP_GETOBJECTHANDLES, (ALL, 0, 0), None)],
        lambda r: [t for h in handles(r)[:3] for t in (
            (OP_GETOBJECTINFO, (h,), None), (OP_GETOBJECTPROPVALUE, (h, PROP_NAME), None),
            (OP_GETOBJECTREFERENCES, (h,), None))],
        lambda r: [(OP_GETOBJECTPROPSSUPPORTED, (FORMAT_MP3,), None),
                   (OP_GETOBJECTPROPDESC, (PROP_NAME, FORMAT_MP3), None),
                   (OP_GETDEVICEPROPDESC, (DEVPROP_FRIENDLY_NAME,), None),
                   (OP_GETDEVICEPROPVALUE, (DEVPROP_FRIENDLY_NAME,), None)],
        lambda r: [(OP_SENDOBJECTINFO, (STORAGE, ALL), object_info(new_name, len(data))),
                   (OP_SENDOBJECT, (), data)],
        lambda r: [(OP_GETOBJECTINFO, (new_handle(r),), None),
                   (OP_SETOBJECTPROPVALUE, (new_handle(r), PROP_NAME), ptp_string(title)),
                   (OP_GETOBJECTPROPVALUE, (new_handle(r), PROP_NAME), None),
                   (OP_GETOBJECTPROPLIST, (new_handle(r), 0, ALL, 0, 0), None),
                   (OP_GETOBJECT, (new_handle(r),), None)],
        lambda r: [(OP_GETOBJECTPROPLIST, (new_handle(r), 0, PROP_NAME, 0, 0), None),
                   (OP_SETOBJECTPROPLIST, (), prop_list([(new_handle(r), PROP_NAME, TYPE_STR,
                                                          ptp_string(title + " 2"))])),
                   (OP_SETOBJECTREFERENCES, (new_handle(r),), struct.pack("<I", 0)),
                   (OP_GETOBJECTREFERENCES, (new_handle(r),), None),
                   (OP_GETINTERDEPENDENTPROPDESC, (FORMAT_MP3,), None)],
        lambda r: [(OP_DELETEOBJECT, (new_handle(r), 0), None), (OP_GETOBJECTHANDLES, (ALL, 0, 0), None)],
        lambda r: [(OP_SENDOBJECTPROPLIST, (STORAGE, ALL, FORMAT_MP3, 0, len(data)),
                    prop_list([(0, PROP_FILENAME, TYPE_STR, ptp_string(list_name))])),
                   (OP_SENDOBJECT, (), data)],
        lambda r: [(OP_GETOBJECTINFO, (new_handle(r, OP_SENDOBJECTPROPLIST),), None),
                   (OP_DELETEOBJECT, (new_handle(r, OP_SENDOBJECTPROPLIST), 0), None),
                   (OP_GETOBJECTPROPLIST, (ALL, 0, PROP_FILENAME, 0, 0), None),
                   (OP_GETOBJECTPROPLIST, (0, 0, PROP_FILENAME, 0, 1), None)],
        lambda r: [(OP_GETOBJECTINFO, (0x7FFFFFFF,), None),                    # error cases
                   (OP_DELETEOBJECT, (new_handle(r), 0), None),
                   (OP_GETOBJECTPROPVALUE, (handles(r)[0] if handles(r) else 0, 0x1234), None),
                   (OP_FORMATSTORE, (STORAGE, 1), None),
                   (OP_COPYOBJECT, (0, 0, 0), None)],
        lambda r: [(OP_SETDEVICEPROPVALUE, (DEVPROP_FRIENDLY_NAME,), ptp_string("Zen MTP host")),
                   (OP_GETDEVICEPROPVALUE, (DEVPROP_FRIENDLY_NAME,), None),
                   (OP_RESETDEVICEPROPVALUE, (DEVPROP_FRIENDLY_NAME,), None),
                   (OP_GETDEVICEPROPVALUE, (DEVPROP_FRIENDLY_NAME,), None)],
        lambda r: [(OP_RESETDEVICE, (), None)],       # closes the session; no command after it
    ]


def summary(results):
    """One line per transaction: opcode, response code, data length."""
    return [f"{t['tid']:3d} op 0x{t['op'] or 0:04x} {t['params']} -> 0x{t['code']:04x} "
            f"{t['rparams']} data {len(t['data'])}" for t in results]


def check_complete(host):
    if not host.done():
        raise EmulationError(f"MTP script: {len(host.transactions())} of {len(host.sent)} answered, "
                             f"{len(host.steps)} steps left")


def fingerprint(host):
    """-> [[op, params, code, response params, data length, data sha256 prefix]] of the
    answered transactions: what the hybrid check compares with the original firmware."""
    return [[t["op"], list(t["params"]), t["code"], list(t["rparams"]), len(t["data"]),
             hashlib.sha256(t["data"]).hexdigest()[:16]] for t in host.transactions()]


def run_session(m, timeout=120.0, settle=5.0):
    """Plug in USB, run session_steps() to the end, unplug, let the firmware settle.
    -> fingerprint. Raises EmulationError when the conversation does not finish."""
    from .addmusic import run_until
    from .usbhle import usb_cable
    host = MtpScript(session_steps())
    host.install(m)
    usb_cable(m, True)
    try:
        run_until(m, host.done, timeout, "MTP session")
    finally:
        usb_cable(m, False)
    m.run(10**15, until_seconds=m.seconds() + settle)
    return fingerprint(host)
