"""The native command-line host rust/c55x_run (c55x-run): a call script over a
memory image and a listing (decoded by the core's Rust frontend), run without
Python, giving output.pcm and run.json (c55x-cli-run/1). Synthetic programs only.

Needs the binary in C55X_RUN (cargo build --release --manifest-path
rust/Cargo.toml -p c55x_run); skipped otherwise. Runs under bin/c55x-contained
like every emulator test: the binary refuses to build a core outside it.
"""

import hashlib
import json
import os
import struct
import subprocess
import tempfile
import unittest
from pathlib import Path

BINARY = os.environ.get("C55X_RUN")
BASE = 0x20000                      # program byte address, two bytes per instruction
SENTINEL = 0xFFFF00
MEM_BYTES = 1 << 24
PLAIN = ["trn0", "trn1", "bk03", "bk47", "bkc", "bsa01", "bsa23", "bsa45", "bsa67", "bsac",
         "pdp", "dbier0", "dbier1", "bios37", "xpc", "rptc"]


def sha(data):
    return hashlib.sha256(data).hexdigest()


def initial_state():
    state = {name: 0 for name in [
        "xcdp", "xdp", "tc1", "tc2", "carry", "st3", "ier0", "ier1", "ifr0", "ifr1", "dbstat",
        "brc0", "brc1", "brs1", "csr", "rsa0", "rsa1", "rea0", "rea1", "reta", "cfct", "pc",
        "cycles"] + PLAIN}
    state.update(ac=[0] * 4, t=[0] * 4, xar=[0] * 8, acov=[0] * 4, loop_active=[None, None],
                 xsp=0x1000, ssp=0x0800, st1=0x4900, st2=0x8000, ivpd=0xFFFF, ivph=0xFFFF,
                 skip_next=False, skip_data_next=False, in_idle=False)
    return state


def write_inputs(d, program, steps, words):
    """Image (zero memory), listing of program at BASE, call script, input."""
    mem = bytes(MEM_BYTES)
    (d / "memory.bin").write_bytes(mem)
    (d / "image.json").write_text(json.dumps({
        "format": "c55x-memory-image/1", "size_bytes": MEM_BYTES, "snapshot": "memory.bin",
        "snapshot_sha256": sha(mem), "segments": []}))
    lines = ["# c55x listing/1 (synthetic)"] + ["%06x\t2\t-\t%s" % (BASE + 2 * i, text)
                                             for i, text in enumerate(program)]
    (d / "code.listing.tsv").write_text("\n".join(lines) + "\n")
    (d / "test.call.json").write_text(json.dumps({
        "schema": "c55x-call-script/1", "image": "image.json", "listing": "code.listing.tsv",
        "return_sentinel": "0x%x" % SENTINEL,
        "limits": {"max_steps_per_call": 1000, "chunk_steps": 16, "max_seconds_per_call": 10},
        "on_unmodeled_port": "error", "on_code_page_write": "error", "register_carry_over": True,
        "initial_state": initial_state(),
        "setup": [{"op": "write", "word_address": "0x1601", "words": [5]}],
        "records": {"record_bytes": 2, "partial_record": "error", "steps": steps},
        "output": {"format": "s16le", "scaling": "none"}}))
    (d / "input.bin").write_bytes(struct.pack(">%dH" % len(words), *words))


def call(regs):
    return {"op": "call", "label": "add", "entry": "0x%x" % BASE, "regs": regs}


