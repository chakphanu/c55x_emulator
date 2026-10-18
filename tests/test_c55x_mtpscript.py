"""The scripted MTP host of the hybrid check (c55x_emu/mtpscript.py), without the firmware."""

import struct
import unittest

from c55x_emu.mtpscript import (OP_GETOBJECTINFO, OP_GETOBJECTHANDLES, OP_RESETDEVICE, MtpScript,
                                fingerprint, prop_list, session_steps, stream_end, u32_array)
from c55x_emu.usbhle import TYPE_DATA, TYPE_RESPONSE, container


def response(tid, code=0x2001, params=()):
    return container(TYPE_RESPONSE, code, tid, params)


class ParserTest(unittest.TestCase):
    def test_data_of_unknown_length_ends_at_its_response(self):
        head = struct.pack("<IHHI", 0xFFFFFFFF, TYPE_DATA, 0x9805, 3)
        payload = b"\x01\x00\x00\x00" + bytes(13)
        buf = head + payload + response(3)
        self.assertEqual(stream_end(buf, 3), len(head + payload))
        self.assertIsNone(stream_end(head + payload, 3))          # response not there yet

    def test_prop_list(self):
        self.assertEqual(prop_list([(7, 0xDC44, 0xFFFF, b"\x01\x00\x00")]),
                         struct.pack("<IIHH", 1, 7, 0xDC44, 0xFFFF) + b"\x01\x00\x00")
        self.assertEqual(prop_list([]), struct.pack("<I", 0))

    def test_session_ends_with_reset_device(self):
        self.assertEqual(session_steps()[-1]([]), [(OP_RESETDEVICE, (), None)])

    def test_u32_array(self):
        self.assertEqual(u32_array(struct.pack("<3I", 2, 7, 9)), [7, 9])
        self.assertEqual(u32_array(b""), [])


class ScriptTest(unittest.TestCase):
    def test_next_step_waits_for_the_answers_and_sees_them(self):
        seen = []
        steps = [lambda r: [(OP_GETOBJECTHANDLES, (0xFFFFFFFF, 0, 0), None)],
                 lambda r: seen.append(r) or [(OP_GETOBJECTINFO, (u32_array(r[0]["data"])[0],), None)]]
        host = MtpScript(steps)
        host._refill()
        self.assertEqual(len(host.sent), 1)
        host.rx.clear()
        host._refill()                              # not answered yet: nothing new
        self.assertEqual(len(host.sent), 1)
        host.tx += container(TYPE_DATA, OP_GETOBJECTHANDLES, 0, payload=struct.pack("<2I", 1, 42))
        host.tx += response(0)
        host._refill()
        self.assertEqual(host.sent[1], (1, OP_GETOBJECTINFO, (42,)))
        self.assertEqual(seen[0][0]["data"], struct.pack("<2I", 1, 42))
        self.assertFalse(host.done())
        host.tx += response(1, 0x2009)
        self.assertTrue(host.done())
        fp = fingerprint(host)
        self.assertEqual([f[:5] for f in fp], [[OP_GETOBJECTHANDLES, [0xFFFFFFFF, 0, 0], 0x2001, [], 8],
                                                [OP_GETOBJECTINFO, [42], 0x2009, [], 0]])


if __name__ == "__main__":
    unittest.main()
