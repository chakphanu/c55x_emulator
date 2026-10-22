"""Rust dis55 front end (c55x_core.frontend_ir / frontend_listing) against ir.py and
listing.py: same IR (same Python types) or the same rejection message.

SYNTHETIC is dis55 -qq -r -g output of a synthetic program assembled with asm55 and
linked at 0x20000 (no firmware code).
"""

import importlib.util
import random
import unittest
from pathlib import Path

from c55x_emu import listing, rust_core

_spec = importlib.util.spec_from_file_location(
    "frontend_ir_check", Path(__file__).resolve().parents[1] / "tools" / "frontend_ir_check.py")
check = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(check)

SYNTHETIC = """\

	.sect ".text"
020000: a061             AC0 = *AR3
020002: a163             AC1 = *AR3+
020004: a285             AC2 = *AR4-
020006: a0b9             AC0 = *AR5(short(#4))
020008: a0bb             AC0 = *AR5(short(#5))
02000a: a06d0002         AC0 = *AR3(#0002h)
02000e: a06f0010         AC0 = *+AR3(#0010h)
020012: a06b             AC0 = *AR3(T0)
020014: a167             AC1 = *(AR3+T0)
020016: a175             AC1 = *AR3(short(#2))
020018: a091             AC0 = *CDP+
02001a: a031001234       AC0 = *(#01234h)
02001f: a0110010         AC0 = *abs16(#00010h)
020023: a006             AC0 = *SP(#03h)
020025: a424             T0 = *SP(#12h)
020027: a526_98          T1 = @AR3_L || mmap()
02002a: a002_98          AC0 = @IFR0_L || mmap()
02002d: c906_98          @ST1_L = AR1 || mmap()
020030: ab510100         AR3 = *port(#00100h)
020034: cb510101         *port(#00101h) = AR3
020038: c081_9a          *AR4 = AC0 || writeport()
02003b: de6104           AC0 = AC0 + (*AR3 << #16)
02003e: 3201             AC1 = |AC0|
020040: 3412             AC2 = -AC1
020042: 18ff00           AC0 = AC0 & #255
020045: e703a8           *AR0+ = HI(AC2 << T2)
020048: 8d001ee736a8     AC3 = (*AR0 << #16) - AC0 || *AR0+ = HI(AC2 << T2)
02004e: 86049000         AC0 = *AR0+ * *AR1+
020052: 83418104         AC0 = AC0 + (*AR2 * coef(*CDP+)), AC1 = AC1 + (*AR3 * coef(*CDP+))
020056: 101910           AC0 = mant(AC1), T1 = exp(AC1)
020059: 100a80           T0 = count(AC0, AC2, TC1)
02005c: 860081e0         sqdst(*AR0,*AR1,AC0,AC1)
020060: ee0110           HI(AC1) = HI(*AR0) + HI(AC0), LO(AC1) = LO(*AR0) + LO(AC0)
020063: ee2122           HI(AC2) = HI(AC0) - HI(*AR1), LO(AC2) = LO(AC0) - LO(*AR1)
020066: de2103           subc(*AR1, AC0, AC0)
020069: 4693             bit(ST1, #9) = #1
02006b: 46b0             bit(ST0, #11) = #0
02006d: e36120           TC1 = bit(*AR3, #2), bit(*AR3, #2) = #1
020070: 5006             push(AC0)
020072: 3845             push(T0,T1)
020074: 50d5             pshboth(XAR5)
020076: 5017             dbl(push(AC1))
020078: b761             push(dbl(*AR3))
02007a: 3a45             T0,T1 = pop()
02007c: 50d4             XAR5 = popboth()
02007e: 5003             AC0 = dbl(pop())
020080: b861             dbl(*AR3) = pop()
020082: 1440b0           mar(AR3 + T0)
020085: 1403a6           mar(AR2 - #3)
020088: 144095           mar(AR1 = #64)
02008b: b463             mar(*AR3+)
02008d: ec31be002000     XAR3 = mar(*(#02000h))
020093: b463_25b0        mar(*AR3+) || AC0 = AC0 + AR3
020097: 8b8604a0         mar(*AR4+) || AC0 = *AR4
02009b: ed6108           AC0 = dbl(*AR3)
02009e: 8662010c         AC1 = uns(*AR3) * uns(*AR4)
0200a2: 4c05             repeat(#5)
0200a4: d66300              AC0 = AC0 + *AR3+
0200a7: 4800             repeat(CSR)
0200a9: d66300              AC0 = AC0 + *AR3+
0200ac: 4a83             localrepeat {
0200ae: d66300              AC0 = AC0 + *AR3+
0200b1: d68311              AC1 = AC1 + *AR4+
                         }
0200b4: 0e0005           blockrepeat {
0200b7: d66300              AC0 = AC0 + *AR3+
0200ba: 5e80                nop_16
0200bc: d68311              AC1 = AC1 + *AR4+
                         }
0200bf: 2201_4b83        AC1 = AC0 || localrepeat {
0200c3: d66300              AC0 = AC0 + *AR3+
0200c6: d68311              AC1 = AC1 + *AR4+
                         }
0200c9: 4a83_2302        localrepeat { || AC2 = AC0
0200cd: d66300              AC0 = AC0 + *AR3+
0200d0: d68311              AC1 = AC1 + *AR4+
                         }
0200d3: 401b_0f0003      AR3 = AR3 + #1 || blockrepeat {
0200d8: d66300              AC0 = AC0 + *AR3+
0200db: d68311              AC1 = AC1 + *AR4+
                         }
0200de: 006405           while (TC1 && (RPTC < #5)) repeat
0200e1: de2103              subc(*AR1, AC0, AC0)
0200e4: 9610             if (AC0 != #0) execute(AD_Unit)
0200e6: 2201             AC1 = AC0
0200e8: 9680             if (AC0 == #0) execute(D_Unit)
0200ea: c163             *AR3+ = AC1
0200ec: 2201_9f64        AC1 = AC0 || if (TC1) execute(AD_Unit)
0200f0: 2202             AC2 = AC0
0200f2: c163_9ee5        if (TC2) execute(D_Unit) || *AR3+ = AC1
0200f6: 6d20ff06         if (AC0 < #0) goto #0x020000
0200fa: 6f900500         compare (AR1 == #5) goto #0x0200fe
0200fe: 06feff           goto #0x020000
020101: 08fefc           call #0x020000
020104: 9200             call AC0
020106: 9101             goto AC1
020108: 6e64fef4         if (TC1) call #0x020000
02010c: 025100           if (AC1 >= #0) return
02010f: 20               nop
020110: 7a00000c         idle
020114: 4805             return_int
020116: 4804             return
020118: 20               nop
"""

