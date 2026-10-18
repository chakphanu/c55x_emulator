#!/usr/bin/env python3
"""C55x blob -> dis55 (algebraic) -> asm55 -> bytes; iterate raw fallbacks until identical.

See docs/2026-10-08-c55x_asm_roundtrip_TLP-CLEAR.md for method and results.
"""
import argparse
import collections
import re
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from c55x_emu.paths import TI_BIN as TI  # noqa: E402
MODE = "\t.mmregs\n\t.cpl_on\n\t.arms_on\n\t.c54cm_off\n"
LINE = re.compile(r"^([0-9a-f]+):\s+([0-9a-f_]+)\s+(.*)$")
FLOW = re.compile(r"\b(goto|call|dgoto|dcall)(\s+)#0x([0-9a-f]+)")
ASM_ERR = re.compile(r'ERROR!\s+at line (\d+)')
ABSOLUTE_FLOW_OPCODES = (0x6a, 0x6c)  # goto P24, call P24


def run(cmd, cwd, check=True):
    r = subprocess.run([str(c) for c in cmd], cwd=cwd, capture_output=True, text=True, errors="surrogateescape")
    if check and r.returncode != 0:
        sys.exit(f"FAILED: {' '.join(map(str, cmd))}\n{r.stdout}\n{r.stderr}")
    return r


def build(asm, addr, work, name):
    """Assemble+link at addr; return (bytes or None, asm error line numbers)."""
    (work / f"{name}.asm").write_text(asm, errors="surrogateescape")
    (work / f"{name}.cmd").write_text(
        f"MEMORY {{ CODE (RX) : origin = 0x{addr:x}, length = 0x01000000 }}\n"
        "SECTIONS { .text : {} > CODE }\n")
    r = run([TI / "asm55", "--output_all_syms", f"{name}.asm", f"{name}.obj"], work, check=False)
    if r.returncode != 0:
        errs = {int(n) for n in ASM_ERR.findall(r.stdout + r.stderr)}
        if not errs:
            sys.exit(f"asm55 failed without line info:\n{r.stdout}\n{r.stderr}")
        return None, errs
    run([TI / "lnk55", f"{name}.obj", f"{name}.cmd", "-o", f"{name}.out"], work)
    run([TI / "hex55", "-b", "--romwidth=8", "--memwidth=8", "-o", f"{name}.bin", f"{name}.out"], work)
    return (work / f"{name}.bin").read_bytes(), set()


def strip_pad(blob, size):
    """lnk55 pads .text to an even length with a single NOP (0x20)."""
    if blob[size:] not in (b"", b"\x20"):
        return blob
    return blob[:size]


def byte_line(b):
    return "\t.byte " + ",".join(f"0x{x:02x}" for x in b)


def disassemble(out_name, work):
    """Return list of items: [addr, bytes, text] or '}' markers."""
    items = []
    for ln in run([TI / "dis55", "-qq", "-r", "-g", out_name], work).stdout.splitlines():
        s = ln.strip()
        if s == "}":
            items.append("}")
            continue
        m = LINE.match(s)
        if m and not m[3].endswith(":"):
            items.append([int(m[1], 16), bytes.fromhex(m[2].replace("_", "")), m[3].strip()])
    return items


def fill_gaps(items, base, data):
    """Keep only dis55 items consistent with the blob; cover everything else with raw items."""
    out, pos, dropped = [], base, 0
    for it in items:
        if it != "}":
            off = it[0] - base
            if it[0] < pos or off + len(it[1]) > len(data) or data[off:off + len(it[1])] != it[1]:
                dropped += 1
                if it[2].rstrip().endswith("{"):
                    out.append("{dropped")
                continue
            if it[0] > pos:
                out.append([pos, data[pos - base:it[0] - base], None])
            pos = it[0] + len(it[1])
        out.append(it)
    if pos < base + len(data):
        out.append([pos, data[pos - base:], None])
    return out, dropped


def emit(items, raw, starts):
    """Generate source with a label per item; return (text, line->addr, line->enclosing opener, used)."""
    used = set()

    def lab(m):
        tgt = int(m[3], 16)
        if tgt in starts:
            used.add(tgt)
            return f"{m[1]}{m[2]}L_{tgt:06x}"
        return m[0]

    lines = MODE.rstrip("\n").split("\n") + ['\t.sect ".text"']
    linemap, ctx, stack = {}, {}, []
    for it in items:
        if it == "{dropped":
            stack.append((None, True))
            continue
        if it == "}":
            opener, was_raw = stack.pop() if stack else (None, True)
            if not was_raw:
                lines.append("\t}")
                ctx[len(lines)] = opener
            continue
        addr, b, text = it
        is_raw = text is None or addr in raw
        lines.append(f"L_{addr:06x}:")
        if is_raw:
            lines.append(byte_line(b))
        elif b[0] in ABSOLUTE_FLOW_OPCODES:
            # absolute P24 goto/call (target was an external symbol): a constant keeps that form
            lines.append("\t" + text)
        else:
            lines.append("\t" + FLOW.sub(lab, text))
        linemap[len(lines)] = addr
        inner = next((o for o, r in reversed(stack) if not r), None)
        if inner is not None:
            ctx[len(lines)] = inner
        if text and text.rstrip().endswith("{"):
            stack.append((addr, is_raw))
    return "\n".join(lines) + "\n", linemap, ctx, used


