//! ir.py instructions -> Insn, and their execution (port of stmt.py / special.py): every
//! data part reads in the read phase and writes in the commit phase; address-phase mar()
//! parts read and commit first; the flow condition is read before the data, the branch
//! is taken last.

use pyo3::prelude::*;

use crate::compile::{int, item, node, reg, text, Node};
use crate::core::Core;
use crate::dest::writable;
use crate::err::{emu, unsupported, R};
use crate::fuse::{run_all, Assign, Run};
use crate::expr::{get, Expr};
use crate::parts::run_parts;
use crate::run::{loops_after, loops_branch};
use crate::state::Reg;

pub enum Part {
    Nop,
    StatusBit(u32, u32, bool),
    RegToggle(Reg, u32),
    TestSet(Reg, Expr, u32, bool),
    RegBit(Reg, u32, bool),
    Subc(Expr, Reg, Reg),
    Push(Vec<(Expr, bool)>),
    PshBoth(Expr),
    DblPushAcc(Reg),
    MarArith(Expr, i32, Expr),
    Mar(Expr),
    Assign(Assign),
    HiLoad(Reg, Reg),                   // HI(ACx) = TAx
    PopBoth(Expr),
    Pop(Vec<Expr>, bool),
    Loop(u8, u32, u32),
}

pub enum Target { Abs(u32), Acc(Reg) }

pub enum Flow {
    Goto(Option<Expr>, Target),
    Call(Option<Expr>, Target, u32),
    Return(Option<Expr>),
    ReturnInt,
    Idle,
}

pub struct Insn {
    parts: Vec<(bool, Part)>,
    flow: Option<Flow>,
    cond: Option<(Expr, u8)>,           // mode: 0 partners, 1 next instruction, 2 its data (ir.py)
    repeat: Option<Option<u32>>, // Some(None) = CSR
    fused: Option<Run>,          // only assignments (fuse.rs); parts is then empty
    ptr_mod: bool,               // a data part modifies a pointer (D_Unit skip, ir.py)
    address_parts: Vec<usize>,   // indices of the mar() parts
    addr: u32,
    nxt: u32,
}

fn cond_node(n: &Node<'_>) -> R<Option<Expr>> {
    if n.is_none() { Ok(None) } else { Ok(Some(node(n, false, false, false)?)) }
}

fn dst(e: Expr) -> R<Expr> {
    if writable(&e) { Ok(e) } else { unsupported("destination is not writable") }
}

pub fn compile_insn(ir: &Node<'_>, pc: u32, size: u32) -> R<Insn> {
    let mmap: bool = item(ir, 1)?.extract()?;
    let (io_r, io_w): (bool, bool) = (item(ir, 2)?.extract()?, item(ir, 3)?.extract()?);
    let io = io_r || io_w;
    let v = |n: &Node<'_>| node(n, false, io, mmap);
    let mut parts = Vec::new();
    for p in item(ir, 4)?.try_iter()? {
        let p = p?;
        let first: bool = item(&p, 0)?.extract()?;
        let d = item(&p, 1)?;
        let part = match text(&d, 0)?.as_str() {
            "nop" => Part::Nop,
            "status_bit" => Part::StatusBit(int(&d, 1)? as u32, int(&d, 2)? as u32, int(&d, 3)? != 0),
            "reg_toggle" => Part::RegToggle(reg(&text(&d, 1)?)?, int(&d, 2)? as u32),
            "test_set" => Part::TestSet(reg(&text(&d, 1)?)?, dst(v(&item(&d, 2)?)?)?, int(&d, 3)? as u32, int(&d, 4)? != 0),
            "reg_bit" => Part::RegBit(reg(&text(&d, 1)?)?, int(&d, 2)? as u32, int(&d, 3)? != 0),
            "subc" => Part::Subc(v(&item(&d, 1)?)?, reg(&text(&d, 2)?)?, reg(&text(&d, 3)?)?),
            "push" => {
                let nodes = item(&d, 1)?;
                let dbl = item(&d, 2)?;
                let mut items = Vec::new();
                for i in 0..nodes.len()? {
                    items.push((v(&item(&nodes, i)?)?, item(&dbl, i)?.extract()?));
                }
                Part::Push(items)
            }
            "pshboth" => Part::PshBoth(v(&item(&d, 1)?)?),
            "dbl_push_acc" => Part::DblPushAcc(reg(&text(&d, 1)?)?),
            "mar_arith" => Part::MarArith(dst(v(&item(&d, 1)?)?)?, int(&d, 2)? as i32, v(&item(&d, 3)?)?),
            "mar" => Part::Mar(v(&item(&d, 1)?)?),
            "assign" => {
                let lhs = dst(node(&item(&d, 1)?, false, io_w, mmap)?)?;
                let rhs = node(&item(&d, 2)?, false, io_r, mmap)?;
                if let (Expr::Half(acc, true), Expr::Reg(src @ (Reg::T(_) | Reg::Ar(_)), _)) = (&lhs, &rhs) {
                    if let Expr::Reg(a @ Reg::Ac(_), _) = acc.as_ref() {
                        parts.push((first, Part::HiLoad(*a, *src)));
                        continue;
                    }
                }
                Part::Assign(Assign::new(lhs, rhs))
            }
            "popboth" => Part::PopBoth(dst(v(&item(&d, 1)?)?)?),
            "pop" => {
                let nodes = item(&d, 1)?;
                let mut out = Vec::new();
                for i in 0..nodes.len()? {
                    out.push(dst(v(&item(&nodes, i)?)?)?);
                }
                Part::Pop(out, item(&d, 2)?.extract()?)
            }
            "loop" => Part::Loop(if text(&d, 1)? == "local" { 1 } else { 2 }, int(&d, 2)? as u32,
                                 item(&d, 3)?.extract::<Option<i64>>()?.unwrap_or(-1) as u32),
            other => return unsupported(format!("part {}", other)),
        };
        parts.push((first, part));
    }
    let f = item(ir, 5)?;
    let flow = if f.is_none() { None } else {
        let target = |t: &Node<'_>| -> R<Target> {
            Ok(if text(t, 0)? == "abs" { Target::Abs(int(t, 1)? as u32) } else { Target::Acc(reg(&text(t, 1)?)?) })
        };
        Some(match text(&f, 0)?.as_str() {
            "return_int" => Flow::ReturnInt,
            "idle" => Flow::Idle,
            "return" => Flow::Return(cond_node(&item(&f, 1)?)?),
            "goto" => Flow::Goto(cond_node(&item(&f, 1)?)?, target(&item(&f, 2)?)?),
            "call" => Flow::Call(cond_node(&item(&f, 1)?)?, target(&item(&f, 2)?)?, int(&f, 3)? as u32),
            other => return unsupported(format!("flow {}", other)),
        })
    };
    let c = item(ir, 6)?;
    let cond = if c.is_none() { None } else {
        Some((node(&item(&c, 0)?, false, false, false)?, item(&c, 1)?.extract()?))
    };
    let r = item(ir, 7)?;
    let repeat = if r.is_none() { None } else if r.extract::<String>().is_ok() { Some(None) } else {
        Some(Some(r.extract::<i64>()? as u32))
    };
    let skip = item(ir, 8)?;
    let (ptr_mod, address_parts): (bool, Vec<usize>) = (item(&skip, 0)?.extract()?, item(&skip, 1)?.extract()?);
    let mut fused = None;
    let assigns = !parts.is_empty() && parts.len() <= 4 && address_parts.is_empty()
        && parts.iter().all(|(_, p)| matches!(p, Part::Assign(..)));
    if assigns && flow.is_none() && cond.is_none() && repeat.is_none() {
        fused = Some(run_all(parts.drain(..).filter_map(|(first, p)| match p {
            Part::Assign(a) => Some((first, a)),
            _ => None,
        }).collect()));
    }
    Ok(Insn { parts, flow, cond, repeat, fused, ptr_mod, address_parts, addr: pc, nxt: pc + size })
}