@unittest.skipUnless(BINARY, "set C55X_RUN to the c55x-run binary")
class NativeRunTests(unittest.TestCase):
    ADD = ["AC0 = *AR0", "AC0 = AC0 + *AR1", "*AR2 = LO(AC0)", "return"]
    STEPS = [{"op": "write", "word_address": "0x1600", "source": "record", "unpack": "words_be"},
             call({"xar0": "0x1600", "xar1": "0x1601", "xar2": "0x1700"}),
             {"op": "read", "word_address": "0x1700", "words": 1, "format": "s16", "to": "output"}]

    def run_cli(self, program, steps, words, setup=None):
        """setup: a version-2 script without records (no --input), running these steps."""
        d = Path(tempfile.mkdtemp())
        write_inputs(d, program, steps, words)
        args = ["--input", str(d / "input.bin")]
        if setup is not None:
            script = json.loads((d / "test.call.json").read_text())
            script.update(schema="c55x-call-script/2", setup=setup)
            del script["records"]
            (d / "test.call.json").write_text(json.dumps(script))
            args = []
        out = d / "out"
        p = subprocess.run([BINARY, "--script", str(d / "test.call.json"), *args,
                            "--out", str(out)], capture_output=True, text=True)
        run = json.loads((out / "run.json").read_text())
        return p.returncode, run, (out / "output.pcm").read_bytes()

    def test_records_run_the_call_and_collect_the_output(self):
        rc, run, pcm = self.run_cli(self.ADD, self.STEPS, [1, 2, 0xFFFE])
        self.assertEqual((rc, run["status"], run["error"]), (0, "completed", None))
        self.assertEqual(struct.unpack("<3h", pcm), (6, 7, 3))
        self.assertEqual((run["output"]["samples"], run["output"]["sha256"]), (3, sha(pcm)))
        self.assertEqual([(c["index"], c["name"], c["instructions"], c["status"]) for c in run["calls"]],
                         [(i, "add", 4, "returned") for i in range(3)])
        t = run["totals"]
        self.assertEqual((t["calls"], t["instructions"], t["unmodeled_port_accesses"], t["code_page_writes"]),
                         (3, 12, 0, 0))
        self.assertEqual(t["cycles"], sum(c["cycles"] for c in run["calls"]))
        self.assertEqual(run["input"]["records"], 3)

    def test_registers_carry_over_between_calls(self):
        # the second call names no registers: AR0..AR2 keep their values
        steps = self.STEPS[:2] + [{"op": "call", "label": "again", "entry": "0x%x" % BASE, "regs": {}},
                                  self.STEPS[2]]
        rc, run, pcm = self.run_cli(self.ADD, steps, [1])
        self.assertEqual((rc, struct.unpack("<h", pcm)), (0, (6,)))
        self.assertEqual([c["name"] for c in run["calls"]], ["add", "again"])

    def test_port_access_fails_the_run(self):
        rc, run, _ = self.run_cli(["AR3 = *port(#0x0001)", "return"], self.STEPS, [1])
        self.assertEqual((rc, run["status"], run["calls"][0]["status"]), (1, "failed", "execution_error"))
        self.assertEqual(run["totals"]["unmodeled_port_accesses"], 1)

    def test_code_page_write_fails_the_run(self):
        steps = [self.STEPS[0], call({"xar0": "0x1600", "xar1": "0x1601", "xar2": "0x%x" % (BASE // 2)})]
        rc, run, _ = self.run_cli(self.ADD, steps, [1])
        self.assertEqual((rc, run["status"], run["calls"][0]["status"]), (1, "failed", "execution_error"))
        self.assertEqual(run["totals"]["code_page_writes"], 1)

    def test_observed_call_without_records(self):
        observe = {"final_state": True, "memory_diff_since": "start", "trace_pcs": 3}
        regs = {"xar0": "0x1600", "xar1": "0x1601", "xar2": "0x1700", "cycles": 0}
        setup = [{"op": "write", "word_address": "0x1600", "words": [1, 5]},
                 {"op": "mark_memory", "name": "start"},
                 dict(call(regs), observe=observe),
                 {"op": "restore_image"},           # the 1, 5 and the result are gone again
                 {"op": "mark_memory", "name": "start"},
                 dict(call(regs), observe=observe)]
        rc, run, pcm = self.run_cli(self.ADD, [], [], setup=setup)
        self.assertEqual((rc, run["format"], run["input"], pcm), (0, "c55x-cli-run/2", None, b""))
        first, second = run["calls"]
        self.assertEqual(first["memory_diff"], [[0x1700, 0, 6]])
        self.assertEqual(first["trace_pcs"], [BASE + 2, BASE + 4, BASE + 6])    # last 3 of 4
        state = first["final_state"]
        self.assertEqual((state["ac"][0], state["xar"][2], state["pc"]), (6, 0x1700, SENTINEL))
        self.assertEqual((state["cycles"], state["acov"], state["loop_active"]),
                         (first["cycles"], [0, 0, 0, 0], [None, None]))
        self.assertEqual(sorted(state), sorted(initial_state()))
        self.assertEqual((second["memory_diff"], second["final_state"]["ac"][0]), ([], 0))

    def test_call_without_return_fails_the_run(self):
        rc, run, _ = self.run_cli(["goto #0x%06x" % BASE], self.STEPS, [1, 2])
        self.assertEqual((rc, run["status"], len(run["calls"])), (1, "failed", 1))
        self.assertIn(run["calls"][0]["status"], ("idle_without_return", "step_limit"))


if __name__ == "__main__":
    unittest.main()
