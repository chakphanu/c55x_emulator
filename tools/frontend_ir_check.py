"""IR equality of the Rust dis55 front end (c55x_core.frontend_ir) and ir.py.

Every case is an instruction (text, addr, size, block_end, symbols). Both front ends must
build the same IR, with the same Python types (bool is not int, tuple is not list), or
reject the instruction with the same message. Listings are also read by both readers
(listing.parse and c55x_core.frontend_listing), which must agree.

Run inside the launcher from the repository root:
  bin/c55x-contained -- python3 tools/frontend_ir_check.py --tests --fuzz 20000
Options:
  --tests            cases from the test suite (every listing a Cpu is built from and
                     every instruction the core compiles while the suite runs)
  --listing FILE     a dis55 listing; with --addresses FILE (one address per line,
                     decimal or 0x hex) only those instructions, else all of them
  --corpus FILE      cases saved with --save (JSON lines)
  --fuzz N           N synthetic cases: operand variations of the loaded cases and of
                     built-in templates (--seed S)
  --save FILE        write the loaded (non-fuzz) cases as JSON lines
  --show N           print up to N differences (default 20)
Exit status 1 if any case differs.
"""

import argparse
import io
import json
import random
import sys
import unittest
from pathlib import Path
from types import SimpleNamespace

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from c55x_emu import ir, listing, rust_core  # noqa: E402
from c55x_emu.cpu import Cpu  # noqa: E402


def same(a, b):
    if type(a) is not type(b):
        return False
    if isinstance(a, (tuple, list)):
        return len(a) == len(b) and all(same(x, y) for x, y in zip(a, b))
    return a == b


def python_ir(case):
    text, addr, size, block_end, symbols = case
    insn = listing.Insn(addr, size, text, block_end)
    resolve = lambda target: Cpu.resolve(SimpleNamespace(symbols=symbols), target)  # noqa: E731
    try:
        return True, ir.build(insn, resolve)
    except Exception as e:                  # the core reports any failure as unsupported
        return False, str(e)


def compare(name, cases, core, show):
    seen, ok, rejected, diffs = set(), 0, 0, []
    for case in cases:
        key = json.dumps(case, sort_keys=True)
        if key in seen:
            continue
        seen.add(key)
        py = python_ir(case)
        rs = tuple(core.frontend_ir(*case))
        if same(py, rs):
            ok += py[0]
            rejected += not py[0]
        else:
            diffs.append((case, py, rs))
    texts = len({c[0] for c in cases})
    print(f"{name}: {len(seen)} cases ({texts} distinct texts): {ok} same IR, "
          f"{rejected} same rejection, {len(diffs)} differ")
    for case, py, rs in diffs[:show]:
        print(f"  {case!r}\n    python {py!r}\n    rust   {rs!r}")
    return len(diffs)


def compare_listing(path, text, core, markers):
    try:
        py = True, [(i.addr, i.size, i.text, i.block_end) for i in sorted(
            listing.parse(text, markers).values(), key=lambda i: i.addr)]
    except Exception as e:
        py = False, str(e)
    rs = tuple(core.frontend_listing(text, markers))
    if not same(py, rs):
        print(f"listing {path}: readers differ\n  python {str(py)[:300]}\n  rust   {str(rs)[:300]}")
        return 1
    return 0


def suite_cases():
    """Run the test suite and record every instruction a Cpu is given or compiles."""
    cases = []

    def record(insn, symbols):
        cases.append([insn.text, insn.addr, insn.size, insn.block_end, dict(symbols or {})])

    build, init = ir.build, Cpu.__init__

    def build_hook(insn, resolve):
        record(insn, getattr(getattr(resolve, "__self__", None), "symbols", {}))
        return build(insn, resolve)

    def init_hook(self, bus, code, symbols=None):
        init(self, bus, code, symbols)
        for insn in list(self.code.insns.values()):
            record(insn, self.symbols)

    ir.build, Cpu.__init__ = build_hook, init_hook
    try:
        tests = unittest.defaultTestLoader.discover("tests", top_level_dir=".")
        result = unittest.TextTestRunner(stream=io.StringIO(), verbosity=0).run(tests)
    finally:
        ir.build, Cpu.__init__ = build, init
    print(f"test suite: {result.testsRun} tests, {len(result.failures)} failures, "
          f"{len(result.errors)} errors, {len(result.skipped)} skipped (recording only)")
    return cases


REGS = ["AC0", "AC1", "AC2", "AC3", "T0", "T1", "T2", "T3", "AR0", "AR1", "AR3", "AR7",
        "XAR0", "XAR5", "CDP", "XCDP", "TC1", "TC2", "SP", "SSP", "XSP", "DP", "XDP",
        "BRC0", "CSR", "RPTC", "ST0", "ST1", "ST3", "CARRY", "AR8", "T4", "AC4", "IMR0"]
