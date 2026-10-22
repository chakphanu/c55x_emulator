//! dis55 instruction text -> the instruction IR of c55x_emu/ir.py (see its module
//! doc for the layout). Each step follows ir.py, so the two front ends build the same
//! Ast and reject the same forms with the same message.

use std::sync::LazyLock;

use regex::Regex;

use crate::ast::Ast;
use crate::err::{unsupported, R};

use super::expr::{number, parse, parse_list, py_int, py_repr, s, tuple, Arg, Expr};
use super::regs::{alias, mmr};

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).unwrap()
}

macro_rules! regex {
    ($name:ident, $pattern:expr) => {
        static $name: LazyLock<Regex> = LazyLock::new(|| re($pattern));
    };
}

// Python re.match patterns start with '^', re.fullmatch patterns also end with '$'.
regex!(FLOW, r"^(?:(?:if|compare) \((?P<cond>.+)\) )?(?P<op>goto|call|return_int|return)(?:\s+(?P<target>\S+))?$");
regex!(EXECUTE, r"^if \((?P<cond>.+)\) execute\((?:D|AD)_Unit\)$");
regex!(REPEAT, r"^repeat\((?:#(\w+)|CSR)\)$");
regex!(REPEAT_CC, r"^while \((!?TC1) && \(RPTC < #(\w+)\)\) repeat$");
regex!(PTR_MOD, r"\*\(?[+-](AR\d|CDP)|\*\(?(AR\d|CDP)[+-]");
regex!(STATUS_BIT, r"^bit\((ST[0-3]), #(\d+)\) = #([01])$");
// bit set/clear in a register; dis55 prints the bit number of the Baddr form as *SP(#k)
regex!(REG_BIT, r"^bit\(((?:X?AR|T|AC)[0-7]), (?:#(\w+)|\*SP\(#(\w+)\))\) = #([01])$");
regex!(ACC, r"^AC[0-3]$");
regex!(REG_TOGGLE, r"^cbit\(((?:X?AR|T|AC)[0-7]), (?:#(\w+)|\*SP\(#(\w+)\))\)$");
regex!(SUBC, r"^subc\((.+), (AC[0-3]), (AC[0-3])\)$");
regex!(ADDRESS_PHASE, r"^(?:(?:X?AR[0-7]|X?CDP) = )?mar\(");
regex!(MAR_TARGET, r"^(?:(X?AR[0-7]|X?CDP) = mar\(|mar\(\*?[+-]?\(?(X?AR[0-7]|X?CDP|T[0-3])\b)");
// ir.py MANT ends with exp(\2); checked after the match
regex!(MANT, r"^(AC[0-3]) = mant\((AC[0-3])\), (T[0-3]) = exp\((AC[0-3])\)$");
regex!(BIT_COUNT, r"^(T[0-3]) = count\((AC[0-3]), (AC[0-3]), (TC[12])\)$");
regex!(SQDST, r"^sqdst\((.+),\s*(.+),\s*(AC[0-3]),\s*(AC[0-3])\)$");
regex!(COEF, r"coef\((\*\(CDP[+-]T[01]\)|[^()]*)\)");
regex!(MAR_ARITH, r"^mar\(((?:X?AR[0-7])|(?:X?CDP)|T[0-3]) ([-+=]) (.+)\)$");
regex!(PUSH, r"^(push|pshboth)\((.+)\)$");
regex!(DBL_PUSH_ACC, r"^dbl\(push\((AC[0-3])\)\)$");
regex!(TEST_SET_PTR, r"[+-]\)?$|\*[+-]");

const QUALIFIERS: [&str; 3] = ["mmap()", "readport()", "writeport()"];
const COEF_FORMS: [&str; 7] = ["*CDP", "*CDP+", "*CDP-", "*(CDP+T0)", "*(CDP-T0)", "*(CDP+T1)", "*(CDP-T1)"];

/// Branch target text -> address (Cpu.resolve); Err is the exception text.
pub type Resolve<'a> = &'a dyn Fn(Option<&str>) -> R<i64>;

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Byte offset of the first ASSIGN match, (?<![=!<>])=(?!=).
fn assign_at(text: &str) -> Option<usize> {
    let b = text.as_bytes();
    (0..b.len()).find(|&i| {
        b[i] == b'=' && (i == 0 || !b"=!<>".contains(&b[i - 1])) && b.get(i + 1) != Some(&b'=')
    })
}

