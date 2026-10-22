"""Containment tests, scope B: bin/c55x-contained.

Must run OUTSIDE a job (plain `python3 -m unittest tests.test_c55x_launcher`):
the launcher is the thing under test and the slot tests need every emulator
slot free. Real kernel enforcement is exercised with tiny budgets (64 MiB)
in bounded jobs; cases that would need real host exhaustion or kernel
mutation outside the job use mocks and say so (MockedLauncherTests makes no
systemd call at all).

Every process these tests start is owned by the test and is cleaned up by
exact handle or exact unit name; nothing broad is killed. Tests that build a
core run the native runner c55x-run (C55X_RUN, default build/rust-target/release/
c55x-run) on the probe of rust/fixtures/launcher; nothing here imports the emulator.
"""

import argparse
import fcntl
import importlib.machinery
import importlib.util
import io
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
EMU_UNITS = (EMU_UNIT,) + tuple("c55x-emu-%d.service" % k for k in range(1, 10))
LEGACY_UNITS = EMU_UNITS[:8]                        # the units older cores accept
PY = sys.executable
ONE_SLOT = dict(os.environ, C55X_TEST_SLOTS="1")    # the job can only get EMU_UNIT
TWO_SLOTS = dict(os.environ, C55X_TEST_SLOTS="2")
# payload: write the name of its own job unit to argv[1], then sleep argv[2] s
UNIT_OF_JOB = ("import pathlib,sys,time\n"
               "rel=[l for l in open('/proc/self/cgroup') if l.startswith('0::')][0][3:].strip()\n"
               "pathlib.Path(sys.argv[1]).write_text(pathlib.PurePath(rel).name)\n"
               "time.sleep(float(sys.argv[2]))\n")


def load_launcher():
    loader = importlib.machinery.SourceFileLoader("c55x_contained", str(LAUNCHER))
    spec = importlib.util.spec_from_loader(loader.name, loader)
    mod = importlib.util.module_from_spec(spec)
    loader.exec_module(mod)
    return mod


L = load_launcher()
HAVE_MANAGER = L.user_bus_env() is not None
IN_JOB = "C55X_CONTAINED_JOB" in os.environ


# The core runs as the native runner c55x-run (C55X_RUN, default: the release build of
# scripts/cargo-test-contained's target dir) on the probe of rust/fixtures/launcher: a call
# that stores 0x100 to output.pcm. Nothing here imports the emulator.
RUNNER = pathlib.Path(os.environ.get("C55X_RUN") or ROOT / "build" / "rust-target" / "release" / "c55x-run")
PROBE = ROOT / "rust" / "fixtures" / "launcher"


def have_runner():
    return RUNNER.is_file()


def probe(out):
    """The c55x-run command line of the probe, writing to the directory out."""
    return [str(RUNNER), "--script", str(PROBE / "probe.call.json"), "--input", str(PROBE / "input.bin"),
            "--out", str(out)]


def launch(*args, cwd=ROOT, env=None, input=None, timeout=90):
    return subprocess.run([str(LAUNCHER), *args], cwd=cwd, env=env, input=input,
                          capture_output=True, text=True, timeout=timeout)


def unit_active(unit=EMU_UNIT):
    return subprocess.run(["systemctl", "--user", "is-active", "--quiet", unit]
                          ).returncode == 0


def any_slot_active():
    return any(unit_active(u) for u in EMU_UNITS)


def written(*paths):
    return all(p.exists() and p.read_text() for p in paths)


def fake_job_tree(root, unit, mem_max):
    """A mocked kernel tree holding exactly the values the launcher sets."""
    slc = root / "c55x.slice"
    job = slc / unit
    job.mkdir(parents=True)
    g = 1 << 30
    vals = {job: {"memory.max": str(mem_max), "memory.swap.max": "0", "pids.max": "32",
                  "memory.oom.group": "1", "cpu.max": "100000 100000"},
            slc: {"memory.max": str(18 * g), "memory.high": str(17 * g),
                  "memory.swap.max": "0", "pids.max": "448", "cpu.max": "1000000 100000"}}
    for d, kv in vals.items():
        for k, v in kv.items():
            (d / k).write_text(v + "\n")
    return job


def inner_args(unit, mem, as_bytes=8 << 30):
    return argparse.Namespace(unit=unit, mem_bytes=mem, cpu_sec=600,
                              as_bytes=as_bytes, cmd=["payload", "arg"])


