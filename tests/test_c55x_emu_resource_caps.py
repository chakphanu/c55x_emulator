"""Emulator-level allocation caps (pure Python, no core needed).

The caps follow the architecture: 24-bit program byte addresses and 23-bit
data word addresses are both 16 MiB (SPRU371F ch. 3), so a load or a whole
image cannot exceed that. The code-cache cap is an engineering limit on the
decoder, not an architectural number.
"""

import time
import types
import unittest

from c55x_emu import decoder
from c55x_emu.bus import MEM_BYTES, Bus
from c55x_emu.machine import MAX_IMAGE_BYTES, Machine


class BusCaps(unittest.TestCase):
    def test_address_space_is_the_architectural_16_mib(self):
        self.assertEqual(MEM_BYTES, 1 << 24)
        self.assertEqual(len(Bus().mem), 1 << 24)

    def test_load_outside_the_address_space_is_refused(self):
        b = Bus()
        b.load(MEM_BYTES - 4, b"\x01\x02\x03\x04")            # exactly fits
        for addr, n in ((MEM_BYTES - 3, 4), (MEM_BYTES, 1), (-1, 2)):
            with self.assertRaises(ValueError):
                b.load(addr, bytes(n))

    def test_many_small_loads_stay_fast_and_keep_no_history(self):
        # a test harness seeds memory one word at a time; an earlier revision
        # kept a per-call log and re-summed it, which was quadratic
        b = Bus()
        t0 = time.time()
        for i in range(200_000):
            b.load(0x40000 + (i % 4096) * 2, b"\x12\x34")
        self.assertLess(time.time() - t0, 5.0)
        self.assertFalse(hasattr(b, "load_log"))


class ImageCaps(unittest.TestCase):
    def program(self, sizes):
        secs = [(0x20000 + i * 0x100, bytes([i % 251]) * n) for i, n in enumerate(sizes)]
        return types.SimpleNamespace(name="img", sections=secs)

    def test_image_info_reports_name_size_sections_and_digest(self):
        info = Machine._image_info(None, self.program([16, 32, 8]))
        self.assertEqual((info["name"], info["total_bytes"], info["section_count"]), ("img", 56, 3))
        self.assertEqual(len(info["sha256"]), 64)
        self.assertEqual([s["size"] for s in info["sections"]], [16, 32, 8])
        self.assertEqual(Machine._image_info(None, self.program([16, 32, 8]))["sha256"], info["sha256"])
        self.assertNotEqual(Machine._image_info(None, self.program([16, 32, 9]))["sha256"], info["sha256"])

    def test_an_image_larger_than_the_address_space_is_refused_before_allocation(self):
        prog = self.program([MAX_IMAGE_BYTES // 2 + 1, MAX_IMAGE_BYTES // 2 + 1])
        with self.assertRaises(ValueError) as cm:
            Machine(prog)
        self.assertIn("exceeds the program address space", str(cm.exception))


class CodeCacheCap(unittest.TestCase):
    def test_runaway_decode_stops_with_an_explicit_error(self):
        cache = decoder.CodeCache(Bus(), {}, decode=False)
        insn = types.SimpleNamespace(size=2)
        with unittest.mock.patch.object(decoder, "MAX_CACHED_INSNS", 5):
            cache.add({a: insn for a in range(0, 10, 2)})        # exactly 5 fit
            with self.assertRaises(RuntimeError) as cm:
                cache.add({100: insn})
        self.assertIn("code cache full", str(cm.exception))
        self.assertEqual(decoder.MAX_CACHED_INSNS, 1 << 20)


import unittest.mock  # noqa: E402  (used above)

if __name__ == "__main__":
    unittest.main()
