"""A Cpu and its native core form a reference cycle (the core holds the Cpu,
the Cpu holds the core). The core reports its Python references to the
garbage collector (__traverse__), so dropping a machine frees it. Before
that, every machine leaked about 16 MiB and the whole suite needed >2 GiB.
"""

import gc
import unittest
import weakref

from tests.test_c55x_emu_long_words import machine


def build():
    cpu, bus = machine("AC0 = #1")
    return weakref.ref(cpu), weakref.ref(bus)


class MachinesAreFreed(unittest.TestCase):
    def test_dropped_machines_are_collected(self):
        try:
            refs = [build() for _ in range(5)]
        except Exception as e:                      # guarded core outside a job
            if type(e).__name__ == "ResourcePolicyError":
                self.skipTest("needs bin/c55x-contained")
            raise
        gc.collect()
        self.assertEqual([r() is not None for pair in refs for r in pair], [False] * 10)

    def test_rss_does_not_grow_with_the_number_of_machines(self):
        def rss_mib():
            with open("/proc/self/status") as f:
                return int([l for l in f if l.startswith("VmRSS")][0].split()[1]) // 1024
        try:
            build()
        except Exception as e:
            if type(e).__name__ == "ResourcePolicyError":
                self.skipTest("needs bin/c55x-contained")
            raise
        gc.collect()
        base = rss_mib()
        for _ in range(40):                          # 40 x 16 MiB would be 640 MiB if leaked
            build()
            gc.collect()
        self.assertLess(rss_mib() - base, 160)


if __name__ == "__main__":
    unittest.main()