def good_rlimits(as_bytes=8 << 30):
    import resource
    return {resource.RLIMIT_CPU: (600, 600), resource.RLIMIT_NOFILE: (512, 512),
            resource.RLIMIT_CORE: (0, 0), resource.RLIMIT_AS: (as_bytes, as_bytes)}


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
        wait_until(lambda: not any_slot_active(), 30)

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
            self.assertEqual(v["unit"] in EMU_UNITS, job == "emulator")
            self.assertEqual(v["job"], [str(64 << 20), "0", "32", "1", "100000 100000"])
            self.assertEqual(v["shared"], [str(18 * g), str(17 * g), "0", "448", "1000000 100000"])

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

    # ---- real kernel enforcement with tiny budgets ----------------------

    def test_children_below_individual_limits_exceed_the_aggregate_budget(self):
        sentinel = self.sentinel()
        code = ("import subprocess,sys,time\n"
                "kids=[subprocess.Popen([sys.executable,'-c',"
                "'b=bytearray(30<<20)\\nfor i in range(0,len(b),4096): b[i]=1\\n"
                "import time;time.sleep(30)']) for _ in range(4)]\n"
                "time.sleep(30)")
        r = launch("--mem", "64M", "--wall-sec", "40", "--", PY, "-c", code, env=ONE_SLOT)
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
        r = launch("--mem", "64M", "--wall-sec", "3", "--", PY, "-c", code, env=ONE_SLOT)
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

    def test_concurrent_emulator_jobs_get_distinct_slots_until_all_are_busy(self):
        names = [self.tmp / "unit-a", self.tmp / "unit-b"]
        jobs = [subprocess.Popen([str(LAUNCHER), "--mem", "64M", "--wall-sec", "30", "--",
                                  PY, "-c", UNIT_OF_JOB, str(n), "8"], cwd=ROOT, env=TWO_SLOTS)
                for n in names]
        try:
            self.assertTrue(wait_until(lambda: written(*names), 20))
            self.assertEqual({n.read_text() for n in names}, {EMU_UNIT, "c55x-emu-1.service"})
            marker = self.tmp / "third-ran"
            other_cwd = pathlib.Path(tempfile.mkdtemp(dir=TMP))   # a different "worktree"
            try:
                r = launch("--mem", "64M", "--wall-sec", "10", "--", "touch", str(marker),
                           cwd=other_cwd, env=TWO_SLOTS)
            finally:
                other_cwd.rmdir()
            self.assertEqual(r.returncode, 75, r.stderr)
            self.assertIn("outcome=busy", r.stderr)
            self.assertIn("all 2 emulator slots are busy", r.stderr)
            self.assertFalse(marker.exists())
            st = launch("--status", env=TWO_SLOTS)
            self.assertIn("emulator slot 0: busy c55x-emu.service", st.stdout)
            self.assertIn("emulator slot 1: busy c55x-emu-1.service", st.stdout)
            # a work job is a different class and is not blocked by the slots
            w = launch("--job", "work", "--mem", "64M", "--wall-sec", "10", "--", "true",
                       env=TWO_SLOTS)
            self.assertEqual(w.returncode, 0, w.stderr)
        finally:
            rcs = [j.wait(timeout=40) for j in jobs]
        self.assertEqual(rcs, [0, 0])
        self.assertEqual(launch("--mem", "64M", "--wall-sec", "10", "--", "true",
                                env=TWO_SLOTS).returncode, 0)

    def test_ten_slots_admit_ten_jobs_and_refuse_an_eleventh(self):
        names = [self.tmp / ("unit-%d" % i) for i in range(10)]
        jobs = [subprocess.Popen([str(LAUNCHER), "--mem", "64M", "--wall-sec", "40", "--",
                                  PY, "-c", UNIT_OF_JOB, str(n), "10"], cwd=ROOT)
                for n in names]
        try:
            self.assertTrue(wait_until(lambda: written(*names), 30))
            self.assertEqual(sorted(n.read_text() for n in names), sorted(EMU_UNITS))
            r = launch("--mem", "64M", "--wall-sec", "10", "--", "true")
            self.assertEqual(r.returncode, 75, r.stderr)
            self.assertIn("all 10 emulator slots are busy", r.stderr)
        finally:
            rcs = [j.wait(timeout=60) for j in jobs]
        self.assertEqual(rcs, [0] * 10)

    def test_legacy_slots_wait_for_slots_0_to_7_while_8_and_9_are_free(self):
        names = [self.tmp / ("unit-%d" % i) for i in range(8)]
        jobs = [subprocess.Popen([str(LAUNCHER), "--legacy-slots", "--mem", "64M", "--wall-sec",
                                  "40", "--", PY, "-c", UNIT_OF_JOB, str(n), "10"], cwd=ROOT)
                for n in names]
        try:
            self.assertTrue(wait_until(lambda: written(*names), 30))
            self.assertEqual(sorted(n.read_text() for n in names), sorted(LEGACY_UNITS))
            r = launch("--legacy-slots", "--mem", "64M", "--wall-sec", "10", "--", "true")
            self.assertEqual(r.returncode, 75, r.stderr)
            self.assertIn("all 8 legacy emulator slots are busy", r.stderr)
            got = self.tmp / "unit"                 # a launch without the option gets slot 8
            r = launch("--mem", "64M", "--wall-sec", "10", "--", PY, "-c", UNIT_OF_JOB, str(got), "0")
            self.assertEqual(r.returncode, 0, r.stderr)
            self.assertEqual(got.read_text(), "c55x-emu-8.service")
        finally:
            rcs = [j.wait(timeout=60) for j in jobs]
        self.assertEqual(rcs, [0] * 8)

    def test_orphaned_job_keeps_its_limits_blocks_only_its_slot_and_ends_by_itself(self):
        first = subprocess.Popen([str(LAUNCHER), "--mem", "64M", "--wall-sec", "12", "--",
                                  "sleep", "60"], cwd=ROOT, env=ONE_SLOT)
        self.assertTrue(wait_until(unit_active, 15))
        first.kill()                                # SIGKILL exactly this launcher process
        first.wait()
        self.assertTrue(unit_active(), "the job must outlive its launcher")
        r = launch("--mem", "64M", "--wall-sec", "5", "--", "true", env=ONE_SLOT)
        self.assertEqual(r.returncode, 75, r.stderr)
        self.assertIn("c55x-emu.service active", r.stderr)
        self.assertTrue(unit_active(), "the replacement must not have killed the orphan")
        got = self.tmp / "unit"                     # the next slot is still available
        r = launch("--mem", "64M", "--wall-sec", "10", "--", PY, "-c", UNIT_OF_JOB, str(got), "0",
                   env=TWO_SLOTS)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(got.read_text(), "c55x-emu-1.service")
        self.assertTrue(unit_active(), "the orphan must keep its own slot")
        self.assertTrue(wait_until(lambda: not unit_active(), 30),
                        "the orphan must stop at its wall-time limit")
        self.assertEqual(launch("--mem", "64M", "--wall-sec", "10", "--", "true",
                                env=ONE_SLOT).returncode, 0)

    def test_launcher_interruption_stops_only_its_own_job(self):
        sentinel = self.sentinel()
        p = subprocess.Popen([str(LAUNCHER), "--mem", "64M", "--wall-sec", "60", "--",
                              "sleep", "60"], cwd=ROOT, env=ONE_SLOT,
                             stderr=subprocess.PIPE, text=True)
        self.assertTrue(wait_until(unit_active, 15))
        p.send_signal(signal.SIGTERM)
        err = p.communicate(timeout=30)[1]
        self.assertEqual(p.returncode, 128 + signal.SIGTERM, err)
        self.assertIn("outcome=interrupted", err)
        self.assertTrue(wait_until(lambda: not unit_active(), 15))
        self.assertIsNone(sentinel.poll())

    def test_status_reports_every_slot(self):
        r = launch("--status")
        self.assertEqual(r.returncode, 0, r.stderr)
        for k, unit in enumerate(EMU_UNITS):
            self.assertIn("emulator slot %d: free %s" % (k, unit), r.stdout)
        self.assertNotIn("emulator slot 10", r.stdout)
        self.assertIn("MemoryMax=19327352832", r.stdout)
        self.assertIn("TasksMax=448", r.stdout)

    # ---- the core guard seen from the launcher --------------------------

    @unittest.skipUnless(have_runner(), "build c55x-run (or set C55X_RUN)")
    def test_work_class_job_cannot_construct_a_core(self):
        TMP.mkdir(parents=True, exist_ok=True)
        out = pathlib.Path(tempfile.mkdtemp(dir=TMP)) / "out"
        r = launch("--job", "work", "--mem", "256M", "--wall-sec", "30", "--", *probe(out))
        self.assertEqual(r.returncode, 3, r.stdout + r.stderr)       # c55x-run: refused
        self.assertIn("may not run the emulator core", r.stderr)
        self.assertFalse((out / "output.pcm").exists())

    @unittest.skipUnless(have_runner(), "build c55x-run (or set C55X_RUN)")
    def test_core_runs_in_a_slot_other_than_zero(self):
        TMP.mkdir(parents=True, exist_ok=True)
        out = pathlib.Path(tempfile.mkdtemp(dir=TMP)) / "out"
        first = subprocess.Popen([str(LAUNCHER), "--mem", "64M", "--wall-sec", "30", "--",
                                  "sleep", "10"], cwd=ROOT, env=ONE_SLOT)
        try:
            self.assertTrue(wait_until(unit_active, 15))
            code = ("import pathlib, subprocess, sys\n"
                    "rel=[l for l in open('/proc/self/cgroup') if l.startswith('0::')][0][3:].strip()\n"
                    "subprocess.run(sys.argv[1:], check=True)\n"
                    "pcm=(pathlib.Path(sys.argv[-1]) / 'output.pcm').read_bytes()\n"
                    "print(pathlib.PurePath(rel).name, pcm.hex())\n")
            r = launch("--mem", "256M", "--wall-sec", "30", "--", PY, "-c", code, *probe(out), env=TWO_SLOTS)
        finally:
            first.wait(timeout=40)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(r.stdout.split(), ["c55x-emu-1.service", "0001"])      # 0x100, s16le

    @unittest.skipUnless(have_runner(), "build c55x-run (or set C55X_RUN)")
    def test_similarly_named_group_outside_the_shared_slice_is_refused(self):
        TMP.mkdir(parents=True, exist_ok=True)
        out = pathlib.Path(tempfile.mkdtemp(dir=TMP)) / "out"
        unit = "c55xfake-%s.service" % uuid.uuid4().hex[:8]
        cmd = ["systemd-run", "--user", "--wait", "--pipe", "--quiet", "--collect",
               "--expand-environment=no", "--slice=c55xfake.slice", "--unit=" + unit,
               "--working-directory", str(ROOT),
               "-p", "MemoryMax=128M", "-p", "MemorySwapMax=0", "-p", "TasksMax=16",
               "-p", "LimitCPU=300", "-p", "LimitNOFILE=256", "-p", "LimitCORE=0",
               "-p", "LimitAS=2147483648", "-p", "RuntimeMaxSec=60",
               "--setenv=C55X_CONTAINED_JOB=emulator", "--", *probe(out)]
        try:
            r = subprocess.run(cmd, capture_output=True, text=True, timeout=60)
        finally:
            subprocess.run(["systemctl", "--user", "stop", unit, "c55xfake.slice"],
                           capture_output=True)                    # our exact fake units
        self.assertEqual(r.returncode, 3, r.stdout + r.stderr)       # c55x-run: refused
        self.assertIn("refused outside the managed containment", r.stderr)
        self.assertFalse((out / "output.pcm").exists())


