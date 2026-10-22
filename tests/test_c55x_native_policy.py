"""Containment tests, scope C: the pure native policy logic.

rust/c55x_core/src/policy.rs has no dependency on pyo3, so its own unit tests
(fake cgroup trees under a temporary directory: wrong group, relaxed
ancestor, missing controller file, relaxed rlimits, wrong job class, ...) are
compiled and run standalone. No kernel state is touched.
"""

import pathlib
import shutil
import subprocess
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parent.parent


@unittest.skipUnless(shutil.which("rustc"), "rustc not installed")
class NativePolicyTests(unittest.TestCase):
    def test_policy_unit_tests_pass(self):
        with tempfile.TemporaryDirectory(dir=ROOT / "build") as d:
            exe = pathlib.Path(d) / "policy_tests"
            c = subprocess.run(["rustc", "--edition", "2021", "--test",
                                str(ROOT / "rust/c55x_core/src/policy.rs"), "-o", str(exe)],
                               capture_output=True, text=True)
            self.assertEqual(c.returncode, 0, c.stderr)
            r = subprocess.run([str(exe)], capture_output=True, text=True)
            self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
            self.assertRegex(r.stdout, r"test result: ok\. (\d+) passed; 0 failed")
            passed = int(__import__("re").search(r"(\d+) passed", r.stdout).group(1))
            self.assertGreaterEqual(passed, 17)


if __name__ == "__main__":
    unittest.main()