def symbols(out_name, work):
    syms = {}
    for ln in run([TI / "nm55", "-a", out_name], work).stdout.splitlines():
        parts = ln.split()
        if len(parts) == 3 and parts[2].startswith("L_"):
            syms[int(parts[2][2:], 16)] = int(parts[0], 16)
    return syms


def converge(items, base, data, work, max_iter=50, log=None):
    """Move items to raw until the assembled source equals data; return (raw, reasons, iterations, labels)."""
    insns = [it for it in items if isinstance(it, list)]
    starts = {it[0] for it in insns}
    raw, why = {it[0] for it in insns if it[2] is None}, collections.Counter()
    for i in range(1, max_iter + 1):
        src, linemap, ctx, used = emit(items, raw, starts)
        rebuilt, errs = build(src, base, work, "re")
        if rebuilt is None:
            new = set()
            for n in errs:
                if n in linemap and linemap[n] not in raw:
                    new.add(linemap[n])
                elif n in ctx:
                    new.add(ctx[n])
            new -= raw
            if not new:
                raise RuntimeError(f"asm errors not attributable to an instruction: {sorted(errs)[:10]}")
            kind = "asm_error"
        else:
            rebuilt = strip_pad(rebuilt, len(data))
            if rebuilt == data:
                return raw, why, i, used
            syms = symbols("re.out", work)
            ends = [syms[it[0]] for it in insns[1:]] + [base + len(rebuilt)]
            lenmis, encmis = set(), set()
            for it, end in zip(insns, ends):
                got = rebuilt[syms[it[0]] - base:end - base]
                if len(got) != len(it[1]):
                    lenmis.add(it[0])
                elif got != it[1]:
                    encmis.add(it[0])
            kind, new = ("length", lenmis - raw) if lenmis - raw else ("encoding", encmis - raw)
            if not new:
                raise RuntimeError("bytes differ but no instruction-level mismatch found")
        raw |= new
        why[kind] += len(new)
        if log:
            log(f"iter {i}: {len(new)} {kind} mismatches -> raw")
    raise RuntimeError("no convergence")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("blob")
    ap.add_argument("addr", type=lambda s: int(s, 0))
    ap.add_argument("--work", required=True)
    ap.add_argument("--length", type=int)
    ap.add_argument("--max-iter", type=int, default=50)
    a = ap.parse_args()
    work = Path(a.work)
    work.mkdir(parents=True, exist_ok=True)
    data = Path(a.blob).read_bytes()[:a.length]

    wrap = "\n".join(['\t.sect ".text"'] + [byte_line(data[i:i + 16]) for i in range(0, len(data), 16)]) + "\n"
    wrapped, _ = build(wrap, a.addr, work, "wrap")
    assert strip_pad(wrapped, len(data)) == data, "wrapper roundtrip failed"

    items, dropped = fill_gaps(disassemble("wrap.out", work), a.addr, data)
    insns = [it for it in items if isinstance(it, list)]
    try:
        raw, why, i, used = converge(items, a.addr, data, work, a.max_iter, log=print)
    except RuntimeError as exc:
        sys.exit(str(exc))

    real = [it for it in insns if it[2] is not None]
    raw_real = [it for it in real if it[0] in raw]
    gap_bytes = sum(len(it[1]) for it in insns if it[2] is None)
    (work / "raw.txt").write_text(
        "".join(f"{it[0]:06x} {it[1].hex():<14} {it[2]}\n" for it in raw_real), errors="surrogateescape")
    print(f"IDENTICAL after {i} iteration(s): {len(data)} bytes, branch_labels={len(used)}")
    print(f"instructions={len(real)} symbolic={len(real) - len(raw_real)} raw_fallback={len(raw_real)}"
          f" ({100 * len(raw_real) / max(len(real), 1):.2f}%), undecoded_gap_bytes={gap_bytes}, dis55_inconsistent_dropped={dropped}")
    print(f"fallback reasons: {dict(why)}")


if __name__ == "__main__":
    main()
