"""dis55 instruction text -> data structure (IR) for the Rust core (rust/c55x_core).

The statement forms and the checks follow the former Python engine; the Rust core
executes what this module describes. Expression ASTs are expr.py tuples with two
rewrites done here: register aliases are resolved and '@NAME || mmap()' operands become
("mmr", address).

IR (tuples and lists only):
  ("insn", mmap, io_read, io_write, parts, flow, cond, repeat, (ptr_mod, address_parts))
  parts:  [(address_phase_first, part)]   part:
    ("nop",) ("status_bit", mmr, bit, val) ("reg_toggle", reg, bit)
    ("test_set", tc, node, bit, val) ("reg_bit", reg, bit, val) ("subc", node, src, dst)
    ("push", [node], [double]) ("pshboth", node) ("dbl_push_acc", acc)
    ("mar_arith", node, sign, node) ("mar", node) ("assign", lhs, rhs)
    ("popboth", node) ("pop", [node], double) ("loop", kind, first, last)
  flow:   None | ("idle",) | ("return_int",) | ("return", cond) |
          ("goto", cond, target) | ("call", cond, target, return address)
          target ("abs", address) or ("acc", name); cond: node or None
  cond:   None | (node, mode)   mode 0: the condition applies to the partners; 1: to the
          next instruction (AD_Unit alone, or after a partner that runs unconditionally);
          2: to the data operations of the next instruction (D_Unit alone; its address
          phase runs anyway, SPRU375G 5-163)
  address_parts: indices of the mar() parts (address phase); ptr_mod: another part
          modifies a pointer through its operand (*ARn+, *(ARn+T0), ...)
  repeat: None | count | "CSR"
Unsupported forms raise NotImplementedError (the core then reports them when executed).
"""

import re

from .expr import number, parse, parse_list
from .registers import ALIASES, MMR

QUALIFIERS = {"mmap()", "readport()", "writeport()"}
ASSIGN = re.compile(r"(?<![=!<>])=(?!=)")
FLOW = re.compile(r"^(?:(?:if|compare) \((?P<cond>.+)\) )?(?P<op>goto|call|return_int|return)(?:\s+(?P<target>\S+))?$")
EXECUTE = re.compile(r"^if \((?P<cond>.+)\) execute\((?:D|AD)_Unit\)$")
REPEAT = re.compile(r"^repeat\((?:#(\w+)|CSR)\)$")
REPEAT_CC = re.compile(r"^while \((!?TC1) && \(RPTC < #(\w+)\)\) repeat$")
PTR_MOD = re.compile(r"\*\(?[+-](AR\d|CDP)|\*\(?(AR\d|CDP)[+-]")
STATUS_BIT = re.compile(r"^bit\((ST[0-3]), #(\d+)\) = #([01])$")
# bit set/clear in a register; dis55 prints the bit number of the Baddr form as *SP(#k)
REG_BIT = re.compile(r"^bit\(((?:X?AR|T|AC)[0-7]), (?:#(\w+)|\*SP\(#(\w+)\))\) = #([01])$")
ACC = re.compile(r"^AC[0-3]$")
REG_TOGGLE = re.compile(r"^cbit\(((?:X?AR|T|AC)[0-7]), (?:#(\w+)|\*SP\(#(\w+)\))\)$")
# test and set/clear a memory bit: 'TC1 = bit(Smem, #k), bit(Smem, #k) = #1'
TEST_SET = re.compile(r"^(TC[12]) = bit\((.+), #(\w+)\), bit\(\2, #\3\) = #([01])$")
SUBC = re.compile(r"subc\((.+), (AC[0-3]), (AC[0-3])\)")
ADDRESS_PHASE = re.compile(r"^(?:(?:X?AR[0-7]|X?CDP) = )?mar\(")
MAR_TARGET = re.compile(r"^(?:(X?AR[0-7]|X?CDP) = mar\(|mar\(\*?[+-]?\(?(X?AR[0-7]|X?CDP|T[0-3])\b)")
MANT = re.compile(r"(AC[0-3]) = mant\((AC[0-3])\), (T[0-3]) = exp\(\2\)")
DUAL_ALU = re.compile(
    r"HI\((AC[0-3])\) = HI\((.+)\) ([+-]) HI\((.+)\), "
    r"LO\(\1\) = LO\(\2\) \3 LO\(\4\)"
)
BIT_COUNT = re.compile(r"(T[0-3]) = count\((AC[0-3]), (AC[0-3]), (TC[12])\)")
SQDST = re.compile(r"sqdst\((.+),\s*(.+),\s*(AC[0-3]),\s*(AC[0-3])\)")
COEF = re.compile(r"coef\((\*\(CDP[+-]T[01]\)|[^()]*)\)")
COEF_FORMS = ("*CDP", "*CDP+", "*CDP-", "*(CDP+T0)", "*(CDP-T0)", "*(CDP+T1)", "*(CDP-T1)")
MAR_ARITH = re.compile(r"mar\(((?:X?AR[0-7])|(?:X?CDP)|T[0-3]) ([-+=]) (.+)\)")


