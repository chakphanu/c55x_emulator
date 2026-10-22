//! dis55 operands and expressions -> Expr, after c55x_emu/expr.py.
//!
//! Expr mirrors the expr.py AST tuples:
//!   ("num", value)                ("reg", name)               ("mmr", address)
//!   ("mem", mode, reg, arg)       ("call", fname, [args])     ("un", op, x)
//!   ("bin", op, a, b)
//! Memory modes (mode, reg, arg):
//!   abs (None, addr)   abs16 (None, k)   port (None, k)   sp (None, k)    dp (None, name)
//!   ind (R, None)      post (R, +1/-1)   pre (R, k)       idx (R, k)      idxreg (R, Tn)
//!   short (R, k)       postreg (R, ("+"|"-", Tn or AR0 or k))
//! Errors carry the text of the Python exception (NotImplementedError, or the ValueError
//! of int()), so both front ends report the same reason for a form they reject.

use std::sync::LazyLock;

use regex::{Captures, Regex};

use crate::ast::Ast;
use crate::err::{unsupported, R};

#[derive(Clone, Debug, PartialEq)]
pub enum Arg {
    None,
    Int(i64),
    Name(String),
    /// postreg: (sign, step)
    Step(String, Box<Arg>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Num(i64),
    Reg(String),
    Mmr(i64),
    Mem(&'static str, Option<String>, Arg),
    Call(String, Vec<Expr>),
    Un(String, Box<Expr>),
    Bin(String, Box<Expr>, Box<Expr>),
}

pub fn s(v: &str) -> Ast {
    Ast::Str(v.to_owned())
}

pub fn tuple(v: Vec<Ast>) -> Ast {
    Ast::Tuple(v)
}

impl Arg {
    fn to_ast(&self) -> Ast {
        match self {
            Arg::None => Ast::None,
            Arg::Int(v) => Ast::Int(*v),
            Arg::Name(n) => s(n),
            Arg::Step(op, x) => tuple(vec![s(op), x.to_ast()]),
        }
    }
}

impl Expr {
    pub fn to_ast(&self) -> Ast {
        match self {
            Expr::Num(v) => tuple(vec![s("num"), Ast::Int(*v)]),
            Expr::Reg(n) => tuple(vec![s("reg"), s(n)]),
            Expr::Mmr(a) => tuple(vec![s("mmr"), Ast::Int(*a)]),
            Expr::Mem(mode, reg, arg) => tuple(vec![
                s("mem"), s(mode), reg.as_deref().map_or(Ast::None, s), arg.to_ast(),
            ]),
            Expr::Call(f, args) => tuple(vec![
                s("call"), s(f), Ast::List(args.iter().map(Expr::to_ast).collect()),
            ]),
            Expr::Un(op, x) => tuple(vec![s("un"), s(op), x.to_ast()]),
            Expr::Bin(op, a, b) => tuple(vec![s("bin"), s(op), a.to_ast(), b.to_ast()]),
        }
    }

    /// ("call", "dbl", ...)
    pub fn is_call(&self, name: &str) -> bool {
        matches!(self, Expr::Call(f, _) if f == name)
    }
}

/// Python repr() of a str.
pub fn py_repr(v: &str) -> String {
    let q = if v.contains('\'') && !v.contains('"') { '"' } else { '\'' };
    let mut out = String::from(q);
    for c in v.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == q => { out.push('\\'); out.push(c); }
            c if (c as u32) < 0x20 || c as u32 == 0x7f => out.push_str(&format!("\\x{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push(q);
    out
}

fn py_space(c: char) -> bool {
    c.is_whitespace() || ('\x1c'..='\x1f').contains(&c)
}

/// Python int(text, base) for base 0, 10 and 16.
pub fn py_int(text: &str, base: u32) -> R<i64> {
    let bad = || unsupported(format!("invalid literal for int() with base {}: {}", base, py_repr(text)));
    let t = text.trim_matches(py_space);
    let (neg, t) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    let prefix = t.get(..2).map(|p| p.to_ascii_lowercase());
    let (radix, digits, prefixed) = match (base, prefix.as_deref()) {
        (0 | 16, Some("0x")) => (16, &t[2..], true),
        (0, Some("0o")) => (8, &t[2..], true),
        (0, Some("0b")) => (2, &t[2..], true),
        (0, _) => (10, t, false),
        (b, _) => (b, t, false),
    };
    let digits = match digits.strip_prefix('_') {
        Some(rest) if prefixed => rest,
        _ => digits,
    };
    if digits.is_empty() || digits.starts_with('_') || digits.ends_with('_') || digits.contains("__") {
        return bad();
    }
    let mut value: i128 = 0;
    for c in digits.chars().filter(|&c| c != '_') {
        let d = match c.to_digit(radix) { Some(d) => d, None => return bad() };
        value = value * radix as i128 + d as i128;
        if value > i64::MAX as i128 + 1 {
            return unsupported(format!("integer {} out of range", text));
        }
    }
    // base 0: a decimal literal has no leading zeros (except zero itself)
    if base == 0 && radix == 10 && digits.starts_with('0') && value != 0 {
        return bad();
    }
    let value = if neg { -value } else { value };
    i64::try_from(value).or_else(|_| unsupported(format!("integer {} out of range", text)))
}

/// '#8191', '#-1', '#0x020120', '#01600h' -> int (expr.number).
pub fn number(text: &str) -> R<i64> {
    let v = text.trim_start_matches('#');
    let digits = v.strip_prefix('-').unwrap_or(v);
    if let Some(hex) = digits.strip_suffix('h') {
        if !hex.is_empty() && hex.bytes().all(|c| c.is_ascii_hexdigit()) {
            return py_int(&v[..v.len() - 1], 16);
        }
    }
    if v.trim_start_matches('-').starts_with("0x") {
        py_int(v, 0)
    } else {
        py_int(v, 10)
    }
}

static MEM: LazyLock<Regex> = LazyLock::new(|| Regex::new(concat!(
    r"^\*(?:\((?P<abs>#[^)]+)\)",
    r"|abs16\((?P<a16>#[^)]+)\)",
    r"|port\((?P<port>#[^)]+)\)",
    r"|SP\((?P<sp>#[^)]+)\)",
    r"|\((?P<pm_r>AR[0-7]|CDP)(?P<pm_op>[+-])(?P<pm_x>T[0-3]|AR0|#[^)]+)\)",
    r"|(?P<pre>[+-])(?P<pre_r>AR[0-7]|CDP)(?:\((?P<pre_k>#[^)]+)\))?",
    r"|(?P<r>AR[0-7]|CDP)(?:\((?P<idx>short\(#[^)]+\)|[^)]+)\)|(?P<post>[+-]))?)",
)).unwrap());

static TOKEN: LazyLock<Regex> = LazyLock::new(|| Regex::new(concat!(
    r"^\s*(?:(?P<num>#[-\w]+)|(?P<id>[A-Za-z_$.][\w$.:]*)|(?P<dp>@[\w#-]+)",
    r"|(?P<op><<<|>>>|<<|>>|==|!=|<=|>=|[-+*&|^~!<>(),]))",
)).unwrap());

static ABS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\|((?:AC|T|AR)[0-7])\|").unwrap());

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// MEM.match at the start of text -> (end, captures). expr.py ends a post-modify
/// '+'/'-' with the lookahead (?![\w#(*]); a failed lookahead leaves the operand
/// without it (no other alternative can match there).
fn mem_match(text: &str) -> Option<(usize, Captures<'_>)> {
    let c = MEM.captures(text)?;
    let mut end = c.get(0).unwrap().end();
    if let Some(post) = c.name("post") {
        if text[post.end()..].chars().next().is_some_and(|n| is_word(n) || "#(*".contains(n)) {
            end = post.start();
        }
    }
    Some((end, c))
}

fn mem(c: &Captures<'_>, end: usize) -> R<Expr> {
    let g = |n: &str| c.name(n).map(|m| m.as_str()).filter(|v| !v.is_empty());
    let r = || g("r").map(str::to_owned);
    Ok(if let Some(v) = g("abs") {
        Expr::Mem("abs", None, Arg::Int(number(v)?))
    } else if let Some(v) = g("a16") {
        Expr::Mem("abs16", None, Arg::Int(number(v)?))
    } else if let Some(v) = g("port") {
        Expr::Mem("port", None, Arg::Int(number(v)?))
    } else if let Some(v) = g("sp") {
        Expr::Mem("sp", None, Arg::Int(number(v)?))
    } else if let Some(reg) = g("pm_r") {
        let x = g("pm_x").unwrap();
        let x = if x.starts_with('#') { Arg::Int(number(x)?) } else { Arg::Name(x.to_owned()) };
        Expr::Mem("postreg", Some(reg.to_owned()), Arg::Step(g("pm_op").unwrap().to_owned(), Box::new(x)))
    } else if let Some(reg) = g("pre_r") {
        let k = match g("pre_k") { Some(k) => number(k)?, None => 1 };
        Expr::Mem("pre", Some(reg.to_owned()), Arg::Int(if g("pre") == Some("+") { k } else { -k }))
    } else if let Some(x) = g("idx") {
        if let Some(inner) = x.strip_prefix("short(") {
            // meaning depends on ARMS (addressing.py)
            Expr::Mem("short", r(), Arg::Int(number(&inner[..inner.len().saturating_sub(1)])?))
        } else if x.starts_with('#') {
            Expr::Mem("idx", r(), Arg::Int(number(x)?))
        } else if matches!(x, "T0" | "T1" | "T2" | "T3" | "AR0") {
            Expr::Mem("idxreg", r(), Arg::Name(x.to_owned()))
        } else {
            return unsupported(format!("index form *{}({})", g("r").unwrap(), x));
        }
    } else if c.name("post").is_some_and(|p| p.end() <= end) {
        Expr::Mem("post", r(), Arg::Int(if g("post") == Some("+") { 1 } else { -1 }))
    } else {
        Expr::Mem("ind", r(), Arg::None)
    })
}

#[derive(PartialEq)]
enum Kind {
    Mem,
    Num,
    Id,
    Dp,
    Op,
}

enum Tok {
    Mem(Expr),
    Text(Kind, String),
}

fn binary(op: &str) -> Option<u32> {
    Some(match op {
        "|" => 10,
        "^" => 11,
        "&" => 12,
        "==" | "!=" | "<" | "<=" | ">" | ">=" => 15,
        "<<" | ">>" | "<<<" | ">>>" => 25,
        "+" | "-" => 30,
        "*" => 40,
        _ => return None,
    })
}

struct Parser {
    text: String,
    pos: usize,
}

impl Parser {
    fn fail<T>(&self, what: &str) -> R<T> {
        unsupported(format!("{} at {} in '{}'", what, self.pos, self.text))
    }

    fn start(&self) -> usize {
        self.text.len() - self.text[self.pos..].trim_start_matches(py_space).len()
    }

    fn at_operand_star(&self) -> bool {
        MEM.is_match(&self.text[self.start()..])
    }

    fn token(&self) -> Option<(Kind, String, usize)> {
        let c = TOKEN.captures(&self.text[self.pos..])?;
        let end = self.pos + c.get(0).unwrap().end();
        for (name, kind) in [("num", Kind::Num), ("id", Kind::Id), ("dp", Kind::Dp), ("op", Kind::Op)] {
            if let Some(m) = c.name(name) {
                return Some((kind, m.as_str().to_owned(), end));
            }
        }
        None
    }

    fn peek(&self) -> Option<(Kind, String)> {
        if self.at_operand_star() {
            return Some((Kind::Mem, String::new()));
        }
        self.token().map(|(k, v, _)| (k, v))
    }

    fn peek_op(&self, op: &str) -> bool {
        matches!(self.peek(), Some((Kind::Op, v)) if v == op)
    }

    fn take(&mut self) -> R<Tok> {
        if self.at_operand_star() {
            self.pos = self.start();
            let text = self.text[self.pos..].to_owned();
            let (end, c) = mem_match(&text).unwrap();
            self.pos += end;
            return Ok(Tok::Mem(mem(&c, end)?));
        }
        match self.token() {
            Some((kind, v, end)) => {
                self.pos = end;
                Ok(Tok::Text(kind, v))
            }
            None => self.fail("token"),
        }
    }

    fn expect(&mut self, op: &str) -> R<()> {
        match self.take()? {
            Tok::Text(_, v) if v == op => Ok(()),
            _ => self.fail(&format!("expected '{}'", op)),
        }
    }

    fn done(&self) -> bool {
        self.text[self.pos..].trim_matches(py_space).is_empty()
    }

    fn expr(&mut self, min_prec: u32) -> R<Expr> {
        let mut left = self.unary()?;
        loop {
            let op = match self.peek() {
                Some((Kind::Op, op)) => op,
                _ => return Ok(left),
            };
            let prec = match binary(&op) {
                Some(p) if p >= min_prec => p,
                _ => return Ok(left),
            };
            self.take()?;
            let right = self.expr(prec + 1)?;
            left = Expr::Bin(op, Box::new(left), Box::new(right));
        }
    }

    fn unary(&mut self) -> R<Expr> {
        let (kind, v) = match self.take()? {
            Tok::Mem(e) => return Ok(e),
            Tok::Text(kind, v) => (kind, v),
        };
        match kind {
            Kind::Num => Ok(Expr::Num(number(&v)?)),
            Kind::Dp => Ok(Expr::Mem("dp", None, Arg::Name(v[1..].to_owned()))),
            Kind::Op if matches!(v.as_str(), "-" | "~" | "!") => {
                Ok(Expr::Un(v, Box::new(self.unary()?)))
            }
            Kind::Op if v == "(" => {
                let e = self.expr(0)?;
                self.expect(")")?;
                Ok(e)
            }
            Kind::Id => {
                if !self.peek_op("(") {
                    return Ok(Expr::Reg(v));
                }
                self.take()?;
                let mut args = Vec::new();
                if !self.peek_op(")") {
                    args.push(self.expr(0)?);
                    while self.peek_op(",") {
                        self.take()?;
                        args.push(self.expr(0)?);
                    }
                }
                self.expect(")")?;
                Ok(Expr::Call(v, args))
            }
            _ => self.fail(&format!("operand '{}'", v)),
        }
    }
}

/// One expression; '|x|' is read as abs(x).
pub fn parse(text: &str) -> R<Expr> {
    let mut p = Parser { text: ABS.replace_all(text, "abs($1)").into_owned(), pos: 0 };
    let e = p.expr(0)?;
    if !p.done() {
        return p.fail("trailing text");
    }
    Ok(e)
}

/// 'T3,T2' -> [expr, expr].
pub fn parse_list(text: &str) -> R<Vec<Expr>> {
    let mut p = Parser { text: text.to_owned(), pos: 0 };
    let mut out = vec![p.expr(0)?];
    while p.peek_op(",") {
        p.take()?;
        out.push(p.expr(0)?);
    }
    if !p.done() {
        return p.fail("trailing text");
    }
    Ok(out)
}
