//! Effective addresses (port of addressing.py): absolute, port, SP/DP relative and the
//! AR/CDP indirect operands with linear or circular modification and the ARMS dependent
//! short operands.

use crate::core::Core;
use crate::err::{emu, unsupported, R};
use crate::state::{Reg, BK03, BK47, BKC, BSA01};

const ST2_ARMS: u32 = 1 << 15;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Ind, Idx(i32), IdxReg(Reg), Post(i32), PostReg(i32, Reg), PostK(i32), Pre(i32), Short(i32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ea {
    Abs(u32),
    Port(u32),
    Abs16(u32),
    Sp(i32),
    Ptr { n: u8, mode: Mode, words: i32 }, // n: AR number, 8 = CDP
}

fn signed16(v: i128) -> i32 {
    let v = (v & 0xFFFF) as i32;
    if v & 0x8000 != 0 { v - 0x10000 } else { v }
}

impl Ea {
    pub fn eval(&self, m: &mut Core) -> R<u32> {
        let s = &mut m.st;
        Ok(match *self {
            Ea::Abs(a) => a & 0x7FFFFF,
            Ea::Port(a) => a & 0xFFFF,
            Ea::Abs16(k) => (s.xdp & 0x7F0000) | (k & 0xFFFF),
            Ea::Sp(k) => (s.xsp & 0x7F0000) | ((s.xsp as i64 + k as i64) as u32 & 0xFFFF),
            Ea::Ptr { n, mode, words } => return ptr(m, n, mode, words),
        })
    }
}

fn buffer(n: u8) -> (usize, usize) {
    match n {
        0 | 1 => (BK03, BSA01), 2 | 3 => (BK03, BSA01 + 1), 4 | 5 => (BK47, BSA01 + 2),
        6 | 7 => (BK47, BSA01 + 3), _ => (BKC, BSA01 + 4),
    }
}

/// ARn / CDP (n = 8) index changed by delta: linear, or circular in the BK buffer when
/// the ARnLC / CDPLC bit of ST2 is set. Used by the indirect operands and by
/// mar(ARn +/- x) (address register arithmetic in the DAGEN, SPRU375G).
pub fn circ_step(m: &Core, n: u8, index: i64, delta: i64) -> R<i64> {
    let bit = if n == 8 { 8 } else { n as u32 };
    if m.st.st2 & (1 << bit) == 0 {
        return Ok((index + delta) & 0xFFFF);
    }
    let size = m.st.plain[buffer(n).0] as i64;
    if size == 0 {
        let name = if n == 8 { "CDP".to_string() } else { format!("AR{}", n) };
        return emu(format!("circular {} with buffer size 0 at pc 0x{:06x}", name, m.st.pc));
    }
    let mut v = index + delta;
    if v >= size { v -= size } else if v < 0 { v += size }
    if !(0..size).contains(&v) {
        let name = if n == 8 { "CDP".to_string() } else { format!("AR{}", n) };
        return emu(format!("circular {}: index {} + step {} leaves the buffer of {} at pc 0x{:06x}",
                           name, index, delta, size, m.st.pc));
    }
    Ok(v & 0xFFFF)
}

/// Indirect operand on ARn / CDP (n = 8): linear, or circular in its BK buffer.
fn ptr(m: &mut Core, n: u8, mode: Mode, words: i32) -> R<u32> {
    let bit = if n == 8 { 8 } else { n as u32 };
    let circular = m.st.st2 & (1 << bit) != 0;
    let reg = if n == 8 { m.st.xcdp } else { m.st.xar[n as usize] };
    let (low, high) = ((reg & 0xFFFF) as i64, reg & 0x7F0000);
    let base = if circular { m.st.plain[buffer(n).1] as i64 } else { 0 };
    let address = |index: i64| -> u32 { high | ((base + index) as u32 & 0xFFFF) };
    let regd = |m: &Core, r: Reg| -> i64 { signed16(m.st.get(r)) as i64 };
    // (delta, write back the new index, address the new index)
    let (d, update, after) = match mode {
        Mode::Ind => (0, false, true),
        Mode::Idx(k) => (k as i64, false, true),
        Mode::IdxReg(r) => (regd(m, r), false, true),
        Mode::Post(k) => ((k * words) as i64, true, false),
        Mode::PostK(k) => (k as i64, true, false),
        Mode::PostReg(sign, r) => (sign as i64 * regd(m, r), true, false),
        Mode::Pre(k) => (k as i64, true, true),
        Mode::Short(k) if m.st.st2 & ST2_ARMS != 0 => (k as i64, false, true),
        Mode::Short(k) => match k {
            1 => (regd(m, Reg::T(1)), true, false),
            2 => (-regd(m, Reg::T(1)), true, false),
            3 => (regd(m, Reg::T(1)), false, true),
            4 => (words as i64, true, true),
            5 => (-(words as i64), true, true),
            _ => return emu(format!("bit-reversed operand (ARMS = 0, short #{}) at pc 0x{:06x}", k, m.st.pc)),
        },
    };
    let i = if !circular { (low + d) & 0xFFFF } else { circ_step(m, n, low, d)? };
    if update {
        let x = if n == 8 { &mut m.st.xcdp } else { &mut m.st.xar[n as usize] };
        *x = (*x & 0x7F0000) | (i as u32 & 0xFFFF);
    }
    Ok(address(if after { i } else { low }))
}

pub fn unsupported_mode<T>(mode: &str) -> R<T> {
    unsupported(format!("addressing mode {}", mode))
}