def split_top(text, sep="||"):
    parts, depth, start, i = [], 0, 0, 0
    while i < len(text):
        c = text[i]
        if c == "(":
            depth += 1
        elif c == ")":
            depth -= 1
        elif depth == 0 and text.startswith(sep, i):
            parts.append(text[start:i].strip())
            i += len(sep)
            start = i
            continue
        i += 1
    parts.append(text[start:].strip())
    return [p for p in parts if p]


def mar_first(part, parts):
    """True if a partner reads the mar() register as a value (see module doc)."""
    m = MAR_TARGET.match(part)
    if not m:
        raise NotImplementedError(f"mar form '{part}'")
    reg = m[1] or m[2]
    base = reg.lstrip("X")
    others = " || ".join(p for p in parts if p is not part)
    as_address = re.search(r"\*[+-]?\(?" + base + r"\b", others)
    as_value = re.search(r"(?<![*+(\w])X?" + base + r"\b", others)
    if as_address and as_value:
        raise NotImplementedError(f"{base} used as address and value next to mar()")
    return bool(as_value)


def mmr_address(name):
    """registers.RegisterFile.mmr_value name resolution."""
    base = name[:-2] if name.endswith("_L") else name
    base = ALIASES.get(base, base)
    if base not in MMR and name.replace("_", "") in MMR:
        base = name.replace("_", "")
    if base not in MMR:
        raise NotImplementedError(f"memory-mapped register @{name}")
    return MMR[base]


def node(tree, mmap):
    """Resolve register aliases and @NAME operands in an expr.py AST."""
    kind = tree[0]
    if kind == "reg":
        return ("reg", ALIASES.get(tree[1], tree[1]))
    if kind == "mem" and tree[1] == "dp":
        if mmap:
            return ("mmr", mmr_address(tree[3]))
        arg = tree[3]
        return ("mem", "dp", None, arg if isinstance(arg, int) else int(arg.lstrip("#"), 0))
    if kind == "call":
        return ("call", tree[1], [node(a, mmap) for a in tree[2]])
    if kind == "un":
        return ("un", tree[1], node(tree[2], mmap))
    if kind == "bin":
        return ("bin", tree[1], node(tree[2], mmap), node(tree[3], mmap))
    return tree


