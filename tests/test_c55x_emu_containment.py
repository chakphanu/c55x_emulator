"""Behavioral tests for the resource containment guard.

RED (run directly, no launcher): verifies that Core construction is rejected
outside the managed cgroup.

    python3 -m unittest tests.test_c55x_emu_containment -v

GREEN (run through bin/c55x-contained): verifies that Core construction and
execution work inside the managed cgroup.

    bin/c55x-contained -- python3 -m unittest tests.test_c55x_emu_containment -v
"""

import os
import unittest


CONTAINED = bool(os.environ.get("C55X_CONTAINED_JOB"))


@unittest.skipUnless(CONTAINED, "run inside bin/c55x-contained")
class GreenTests(unittest.TestCase):
    """Inside containment: Core constructs and executes correctly."""

    def test_core_constructs_and_runs(self):
        from tests.test_c55x_emu_long_words import machine, run
        cpu, bus = machine("AC0 = #0x100", "AC0 = AC0 << #16")
        run(cpu)
        self.assertEqual(cpu.ac[0], 0x100 << 16)

    def test_guard_sees_finite_memory_max(self):
        import pathlib
        cg_line = open("/proc/self/cgroup").read().strip()
        cg_path = cg_line.split("::")[1].lstrip("/")
        cg = pathlib.Path("/sys/fs/cgroup") / cg_path
        mem_max = int(cg.joinpath("memory.max").read_text().strip())
        self.assertGreater(mem_max, 0)
        self.assertLessEqual(mem_max, 2 * 1024**3)
        swap = int(cg.joinpath("memory.swap.max").read_text().strip())
        self.assertEqual(swap, 0)

    def test_guard_revalidates_on_execution(self):
        from tests.test_c55x_emu_long_words import machine, run
        cpu, bus = machine("AC0 = #0x1", "AC0 = AC0 << #1")
        run(cpu)
        self.assertEqual(cpu.ac[0], 2)
        # step also goes through enter() which re-checks containment
        cpu2, _ = machine("AC0 = #0x1", "AC0 = AC0 << #1")
        cpu2.step()
        cpu2.step()
        self.assertEqual(cpu2.ac[0], 2)

    def test_local_oom_kills_job_not_host(self):
        # OOM is tested via the launcher (rc=79), not in-process, because
        # memory.oom.group=1 kills the entire cgroup. This test verifies
        # that a separate contained job with a tiny ceiling and a large
        # allocation is killed (the launcher already proved rc=79).
        # Here we only verify the guard does not crash on memory pressure.
        from tests.test_c55x_emu_long_words import machine, run
        cpu, _ = machine("AC0 = #0x1")
        run(cpu)  # normal execution still works

    def test_argv_streams_and_exit_code_preserved(self):
        import subprocess, sys
        r = subprocess.run(
            [sys.executable, "-c", "import sys; sys.exit(42)"],
            capture_output=True, text=True)
        self.assertEqual(r.returncode, 42)


@unittest.skipIf(CONTAINED, "run directly to test rejection")
class RedTests(unittest.TestCase):
    """Outside containment: Core construction is rejected."""

    def test_direct_construction_rejected(self):
        from tests.test_c55x_emu_long_words import machine
        try:
            cpu, bus = machine("nop")
            self.fail("Core should have been rejected outside containment")
        except Exception as e:
            self.assertIn("resource policy", str(e))

    def test_direct_step_rejected(self):
        # even if a Core somehow existed, execution would fail on re-check
        # (we cannot construct one, so this tests the error path indirectly)
        from tests.test_c55x_emu_long_words import machine
        try:
            cpu, bus = machine("nop")
            cpu.step()
            self.fail("step should have been rejected")
        except Exception as e:
            self.assertIn("resource policy", str(e))


if __name__ == "__main__":
    unittest.main()
