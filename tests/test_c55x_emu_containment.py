"""Containment: the trace length cap of the Python binding (the cap moves into the core
next; the guard tests are rust/c55x_core/tests/containment.rs). Run through the launcher:
  bin/c55x-contained -- python3 -m unittest tests.test_c55x_emu_containment -v
"""

import os
import unittest

IN_JOB = os.environ.get("C55X_CONTAINED_JOB") == "emulator"


@unittest.skipUnless(IN_JOB, "run through bin/c55x-contained")
class InsideJob(unittest.TestCase):
    def test_trace_length_cap_is_an_explicit_error(self):
        from c55x_emu.testutil import machine
        cpu, _ = machine("nop")
        with self.assertRaises(ValueError):
            cpu.core.set_trace((1 << 22) + 1)
        cpu.core.set_trace(1 << 22)                  # the maximum itself is fine
        cpu.core.set_trace(0)


if __name__ == "__main__":
    unittest.main()