def build(insn, resolve):
    """insn: listing.Insn; resolve: branch target text -> address (Cpu.resolve)."""
    parts = split_top(insn.text)
    flags = {p for p in parts if p in QUALIFIERS}
    parts = [q for p in parts if p not in QUALIFIERS for q in _dual(p)]
    mmap = "mmap()" in flags
    io = _port_access(flags, parts)
    nxt = insn.addr + insn.size
    flow, out, cond, repeat, after = None, [], None, None, False
    for i, p in enumerate(parts):
        if p == "idle":
            flow = ("idle",)
            continue
        m = EXECUTE.match(p)
        if m:
            cond = node(parse(m["cond"]), False)
            after = i > 0           # 'previous || if (cond) execute': the next instruction
            continue
        f = _flow(p, nxt, resolve)
        if f:
            flow = f
            continue
        if p in ("localrepeat {", "blockrepeat {"):
            kind = "local" if p.startswith("local") else "ext"
            out.append((False, ("loop", kind, nxt, insn.block_end)))
            continue
        m = REPEAT.match(p)
        if m:
            repeat = int(m[1], 0) if m[1] else "CSR"
            continue
        m = REPEAT_CC.match(p)
        if m:
            # RPTCC k8, cond: (count, condition, condition field of the encoding)
            count = int(m[2], 0)
            if not 0 <= count <= 254:
                raise NotImplementedError("conditional repeat count outside 0..254")
            repeat = (count, node(parse(m[1]), False), 0x74 if m[1] == "!TC1" else 0x64)
            continue
        first = bool(ADDRESS_PHASE.match(p) and len(parts) > 1
                     and mar_first(p, parts))
        out.append((first, _data(p, mmap)))
    if repeat is not None and (flow or cond is not None):
        raise NotImplementedError("repeat bundled with flow or conditional execute")
    data_parts = [p for p in parts if EXECUTE.match(p) is None and not ADDRESS_PHASE.match(p)]
    skip = (any(PTR_MOD.search(p) for p in data_parts),
            [i for i, (_, d) in enumerate(out) if d[0] in ("mar", "mar_arith") or _is_mar_assign(d)])
    tail = (repeat, skip)
    if cond is not None and after:
        # SPRU375G 5-160: the partner runs unconditionally, the condition (read in the
        # address phase, before the partner executes) applies to the next instruction
        if flow:
            raise NotImplementedError("'previous || if (cond) execute' with flow")
        # SWPU067E pp. 5-652..653 also permits previous || XCCPART.
        mode = 1 if "AD_Unit" in insn.text else 2
        return ("insn", mmap, *io, out, flow, (cond, mode), *tail)
    if cond is not None and not out and not flow:
        # alone: AD_Unit makes the whole next instruction conditional, D_Unit only its
        # data operations (SPRU375G 5-160, 5-163)
        cond = (cond, 1 if "AD_Unit" in insn.text else 2)
    elif cond is not None:
        if flow:
            raise NotImplementedError("conditional execute bundled with flow")
        # Mode 3 retains the unconditional address phase of an XCCPART
        # partner. The native compiler restricts pointer-updating partners
        # to the supported single Smem store from a register or constant.
        pointer_store = ("D_Unit" in insn.text and "AD_Unit" not in insn.text
                         and skip[0])
        cond = (cond, 3 if pointer_store else 0)
    return ("insn", mmap, *io, out, flow, cond, *tail)


def _port_access(flags, parts):
    """(io_read, io_write). A qualifier switches the instruction's Smem field to the I/O
    space (SPRU375G 5-327): when one Smem is read and written ('*AR3 = *AR3 | #1'),
    either qualifier applies to both accesses."""
    read, write = "readport()" in flags, "writeport()" in flags
    for p in parts:
        sides = ASSIGN.split(p, maxsplit=1)
        if len(sides) == 2 and sides[0].strip().startswith("*") and sides[0].strip() in sides[1]:
            return (read or write, read or write)
    return read, write


def _is_mar_assign(part):
    """'XARn = mar(...)': the address phase loads the register."""
    return part[0] == "assign" and isinstance(part[2], tuple) and part[2][:2] == ("call", "mar")


def _dual(text):
    """'A = .., B = ..' (dual multiply) -> two statements. Both read the same Cmem
    operand coef(*CDP[+-]) once: the first statement reads it without the CDP update,
    the second with it (all reads of a bundle come before its writes, but address
    updates happen at the read)."""
    if TEST_SET.match(text):
        return [text]
    square = SQDST.fullmatch(text)
    if square:
        # SPRU374E 4.77: ACy = ACy + (ACx * ACx) :: ACx = (Xmem << #16) - (Ymem << #16).
        # The subtraction reports its borrow in CARRY every time (sqdst_difference),
        # unlike SUB Smem << #16, which only clears CARRY on a borrow (4.80.11).
        xmem, ymem, acc, total = square.groups()
        return [f"{total} = {total} + sqdst_square({acc})",
                f"{acc} = sqdst_difference(({xmem} << #16) - ({ymem} << #16))"]
    if DUAL_ALU.fullmatch(text):
        return [text]
    m = MANT.fullmatch(text)
    if m:
        # dis55 prints 'Tx = exp(ACx)'; the instruction stores -exp (SPRU375G, mant)
        return [f"{m[1]} = mant({m[2]})", f"{m[3]} = mant_exp({m[2]})"]
    sides = split_top(text, ", ")
    if len(sides) != 2 or not all(ASSIGN.search(s) for s in sides):
        return [text]
    coefs = [COEF.findall(s) for s in sides]
    if coefs[0] or coefs[1]:
        if coefs[0] != coefs[1] or len(coefs[0]) != 1 or coefs[0][0] not in COEF_FORMS:
            raise NotImplementedError(f"dual statement with Cmem operands {coefs}")
        sides[0] = sides[0].replace(f"coef({coefs[0][0]})", "coef(*CDP)")
    return sides


