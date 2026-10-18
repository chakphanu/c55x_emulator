"""C55x register names and constants shared by the IR front end (ir.py), the Python Cpu
state holder (cpu.py) and the tests; the registers themselves live in the Rust core
(rust/c55x_core/src/state.rs).

MMR layout: SPRU371F table 2-2 (words 0x00..0x4f, 0x50..0x5f reserved). Only the CPU
modes of compiled C code are supported (ST1_REQUIRED / ST1_UNSUPPORTED, checked by the
core).
"""

ST1_CPL, ST1_INTM, ST1_M40, ST1_SATD, ST1_SXMD = 1 << 14, 1 << 11, 1 << 10, 1 << 9, 1 << 8
ST1_FRCT, ST1_C54CM = 1 << 6, 1 << 5
ST1_REQUIRED = ST1_CPL
ST1_UNSUPPORTED = ST1_C54CM     # SXMD = 0, M40, FRCT and SATD are supported
ST3_SATA = 1 << 5
PLAIN = ("trn0", "trn1", "bk03", "bk47", "bkc", "bsa01", "bsa23", "bsa45", "bsa67", "bsac",
         "pdp", "dbier0", "dbier1", "bios37", "xpc", "rptc")
MMR = {"IER0": 0x00, "IFR0": 0x01, "ST0": 0x02, "ST1": 0x03, "ST3": 0x04, "TRN0": 0x0F,
       "SP": 0x18, "BK03": 0x19, "BRC0": 0x1A, "XPC": 0x1E, "CDP": 0x27, "DPH": 0x2B,
       "DP": 0x2E, "PDP": 0x2F, "BK47": 0x30, "BKC": 0x31, "BSA01": 0x32, "BSA23": 0x33,
       "BSA45": 0x34, "BSA67": 0x35, "BSAC": 0x36, "TRN1": 0x38, "BRC1": 0x39, "BRS1": 0x3A,
       "CSR": 0x3B, "RSA0H": 0x3C, "RSA0": 0x3D, "REA0H": 0x3E, "REA0": 0x3F, "RSA1H": 0x40,
       "RSA1": 0x41, "REA1H": 0x42, "REA1": 0x43, "RPTC": 0x44, "IER1": 0x45, "IFR1": 0x46,
       "DBIER0": 0x47, "DBIER1": 0x48, "IVPD": 0x49, "IVPH": 0x4A, "ST2": 0x4B, "SSP": 0x4C,
       "SPH": 0x4E, "CDPH": 0x4F, "BIOS37": 0x37}
MMR.update({f"AR{i}": 0x10 + i for i in range(8)})
MMR.update({f"T{i}": 0x20 + i for i in range(4)})
for _n, _a in (("AC0", 0x08), ("AC1", 0x0B), ("AC2", 0x24), ("AC3", 0x28)):
    MMR.update({_n + "L": _a, _n + "H": _a + 1, _n + "G": _a + 2})
# dis55 '@NAME_L || mmap()' spellings; checked against encodings: IMR0_L 0x00, ST1_L 0x03,
# IMR1_L 0x45, ST2_L 0x4b, IVPD_L 0x49, IVPH_L 0x4a
ALIASES = {"IMR0": "IER0", "IMR1": "IER1", "IFR0": "IFR0", "PMST": "ST3", "BIOS": "BIOS37",
           "BOF01": "BSA01", "BOF23": "BSA23", "BOF45": "BSA45", "BOF67": "BSA67", "BOFC": "BSAC"}


class EmulationError(Exception):
    pass