NUMS = ["#0", "#1", "#-1", "#7", "#16", "#255", "#-32768", "#32767", "#0x10", "#0x020120",
        "#-0x8", "#0x_10", "#1_0", "#01600h", "#-01600h", "#0ah", "#1H", "#0x", "#h", "#08",
        "#00", "#abc", "#0xffffffff", "#254", "#255", "#--1", "#0b1", "#0o7", "#9" * 3]
MEMS = ["*AR3", "*AR3+", "*AR3-", "*+AR3", "*-AR3", "*+AR3(#2)", "*-AR3(#0x10)", "*AR3(#5)",
        "*AR3(short(#3))", "*AR3(T0)", "*AR3(AR0)", "*AR3(T5)", "*AR3(AR1)", "*(AR3+T0)",
        "*(AR3-T1)", "*(AR3+AR0)", "*(AR3+#4)", "*CDP", "*CDP+", "*CDP-", "*(#0x1234)",
        "*abs16(#0x10)", "*port(#0x100)", "*SP(#3)", "*SP(#-1)", "@0x10", "@#5", "@AR3",
        "@ST1_L", "@IMR0_L", "@AC0L", "@AC1_H", "@XYZ", "@BRC0", "*AR3+#", "*AR8", "*(CDP+T0)",
        "*AR3(short(#3)", "*(AR3+T0", "@ST0_1", "@IVPD_L", "@BOF01"]
OPS = [" + ", " - ", " * ", " & ", " | ", " ^ ", " << ", " >> ", " <<< ", " >>> ", " == ",
       " != ", " < ", " <= ", " >= ", " > ", ", "]
FUNCS = ["HI", "LO", "dbl", "uns", "pair", "abs", "rnd", "sat", "mar", "coef", "exp",
         "mant", "max", "min", "high_byte", "low_byte", "field_expand", "count"]
TEMPLATES = [
    "{r} = {x}", "{r} = {x}{op}{y}", "{m} = {x}", "{r} = {f}({x})", "{r} = {f}({x}, {y})",
    "{m} = {f}({r})", "{r} = |{r}|", "{r} = -{x}", "{r} = ~{x}", "{r} = !{x}",
    "{r} = ({x}{op}{y}){op}{z}", "{r} += {x}", "{r} = {x} || mmap()", "{m} = {x} || readport()",
    "{m} = {x} || writeport()", "{m} = {m} | {n} || readport()", "goto {n}", "goto {r}",
    "call {n}", "call {r}", "goto L1", "call L2", "if ({x}{op}{y}) goto {n}", "goto",
    "compare ({r} == {n}) goto {n}", "if ({r} != #0) return", "return", "return_int",
    "if ({r}) return_int", "return {r}", "idle", "if ({x}{op}{y}) execute(AD_Unit)",
    "if ({x}{op}{y}) execute(D_Unit)", "repeat({n})", "repeat(CSR)", "repeat(#{k})",
    "while (TC1 && (RPTC < {n})) repeat", "while (!TC1 && (RPTC < {n})) repeat",
    "while (TC2 && (RPTC < {n})) repeat", "localrepeat {{", "blockrepeat {{",
    "bit(ST{d}, #{k}) = #{b}", "bit({r}, {n}) = #{b}", "bit({r}, *SP({n})) = #{b}",
    "cbit({r}, {n})", "cbit({r}, *SP({n}))", "TC{b1} = bit({m}, {n}), bit({m}, {n}) = #{b}",
    "TC1 = bit({m}, {n}), bit({m}, #3) = #1", "subc({m}, {r}, {r})", "subc({x}, AC0, AC1)",
    "push({r})", "push({r},{r})", "push(dbl({m}))", "push({m})", "pshboth({r})",
    "dbl(push({r}))", "mar({r} + {x})", "mar({r} - {x})", "mar({r} = {x})", "mar({m})",
    "{r} = mar({m})", "{r},{r} = pop()", "{r} = pop()", "{r} = dbl(pop())", "dbl({m}) = pop()",
    "{m} = pop()", "{r} = popboth()", "pair({r}) = {m}", "{r} = pair({m})",
    "{r} = {m} * coef({c}), {r} = {m} * coef({c})", "{r} = {m} * coef({c}), {r} = {m}",
    "{r} = {x}, {r} = {y}", "{r} = mant({r}), {r} = exp({r})", "AC0 = mant(AC1), T0 = exp(AC1)",
    "sqdst({m}, {m}, {r}, {r})", "T{d} = count({r}, {r}, TC{b1})",
    "HI({r}) = HI({x}) + HI({y}), LO({r}) = LO({x}) + LO({y})",
    "HI(AC1) = HI(*AR0) - HI(AC0), LO(AC1) = LO(*AR0) - LO(AC0)",
    "HI(AC1) = HI(*AR0) + HI(AC0), LO(AC2) = LO(*AR0) + LO(AC0)", "nop", "nop_16",
    "{r} = {x} ? {y}", "{r} = ({x}", "{r} = {x})", "{r} = {r}({x})", "{r} == {x}",
    "{r} = {f}()", "{r} = {x} {y}", "{r} = {x}{op}", "{r} = *AR3+ * *AR4+", "{r} = .L1",
]
COEFS = ["*CDP", "*CDP+", "*CDP-", "*(CDP+T0)", "*(CDP-T1)", "*AR3", "*(CDP+T2)", ""]