fn truth(m: &mut Core, c: &Option<Expr>) -> R<bool> {
    match c { None => Ok(true), Some(e) => Ok(get(m, e)? != 0) }
}

pub fn exec(m: &mut Core, i: &Insn) -> R<Option<u32>> {
    if let Some(f) = &i.fused {
        f(m)?;
        return Ok(None);
    }
    match &i.cond {
        Some((c, 1)) => {                   // the condition is for the next instruction
            let skip = get(m, c)? == 0;
            let r = exec_body(m, i, true)?;
            m.st.skip_next = skip;
            Ok(r)
        }
        Some((c, 2)) => {                   // ... for the data operations of the next one
            let skip = get(m, c)? == 0;
            let r = exec_body(m, i, true)?;
            m.st.skip_data_next = skip;
            Ok(r)
        }
        Some((c, _)) => { let data = get(m, c)? != 0; exec_body(m, i, data) }
        None => exec_body(m, i, true),
    }
}

/// The next instruction after a false 'if (cond) execute(D_Unit)': only its address phase
/// (the mar() parts) runs (SPRU375G 5-163).
pub fn exec_address_phase(m: &mut Core, i: &Insn) -> R<()> {
    if i.ptr_mod {
        return emu(format!("0x{:06x}: skipped by execute(D_Unit) with a pointer modifying operand: \
                            its address phase is not modeled", i.addr));
    }
    if i.flow.is_some() || i.repeat.is_some() || i.cond.is_some() {
        return emu(format!("0x{:06x}: flow, repeat or execute controlled by execute(D_Unit)", i.addr));
    }
    for &k in &i.address_parts {
        run_parts(m, std::slice::from_ref(&i.parts[k]))?;
    }
    Ok(())
}

fn exec_body(m: &mut Core, i: &Insn, data: bool) -> R<Option<u32>> {
    let take = match &i.flow {
        Some(Flow::Goto(c, _)) | Some(Flow::Call(c, _, _)) | Some(Flow::Return(c)) => truth(m, c)?,
        _ => true,
    };
    if data { run_parts(m, &i.parts)? }
    if let Some(count) = i.repeat {
        let body = m.fetch(i.nxt)?;
        let n = count.unwrap_or(m.st.csr);
        for _ in 0..=n {
            m.exec_compiled(&body)?;
        }
        m.st.cycles += n as u64;
        let after = i.nxt + body.size;
        return Ok(Some(if m.st.loop_active[0] != 0 { loops_after(m, body.addr, after) } else { after }));
    }
    if !take { return Ok(None); }
    Ok(match &i.flow {
        None => None,
        Some(Flow::Goto(_, t)) => { let d = target(m, t); Some(loops_branch(m, d)) }
        Some(Flow::Call(_, t, ret)) => { let d = target(m, t); Some(m.call(d, *ret)?) }
        Some(Flow::Return(_)) => Some(m.ret()?),
        Some(Flow::ReturnInt) => Some(m.return_int()?),
        Some(Flow::Idle) => Some(m.idle_insn(i.addr, i.nxt)?),
    })
}

fn target(m: &Core, t: &Target) -> u32 {
    match t { Target::Abs(a) => *a, Target::Acc(r) => (m.st.get(*r) & 0xFFFFFF) as u32 }
}
