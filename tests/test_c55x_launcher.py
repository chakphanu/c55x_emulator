"""Containment tests, scope B: bin/c55x-contained.

Must run OUTSIDE a job (plain `python3 -m unittest tests.test_c55x_launcher`):
the launcher is the thing under test and an emulator job already holds the
single slot. Real kernel enforcement is exercised with tiny budgets (64 MiB)
in bounded jobs; cases that would need real host exhaustion or kernel
mutation outside the job use mocks and say so.

Every process these tests start is owned by the test and is cleaned up by
exact handle or exact unit name; nothing broad is killed. Tests that build a
Core need a guarded core: set ZEN_CORE_DIR to a directory holding it.
"""

import importlib.machinery
import importlib.util
import json
import os
import pathlib
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import unittest
import uuid
from unittest import mock

ROOT = pathlib.Path(__file__).resolve().parent.parent
LAUNCHER = ROOT / "bin" / "c55x-contained"
TMP = ROOT / "build" / "test-tmp"
EMU_UNIT = "c55x-emu.service"
PY = sys.executable


def load_launcher():
    loader = importlib.machinery.SourceFileLoader("c55x_contained", str(LAUNCHER))
    spec = importlib.util.spec_from_loader(loader.name, loader)
    mod = importlib.util.module_from_spec(spec)
    loader.exec_module(mod)
    return mod


L = load_launcher()
HAVE_MANAGER = L.user_bus_env() is not None
IN_JOB = "C55X_CONTAINED_JOB" in os.environ


def have_guarded_core():
    d = os.environ.get("ZEN_CORE_DIR")
    return bool(d) and (pathlib.Path(d) / "c55x_core.so").exists()


def launch(*args, cwd=ROOT, env=None, input=None, timeout=90):
    return subprocess.run([str(LAUNCHER), *args], cwd=cwd, env=env, input=input,
                          capture_output=True, text=True, timeout=timeout)


def unit_active(unit=EMU_UNIT):
    return subprocess.run(["systemctl", "--user", "is-active", "--quiet", unit]
                          ).returncode == 0


def wait_until(pred, secs, step=0.2):
    end = time.time() + secs
    while time.time() < end:
        if pred():
            return True
        time.sleep(step)
    return pred()


