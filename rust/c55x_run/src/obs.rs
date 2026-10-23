//! Opt-in observers of a call (c55x_core::observe): the "write_events", "pc_states" and
//! "entry_snapshots" keys of a call's "observe" object, and their output in run.json. A
//! call without these keys runs exactly as before and its output has no new fields.

use std::collections::HashSet;

use c55x_core::core::{Core, MEM_BYTES, MMR_END};
use c55x_core::observe::{Entries, ExecMode, Observer, Origin, PcStates, Writes, MAX_EVENTS, MAX_PCS,
                         MAX_SNAPSHOT_BYTES};
use serde_json::{json, Map, Value};

use crate::state::{self, num};

/// Most words all windows of one entry snapshot request may name.
pub const MAX_WINDOW_WORDS: u64 = 1 << 20;

fn cap(o: &Value, key: &str, default: u64, max: u64) -> Result<usize, String> {
    let n = match o.get(key) {
        None => default,
        Some(v) => u64::try_from(num(v)?).map_err(|_| format!("{} is negative", key))?,
    };
    if n == 0 || n > max {
        return Err(format!("{} must be 1..{}", key, max));
    }
    Ok(n as usize)
}

fn pcs(o: &Value, what: &str) -> Result<HashSet<u32>, String> {
    let list = o.get("pcs").and_then(|v| v.as_array()).ok_or_else(|| format!("{}: \"pcs\" list required", what))?;
    if list.is_empty() || list.len() > MAX_PCS {
        return Err(format!("{}: 1..{} pcs", what, MAX_PCS));
    }
    list.iter().map(|v| {
        let pc = num(v)?;
        u32::try_from(pc).ok().filter(|&p| (p as usize) < MEM_BYTES).ok_or_else(|| format!("{}: pc {} outside memory", what, pc))
    }).collect()
}

fn flag(o: &Value, key: &str) -> Result<bool, String> {
    match o.get(key) {
        None => Ok(false),
        Some(Value::Bool(b)) => Ok(*b),
        Some(_) => Err(format!("{} must be true or false", key)),
    }
}

/// The observer a call's observe object asks for, or None (also for an absent object).
pub fn observer(observe: Option<&Value>, core: &Core) -> Result<Option<Box<Observer>>, String> {
    let Some(o) = observe else { return Ok(None) };
    let writes = match o.get("write_events") {
        None => None,
        Some(w) => Some(Writes::new(cap(w, "max_events", 65536, MAX_EVENTS as u64)?, flag(w, "mmr")?)),
    };
    let pc_states = match o.get("pc_states") {
        None => None,
        Some(p) => Some(PcStates::new(pcs(p, "pc_states")?, cap(p, "max_events", 4096, MAX_EVENTS as u64)?)),
    };
    let entries = match o.get("entry_snapshots") {
        None => None,
        Some(e) => {
            let mut windows = Vec::new();
            let mut total = 0u64;
            for w in e.get("windows").and_then(|v| v.as_array()).unwrap_or(&Vec::new()) {
                let pair = w.as_array().filter(|p| p.len() == 2).ok_or("entry_snapshots: a window is [word, words]")?;
                let (at, n) = (num(&pair[0])?, num(&pair[1])?);
                if at < MMR_END as i128 || n < 1 || at + n > (MEM_BYTES / 2) as i128 {
                    return Err(format!("entry_snapshots: window [{}, {}] outside data words 0x60..", at, n));
                }
                total += n as u64;
                windows.push((at as u32, n as u32));
            }
            if total > MAX_WINDOW_WORDS {
                return Err(format!("entry_snapshots: windows name more than {} words", MAX_WINDOW_WORDS));
            }
            let baseline = if flag(e, "patch_since_call")? { Some(core.memory().to_vec()) } else { None };
            Some(Entries::new(pcs(e, "entry_snapshots")?, windows, baseline, cap(e, "max_hits", 16, MAX_EVENTS as u64)?,
                              cap(e, "max_bytes", 16 << 20, MAX_SNAPSHOT_BYTES as u64)?))
        }
    };
    if writes.is_none() && pc_states.is_none() && entries.is_none() {
        return Ok(None);
    }
    Ok(Some(Box::new(Observer::new(writes, pc_states, entries))))
}

fn origin(o: Origin) -> &'static str {
    match o {
        Origin::Insn => "instruction",
        Origin::CallEntry => "call_entry",
        Origin::Interrupt => "interrupt",
        Origin::Peripheral => "peripheral",
    }
}

fn mode(m: ExecMode) -> &'static str {
    match m {
        ExecMode::Execute => "execute",
        ExecMode::Skipped => "skipped",
        ExecMode::AddressPhase => "address_phase",
    }
}

/// The observations of one call into observed (ordinals relative to native0, the core's
/// instruction count when the call started).
pub fn report(obs: Observer, native0: u64, observed: &mut Map<String, Value>) {
    let rel = |n: u64| n - native0;
    if let Some(w) = obs.writes {
        let b = w.events;
        let events: Vec<Value> = b.items.iter().map(|e| json!([e.seq, rel(e.ordinal), e.pc, e.iteration,
            origin(e.origin), e.word, e.value, e.mmr, e.committed])).collect();
        observed.insert("write_events".into(), json!({
            "complete": b.dropped == 0, "cap": b.cap, "recorded": b.items.len(), "dropped": b.dropped,
            "mmr_included": w.mmr,
            "fields": ["seq", "ordinal", "pc", "iteration", "origin", "word", "value", "mmr", "committed"],
            "events": events,
        }));
    }
    if let Some(p) = obs.pc_states {
        let b = p.events;
        let mut pcs: Vec<u32> = p.pcs.into_iter().collect();
        pcs.sort_unstable();
        let events: Vec<Value> = b.items.iter().map(|e| json!({
            "seq": e.seq, "ordinal": rel(e.ordinal), "pc": e.pc, "iteration": e.iteration, "mode": mode(e.mode),
            "condition": e.condition, "state": Value::Object(state::dump(&e.state)),
        })).collect();
        observed.insert("pc_states".into(), json!({
            "complete": b.dropped == 0, "cap": b.cap, "recorded": b.items.len(), "dropped": b.dropped,
            "pcs": pcs, "events": events,
        }));
    }
    if let Some(e) = obs.entries {
        let b = e.snapshots;
        let mut pcs: Vec<u32> = e.pcs.into_iter().collect();
        pcs.sort_unstable();
        let snapshots: Vec<Value> = b.items.iter().map(|s| json!({
            "seq": s.seq, "ordinal": rel(s.ordinal), "pc": s.pc, "state": Value::Object(state::dump(&s.state)),
            "windows": s.windows.iter().map(|(a, v)| json!({"word": a, "values": v})).collect::<Vec<_>>(),
            "patch_since_call": s.patch.as_ref().map(|p| p.iter().map(|(w, a, b)| json!([w, a, b])).collect::<Vec<_>>()),
        })).collect();
        observed.insert("entry_snapshots".into(), json!({
            "complete": b.dropped == 0 && e.omitted_bytes == 0 && e.pending.is_none(),
            "max_hits": b.cap, "max_bytes": e.max_bytes, "bytes": e.bytes, "recorded": b.items.len(),
            "omitted_hits": b.dropped, "omitted_for_bytes": e.omitted_bytes,
            "pending_entry": e.pending, "pcs": pcs, "snapshots": snapshots,
        }));
    }
}
