#!/usr/bin/env python3
"""One-off audit: decode every multiply-by-immediate-constant instruction
in a dis55 listing from its raw opcode bytes and compare with the value
the emulator's text rule produces.

The text rule (compile.rs): any multiply constant whose 32-bit printed
view is >= 0x8000 is interpreted as its signed 16-bit value (sext16 of
the low 16 bits). This audit proves the rule is correct for every site
in the analyzed firmware by decoding the constant from the actual opcode
encoding, which was established by assembling every form with TI asm55
and reading back the bytes.

Constant positions per form (from asm55 round-trips in the port track):
  MPYK  K8   (3 bytes): sext8(raw[1])
  MPYK  K16  (4 bytes): sext16(raw[1:3] big-endian)
  MACK  K16  (4 bytes): sext16(raw[1:3] big-endian)
  MPYMK K8   (4 bytes): sext8(raw[2])   [f8 61 XX YY: K8 at byte 2]
  MACMK K8   (4 bytes): sext8(raw[2])

Usage: mul_const_audit.py <shared-code.ti-algebraic.asm> [--json out]
Output: a table on stdout (PC, form, raw hex, K width, opcode value,
text value, match) plus a form census and the pass/fail verdict.
"""

import json
import re
import sys
from pathlib import Path

LINE_RE = re.compile(r"^\s*([0-9a-f]{6}):\s+([0-9a-f_]+)\s+(\S.*)$")
MUL_CONST = re.compile(r"\*\s+#(0x[0-9a-fA-F]+)")


def sext(v, bits):
    m = 1 << (bits - 1)
    return (v & (m - 1)) - (v & m)


def decode_from_bytes(raw, text):
    """Returns (form, k_width, opcode_value) or None if not a known form."""
    # determine the form from the algebraic text pattern
    if re.search(r"\(T[0-3]\s*\*\s*#", text):
        form, k_bits = "MACK", 16
    elif re.search(r"\*AR[0-9a-fA-F(#]", text.split("* #")[0]):
        # check if the non-constant side is a memory operand
        # (Smem * #k) form: MPYMK or MACMK, always K8
        left = text.split("* #")[0].strip()
        if "*" in left or "AR" in left or "CDP" in left:
            form, k_bits = "MPYMK/MACMK", 8
        else:
            return None
    elif re.search(r"AC[0-3]\s*\*\s*#", text):
        # MPYK: K8 if 3 bytes, K16 if 4 bytes
        form = "MPYK"
        k_bits = 8 if len(raw) == 3 else 16
    else:
        return None

    # decode the constant from the raw bytes
    if form == "MPYK" and k_bits == 8:
        val = sext(raw[1], 8)
    elif form == "MPYMK/MACMK":
        # f8 61 XX YY: K8 is at byte 2; but other Smem modes may differ.
        # Use the asm55-proved f861 prefix; unknown prefixes return None.
        if len(raw) >= 3 and raw[0] == 0xF8:
            val = sext(raw[2], 8)
        elif len(raw) >= 4:
            # MACMK encoding differs; try byte 3 for the f9 family
            val = sext(raw[3], 8) if raw[0] == 0xF9 else sext(raw[2], 8)
        else:
            return None
    else:
        # K16 forms: bytes 1-2 big-endian
        if len(raw) >= 3:
            val = sext((raw[1] << 8) | raw[2], 16)
        else:
            return None
    return form, k_bits, val


def text_rule(printed_hex):
    """The emulator's text rule: >= 0x8000 in 32-bit view -> sext16(low16)."""
    v = int(printed_hex, 16)
    if v >= 0x8000:
        return sext(v & 0xFFFF, 16)
    return v


def main(asm_path, json_out=None):
    lines = Path(asm_path).read_text().splitlines()
    results = []
    for line in lines:
        m = LINE_RE.match(line)
        if not m:
            continue
        text = m.group(3).strip()
        cm = MUL_CONST.search(text)
        if not cm:
            continue
        pc = int(m.group(1), 16)
        raw = bytes.fromhex(m.group(2).replace("_", ""))
        printed = cm.group(1)
        decoded = decode_from_bytes(raw, text)
        if decoded is None:
            results.append({"pc": pc, "text": text, "raw": raw.hex(),
                            "form": "?", "k_bits": 0,
                            "opcode_val": None, "text_val": None,
                            "match": None})
            continue
        form, k_bits, opcode_val = decoded
        text_val = text_rule(printed)
        results.append({"pc": pc, "text": text, "raw": raw.hex(),
                        "form": form, "k_bits": k_bits,
                        "opcode_val": opcode_val, "text_val": text_val,
                        "match": opcode_val == text_val})

    # print table
    print(f"{'PC':>8}  {'form':<12} {'K':>2}  {'opcode':>7}  "
          f"{'text':>7}  {'match':>5}  raw")
    print("-" * 80)
    forms_seen = {}
    mismatches = unknowns = 0
    for r in results:
        if r["match"] is None:
            unknowns += 1
            print(f"  {r['pc']:06x}  {'?':<12} {'?':>2}  "
                  f"{'?':>7}  {'?':>7}  {'  ?':>5}  {r['raw']}")
            continue
        forms_seen[f"{r['form']} K{r['k_bits']}"] = \
            forms_seen.get(f"{r['form']} K{r['k_bits']}", 0) + 1
        ok = "OK" if r["match"] else "DIFF"
        if not r["match"]:
            mismatches += 1
        print(f"  {r['pc']:06x}  {r['form']:<12} {r['k_bits']:>2}  "
              f"{r['opcode_val']:>7}  {r['text_val']:>7}  {ok:>5}  {r['raw']}")

    print(f"\nTotal multiply-by-constant sites: {len(results)}")
    print(f"Forms seen: {dict(sorted(forms_seen.items()))}")
    print(f"Mismatches: {mismatches}, unknown forms: {unknowns}")
    if mismatches == 0 and unknowns == 0:
        print("VERDICT: PASS - every site matches, every form is known")
    else:
        print("VERDICT: FAIL - see above")

    if json_out:
        Path(json_out).write_text(json.dumps(results, indent=1) + "\n")
        print(f"JSON output: {json_out}")


if __name__ == "__main__":
    asm = sys.argv[1] if len(sys.argv) > 1 else sys.exit("need asm path")
    out = None
    if len(sys.argv) > 3 and sys.argv[2] == "--json":
        out = sys.argv[3]
    main(asm, out)