def _flow(text, nxt, resolve):
    m = FLOW.match(text)
    if not m:
        return None
    c = node(parse(m["cond"]), False) if m["cond"] else None
    op, target = m["op"], m["target"]
    if op == "return_int":
        if m["cond"] or target:
            raise NotImplementedError("conditional return_int")
        return ("return_int",)
    if op == "return":
        if target:
            raise NotImplementedError("return with operand")
        return ("return", c)
    dest = ("acc", target) if ACC.match(target or "") else ("abs", resolve(target))
    return ("goto", c, dest) if op == "goto" else ("call", c, dest, nxt)


def _data(text, mmap):
    count = BIT_COUNT.fullmatch(text)
    if count:
        return ("bit_count", *count.groups())
    dual = DUAL_ALU.fullmatch(text)
    if dual:
        return ("dual_alu", dual[1], node(parse(dual[2]), mmap),
                node(parse(dual[4]), mmap), dual[3] == "-")
    if text in ("nop", "nop_16"):
        return ("nop",)
    m = STATUS_BIT.match(text)
    if m:
        return ("status_bit", MMR[m[1]], int(m[2]), int(m[3]))
    m = REG_TOGGLE.match(text)
    if m:
        return ("reg_toggle", ALIASES.get(m[1], m[1]), number("#" + (m[2] or m[3])))
    m = TEST_SET.match(text)
    if m:
        if re.search(r"[+-]\)?$|\*[+-]", m[2]):
            raise NotImplementedError("bit test-and-set with pointer modification")
        return ("test_set", m[1], node(parse(m[2]), mmap), number("#" + m[3]), int(m[4]))
    m = REG_BIT.match(text)
    if m:
        return ("reg_bit", ALIASES.get(m[1], m[1]), number("#" + (m[2] or m[3])), int(m[4]))
    m = SUBC.fullmatch(text)
    if m:
        return ("subc", node(parse(m[1]), mmap), m[2], m[3])
    m = re.fullmatch(r"(push|pshboth)\((.+)\)", text)
    if m:
        nodes = parse_list(m[2])
        if m[1] == "pshboth":
            return ("pshboth", node(nodes[0], mmap))
        return ("push", [node(n, mmap) for n in nodes],
                [n[0] == "call" and n[1] == "dbl" for n in nodes])
    m = re.fullmatch(r"dbl\(push\((AC[0-3])\)\)", text)
    if m:
        return ("dbl_push_acc", m[1])
    m = MAR_ARITH.fullmatch(text)
    if m:
        return ("mar_arith", node(parse(m[1]), mmap), {"+": 1, "-": -1, "=": 0}[m[2]],
                node(parse(m[3]), mmap))
    if text.startswith("mar("):
        return ("mar", node(parse(text), mmap))
    sides = ASSIGN.split(text, maxsplit=1)
    if len(sides) != 2:
        raise NotImplementedError("unknown statement")
    lhs, rhs = sides[0].strip(), sides[1].strip()
    if rhs == "popboth()":
        return ("popboth", node(parse_list(lhs)[0], mmap))
    if rhs in ("pop()", "dbl(pop())"):
        nodes = parse_list(lhs)
        double = rhs == "dbl(pop())" or (len(nodes) == 1 and nodes[0][0] == "call"
                                         and nodes[0][1] == "dbl")
        return ("pop", [node(n, mmap) for n in nodes], double)
    if rhs.startswith("pair(") and not lhs.startswith("dbl("):
        lhs = f"dbl({lhs})"
    if lhs.startswith("pair(") and not rhs.startswith("dbl("):
        rhs = f"dbl({rhs})"
    dst = parse_list(lhs)
    if len(dst) != 1:
        raise NotImplementedError("multiple destinations")
    return ("assign", node(dst[0], mmap), node(parse(rhs), mmap))
