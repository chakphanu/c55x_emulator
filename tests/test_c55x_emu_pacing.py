"""C55x emulator: real-time pacing of the front ends (pacing.py)."""

import unittest

from c55x_emu.pacing import Pacer


class PacerTests(unittest.TestCase):
    def setUp(self):
        self.now = 100.0
        self.p = Pacer(5.0, max_lag=0.25, clock=lambda: self.now)

    def test_follows_the_wall_clock(self):
        self.now += 2
        self.assertAlmostEqual(self.p.target(), 7.0)

    def test_slow_emulator_lags_at_most_max_lag(self):
        emulated = 5.0
        for _ in range(1000):                       # emulator at a tenth of real time
            self.now += 0.015
            emulated += 0.0015
            self.p.lagged(emulated)
            self.assertLessEqual(self.p.target() - emulated, 0.25 + 1e-9)
        self.assertGreater(self.p.target(), emulated)

    def test_rebase(self):
        self.now += 10
        self.p.rebase(42.0)
        self.assertAlmostEqual(self.p.target(), 42.0)


if __name__ == "__main__":
    unittest.main()