REJECTED = [
    "AC0 = AC0 ? AC1",                      # unknown token
    "AC0 = @NOPE_L || mmap()",              # unknown memory-mapped register
    "if (TC1) return_int",
    "return AC0",
    "repeat(#010)",                         # int(..., 0) ValueError
    "AC0 = *AR3(AR1)",                      # index form
    "AC0 = *AR3 * coef(*CDP+), AC1 = *AR4 * coef(*CDP-)",
    "TC1 = bit(*AR3+, #2), bit(*AR3+, #2) = #1",
    "while (TC1 && (RPTC < #255)) repeat",
    "repeat(#5) || goto #0x20000",
    "AC0 = AC1 || if (TC1) execute(AD_Unit) || goto #0x20000",
    "goto L9",                              # unknown branch target
    "goto",
    "mar(*AR3+) || AC0 = *AR3 + AR3",
    "T0,T1 = AC0",
]


def core():
    c = rust_core.load()
    if not hasattr(c, "frontend_ir"):
        raise unittest.SkipTest("core built without the Rust front end")
    return c


class FrontendTest(unittest.TestCase):
    def setUp(self):
        self.core = core()

    def assertSame(self, case):
        py, rs = check.python_ir(case), tuple(self.core.frontend_ir(*case))
        self.assertTrue(check.same(py, rs), f"{case!r}\npython {py!r}\nrust   {rs!r}")
        return py

    def test_listing_readers_agree(self):
        for markers in (True, False):
            py = [(i.addr, i.size, i.text, i.block_end)
                  for _, i in sorted(listing.parse(SYNTHETIC, markers).items())]
            ok, rs = self.core.frontend_listing(SYNTHETIC, markers)
            self.assertTrue(ok)
            self.assertTrue(check.same(py, rs))
        ends = {i.text: i.block_end for i in listing.parse(SYNTHETIC).values() if i.block_end}
        self.assertEqual(ends["AC1 = AC0 || localrepeat {"], 0x200C6)
        self.assertEqual(ends["AR3 = AR3 + #1 || blockrepeat {"], 0x200DB)

    def test_listing_errors_agree(self):
        cut = SYNTHETIC.replace("0200c6: d68311              AC1 = AC1 + *AR4+\n", "")
        for text in (cut, SYNTHETIC + "\n}", SYNTHETIC.rsplit("}", 1)[0]):
            try:
                listing.parse(text)
                self.fail("listing.parse accepted a broken listing")
            except ValueError as e:
                self.assertEqual(tuple(self.core.frontend_listing(text)), (False, str(e)))

    def test_synthetic_listing(self):
        built = 0
        for insn in listing.parse(SYNTHETIC).values():
            built += self.assertSame([insn.text, insn.addr, insn.size, insn.block_end, {}])[0]
        self.assertEqual(built, 98)          # all of them

    def test_rejections(self):
        for text in REJECTED:
            ok, why = self.assertSame([text, 0x20000, 2, None, {"L1": 0x20010}])
            self.assertFalse(ok, text)

    def test_fuzz(self):
        base = [[i.text, i.addr, i.size, i.block_end, {}] for i in listing.parse(SYNTHETIC).values()]
        for case in check.fuzz_cases(random.Random(5), 3000, base):
            self.assertSame(case)


if __name__ == "__main__":
    unittest.main()
