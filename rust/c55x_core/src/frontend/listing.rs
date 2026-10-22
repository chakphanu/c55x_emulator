//! Decoded instructions from dis55 text, after c55x_emu/listing.py: {address: Insn}.
//!
//! Accepts dis55 output ('addr: hex  text', parallel parts separated by '_' in the hex
//! column) and the project listing ('addr  hex  text'). The last instruction of a
//! localrepeat/blockrepeat body comes from the encoding (block_end) and is checked
//! against the '}' markers of dis55 when present.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;

use crate::err::{emu, R};

static LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([0-9a-f]{6,8}):?\s+([0-9a-f_]+)\s+(\S.*)$").unwrap());
static SECTION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(TEXT|DATA) section ").unwrap());

#[derive(Clone, Debug, PartialEq)]
pub struct Insn {
    pub addr: u32,
    pub size: u32,
    pub text: String,
    /// localrepeat/blockrepeat: address of the last body instruction
    pub block_end: Option<u32>,
}

/// Last body instruction of a repeat block opened by this bundle, else None. The block
/// opener is the first or last part of its bundle; localrepeat '0100101E 1lllllll'
/// (offset l7), blockrepeat '0000111E llllllll llllllll' (offset l16); last body
/// instruction = bundle end + offset.
pub fn block_end(addr: u32, raw: &[u8], text: &str) -> R<Option<u32>> {
    let parts: Vec<&str> = text.split("||").map(str::trim).collect();
    let Some(opener) = parts.iter().position(|p| *p == "localrepeat {" || *p == "blockrepeat {") else {
        return Ok(None);
    };
    let local = parts[opener] == "localrepeat {";
    let n = if local { 2 } else { 3 };
    let b = if opener == 0 { &raw[..n.min(raw.len())] } else { &raw[raw.len().saturating_sub(n)..] };
    let end = addr + raw.len() as u32;
    if local && b.len() >= 2 && b[0] & 0xFE == 0x4A && b[1] & 0x80 != 0 {
        return Ok(Some(end + (b[1] & 0x7F) as u32));
    }
    if !local && b.len() >= 3 && b[0] & 0xFE == 0x0E {
        return Ok(Some(end + u16::from_be_bytes([b[1], b[2]]) as u32));
    }
    let hex: String = raw.iter().map(|x| format!("{:02x}", x)).collect();
    emu(format!("0x{:06x}: cannot locate the repeat opcode in {} '{}'", addr, hex, text))
}

fn from_hex(text: &str) -> R<Vec<u8>> {
    let digits: Vec<u8> = text.bytes().filter(|&c| c != b'_').collect();
    if digits.len() % 2 != 0 {
        return emu(format!("odd number of hex digits in '{}'", text));
    }
    Ok(digits.chunks(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect())
}

/// dis55 (TEXT sections) or project listing text -> {address: Insn}. markers=false
/// ignores dis55 '}' lines (a window cut may separate a block from its end).
pub fn parse(text: &str, markers: bool) -> R<BTreeMap<u32, Insn>> {
    let mut insns: BTreeMap<u32, Insn> = BTreeMap::new();
    let (mut opened, mut last, mut in_text, mut dis_format) = (Vec::new(), None::<u32>, true, None);
    for line in text.lines() {
        if let Some(m) = SECTION.captures(line) {
            in_text = &m[1] == "TEXT";
            continue;
        }
        if !in_text {
            continue;
        }
        let s = line.trim();
        if s == "}" {
            if dis_format == Some(false) || !markers {
                continue;                   // project listing: ends come from the encoding
            }
            let Some(start) = opened.pop() else {
                return emu(format!("unbalanced '}}' after 0x{:06x}", last.unwrap_or(0)));
            };
            let encoded = insns[&start].block_end;
            if encoded != last {
                return emu(format!("0x{:06x}: encoded block end 0x{:06x} differs from dis55 0x{:06x}",
                                   start, encoded.unwrap_or(0), last.unwrap_or(0)));
            }
            continue;
        }
        let Some(m) = LINE.captures(s) else { continue };
        let body = &m[3];
        if body.ends_with(':') || body.starts_with(".byte") || body.starts_with(".word") {
            continue;
        }
        let addr = u32::from_str_radix(&m[1], 16).unwrap();
        let raw = from_hex(&m[2])?;
        let text = body.trim().to_owned();
        let block_end = block_end(addr, &raw, &text)?;
        insns.insert(addr, Insn { addr, size: raw.len() as u32, text, block_end });
        last = Some(addr);
        let dis = s.split_whitespace().next().unwrap().ends_with(':');
        dis_format = Some(dis);
        if block_end.is_some() && dis && markers {
            opened.push(addr);              // dis55 format: a '}' marker will follow
        }
    }
    if let Some(start) = opened.last() {
        return emu(format!("unterminated repeat block at 0x{:06x}", start));
    }
    Ok(insns)
}
