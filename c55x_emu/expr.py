"""Parser for dis55 algebraic operands and expressions -> small AST tuples.

AST nodes:
  ("num", value)                ("reg", name)               ("mmr", name)
  ("mem", mode, reg, arg)       ("call", fname, [args])     ("un", op, x)
  ("bin", op, a, b)

Memory modes (mode, reg, arg):
  abs (None, addr)   abs16 (None, k)   port (None, k)   sp (None, k)    dp (None, k)
  ind (R, None)      post (R, +1/-1)   pre (R, k)       idx (R, k)      idxreg (R, Tn)
  postreg (R, ("+"|"-", Tn or AR0 or k))
"""

import re

MEM = re.compile(
    r"\*(?:\((?P<abs>#[^)]+)\)"
    r"|abs16\((?P<a16>#[^)]+)\)"
    r"|port\((?P<port>#[^)]+)\)"
    r"|SP\((?P<sp>#[^)]+)\)"
    r"|\((?P<pm_r>AR[0-7]|CDP)(?P<pm_op>[+-])(?P<pm_x>T[0-3]|AR0|#[^)]+)\)"
    r"|(?P<pre>[+-])(?P<pre_r>AR[0-7]|CDP)(?:\((?P<pre_k>#[^)]+)\))?"
    r"|(?P<r>AR[0-7]|CDP)(?:\((?P<idx>short\(#[^)]+\)|[^)]+)\)|(?P<post>[+-])(?![\w#(*]))?)")
TOKEN = re.compile(r"\s*(?:(?P<num>#[-\w]+)|(?P<id>[A-Za-z_$.][\w$.:]*)|(?P<dp>@[\w#-]+)"
                   r"|(?P<op><<<|>>>|<<|>>|==|!=|<=|>=|[-+*&|^~!<>(),]))")
BINARY = {"|": 10, "^": 11, "&": 12, "==": 15, "!=": 15, "<": 15, "<=": 15, ">": 15, ">=": 15,
          "<<": 25, ">>": 25, "<<<": 25, ">>>": 25, "+": 30, "-": 30, "*": 40}


def number(text):
    """'#8191', '#-1', '#0x020120', '#01600h' -> int."""
    s = text.lstrip("#")
    if s.lower().endswith("h") and re.fullmatch(r"-?[0-9a-fA-F]+h", s):
        return int(s[:-1], 16)
    return int(s, 0) if s.lstrip("-").startswith("0x") else int(s, 10)


def _mem(m):
    g = m.groupdict()
    if g["abs"]:
        return ("mem", "abs", None, number(g["abs"]))
    if g["a16"]:
        return ("mem", "abs16", None, number(g["a16"]))
    if g["port"]:
        return ("mem", "port", None, number(g["port"]))
    if g["sp"]:
        return ("mem", "sp", None, number(g["sp"]))
    if g["pm_r"]:
        x = g["pm_x"]
        return ("mem", "postreg", g["pm_r"], (g["pm_op"], number(x) if x.startswith("#") else x))
    if g["pre_r"]:
        k = number(g["pre_k"]) if g["pre_k"] else 1
        return ("mem", "pre", g["pre_r"], k if g["pre"] == "+" else -k)
    if g["idx"]:
        x = g["idx"]
        if x.startswith("short("):          # meaning depends on ARMS (addressing.py)
            return ("mem", "short", g["r"], number(x[len("short("):-1]))
        if x.startswith("#"):
            return ("mem", "idx", g["r"], number(x))
        if re.fullmatch(r"T[0-3]|AR0", x):
            return ("mem", "idxreg", g["r"], x)
        raise NotImplementedError(f"index form *{g['r']}({x})")
    if g["post"]:
        return ("mem", "post", g["r"], 1 if g["post"] == "+" else -1)
    return ("mem", "ind", g["r"], None)


class Parser:
    def __init__(self, text):
        self.text, self.pos = text, 0

    def fail(self, what):
        raise NotImplementedError(f"{what} at {self.pos} in '{self.text}'")

    def peek(self):
        if self.at_operand_star():
            return ("mem", None)
        m = TOKEN.match(self.text, self.pos)
        if not m or m.end() == self.pos:
            return (None, None)
        kind = m.lastgroup
        return (kind, m[kind])

    def at_operand_star(self):
        start = len(self.text) - len(self.text[self.pos:].lstrip())
        return MEM.match(self.text, start) is not None

    def take(self):
        if self.at_operand_star():
            self.pos = len(self.text) - len(self.text[self.pos:].lstrip())
            m = MEM.match(self.text, self.pos)
            if not m:
                self.fail("memory operand")
            self.pos = m.end()
            return ("mem", _mem(m))
        m = TOKEN.match(self.text, self.pos)
        if not m or m.end() == self.pos:
            self.fail("token")
        self.pos = m.end()
        return (m.lastgroup, m[m.lastgroup])

    def expect(self, op):
        kind, val = self.take()
        if val != op:
            self.fail(f"expected '{op}'")

    def done(self):
        return self.text[self.pos:].strip() == ""

    def expr(self, min_prec=0):
        left = self.unary()
        while True:
            kind, op = self.peek()
            if kind != "op" or op not in BINARY or BINARY[op] < min_prec:
                return left
            self.take()
            right = self.expr(BINARY[op] + 1)
            left = ("bin", op, left, right)

    def unary(self):
        kind, val = self.take()
        if kind == "mem":
            return val
        if kind == "num":
            return ("num", number(val))
        if kind == "dp":
            return ("mem", "dp", None, val[1:])
        if kind == "op" and val in ("-", "~", "!"):
            return ("un", val, self.unary())
        if kind == "op" and val == "(":
            e = self.expr()
            self.expect(")")
            return e
        if kind == "id":
            if self.peek() == ("op", "("):
                self.take()
                args = []
                if self.peek() != ("op", ")"):
                    args.append(self.expr())
                    while self.peek() == ("op", ","):
                        self.take()
                        args.append(self.expr())
                self.expect(")")
                return ("call", val, args)
            return ("reg", val)
        self.fail(f"operand '{val}'")


ABS = re.compile(r"\|((?:AC|T|AR)[0-7])\|")


def parse(text):
    p = Parser(ABS.sub(r"abs(\1)", text))      # |x| -> abs(x)
    node = p.expr()
    if not p.done():
        p.fail("trailing text")
    return node


def parse_list(text):
    """'T3,T2' -> [node, node]."""
    p = Parser(text)
    out = [p.expr()]
    while p.peek() == ("op", ","):
        p.take()
        out.append(p.expr())
    if not p.done():
        p.fail("trailing text")
    return out
