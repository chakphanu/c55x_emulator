//! Assignments compiled once: source closure (fast.rs), destination closure and the
//! static SATD / CARRY / TCx decisions. The most common instruction shape, one
//! assignment without condition, flow or repeat, runs as a single closure without the
//! generic part machinery; the order is that of parts.rs read / commit.

use std::rc::Rc;

use crate::core::Core;
use crate::dest::set;
use crate::dsp::{d_unit_result, saturate_acc};
use crate::err::R;
use crate::expr::{carry_of, is_acc, Expr, M32};
use crate::fast::{arith_fx, Fx};
use crate::state::{Reg, AC_MASK};

pub type Run = Box<dyn Fn(&mut Core) -> R<()>>;
type Store = Box<dyn Fn(&mut Core, i128) -> R<()>>;

/// dest::set
fn store_fx(d: Expr) -> Store {
    match d {
        Expr::Reg(Reg::Ac(i), _) => Box::new(move |m, v| { m.st.ac[i as usize] = (v & AC_MASK as i128) as u64; Ok(()) }),
        Expr::Reg(r, _) => Box::new(move |m, v| { m.st.set(r, v); Ok(()) }),
        Expr::Mem { ea, words, port, .. } => match (port, words) {
            (true, _) => Box::new(move |m, v| { let a = ea.eval(m)?; m.port_write(a, v) }),
            (false, 2) => Box::new(move |m, v| { let a = ea.eval(m)?; m.write32(a, v & M32) }),
            _ => Box::new(move |m, v| { let a = ea.eval(m)?; m.write(a, v) }),
        },
        d => { let d = Rc::new(d); Box::new(move |m, v| set(m, &d, v)) }
    }
}

/// dst = src: carry when an accumulator gets a non-product add / subtract, test when
/// the destination is TC1 / TC2 (value != 0).
pub struct Assign {
    src: Rc<Expr>,
    fx: Fx,
    store: Store,
    sat: bool,                  // SATD applies: accumulator destination, D-unit result
    carry: bool,
    test: bool,
}

impl Assign {
    pub fn new(dst: Expr, src: Expr) -> Assign {
        let carry = is_acc(&dst) && matches!(src, Expr::AddSub { product: false, .. });
        let test = matches!(dst, Expr::Reg(Reg::Tc1, _) | Expr::Reg(Reg::Tc2, _));
        let sat = is_acc(&dst) && d_unit_result(&src);
        let src = Rc::new(src);
        Assign { fx: arith_fx(&src, false, true, false), src, store: store_fx(dst), sat, carry, test }
    }

    /// Read phase: (value, CARRY result if the assignment sets it).
    #[inline]
    pub fn read(&self, m: &mut Core) -> R<(i128, Option<u8>)> {
        let v = (self.fx)(m)?;
        let v = if self.test { (v != 0) as i128 } else { v };
        Ok((v, if self.carry { carry_of(m, &self.src)?.flatten() } else { None }))
    }

    /// Commit phase.
    #[inline]
    pub fn commit(&self, m: &mut Core, v: i128, carry: Option<u8>) -> R<()> {
        (self.store)(m, if self.sat { saturate_acc(m, v) } else { v })?;
        if let Some(c) = carry { m.st.carry = c as u32 }
        Ok(())
    }

    /// The assignment as a whole instruction.
    pub fn run(self) -> Run {
        Box::new(move |m| {
            let (v, c) = self.read(m)?;
            self.commit(m, v, c)
        })
    }
}

/// Up to four assignments run as one instruction, phase by phase as parts::run_parts:
/// the address-phase (first) ones read and commit before the others.
pub fn run_all(mut list: Vec<(bool, Assign)>) -> Run {
    assert!(!list.is_empty() && list.len() <= 4);
    if list.len() == 1 {
        if let Some((_, a)) = list.pop() {
            return a.run();
        }
    }
    Box::new(move |m| {
        for phase_first in [true, false] {
            let (mut vs, mut cs) = ([0i128; 4], [None; 4]);
            for (k, (first, a)) in list.iter().enumerate() {
                if *first == phase_first { (vs[k], cs[k]) = a.read(m)?; }
            }
            for (k, (first, a)) in list.iter().enumerate() {
                if *first == phase_first { a.commit(m, vs[k], cs[k])?; }
            }
        }
        Ok(())
    })
}
