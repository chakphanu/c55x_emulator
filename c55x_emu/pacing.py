"""Real-time pacing for the front ends: emulated seconds follow the wall clock.

target(now) is the emulated time to reach; while the emulator is slower than real time
it lags at most max_lag seconds behind (the reference moves, so a slow phase is not
made up later by running ahead of real time).
"""

import time


class Pacer:
    def __init__(self, emulated_now, max_lag=0.25, clock=time.monotonic):
        self.clock, self.max_lag = clock, max_lag
        self.rebase(emulated_now)

    def rebase(self, emulated_now):
        """Emulated time emulated_now corresponds to the wall clock now."""
        self.t0, self.s0 = self.clock(), emulated_now

    def target(self, now=None):
        return self.s0 + ((self.clock() if now is None else now) - self.t0)

    def lagged(self, emulated_now, now=None):
        """After a slice: if more than max_lag behind, let the reference follow."""
        behind = self.target(now) - emulated_now
        if behind > self.max_lag:
            self.s0 -= behind - self.max_lag
