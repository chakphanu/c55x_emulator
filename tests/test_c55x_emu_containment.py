"""Containment tests, scope A: the native core guard.

Two groups, selected by where the suite runs:

  RejectedOutside  - run directly (no launcher): the core must refuse.
  InsideJob        - run through the launcher:
                       bin/c55x-contained -- python3 -m unittest tests.test_c55x_emu_containment -v
                     real kernel readbacks and live revalidation.

The launcher itself is tested in tests/test_c55x_launcher.py (must run
outside a job) and the pure policy logic in tests/test_c55x_native_policy.py.
"""

import os
import pathlib
import subprocess
import sys
import time
import unittest

ROOT = pathlib.Path(__file__).resolve().parent.parent
IN_JOB = os.environ.get("C55X_CONTAINED_JOB") == "emulator"
GIB = 1 << 30
EMU_UNITS = ("c55x-emu.service",) + tuple("c55x-emu-%d.service" % k for k in range(1, 8))


def job_cgroup():
    with open("/proc/self/cgroup") as f:
        rel = [l for l in f if l.startswith("0::")][0][3:].strip()
    return pathlib.Path("/sys/fs/cgroup") / rel.lstrip("/")


def read(path, name):
    return (path / name).read_text().strip()


def policy_error():
    from c55x_emu.rust_core import load
    return load().ResourcePolicyError


@unittest.skipIf(IN_JOB, "run directly to test the refusal")
class RejectedOutside(unittest.TestCase):
    def test_direct_construction_is_refused(self):
        from tests.test_c55x_emu_long_words import machine
        with self.assertRaises(policy_error()) as cm:
            machine("nop")
        self.assertIn("resource policy", str(cm.exception))
        self.assertIn("bin/c55x-contained", str(cm.exception))

    def test_fake_environment_marker_alone_is_refused(self):
        code = ("from tests.test_c55x_emu_long_words import machine\n"
                "try:\n    machine('nop')\nexcept Exception as e:\n"
                "    print(type(e).__name__, e); raise SystemExit(3)\n"
                "print('ADMITTED')\n")
        env = dict(os.environ, C55X_CONTAINED_JOB="emulator", PYTHONPATH=str(ROOT))
        r = subprocess.run([sys.executable, "-c", code], cwd=ROOT, env=env,
                           capture_output=True, text=True)
        self.assertEqual(r.returncode, 3, r.stdout + r.stderr)
        self.assertIn("ResourcePolicyError", r.stdout)
        self.assertNotIn("ADMITTED", r.stdout)