def operand(rng):
    pick = rng.random()
    if pick < 0.35:
        return rng.choice(REGS)
    if pick < 0.6:
        return rng.choice(MEMS)
    if pick < 0.85:
        return rng.choice(NUMS)
    return f"{rng.choice(FUNCS)}({rng.choice(REGS + MEMS)})"


def template(rng):
    def fill(t):
        return t.format(r=rng.choice(REGS), m=rng.choice(MEMS), n=rng.choice(NUMS),
                        x=operand(rng), y=operand(rng), z=operand(rng), op=rng.choice(OPS),
                        f=rng.choice(FUNCS), c=rng.choice(COEFS), d=rng.randrange(5),
                        k=rng.choice(["5", "0x10", "010", "x", "255"]), b=rng.choice("01"),
                        b1=rng.choice("123"))
    return " || ".join(fill(rng.choice(TEMPLATES)) for _ in range(rng.choice((1, 1, 2, 2, 3))))


def mutate(rng, text):
    """One operand-level change of a known text."""
    words = text.replace("(", " ( ").replace(")", " ) ").replace(",", " , ").split()
    choice = rng.random()
    if choice < 0.5 and words:
        i = rng.randrange(len(words))
        w = words[i]
        pool = NUMS if w.startswith("#") else MEMS if w.startswith(("*", "@")) else REGS
        out = text.replace(w, rng.choice(pool), 1)
    elif choice < 0.65:
        i = rng.randrange(len(text) + 1)
        out = text[:i] + rng.choice("()*+-,|#= _X") + text[i:]
    elif choice < 0.8 and text:
        i = rng.randrange(len(text))
        out = text[:i] + text[i + 1:]
    elif choice < 0.9:
        out = f"{text} || {rng.choice([template(rng), 'mmap()', 'readport()', 'writeport()'])}"
    else:
        out = f"{template(rng)} || {text}"
    return out


def fuzz_cases(rng, n, base):
    texts = sorted({c[0] for c in base})
    cases = []
    for _ in range(n):
        if texts and rng.random() < 0.6:
            text = mutate(rng, rng.choice(texts))
        else:
            text = template(rng)
        size = rng.randrange(1, 7)
        addr = rng.randrange(0, 1 << 24)
        block_end = rng.choice([None, addr + size + rng.randrange(0, 128)])
        symbols = rng.choice([{}, {"L1": 0x1234, "L2": 0x20000}])
        cases.append([text, addr, size, block_end, symbols])
    return cases


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--tests", action="store_true")
    ap.add_argument("--listing", action="append", default=[])
    ap.add_argument("--addresses")
    ap.add_argument("--corpus", action="append", default=[])
    ap.add_argument("--fuzz", type=int, default=0)
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--save")
    ap.add_argument("--show", type=int, default=20)
    args = ap.parse_args()
    core = rust_core.load()
    failed, base = 0, []
    if args.tests:
        cases = suite_cases()
        failed += compare("test suite", cases, core, args.show)
        base += cases
    for path in args.corpus:
        cases = [json.loads(line) for line in Path(path).read_text().splitlines() if line]
        failed += compare(f"corpus {path}", cases, core, args.show)
        base += cases
    wanted = None
    if args.addresses:
        wanted = {int(w, 0) for w in Path(args.addresses).read_text().split()}
    for path in args.listing:
        text = Path(path).read_text()
        failed += compare_listing(path, text, core, True)
        try:
            insns = listing.parse(text)
        except ValueError as e:         # a cut window: ends come from the encoding only
            print(f"listing {path}: {e}; reading it without '}}' markers")
            failed += compare_listing(path, text, core, False)
            insns = listing.parse(text, markers=False)
        cases = [[i.text, i.addr, i.size, i.block_end, {}] for a, i in sorted(insns.items())
                 if wanted is None or a in wanted]
        failed += compare(f"listing {path}", cases, core, args.show)
        base += cases
    if args.save:
        Path(args.save).write_text("".join(json.dumps(c) + "\n" for c in base))
    if args.fuzz:
        cases = fuzz_cases(random.Random(args.seed), args.fuzz, base)
        failed += compare(f"fuzz (seed {args.seed})", cases, core, args.show)
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