/// ASSIGN.split(text, maxsplit=1)
fn assign_split(text: &str) -> Vec<&str> {
    match assign_at(text) {
        Some(i) => vec![&text[..i], &text[i + 1..]],
        None => vec![text],
    }
}

fn split_top(text: &str, sep: &str) -> Vec<String> {
    let b = text.as_bytes();
    let (mut parts, mut depth, mut start, mut i) = (Vec::new(), 0i32, 0, 0);
    while i < b.len() {
        match b[i] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            _ if depth == 0 && b[i..].starts_with(sep.as_bytes()) => {
                parts.push(text[start..i].trim().to_owned());
                i += sep.len();
                start = i;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    parts.push(text[start..].trim().to_owned());
    parts.retain(|p| !p.is_empty());
    parts
}

/// TEST_SET, '^(TC[12]) = bit\((.+), #(\w+)\), bit\(\2, #\3\) = #([01])$', matched
/// in the backtracking order of re: the longest Smem text first.
fn test_set(text: &str) -> Option<(String, String, String, String)> {
    let tc = text.get(..3).filter(|t| *t == "TC1" || *t == "TC2")?;
    let rest = text.get(3..)?.strip_prefix(" = bit(")?;
    for e2 in (1..=rest.len()).rev() {
        if !rest.is_char_boundary(e2) {
            continue;
        }
        let (smem, tail) = rest.split_at(e2);
        let Some(tail) = tail.strip_prefix(", #") else { continue };
        let k_len = tail.find(|c: char| !is_word(c)).unwrap_or(tail.len());
        if k_len == 0 {
            continue;
        }
        let k = &tail[..k_len];
        let expect = format!("), bit({}, #{}) = #", smem, k);
        if let Some(v) = tail[k_len..].strip_prefix(expect.as_str()) {
            if v == "0" || v == "1" {
                return Some((tc.to_owned(), smem.to_owned(), k.to_owned(), v.to_owned()));
            }
        }
    }
    None
}

/// DUAL_ALU fullmatch, 'HI\((AC[0-3])\) = HI\((.+)\) ([+-]) HI\((.+)\), LO\(\1\) =
/// LO\(\2\) \3 LO\(\4\)', in the backtracking order of re.
fn dual_alu(text: &str) -> Option<(String, String, String, String)> {
    let rest = text.strip_prefix("HI(")?;
    let acc = rest.get(..3).filter(|a| matches!(*a, "AC0" | "AC1" | "AC2" | "AC3"))?;
    let rest = rest.get(3..)?.strip_prefix(") = HI(")?;
    for e2 in (1..=rest.len()).rev() {
        if !rest.is_char_boundary(e2) {
            continue;
        }
        let (a, tail) = rest.split_at(e2);
        let sign = match tail.get(..7) {
            Some(") + HI(") => "+",
            Some(") - HI(") => "-",
            _ => continue,
        };
        let tail = &tail[7..];
        for e4 in (1..=tail.len()).rev() {
            if !tail.is_char_boundary(e4) {
                continue;
            }
            let (b, end) = tail.split_at(e4);
            if end == format!("), LO({}) = LO({}) {} LO({})", acc, a, sign, b) {
                return Some((acc.to_owned(), a.to_owned(), sign.to_owned(), b.to_owned()));
            }
        }
    }
    None
}

/// registers.RegisterFile.mmr_value name resolution.
fn mmr_address(name: &str) -> R<i64> {
    let base = name.strip_suffix("_L").unwrap_or(name);
    let mut base = alias(base).to_owned();
    let joined = name.replace('_', "");
    if mmr(&base).is_none() && mmr(&joined).is_some() {
        base = joined;
    }
    match mmr(&base) {
        Some(a) => Ok(a),
        None => unsupported(format!("memory-mapped register @{}", name)),
    }
}

/// Resolve register aliases and @NAME operands.
fn node(tree: Expr, mmap: bool) -> R<Expr> {
    Ok(match tree {
        Expr::Reg(n) => Expr::Reg(alias(&n).to_owned()),
        Expr::Mem("dp", _, arg) => {
            let name = match arg {
                Arg::Name(n) => n,
                Arg::Int(v) => return Ok(Expr::Mem("dp", None, Arg::Int(v))),
                _ => unreachable!(),
            };
            if mmap {
                Expr::Mmr(mmr_address(&name)?)
            } else {
                Expr::Mem("dp", None, Arg::Int(py_int(name.trim_start_matches('#'), 0)?))
            }
        }
        Expr::Call(f, args) => Expr::Call(f, args.into_iter().map(|a| node(a, mmap)).collect::<R<_>>()?),
        Expr::Un(op, x) => Expr::Un(op, Box::new(node(*x, mmap)?)),
        Expr::Bin(op, a, b) => Expr::Bin(op, Box::new(node(*a, mmap)?), Box::new(node(*b, mmap)?)),
        other => other,
    })
}

fn pnode(text: &str, mmap: bool) -> R<Ast> {
    Ok(node(parse(text)?, mmap)?.to_ast())
}

/// True if a partner reads the mar() register as a value.
fn mar_first(index: usize, parts: &[String]) -> R<bool> {
    let part = &parts[index];
    let Some(m) = MAR_TARGET.captures(part) else {
        return unsupported(format!("mar form '{}'", part));
    };
    let reg = m.get(1).or(m.get(2)).unwrap().as_str();
    let base = reg.trim_start_matches('X');
    let others = parts.iter().enumerate().filter(|(i, _)| *i != index)
        .map(|(_, p)| p.as_str()).collect::<Vec<_>>().join(" || ");
    let as_address = re(&format!(r"\*[+-]?\(?{}\b", base)).is_match(&others);
    // (?<![*+(\w])X?BASE\b
    let as_value = others.char_indices().any(|(i, _)| {
        if let Some(prev) = others[..i].chars().next_back() {
            if "*+(".contains(prev) || is_word(prev) {
                return false;
            }
        }
        let rest = &others[i..];
        let rest = rest.strip_prefix('X').filter(|r| r.starts_with(base)).unwrap_or(rest);
        rest.strip_prefix(base).is_some_and(|after| !after.chars().next().is_some_and(is_word))
    });
    if as_address && as_value {
        return unsupported(format!("{} used as address and value next to mar()", base));
    }
    Ok(as_value)
}

fn is_mar_assign(part: &Ast) -> bool {
    let Ok(rhs) = part.get_item(2) else { return false };
    matches!(part.get_item(0), Ok(Ast::Str(k)) if k == "assign")
        && matches!(rhs.get_item(0), Ok(Ast::Str(k)) if k == "call")
        && matches!(rhs.get_item(1), Ok(Ast::Str(f)) if f == "mar")
}

fn kind(part: &Ast) -> &str {
    match part.get_item(0) {
        Ok(Ast::Str(k)) => k,
        _ => "",
    }
}

/// The instruction at addr: its text, size in bytes and the last body instruction of
/// a repeat block it opens (listing block_end).
pub fn build(text: &str, addr: i64, size: i64, block_end: Option<i64>, resolve: Resolve<'_>) -> R<Ast> {
    let parts = split_top(text, "||");
    let flags: Vec<&String> = parts.iter().filter(|p| QUALIFIERS.contains(&p.as_str())).collect();
    let mmap = flags.iter().any(|f| *f == "mmap()");
    let mut split = Vec::new();
    for p in parts.iter().filter(|p| !QUALIFIERS.contains(&p.as_str())) {
        split.extend(dual(p)?);
    }
    let parts = split;
    let (io_read, io_write) = port_access(&flags, &parts);
    let nxt = addr + size;
    let (mut flow, mut out, mut cond, mut repeat, mut after) = (None, Vec::new(), None, None, false);
    for (i, p) in parts.iter().enumerate() {
        if p == "idle" {
            flow = Some(tuple(vec![s("idle")]));
            continue;
        }
        if let Some(m) = EXECUTE.captures(p) {
            cond = Some(pnode(&m["cond"], false)?);
            after = i > 0;          // 'previous || if (cond) execute': the next instruction
            continue;
        }
        if let Some(f) = flow_part(p, nxt, resolve)? {
            flow = Some(f);
            continue;
        }
        if p == "localrepeat {" || p == "blockrepeat {" {
            let kind = if p.starts_with("local") { "local" } else { "ext" };
            let end = block_end.map_or(Ast::None, Ast::Int);
            out.push(tuple(vec![Ast::Bool(false), tuple(vec![s("loop"), s(kind), Ast::Int(nxt), end])]));
            continue;
        }
        if let Some(m) = REPEAT.captures(p) {
            repeat = Some(match m.get(1) {
                Some(k) => Ast::Int(py_int(k.as_str(), 0)?),
                None => s("CSR"),
            });
            continue;
        }
        if let Some(m) = REPEAT_CC.captures(p) {
            // RPTCC k8, cond: (count, condition, condition field of the encoding)
            let count = py_int(&m[2], 0)?;
            if !(0..=254).contains(&count) {
                return unsupported("conditional repeat count outside 0..254");
            }
            let field = if &m[1] == "!TC1" { 0x74 } else { 0x64 };
            repeat = Some(tuple(vec![Ast::Int(count), pnode(&m[1], false)?, Ast::Int(field)]));
            continue;
        }
        let first = ADDRESS_PHASE.is_match(p) && parts.len() > 1 && mar_first(i, &parts)?;
        out.push(tuple(vec![Ast::Bool(first), data(p, mmap)?]));
    }
    if repeat.is_some() && (flow.is_some() || cond.is_some()) {
        return unsupported("repeat bundled with flow or conditional execute");
    }
    let ptr_mod = parts.iter()
        .filter(|p| !EXECUTE.is_match(p) && !ADDRESS_PHASE.is_match(p))
        .any(|p| PTR_MOD.is_match(p));
    let address_parts = out.iter().enumerate().filter(|(_, o)| {
        o.get_item(1).is_ok_and(|d| matches!(kind(d), "mar" | "mar_arith") || is_mar_assign(d))
    }).map(|(i, _)| Ast::Int(i as i64)).collect();
    let skip = tuple(vec![Ast::Bool(ptr_mod), Ast::List(address_parts)]);
    let ad_unit = text.contains("AD_Unit");
    let cond = match cond {
        None => Ast::None,
        Some(c) if after => {
            // SPRU375G 5-160: the partner runs unconditionally, the condition (read in
            // the address phase, before the partner executes) applies to the next
            // instruction
            if flow.is_some() {
                return unsupported("'previous || if (cond) execute' with flow");
            }
            // SWPU067E pp. 5-652..653 also permits previous || XCCPART.
            tuple(vec![c, Ast::Int(if ad_unit { 1 } else { 2 })])
        }
        Some(c) if out.is_empty() && flow.is_none() => {
            // alone: AD_Unit makes the whole next instruction conditional, D_Unit only
            // its data operations (SPRU375G 5-160, 5-163)
            tuple(vec![c, Ast::Int(if ad_unit { 1 } else { 2 })])
        }
        Some(c) => {
            if flow.is_some() {
                return unsupported("conditional execute bundled with flow");
            }
            // Mode 3 retains the unconditional address phase of an XCCPART partner.
            let pointer_store = text.contains("D_Unit") && !ad_unit && ptr_mod;
            tuple(vec![c, Ast::Int(if pointer_store { 3 } else { 0 })])
        }
    };
    Ok(tuple(vec![
        s("insn"), Ast::Bool(mmap), Ast::Bool(io_read), Ast::Bool(io_write), Ast::List(out),
        flow.unwrap_or(Ast::None), cond, repeat.unwrap_or(Ast::None), skip,
    ]))
}

/// (io_read, io_write). A qualifier switches the Smem field to the I/O space; when one
/// Smem is read and written, either qualifier applies to both accesses.
fn port_access(flags: &[&String], parts: &[String]) -> (bool, bool) {
    let read = flags.iter().any(|f| *f == "readport()");
    let write = flags.iter().any(|f| *f == "writeport()");
    for p in parts {
        let sides = assign_split(p);
        if sides.len() == 2 {
            let lhs = sides[0].trim();
            if lhs.starts_with('*') && sides[1].contains(lhs) {
                return (read || write, read || write);
            }
        }
    }
    (read, write)
}

/// 'A = .., B = ..' (dual multiply) -> two statements (ir.py _dual).
fn dual(text: &str) -> R<Vec<String>> {
    if test_set(text).is_some() {
        return Ok(vec![text.to_owned()]);
    }
    if let Some(m) = SQDST.captures(text) {
        // SPRU374E 4.77: ACy = ACy + (ACx * ACx) :: ACx = (Xmem << #16) - (Ymem << #16).
        let (xmem, ymem, acc, total) = (&m[1], &m[2], &m[3], &m[4]);
        return Ok(vec![
            format!("{} = {} + sqdst_square({})", total, total, acc),
            format!("{} = sqdst_difference(({} << #16) - ({} << #16))", acc, xmem, ymem),
        ]);
    }
    if dual_alu(text).is_some() {
        return Ok(vec![text.to_owned()]);
    }
    if let Some(m) = MANT.captures(text).filter(|m| m[2] == m[4]) {
        // dis55 prints 'Tx = exp(ACx)'; the instruction stores -exp (SPRU375G, mant)
        return Ok(vec![format!("{} = mant({})", &m[1], &m[2]), format!("{} = mant_exp({})", &m[3], &m[2])]);
    }
    let mut sides = split_top(text, ", ");
    if sides.len() != 2 || !sides.iter().all(|x| assign_at(x).is_some()) {
        return Ok(vec![text.to_owned()]);
    }
    let coefs: Vec<Vec<String>> = sides.iter()
        .map(|x| COEF.captures_iter(x).map(|c| c[1].to_owned()).collect()).collect();
    if !coefs[0].is_empty() || !coefs[1].is_empty() {
        if coefs[0] != coefs[1] || coefs[0].len() != 1 || !COEF_FORMS.contains(&coefs[0][0].as_str()) {
            let list = |v: &Vec<String>| format!("[{}]", v.iter().map(|x| py_repr(x)).collect::<Vec<_>>().join(", "));
            return unsupported(format!("dual statement with Cmem operands [{}, {}]", list(&coefs[0]), list(&coefs[1])));
        }
        sides[0] = sides[0].replace(&format!("coef({})", coefs[0][0]), "coef(*CDP)");
    }
    Ok(sides)
}

fn flow_part(text: &str, nxt: i64, resolve: Resolve<'_>) -> R<Option<Ast>> {
    let Some(m) = FLOW.captures(text) else { return Ok(None) };
    let c = match m.name("cond") {
        Some(c) => pnode(c.as_str(), false)?,
        None => Ast::None,
    };
    let (op, target) = (&m["op"], m.name("target").map(|t| t.as_str()));
    if op == "return_int" {
        if m.name("cond").is_some() || target.is_some() {
            return unsupported("conditional return_int");
        }
        return Ok(Some(tuple(vec![s("return_int")])));
    }
    if op == "return" {
        if target.is_some() {
            return unsupported("return with operand");
        }
        return Ok(Some(tuple(vec![s("return"), c])));
    }
    let dest = match target {
        Some(t) if ACC.is_match(t) => tuple(vec![s("acc"), s(t)]),
        _ => tuple(vec![s("abs"), Ast::Int(resolve(target)?)]),
    };
    Ok(Some(if op == "goto" {
        tuple(vec![s("goto"), c, dest])
    } else {
        tuple(vec![s("call"), c, dest, Ast::Int(nxt)])
    }))
}

fn data(text: &str, mmap: bool) -> R<Ast> {
    if let Some(m) = BIT_COUNT.captures(text) {
        return Ok(tuple(vec![s("bit_count"), s(&m[1]), s(&m[2]), s(&m[3]), s(&m[4])]));
    }
    if let Some((acc, a, sign, b)) = dual_alu(text) {
        return Ok(tuple(vec![s("dual_alu"), s(&acc), pnode(&a, mmap)?, pnode(&b, mmap)?, Ast::Bool(sign == "-")]));
    }
    if text == "nop" || text == "nop_16" {
        return Ok(tuple(vec![s("nop")]));
    }
    if let Some(m) = STATUS_BIT.captures(text) {
        return Ok(tuple(vec![
            s("status_bit"), Ast::Int(mmr(&m[1]).unwrap()), Ast::Int(py_int(&m[2], 10)?), Ast::Int(py_int(&m[3], 10)?),
        ]));
    }
    if let Some(m) = REG_TOGGLE.captures(text) {
        let k = m.get(2).or(m.get(3)).unwrap().as_str();
        return Ok(tuple(vec![s("reg_toggle"), s(alias(&m[1])), Ast::Int(number(&format!("#{}", k))?)]));
    }
    if let Some((tc, smem, k, v)) = test_set(text) {
        if TEST_SET_PTR.is_match(&smem) {
            return unsupported("bit test-and-set with pointer modification");
        }
        return Ok(tuple(vec![
            s("test_set"), s(&tc), pnode(&smem, mmap)?, Ast::Int(number(&format!("#{}", k))?), Ast::Int(py_int(&v, 10)?),
        ]));
    }
    if let Some(m) = REG_BIT.captures(text) {
        let k = m.get(2).or(m.get(3)).unwrap().as_str();
        return Ok(tuple(vec![
            s("reg_bit"), s(alias(&m[1])), Ast::Int(number(&format!("#{}", k))?), Ast::Int(py_int(&m[4], 10)?),
        ]));
    }
    if let Some(m) = SUBC.captures(text) {
        return Ok(tuple(vec![s("subc"), pnode(&m[1], mmap)?, s(&m[2]), s(&m[3])]));
    }
    if let Some(m) = PUSH.captures(text) {
        let nodes = parse_list(&m[2])?;
        if &m[1] == "pshboth" {
            return Ok(tuple(vec![s("pshboth"), node(nodes[0].clone(), mmap)?.to_ast()]));
        }
        let double = nodes.iter().map(|n| Ast::Bool(n.is_call("dbl"))).collect();
        let nodes = nodes.into_iter().map(|n| Ok(node(n, mmap)?.to_ast())).collect::<R<_>>()?;
        return Ok(tuple(vec![s("push"), Ast::List(nodes), Ast::List(double)]));
    }
    if let Some(m) = DBL_PUSH_ACC.captures(text) {
        return Ok(tuple(vec![s("dbl_push_acc"), s(&m[1])]));
    }
    if let Some(m) = MAR_ARITH.captures(text) {
        let sign = match &m[2] { "+" => 1, "-" => -1, _ => 0 };
        return Ok(tuple(vec![s("mar_arith"), pnode(&m[1], mmap)?, Ast::Int(sign), pnode(&m[3], mmap)?]));
    }
    if text.starts_with("mar(") {
        return Ok(tuple(vec![s("mar"), pnode(text, mmap)?]));
    }
    let sides = assign_split(text);
    if sides.len() != 2 {
        return unsupported("unknown statement");
    }
    let (mut lhs, mut rhs) = (sides[0].trim().to_owned(), sides[1].trim().to_owned());
    if rhs == "popboth()" {
        let n = parse_list(&lhs)?.swap_remove(0);
        return Ok(tuple(vec![s("popboth"), node(n, mmap)?.to_ast()]));
    }
    if rhs == "pop()" || rhs == "dbl(pop())" {
        let nodes = parse_list(&lhs)?;
        let double = rhs == "dbl(pop())" || (nodes.len() == 1 && nodes[0].is_call("dbl"));
        let nodes = nodes.into_iter().map(|n| Ok(node(n, mmap)?.to_ast())).collect::<R<_>>()?;
        return Ok(tuple(vec![s("pop"), Ast::List(nodes), Ast::Bool(double)]));
    }
    if rhs.starts_with("pair(") && !lhs.starts_with("dbl(") {
        lhs = format!("dbl({})", lhs);
    }
    if lhs.starts_with("pair(") && !rhs.starts_with("dbl(") {
        rhs = format!("dbl({})", rhs);
    }
    let mut dst = parse_list(&lhs)?;
    if dst.len() != 1 {
        return unsupported("multiple destinations");
    }
    Ok(tuple(vec![s("assign"), node(dst.swap_remove(0), mmap)?.to_ast(), pnode(&rhs, mmap)?]))
}