@unittest.skipUnless(IN_JOB, "run through bin/c55x-contained")
class InsideJob(unittest.TestCase):
    def test_core_constructs_and_runs(self):
        from tests.test_c55x_emu_long_words import machine, run
        cpu, _ = machine("AC0 = #0x100", "AC0 = AC0 << #16")
        run(cpu)
        self.assertEqual(cpu.ac[0], 0x100 << 16)

    def test_kernel_values_seen_by_the_job(self):
        job = job_cgroup()
        slc = job.parent
        self.assertIn(job.name, EMU_UNITS)
        self.assertEqual(slc.name, "c55x.slice")
        self.assertTrue(0 < int(read(job, "memory.max")) <= 2 * GIB)
        self.assertEqual(read(job, "memory.swap.max"), "0")
        self.assertEqual(read(job, "pids.max"), "32")
        self.assertEqual(read(job, "memory.oom.group"), "1")
        self.assertEqual(read(job, "cpu.max"), "100000 100000")
        self.assertEqual(read(slc, "memory.max"), str(18 * GIB))
        self.assertEqual(read(slc, "memory.high"), str(17 * GIB))
        self.assertEqual(read(slc, "memory.swap.max"), "0")
        self.assertEqual(read(slc, "pids.max"), "448")
        self.assertEqual(read(slc, "cpu.max"), "1000000 100000")

    def test_policy_is_revalidated_when_the_job_ceiling_is_relaxed(self):
        # A Core built under valid policy must stop executing once the group
        # is relaxed. The job cgroup is a bounded test group: the shared
        # ancestor keeps its 18 GiB cap throughout, and the value is restored.
        from tests.test_c55x_emu_long_words import machine
        cpu, _ = machine("AC0 = #1", "AC0 = AC0 << #1")
        cpu.step()                                   # valid policy: executes
        job = job_cgroup()
        original = read(job, "memory.max")
        try:
            (job / "memory.max").write_text("max\n")
            time.sleep(0.1)                          # revalidation interval is 50 ms
            with self.assertRaises(policy_error()) as cm:
                cpu.step()
            self.assertIn("unlimited", str(cm.exception))
            with self.assertRaises(policy_error()):
                cpu.run(10)
        finally:
            (job / "memory.max").write_text(original + "\n")
        cpu.step()                                   # restored: executes again

    def test_policy_is_revalidated_when_the_marker_disappears(self):
        from tests.test_c55x_emu_long_words import machine
        cpu, _ = machine("AC0 = #1", "AC0 = AC0 << #1")
        cpu.step()
        saved = os.environ.pop("C55X_CONTAINED_JOB")
        try:
            time.sleep(0.1)                          # revalidation interval is 50 ms
            with self.assertRaises(policy_error()):
                cpu.step()
        finally:
            os.environ["C55X_CONTAINED_JOB"] = saved
        cpu.step()

    def test_every_execution_entry_of_an_existing_core_is_guarded(self):
        # run/step/call/ret/advance are the native entries that execute guest
        # code or advance guest time; all reach the guard through enter().
        # (ata_io validates its register address first, then enters.)
        from tests.test_c55x_emu_long_words import machine
        cpu, _ = machine("AC0 = #1")
        core = cpu.core
        entries = {"run": lambda: core.run(1, set()), "step": core.step,
                   "call": lambda: core.call(0x20000, 0), "ret": core.ret,
                   "advance": lambda: core.advance(1)}
        saved = os.environ.pop("C55X_CONTAINED_JOB")
        try:
            time.sleep(0.1)                          # revalidation interval is 50 ms
            for name, call in entries.items():
                with self.subTest(entry=name):
                    with self.assertRaises(policy_error()):
                        call()
        finally:
            os.environ["C55X_CONTAINED_JOB"] = saved

    def test_descendants_share_the_task_limit(self):
        children = []
        try:
            with self.assertRaises(OSError):
                for _ in range(64):                  # job limit is 32 tasks
                    children.append(subprocess.Popen(["sleep", "30"]))
            self.assertLess(len(children), 32)
            self.assertGreater(len(children), 10)
        finally:
            for c in children:                       # exactly the processes we started
                c.kill()
            for c in children:
                c.wait()

    def test_small_tmpfs_allocation_is_charged_to_the_job(self):
        job = job_cgroup()

        def shmem():
            for line in read(job, "memory.stat").splitlines():
                if line.startswith("shmem "):
                    return int(line.split()[1])
            return 0
        path = pathlib.Path("/dev/shm") / ("c55x-test-%d" % os.getpid())
        before = shmem()
        try:
            with open(path, "wb") as f:
                f.write(b"\0" * (8 << 20))           # 8 MiB, deliberately tiny
            self.assertGreaterEqual(shmem() - before, 7 << 20)
        finally:
            path.unlink()

    def test_trace_length_cap_is_an_explicit_error(self):
        from tests.test_c55x_emu_long_words import machine
        cpu, _ = machine("nop")
        with self.assertRaises(ValueError):
            cpu.core.set_trace((1 << 22) + 1)
        cpu.core.set_trace(1 << 22)                  # the maximum itself is fine
        cpu.core.set_trace(0)


if __name__ == "__main__":
    unittest.main()