class MockedLauncherTests(unittest.TestCase):
    """Slot naming, the C55X_TEST_SLOTS hook, the slot choice, unit start
    failures, --status and the job-side verifier, against a fake unit table,
    a private lock directory and mocked kernel trees. No systemd call and no
    subprocess is made, and the real locks are never touched, so these run
    anywhere."""

    def setUp(self):
        TMP.mkdir(parents=True, exist_ok=True)
        self.tmp = pathlib.Path(tempfile.mkdtemp(dir=TMP))
        self.addCleanup(shutil.rmtree, self.tmp)
        self.states = {}           # unit -> ActiveState (default inactive, not loaded)
        self.props = {}            # unit -> other properties (LoadState, InvocationID, ...)
        self.unreadable = set()    # units whose `show` fails
        self.calls = []

        def fake_systemctl(env, *args):
            self.calls.append(args)
            if args[0] == "show":
                unit, names = args[1], args[3::2]
                if unit in self.unreadable:
                    return subprocess.CompletedProcess(args, 1, "", "")
                state = self.states.get(unit, "inactive")
                props = {"ActiveState": state, "InvocationID": "",
                         "LoadState": "not-found" if state == "inactive" else "loaded"}
                props.update(self.props.get(unit, {}))
                out = "".join("%s=%s\n" % (n, props.get(n, "")) for n in names)
                return subprocess.CompletedProcess(args, 0, out, "")
            self.assertIn(args[0], ("set-property", "reset-failed", "stop"), args)
            return subprocess.CompletedProcess(args, 0, "", "")

        for p in (mock.patch.object(L, "systemctl", fake_systemctl),
                  mock.patch.object(L, "LOCK_DIR", self.tmp / "locks"),
                  mock.patch.object(L.subprocess, "run", side_effect=AssertionError("subprocess")),
                  mock.patch.object(L.subprocess, "Popen", side_effect=AssertionError("subprocess")),
                  mock.patch.dict(os.environ)):
            p.start()
            self.addCleanup(p.stop)
        os.environ.pop("C55X_TEST_SLOTS", None)
        os.environ.pop("C55X_CONTAINED_JOB", None)

    def test_slot_names(self):
        self.assertEqual((L.SLOTS, L.LEGACY_SLOTS), (10, 8))
        self.assertEqual(L.EMU_UNITS, EMU_UNITS)
        self.assertEqual([L.slot_unit(k) for k in range(10)], list(EMU_UNITS))
        self.assertEqual([L.slot_lock(k) for k in (0, 1, 7, 9)],
                         ["emulator.lock", "emulator-1.lock", "emulator-7.lock", "emulator-9.lock"])

    def test_slot_hook_can_only_lower_the_count(self):
        for inj, want, legacy in (("2", 2, 2), ("1", 1, 1), ("0", 0, 0), ("8", 8, 8),
                                  ("9", 9, 8), ("10", 10, 8), ("99", 10, 8), ("-1", 10, 8),
                                  ("x", 10, 8), ("", 10, 8)):
            os.environ["C55X_TEST_SLOTS"] = inj
            self.assertEqual(L.slot_count(), want, inj)
            self.assertEqual(L.slot_count(legacy=True), legacy, inj)

    def test_first_free_slot_is_taken_and_a_freed_slot_is_reused(self):
        a, b = L.Admission(), L.Admission()
        self.assertEqual(a.acquire(None), EMU_UNIT)
        self.assertEqual(b.acquire(None), "c55x-emu-1.service")       # slot 0 lock is held
        self.assertEqual((self.tmp / "locks" / "emulator-1.lock").read_text(),
                         "%d %d\n" % (os.getpid(), L.MAX_MEM_BYTES))     # pid, memory ceiling
        a.release()
        c = L.Admission()
        self.assertEqual(c.acquire(None), EMU_UNIT)
        b.release()
        c.release()

    def test_active_unit_blocks_only_its_slot_and_its_lock_is_not_kept(self):
        self.states = {EMU_UNIT: "active", "c55x-emu-2.service": "active"}   # e.g. orphans
        a, b = L.Admission(), L.Admission()
        self.assertEqual(a.acquire(None), "c55x-emu-1.service")
        self.assertEqual(b.acquire(None), "c55x-emu-3.service")
        for name in ("emulator.lock", "emulator-2.lock"):   # the skipped slots' locks are free
            fd = os.open(str(self.tmp / "locks" / name), os.O_RDWR)
            try:
                fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            finally:
                os.close(fd)
        a.release()
        b.release()

    def test_unit_states_other_than_inactive_or_failed_keep_a_slot_busy(self):
        for state in ("activating", "active", "reloading", "deactivating", "maintenance",
                      "refreshing", ""):
            with self.subTest(state=state):
                self.states = {EMU_UNIT: state}
                a = L.Admission()
                self.assertEqual(a.acquire(None), "c55x-emu-1.service")
                self.assertEqual(a.busy, ["c55x-emu.service %s" % (state or "state unreadable")])
                a.release()
        self.states = {}
        self.unreadable = {EMU_UNIT}                         # `systemctl show` fails
        a = L.Admission()
        self.assertEqual(a.acquire(None), "c55x-emu-1.service")
        self.assertEqual(a.busy, ["c55x-emu.service state unreadable"])
        a.release()
        self.unreadable = set()
        for state in ("inactive", "failed"):
            with self.subTest(state=state):
                self.states = {EMU_UNIT: state}
                a = L.Admission()
                self.assertEqual(a.acquire(None), EMU_UNIT)
                a.release()

    def test_no_free_slot_names_every_slot(self):
        self.states = dict.fromkeys(EMU_UNITS[1:], "active")
        holder = L.Admission()
        self.assertEqual(holder.acquire(None), EMU_UNIT)
        other = L.Admission()
        self.assertIsNone(other.acquire(None))
        self.assertIsNone(other.fd)
        self.assertEqual(other.busy, ["c55x-emu.service held"] +
                         ["%s active" % u for u in EMU_UNITS[1:]])
        holder.release()
        os.environ["C55X_TEST_SLOTS"] = "0"
        self.assertIsNone(L.Admission().acquire(None))

    def test_launcher_exits_75_when_every_slot_is_busy(self):
        self.states = dict.fromkeys(EMU_UNITS, "active")
        self.states["c55x-emu-4.service"] = "deactivating"
        err = io.StringIO()
        with mock.patch.object(L, "user_bus_env", lambda: ({}, self.tmp)), \
                mock.patch.object(L, "mem_available", lambda: 64 << 30), \
                mock.patch.object(L.sys, "stderr", err):
            with self.assertRaises(SystemExit) as cm:
                L.main(["--mem", "64M", "--work-dir", str(self.tmp / "w"), "--", "true"])
        self.assertEqual(cm.exception.code, 75)
        self.assertIn("outcome=busy all 10 emulator slots are busy", err.getvalue())
        for u in EMU_UNITS:
            state = "deactivating" if u == "c55x-emu-4.service" else "active"
            self.assertIn("%s %s" % (u, state), err.getvalue())
        self.assertFalse([c for c in self.calls if c[0] == "set-property"])
        os.environ["C55X_TEST_SLOTS"] = "2"
        err = io.StringIO()
        with mock.patch.object(L, "user_bus_env", lambda: ({}, self.tmp)), \
                mock.patch.object(L, "mem_available", lambda: 64 << 30), \
                mock.patch.object(L.sys, "stderr", err):
            with self.assertRaises(SystemExit) as cm:
                L.main(["--mem", "64M", "--work-dir", str(self.tmp / "w"), "--", "true"])
        self.assertEqual(cm.exception.code, 75)
        self.assertIn("all 2 emulator slots are busy", err.getvalue())
        self.assertNotIn("c55x-emu-2.service", err.getvalue())

    def test_legacy_slots_take_only_slots_0_to_7(self):
        self.states = dict.fromkeys(LEGACY_UNITS, "active")
        a = L.Admission()
        self.assertIsNone(a.acquire(None, legacy=True))
        self.assertEqual(a.busy, ["%s active" % u for u in LEGACY_UNITS])
        self.assertEqual(a.acquire(None), "c55x-emu-8.service")
        a.release()
        self.states = {}
        b = L.Admission()
        self.assertEqual(b.acquire(None, legacy=True), EMU_UNIT)       # lowest slot first
        b.release()

    def test_legacy_launch_exits_75_while_only_slots_8_and_9_are_free(self):
        self.states = dict.fromkeys(LEGACY_UNITS, "active")
        err = io.StringIO()
        with mock.patch.object(L, "user_bus_env", lambda: ({}, self.tmp)), \
                mock.patch.object(L, "mem_available", lambda: 64 << 30), \
                mock.patch.object(L.sys, "stderr", err):
            with self.assertRaises(SystemExit) as cm:
                L.main(["--legacy-slots", "--mem", "64M", "--work-dir", str(self.tmp / "w"),
                        "--", "true"])
            self.assertEqual(cm.exception.code, 75)
            self.assertIn("outcome=busy all 8 legacy emulator slots are busy", err.getvalue())
            self.assertNotIn("c55x-emu-8.service", err.getvalue())
            self.assertFalse([c for c in self.calls if c[0] == "set-property"])
            with self.assertRaises(SystemExit) as cm:          # meaningless for work jobs
                L.main(["--job", "work", "--legacy-slots", "--mem", "64M",
                        "--work-dir", str(self.tmp / "w"), "--", "true"])
            self.assertEqual(cm.exception.code, 77)
            self.assertIn("--legacy-slots applies to emulator jobs only", err.getvalue())

    def test_legacy_launch_runs_in_a_low_slot(self):
        self.states = dict.fromkeys(LEGACY_UNITS[:7], "active")
        seen = []

        class FakeProc:
            def __init__(self, cmd, env=None):
                seen.append(cmd)

            def wait(self):
                return 0
        err = io.StringIO()
        with mock.patch.object(L, "user_bus_env", lambda: ({}, self.tmp)), \
                mock.patch.object(L, "mem_available", lambda: 64 << 30), \
                mock.patch.object(L.subprocess, "Popen", FakeProc), \
                mock.patch.object(L.sys, "stderr", err):
            with self.assertRaises(SystemExit) as cm:
                L.main(["--legacy-slots", "--mem", "64M", "--work-dir", str(self.tmp / "w"),
                        "--", "true"])
        self.assertEqual(cm.exception.code, 0, err.getvalue())
        self.assertIn("--unit=c55x-emu-7.service", seen[0])

    def hold_old_style_lock(self, k, content):
        """Hold slot k's lock the way another process would, with content."""
        (self.tmp / "locks").mkdir(parents=True, exist_ok=True, mode=0o700)
        fd = os.open(str(self.tmp / "locks" / L.slot_lock(k)), os.O_CREAT | os.O_RDWR, 0o600)
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        os.ftruncate(fd, 0)
        os.write(fd, content)
        self.addCleanup(os.close, fd)
        return fd

    def test_memory_admission_counts_held_locks_and_orphans(self):
        gib, mib = 1 << 30, 1 << 20
        a, b = L.Admission(), L.Admission()
        self.assertEqual(a.acquire(None, mem=gib), EMU_UNIT)
        self.assertEqual(b.acquire(None, mem=256 * mib), "c55x-emu-1.service")
        self.assertEqual(b.memory_of_others(None), gib)              # a's recorded ceiling
        self.assertEqual(a.memory_of_others(None), 256 * mib)
        # an orphan (unit running, lock free) counts its MemoryMax, unknown counts 2 GiB
        self.states = {"c55x-emu-5.service": "active", "c55x-emu-9.service": "active"}
        self.props = {"c55x-emu-5.service": {"MemoryMax": str(gib)},
                      "c55x-emu-9.service": {"MemoryMax": "infinity"}}
        self.assertEqual(b.memory_of_others(None), gib + gib + 2 * gib)
        self.states, self.props = {}, {}
        # an older launcher records only its pid: the unit's MemoryMax, else 2 GiB
        fd = self.hold_old_style_lock(3, b"12345\n")
        self.assertEqual(b.memory_of_others(None), gib + 2 * gib)
        self.props = {"c55x-emu-3.service": {"MemoryMax": str(512 * mib)}}
        self.assertEqual(b.memory_of_others(None), gib + 512 * mib)
        fcntl.flock(fd, fcntl.LOCK_UN)
        # a released lock no longer counts, whatever its file still says
        a.release()
        self.assertEqual(b.memory_of_others(None), 0)
        self.assertIn(b"%d" % gib, (self.tmp / "locks" / "emulator.lock").read_bytes())
        # /proc/locks unreadable: every existing lock file counts as held (stricter):
        # slot 0's stale record and slot 3's pid-only file (its unit's MemoryMax)
        with mock.patch.object(L, "held_flocks", lambda: None):
            self.assertEqual(b.memory_of_others(None), gib + 512 * mib)
        b.release()

    def test_launcher_exits_75_when_the_memory_ceilings_would_exceed_memory_high(self):
        gib = 1 << 30
        holders = [L.Admission() for _ in range(8)]
        for h in holders:
            self.assertIsNotNone(h.acquire(None, mem=2 * gib))        # 16 GiB in use
        code, err = self._launch_mocked(lambda: None, 0, ("--mem", "2G"))
        self.assertEqual(code, 75, err)
        self.assertIn("outcome=busy memory admission", err)
        self.assertIn("(16384 MiB) plus --mem (2048 MiB) exceed the shared memory.high "
                      "(17408 MiB)", err)
        self.assert_slot_lock_free("emulator-8.lock")                # its slot was given back
        self.assertFalse([c for c in self.calls if c[0] == "set-property"])
        code, err = self._launch_mocked(lambda: None, 0, ("--mem", "1G"))   # exactly 17 GiB
        self.assertEqual(code, 0, err)
        for h in holders:
            h.release()

    def _launch_mocked(self, on_wait, rc, argv=("--mem", "64M")):
        """L.main for one launch whose systemd-run is a fake: on_wait runs
        when the launcher waits for it, rc is its exit status."""
        class FakeProc:
            def __init__(self, cmd, env=None):
                self.cmd = cmd

            def wait(self):
                on_wait()
                return rc
        err = io.StringIO()
        with mock.patch.object(L, "user_bus_env", lambda: ({}, self.tmp)), \
                mock.patch.object(L, "mem_available", lambda: 64 << 30), \
                mock.patch.object(L.subprocess, "Popen", FakeProc), \
                mock.patch.object(L.sys, "stderr", err):
            with self.assertRaises(SystemExit) as cm:
                L.main([*argv, "--work-dir", str(self.tmp / "w"), "--", "true"])
        return cm.exception.code, err.getvalue()

    def assert_slot_lock_free(self, name="emulator.lock"):
        fd = os.open(str(self.tmp / "locks" / name), os.O_RDWR)
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        finally:
            os.close(fd)

    def test_a_unit_that_systemd_run_could_not_start_is_not_classified(self):
        # the slot's unit is still loaded (inactive) with an earlier invocation;
        # systemd-run fails and the unit keeps that invocation
        self.states = {EMU_UNIT: "inactive"}
        self.props = {EMU_UNIT: {"LoadState": "loaded", "InvocationID": "old",
                                 "Result": "oom-kill", "ExecMainStatus": "9"}}
        with mock.patch.object(L, "classify", side_effect=AssertionError("classified")):
            code, err = self._launch_mocked(lambda: None, 1)
        self.assertEqual(code, 75, err)
        self.assertIn("outcome=busy systemd-run (rc=1) did not start a new c55x-emu.service", err)
        self.assertNotIn("oom", err)
        self.assert_slot_lock_free()
        # the same for a loaded unit that never ran (no invocation at all)
        self.props = {EMU_UNIT: {"LoadState": "loaded"}}
        with mock.patch.object(L, "classify", side_effect=AssertionError("classified")):
            code, err = self._launch_mocked(lambda: None, 1)
        self.assertEqual(code, 75, err)
        self.assert_slot_lock_free()

    def test_a_unit_that_was_started_is_classified(self):
        self.states = {EMU_UNIT: "inactive"}
        self.props = {EMU_UNIT: {"LoadState": "loaded", "InvocationID": "old"}}

        def ran_and_failed():                       # a new invocation that exited 3
            self.states[EMU_UNIT] = "failed"
            self.props[EMU_UNIT] = {"LoadState": "loaded", "InvocationID": "new",
                                    "Result": "exit-code", "ExecMainStatus": "3"}
        code, err = self._launch_mocked(ran_and_failed, 3)
        self.assertEqual(code, 3, err)
        self.assertIn("outcome=payload rc=3", err)
        self.assert_slot_lock_free()

        def ran_and_was_collected():                # succeeded, unit already unloaded
            self.states.pop(EMU_UNIT, None)
            self.props.pop(EMU_UNIT, None)
        code, err = self._launch_mocked(ran_and_was_collected, 0)
        self.assertEqual(code, 0, err)
        self.assert_slot_lock_free()

    def test_status_is_read_only_and_lists_every_slot(self):
        self.states = {EMU_UNIT: "active", "c55x-emu-3.service": "deactivating"}
        out = io.StringIO()
        with mock.patch.object(L, "user_bus_env", lambda: ({}, self.tmp)), \
                mock.patch.object(L, "mem_available", lambda: 64 << 30), \
                mock.patch.object(L.sys, "stdout", out):
            self.assertEqual(L.main(["--status"]), 0)
        self.assertEqual([c[0] for c in self.calls if c[0] != "show"], [])
        lines = [l for l in out.getvalue().splitlines() if l.startswith("emulator slot")]
        self.assertEqual(len(lines), 10)
        self.assertIn("emulator slot 0: busy c55x-emu.service (active)", lines)
        self.assertIn("emulator slot 1: free c55x-emu-1.service", lines)
        self.assertIn("emulator slot 3: busy c55x-emu-3.service (deactivating)", lines)
        self.assertIn("emulator slot 7: free c55x-emu-7.service", lines)
        self.assertIn("emulator slot 9: free c55x-emu-9.service", lines)
        self.assertIn("legacy slots (--legacy-slots, cores built before the ten-slot change): 0..7",
                      out.getvalue().splitlines())

    def test_job_side_verification_execs_only_when_values_match(self):
        import resource
        job = fake_job_tree(self.tmp, EMU_UNIT, 64 << 20)
        ok = good_rlimits()
        os.environ["C55X_CONTAINED_JOB"] = "emulator"
        with mock.patch.object(L, "cgroup2_dir", lambda: job), \
                mock.patch("resource.getrlimit", lambda r: ok[r]), \
                mock.patch("os.execvp") as ex:
            L.inner_verify(inner_args(EMU_UNIT, 64 << 20))
            ex.assert_called_once_with("payload", ["payload", "arg"])
            ex.reset_mock()
            with self.assertRaises(SystemExit) as cm:        # expectation differs from kernel
                L.inner_verify(inner_args(EMU_UNIT, 128 << 20))
            self.assertEqual(cm.exception.code, 78)
            self.assertTrue((self.tmp / "locks" / (EMU_UNIT + ".verify-failed")).exists())
            (job / "memory.max").write_text("max\n")         # relaxed job
            with self.assertRaises(SystemExit) as cm:
                L.inner_verify(inner_args(EMU_UNIT, 64 << 20))
            self.assertEqual(cm.exception.code, 78)
            ex.assert_not_called()
        bad = dict(ok)
        bad[resource.RLIMIT_NOFILE] = (1024, 1024)
        (job / "memory.max").write_text(str(64 << 20) + "\n")
        with mock.patch.object(L, "cgroup2_dir", lambda: job), \
                mock.patch("resource.getrlimit", lambda r: bad[r]), \
                mock.patch("os.execvp") as ex:
            with self.assertRaises(SystemExit) as cm:
                L.inner_verify(inner_args(EMU_UNIT, 64 << 20))
            self.assertEqual(cm.exception.code, 78)
            ex.assert_not_called()

    def test_job_side_verification_checks_the_slot_unit_and_the_shared_values(self):
        unit = "c55x-emu-5.service"
        job = fake_job_tree(self.tmp, unit, 64 << 20)
        ok = good_rlimits()
        os.environ["C55X_CONTAINED_JOB"] = "emulator"
        err = io.StringIO()
        with mock.patch.object(L, "cgroup2_dir", lambda: job), \
                mock.patch("resource.getrlimit", lambda r: ok[r]), \
                mock.patch.object(L.sys, "stderr", err), \
                mock.patch("os.execvp") as ex:
            L.inner_verify(inner_args(unit, 64 << 20))
            ex.assert_called_once_with("payload", ["payload", "arg"])
            ex.reset_mock()
            for other, why in (("c55x-emu-4.service", "is not in c55x.slice/c55x-emu-4.service"),
                               ("c55x-emu-8.service", "is not in c55x.slice/c55x-emu-8.service"),
                               ("c55x-emu-10.service", "is not an emulator slot unit"),
                               ("c55x-work-1-abcdef12.service", "is not an emulator slot unit")):
                with self.assertRaises(SystemExit) as cm:
                    L.inner_verify(inner_args(other, 64 << 20))
                self.assertEqual(cm.exception.code, 78)
                self.assertIn(why, err.getvalue())
            # the earlier single-slot shared values no longer match
            for name, old in (("memory.max", str(4 << 30)), ("pids.max", "64"),
                              ("cpu.max", "200000 100000")):
                saved = (job.parent / name).read_text()
                (job.parent / name).write_text(old + "\n")
                with self.assertRaises(SystemExit) as cm:
                    L.inner_verify(inner_args(unit, 64 << 20))
                self.assertEqual(cm.exception.code, 78)
                (job.parent / name).write_text(saved)
            ex.assert_not_called()

    def test_job_side_verification_checks_the_address_space_of_each_class(self):
        tib = 1 << 40
        self.assertEqual((L.AS_EMULATOR, L.AS_WORK), (8 << 30, 64 * tib))
        work_unit = "c55x-work-1-abcdef12.service"
        job = fake_job_tree(self.tmp, work_unit, 64 << 20)
        err = io.StringIO()
        with mock.patch.object(L, "cgroup2_dir", lambda: job), \
                mock.patch.object(L.sys, "stderr", err), \
                mock.patch("os.execvp") as ex:
            os.environ["C55X_CONTAINED_JOB"] = "work"
            with mock.patch("resource.getrlimit", lambda r: good_rlimits(64 * tib)[r]):
                L.inner_verify(inner_args(work_unit, 64 << 20, 64 * tib))
                ex.assert_called_once_with("payload", ["payload", "arg"])
                ex.reset_mock()
                for wrong in (16 << 30, 8 << 30, 128 * tib):   # the old, the emulator, more
                    with self.assertRaises(SystemExit) as cm:
                        L.inner_verify(inner_args(work_unit, 64 << 20, wrong))
                    self.assertEqual(cm.exception.code, 78)
                self.assertIn("is not the work value", err.getvalue())
            # the kernel must report the work value too
            with mock.patch("resource.getrlimit", lambda r: good_rlimits(16 << 30)[r]):
                with self.assertRaises(SystemExit) as cm:
                    L.inner_verify(inner_args(work_unit, 64 << 20, 64 * tib))
                self.assertEqual(cm.exception.code, 78)
            # an emulator job keeps 8 GiB and cannot claim the work value
            os.environ["C55X_CONTAINED_JOB"] = "emulator"
            with mock.patch("resource.getrlimit", lambda r: good_rlimits(64 * tib)[r]):
                with self.assertRaises(SystemExit) as cm:
                    L.inner_verify(inner_args(EMU_UNIT, 64 << 20, 64 * tib))
                self.assertEqual(cm.exception.code, 78)
            os.environ.pop("C55X_CONTAINED_JOB")
            with self.assertRaises(SystemExit) as cm:
                L.inner_verify(inner_args(work_unit, 64 << 20, 64 * tib))
            self.assertEqual(cm.exception.code, 78)
            ex.assert_not_called()
        self.assertIn("job class None is not emulator or work", err.getvalue())

    def test_launch_asks_systemd_for_the_address_space_of_its_class(self):
        seen = []

        class FakeProc:
            def __init__(self, cmd, env=None):
                seen.append(cmd)

            def wait(self):
                return 0
        err = io.StringIO()
        with mock.patch.object(L, "user_bus_env", lambda: ({}, self.tmp)), \
                mock.patch.object(L, "mem_available", lambda: 64 << 30), \
                mock.patch.object(L.subprocess, "Popen", FakeProc), \
                mock.patch.object(L.sys, "stderr", err):
            for job, want in (("work", 64 << 40), ("emulator", 8 << 30)):
                with self.assertRaises(SystemExit) as cm:
                    L.main(["--job", job, "--mem", "64M", "--work-dir", str(self.tmp / "w"),
                            "--", "true"])
                self.assertEqual(cm.exception.code, 0, err.getvalue())
                cmd = seen.pop()
                self.assertIn("LimitAS=%d" % want, cmd)
                self.assertEqual(cmd[cmd.index("--as-bytes") + 1], str(want))
                self.assertIn("MemoryMax=%d" % (64 << 20), cmd)       # RAM stays bounded


if __name__ == "__main__":
    unittest.main()