@unittest.skipIf(IN_JOB, "launcher tests must run outside a job")
@unittest.skipUnless(HAVE_MANAGER, "needs a systemd user manager")
class LauncherTests(unittest.TestCase):
    def setUp(self):
        TMP.mkdir(parents=True, exist_ok=True)
        self.tmp = pathlib.Path(tempfile.mkdtemp(dir=TMP))
        self.sentinels = []
        wait_until(lambda: not unit_active(), 30)

    def tearDown(self):
        for p in self.sentinels:                  # exactly the processes we started
            p.kill()
            p.wait()
        shutil.rmtree(self.tmp)

    def sentinel(self):
        p = subprocess.Popen(["sleep", "300"])
        self.sentinels.append(p)
        return p

    # ---- kernel readback, argv/streams/cwd/exit code -------------------

    def test_kernel_values_for_both_job_classes(self):
        code = ("import json,pathlib\n"
                "rel=[l for l in open('/proc/self/cgroup') if l.startswith('0::')][0][3:].strip()\n"
                "j=pathlib.Path('/sys/fs/cgroup')/rel.lstrip('/')\n"
                "r=lambda d,n:(d/n).read_text().strip()\n"
                "print(json.dumps({'unit':j.name,'slice':j.parent.name,"
                "'job':[r(j,n) for n in ('memory.max','memory.swap.max','pids.max','memory.oom.group','cpu.max')],"
                "'shared':[r(j.parent,n) for n in ('memory.max','memory.high','memory.swap.max','pids.max','cpu.max')]}))")
        g = 1 << 30
        for job in ("emulator", "work"):
            r = launch("--job", job, "--mem", "64M", "--wall-sec", "20", "--", PY, "-c", code)
            self.assertEqual(r.returncode, 0, r.stderr)
            v = json.loads(r.stdout)
            self.assertEqual(v["slice"], "c55x.slice")
            self.assertEqual(v["unit"] == EMU_UNIT, job == "emulator")
            self.assertEqual(v["job"], [str(64 << 20), "0", "32", "1", "100000 100000"])
            self.assertEqual(v["shared"], [str(4 * g), str(3 * g), "0", "64", "200000 100000"])

    def test_argv_cwd_streams_stdin_and_exit_code_are_preserved(self):
        code = ("import sys,os,json\n"
                "print(json.dumps(sys.argv[1:])); print(os.getcwd())\n"
                "print('E', file=sys.stderr); print('IN:'+sys.stdin.read())\n"
                "sys.exit(5)")
        argv = ["a b", "$HOME", "%h", '"q"', "", "-x", "--mem"]
        r = launch("--mem", "64M", "--wall-sec", "20", "--", PY, "-c", code, *argv,
                   cwd=self.tmp, input="hello")
        self.assertEqual(r.returncode, 5)
        out = r.stdout.splitlines()
        self.assertEqual(json.loads(out[0]), argv)
        self.assertEqual(out[1], str(self.tmp.resolve()))
        self.assertEqual(out[2], "IN:hello")
        self.assertIn("E", r.stderr)
        self.assertIn("outcome=payload rc=5", r.stderr)

    def test_environment_threads_and_tmpdir(self):
        env = dict(os.environ, C55X_TEST_FORWARD="yes", UNLISTED_VAR="no", OMP_NUM_THREADS="9")
        code = ("import os;print(os.environ.get('C55X_TEST_FORWARD'),"
                "os.environ.get('UNLISTED_VAR'),os.environ.get('OMP_NUM_THREADS'),"
                "os.environ.get('TMPDIR'),os.environ.get('C55X_CONTAINED_JOB'))")
        r = launch("--mem", "64M", "--wall-sec", "20", "--", PY, "-c", code, env=env)
        self.assertEqual(r.returncode, 0, r.stderr)
        fwd, unl, omp, tmpdir, job = r.stdout.split()
        self.assertEqual((fwd, unl, omp, job), ("yes", "None", "1", "emulator"))
        self.assertEqual(tmpdir, str(ROOT / "build" / "work"))
        env2 = dict(env, UNLISTED_VAR="explicit")
        r = launch("--mem", "64M", "--wall-sec", "20", "--env", "UNLISTED_VAR", "--", PY, "-c",
                   "import os;print(os.environ.get('UNLISTED_VAR'))", env=env2)
        self.assertEqual(r.stdout.strip(), "explicit")

    # ---- refusals before the payload starts -----------------------------

    def test_excessive_limits_cannot_widen_the_hard_policy(self):
        marker = self.tmp / "ran"
        for flags in (["--mem", "3G"], ["--mem", "4G"], ["--cpu-sec", "601"],
                      ["--wall-sec", "901"], ["--mem", "1M"], ["--cpu-sec", "0"]):
            r = launch(*flags, "--", "touch", str(marker))
            self.assertEqual(r.returncode, 77, (flags, r.stderr))
            self.assertIn("outcome=policy", r.stderr)
        self.assertFalse(marker.exists())

    def test_low_memory_headroom_refuses_before_the_payload(self):
        marker = self.tmp / "ran"
        env = dict(os.environ, C55X_TEST_MEM_AVAILABLE_BYTES=str(2 << 30))
        r = launch("--mem", "64M", "--", "touch", str(marker), env=env)
        self.assertEqual(r.returncode, 74, r.stderr)
        self.assertIn("outcome=headroom", r.stderr)
        self.assertFalse(marker.exists())
        # the injected value can only lower the observation, never raise it
        env = dict(os.environ, C55X_TEST_MEM_AVAILABLE_BYTES=str(1 << 60))
        self.assertEqual(launch("--mem", "64M", "--wall-sec", "10", "--", "true", env=env).returncode, 0)

    def test_tmpfs_work_dir_is_rejected_before_anything_is_created(self):
        probe = pathlib.Path("/dev/shm") / ("c55x-never-%s" % uuid.uuid4().hex[:8])
        r = launch("--mem", "64M", "--work-dir", str(probe / "w"), "--", "true")
        self.assertEqual(r.returncode, 77, r.stderr)
        self.assertIn("tmpfs", r.stderr)
        self.assertFalse(probe.exists())

    def test_unavailable_manager_refuses_without_fallback(self):
        # mock: the real user manager cannot be removed from under other jobs
        with mock.patch.object(L, "user_bus_env", lambda: None):
            with self.assertRaises(SystemExit) as cm:
                L.main(["--mem", "64M", "--", "true"])
        self.assertEqual(cm.exception.code, 77)

    def test_reserved_exit_codes_from_the_payload_are_not_mistaken_for_outcomes(self):
        for code in (70, 74, 75, 77, 78, 79, 124):
            r = launch("--mem", "64M", "--wall-sec", "20", "--", "sh", "-c", "exit %d" % code)
            self.assertEqual(r.returncode, 70, (code, r.stderr))
            self.assertIn("outcome=incomplete", r.stderr)

    # ---- job-side verification (mocked kernel tree) ---------------------

    def _fake_job_tree(self, mem_max):
        slc = self.tmp / "c55x.slice"
        job = slc / L.EMU_UNIT
        job.mkdir(parents=True)
        g = 1 << 30
        vals = {job: {"memory.max": str(mem_max), "memory.swap.max": "0", "pids.max": "32",
                      "memory.oom.group": "1", "cpu.max": "100000 100000"},
                slc: {"memory.max": str(4 * g), "memory.high": str(3 * g),
                      "memory.swap.max": "0", "pids.max": "64", "cpu.max": "200000 100000"}}
        for d, kv in vals.items():
            for k, v in kv.items():
                (d / k).write_text(v + "\n")
        return job

    def _inner_args(self, mem):
        import argparse
        return argparse.Namespace(unit=L.EMU_UNIT, mem_bytes=mem, cpu_sec=600,
                                  as_bytes=2 << 30, cmd=["payload", "arg"])

    def test_job_side_verification_execs_only_when_values_match(self):
        import resource
        job = self._fake_job_tree(64 << 20)
        ok = {resource.RLIMIT_CPU: (600, 600), resource.RLIMIT_NOFILE: (512, 512),
              resource.RLIMIT_CORE: (0, 0), resource.RLIMIT_AS: (2 << 30, 2 << 30)}
        with mock.patch.object(L, "cgroup2_dir", lambda: job), \
                mock.patch.object(L, "LOCK_DIR", self.tmp / "locks"), \
                mock.patch("resource.getrlimit", lambda r: ok[r]), \
                mock.patch("os.execvp") as ex:
            L.inner_verify(self._inner_args(64 << 20))
            ex.assert_called_once_with("payload", ["payload", "arg"])
            ex.reset_mock()
            with self.assertRaises(SystemExit) as cm:        # expectation differs from kernel
                L.inner_verify(self._inner_args(128 << 20))
            self.assertEqual(cm.exception.code, 78)
            self.assertTrue((self.tmp / "locks" / (L.EMU_UNIT + ".verify-failed")).exists())
            (job / "memory.max").write_text("max\n")         # relaxed job
            with self.assertRaises(SystemExit) as cm:
                L.inner_verify(self._inner_args(64 << 20))
            self.assertEqual(cm.exception.code, 78)
            ex.assert_not_called()
        bad = dict(ok)
        bad[resource.RLIMIT_NOFILE] = (1024, 1024)
        (job / "memory.max").write_text(str(64 << 20) + "\n")
        with mock.patch.object(L, "cgroup2_dir", lambda: job), \
                mock.patch.object(L, "LOCK_DIR", self.tmp / "locks"), \
                mock.patch("resource.getrlimit", lambda r: bad[r]), \
                mock.patch("os.execvp") as ex:
            with self.assertRaises(SystemExit) as cm:
                L.inner_verify(self._inner_args(64 << 20))
            self.assertEqual(cm.exception.code, 78)
            ex.assert_not_called()

    # ---- real kernel enforcement with tiny budgets ----------------------

    def test_children_below_individual_limits_exceed_the_aggregate_budget(self):
        sentinel = self.sentinel()
        code = ("import subprocess,sys,time\n"
                "kids=[subprocess.Popen([sys.executable,'-c',"
                "'b=bytearray(30<<20)\\nfor i in range(0,len(b),4096): b[i]=1\\n"
                "import time;time.sleep(30)']) for _ in range(4)]\n"
                "time.sleep(30)")
        r = launch("--mem", "64M", "--wall-sec", "40", "--", PY, "-c", code)
        self.assertEqual(r.returncode, 79, r.stderr)           # 4 x 30 MiB > 64 MiB
        self.assertIn("outcome=oom", r.stderr)
        self.assertIsNone(sentinel.poll(), "outside sentinel must survive a local OOM")
        self.assertTrue(wait_until(lambda: not unit_active(), 10))

    def test_tmpfs_allocation_is_contained_by_the_cgroup(self):
        sentinel = self.sentinel()
        f = pathlib.Path("/dev/shm") / ("c55x-test-%s" % uuid.uuid4().hex[:8])
        try:
            code = ("import sys\nwith open(sys.argv[1],'wb') as f:\n"
                    "  for _ in range(96): f.write(b'\\1'*(1<<20)); f.flush()\n")
            r = launch("--mem", "64M", "--wall-sec", "30", "--", PY, "-c", code, str(f))
            self.assertIn(r.returncode, (79, 1), r.stderr)      # killed locally or ENOSPC-style failure
            self.assertIsNone(sentinel.poll())
        finally:
            if f.exists():
                f.unlink()                                      # our exact file

    def test_wall_timeout_stops_a_term_ignoring_child(self):
        code = ("import signal,time\nsignal.signal(signal.SIGTERM, signal.SIG_IGN)\n"
                "time.sleep(120)")
        t0 = time.time()
        r = launch("--mem", "64M", "--wall-sec", "3", "--", PY, "-c", code)
        self.assertEqual(r.returncode, 124, r.stderr)
        self.assertIn("outcome=timeout", r.stderr)
        self.assertLess(time.time() - t0, 25)
        self.assertFalse(unit_active())

    def test_cpu_limit_is_reported_as_a_timeout(self):
        r = launch("--mem", "64M", "--cpu-sec", "2", "--wall-sec", "60", "--",
                   PY, "-c", "while True: pass")
        self.assertEqual(r.returncode, 124, r.stderr)
        self.assertIn("CPU-time", r.stderr)

    def test_signal_death_is_reported_as_128_plus_n(self):
        r = launch("--mem", "64M", "--wall-sec", "20", "--", PY, "-c",
                   "import os,signal;os.kill(os.getpid(),signal.SIGSEGV)")
        self.assertEqual(r.returncode, 128 + 11, r.stderr)
        self.assertIn("outcome=signal", r.stderr)

    # ---- admission, orphans, interruption -------------------------------

    def test_two_launchers_cannot_run_two_emulator_payloads(self):
        marker = self.tmp / "second-ran"
        first = subprocess.Popen([str(LAUNCHER), "--mem", "64M", "--wall-sec", "20", "--",
                                  "sleep", "6"], cwd=ROOT)
        try:
            self.assertTrue(wait_until(unit_active, 15))
            other_cwd = pathlib.Path(tempfile.mkdtemp(dir=TMP))   # a different "worktree"
            try:
                r = launch("--mem", "64M", "--wall-sec", "10", "--", "touch", str(marker),
                           cwd=other_cwd)
            finally:
                other_cwd.rmdir()
            self.assertEqual(r.returncode, 75, r.stderr)
            self.assertIn("outcome=busy", r.stderr)
            self.assertFalse(marker.exists())
            # a work job is a different class and is not blocked by the slot
            w = launch("--job", "work", "--mem", "64M", "--wall-sec", "10", "--", "true")
            self.assertEqual(w.returncode, 0, w.stderr)
        finally:
            first.wait(timeout=30)
        self.assertEqual(launch("--mem", "64M", "--wall-sec", "10", "--", "true").returncode, 0)

    def test_orphaned_job_keeps_its_limits_blocks_a_replacement_and_ends_by_itself(self):
        first = subprocess.Popen([str(LAUNCHER), "--mem", "64M", "--wall-sec", "8", "--",
                                  "sleep", "60"], cwd=ROOT)
        self.assertTrue(wait_until(unit_active, 15))
        first.kill()                                # SIGKILL exactly this launcher process
        first.wait()
        self.assertTrue(unit_active(), "the job must outlive its launcher")
        r = launch("--mem", "64M", "--wall-sec", "5", "--", "true")
        self.assertEqual(r.returncode, 75, r.stderr)
        self.assertTrue(unit_active(), "the replacement must not have killed the orphan")
        self.assertTrue(wait_until(lambda: not unit_active(), 25),
                        "the orphan must stop at its wall-time limit")
        self.assertEqual(launch("--mem", "64M", "--wall-sec", "10", "--", "true").returncode, 0)

    def test_launcher_interruption_stops_only_its_own_job(self):
        sentinel = self.sentinel()
        p = subprocess.Popen([str(LAUNCHER), "--mem", "64M", "--wall-sec", "60", "--",
                              "sleep", "60"], cwd=ROOT, stderr=subprocess.PIPE, text=True)
        self.assertTrue(wait_until(unit_active, 15))
        p.send_signal(signal.SIGTERM)
        err = p.communicate(timeout=30)[1]
        self.assertEqual(p.returncode, 128 + signal.SIGTERM, err)
        self.assertIn("outcome=interrupted", err)
        self.assertTrue(wait_until(lambda: not unit_active(), 15))
        self.assertIsNone(sentinel.poll())

    def test_status_reports_the_slot(self):
        r = launch("--status")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("emulator slot: free", r.stdout)
        self.assertIn("MemoryMax=4294967296", r.stdout)

    # ---- the core guard seen from the launcher --------------------------

    @unittest.skipUnless(have_guarded_core(), "set ZEN_CORE_DIR to a guarded core build")
    def test_work_class_job_cannot_construct_a_core(self):
        code = ("from tests.test_c55x_emu_long_words import machine\n"
                "try:\n    machine('nop')\nexcept Exception as e:\n    print(type(e).__name__, e)\n"
                "else:\n    print('ADMITTED')\n")
        r = launch("--job", "work", "--mem", "256M", "--wall-sec", "30", "--", PY, "-c", code)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("ResourcePolicyError", r.stdout)
        self.assertIn("may not run the emulator core", r.stdout)

    @unittest.skipUnless(have_guarded_core(), "set ZEN_CORE_DIR to a guarded core build")
    def test_similarly_named_group_outside_the_shared_slice_is_refused(self):
        unit = "c55xfake-%s.service" % uuid.uuid4().hex[:8]
        code = ("from tests.test_c55x_emu_long_words import machine\n"
                "try:\n    machine('nop')\nexcept Exception as e:\n    print(type(e).__name__, e)\n"
                "else:\n    print('ADMITTED')\n")
        cmd = ["systemd-run", "--user", "--wait", "--pipe", "--quiet", "--collect",
               "--expand-environment=no", "--slice=c55xfake.slice", "--unit=" + unit,
               "--working-directory", str(ROOT),
               "-p", "MemoryMax=128M", "-p", "MemorySwapMax=0", "-p", "TasksMax=16",
               "-p", "LimitCPU=300", "-p", "LimitNOFILE=256", "-p", "LimitCORE=0",
               "-p", "LimitAS=2147483648", "-p", "RuntimeMaxSec=60",
               "--setenv=C55X_CONTAINED_JOB=emulator", "--setenv=ZEN_CORE_DIR",
               "--setenv=PYTHONPATH=" + str(ROOT), "--", PY, "-c", code]
        try:
            r = subprocess.run(cmd, capture_output=True, text=True, timeout=60)
        finally:
            subprocess.run(["systemctl", "--user", "stop", unit, "c55xfake.slice"],
                           capture_output=True)                    # our exact fake units
        self.assertIn("ResourcePolicyError", r.stdout, r.stdout + r.stderr)
        self.assertNotIn("ADMITTED", r.stdout)


if __name__ == "__main__":
    unittest.main()
