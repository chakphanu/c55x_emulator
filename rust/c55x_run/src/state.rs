//! CPU state fields of a call script (initial_state, call regs) on the core's State: the
//! fields the Python host loads from its Cpu at every entry (rust/c55x_py sync.rs), plus
//! the core-only ACOV flags, with the same value rules.

use c55x_core::state::{State, PLAIN};
use serde_json::Value;

/// Every field initial_state must give.
pub const FIELDS: [&str; 37] = [
    "ac", "t", "xar", "xcdp", "xdp", "xsp", "ssp", "tc1", "tc2", "carry", "st1", "st2", "st3",
    "ier0", "ier1", "ifr0", "ifr1", "ivpd", "ivph", "dbstat", "brc0", "brc1", "brs1", "csr",
    "rsa0", "rsa1", "rea0", "rea1", "loop_active", "reta", "cfct", "pc", "cycles", "skip_next",
    "skip_data_next", "in_idle", "acov",
];

/// A JSON integer or a "0x..." / decimal string.
pub fn num(v: &Value) -> Result<i128, String> {
    match v {
        Value::Number(n) => n.as_i64().map(|x| x as i128).or_else(|| n.as_u64().map(|x| x as i128))
            .ok_or_else(|| format!("not an integer: {}", n)),
        Value::String(s) => {
            let t = s.trim();
            let r = match t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
                Some(h) => i128::from_str_radix(h, 16),
                None => t.parse::<i128>(),
            };
            r.map_err(|_| format!("not an integer: {:?}", s))
        }
        other => Err(format!("not an integer: {}", other)),
    }
}

fn u32_of(name: &str, v: &Value) -> Result<u32, String> {
    let x = num(v)?;
    u32::try_from(x).map_err(|_| format!("{}: {} is not a 32-bit unsigned value", name, x))
}

fn u64_of(name: &str, v: &Value) -> Result<u64, String> {
    let x = num(v)?;
    u64::try_from(x).map_err(|_| format!("{}: {} is not a 64-bit unsigned value", name, x))
}

fn bool_of(name: &str, v: &Value) -> Result<bool, String> {
    v.as_bool().ok_or_else(|| format!("{}: {} is not a bool", name, v))
}

fn list<'a>(name: &str, v: &'a Value, len: usize) -> Result<&'a Vec<Value>, String> {
    match v.as_array() {
        Some(a) if a.len() == len => Ok(a),
        _ => Err(format!("{}: expected a list of {}", name, len)),
    }
}

/// Set one field: a whole field of initial_state ("ac": [..]) or one call register
/// ("ac0", "t1", "xar3") or a scalar ("xsp", "st1", a PLAIN register).
pub fn set(st: &mut State, name: &str, v: &Value) -> Result<(), String> {
    for (prefix, len) in [("xar", 8usize), ("ac", 4), ("t", 4), ("acov", 4)] {
        if let Some(idx) = name.strip_prefix(prefix) {
            if !idx.is_empty() && idx.bytes().all(|b| b.is_ascii_digit()) {
                let i: usize = idx.parse().map_err(|_| format!("bad register {}", name))?;
                if i >= len { return Err(format!("bad register {}", name)); }
                match prefix {
                    "xar" => st.xar[i] = u32_of(name, v)?,
                    "ac" => st.ac[i] = u64_of(name, v)?,
                    "t" => st.t[i] = u32_of(name, v)?,
                    _ => st.acov[i] = u32_of(name, v)?,
                }
                return Ok(());
            }
        }
    }
    match name {
        "ac" => for (i, x) in list(name, v, 4)?.iter().enumerate() { st.ac[i] = u64_of(name, x)? },
        "t" => for (i, x) in list(name, v, 4)?.iter().enumerate() { st.t[i] = u32_of(name, x)? },
        "xar" => for (i, x) in list(name, v, 8)?.iter().enumerate() { st.xar[i] = u32_of(name, x)? },
        "acov" => for (i, x) in list(name, v, 4)?.iter().enumerate() { st.acov[i] = u32_of(name, x)? },
        "loop_active" => for (i, x) in list(name, v, 2)?.iter().enumerate() {
            st.loop_active[i] = match x {
                Value::Null => 0,
                Value::String(s) if s == "local" => 1,
                Value::String(_) => 2,
                other => return Err(format!("loop_active: {} is not null or a string", other)),
            }
        },
        "cycles" => st.cycles = u64_of(name, v)?,
        "skip_next" => st.skip_next = bool_of(name, v)?,
        "skip_data_next" => st.skip_data_next = bool_of(name, v)?,
        "in_idle" => st.in_idle = bool_of(name, v)?,
        _ => {
            let x = u32_of(name, v)?;
            let slot = match name {
                "xcdp" => &mut st.xcdp, "xdp" => &mut st.xdp, "xsp" => &mut st.xsp, "ssp" => &mut st.ssp,
                "tc1" => &mut st.tc1, "tc2" => &mut st.tc2, "carry" => &mut st.carry,
                "st1" => &mut st.st1, "st2" => &mut st.st2, "st3" => &mut st.st3,
                "ier0" => &mut st.ier0, "ier1" => &mut st.ier1, "ifr0" => &mut st.ifr0, "ifr1" => &mut st.ifr1,
                "ivpd" => &mut st.ivpd, "ivph" => &mut st.ivph, "dbstat" => &mut st.dbstat,
                "brc0" => &mut st.brc0, "brc1" => &mut st.brc1, "brs1" => &mut st.brs1, "csr" => &mut st.csr,
                "rsa0" => &mut st.rsa[0], "rsa1" => &mut st.rsa[1], "rea0" => &mut st.rea[0], "rea1" => &mut st.rea[1],
                "reta" => &mut st.reta, "cfct" => &mut st.cfct, "pc" => &mut st.pc,
                _ => match PLAIN.iter().position(|p| *p == name) {
                    Some(i) => &mut st.plain[i],
                    None => return Err(format!("unknown state field {}", name)),
                },
            };
            *slot = x;
        }
    }
    Ok(())
}

/// Apply a whole initial_state; every field (FIELDS and the PLAIN registers) must be there.
pub fn initial(st: &mut State, state: &serde_json::Map<String, Value>) -> Result<(), String> {
    let missing: Vec<&str> = FIELDS.iter().copied().chain(PLAIN.iter().copied())
        .filter(|f| !state.contains_key(*f)).collect();
    if !missing.is_empty() {
        return Err(format!("initial_state lacks {}", missing.join(", ")));
    }
    for (name, v) in state {
        set(st, name, v)?;
    }
    Ok(())
}
