//! The opt-in observers of c55x-run (c55x_core::observe): ordered memory write events,
//! the state before filtered pcs and routine-entry snapshots. The routines come from the
//! fixture rust/fixtures/observe. Observation must never change a result: every test also
//! compares instructions, cycles, final state and memory with the same call unobserved.
//! These tests start c55x-run, so they run in an emulator job (scripts/cargo-test-contained).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const SENTINEL: u32 = 0xFFFF00;
const MEM_BYTES: usize = 1 << 24;
const PLAIN: [&str; 16] = ["trn0", "trn1", "bk03", "bk47", "bkc", "bsa01", "bsa23", "bsa45", "bsa67", "bsac",
                           "pdp", "dbier0", "dbier1", "bios37", "xpc", "rptc"];

fn sha(data: &[u8]) -> String {
    Sha256::digest(data).iter().map(|b| format!("{:02x}", b)).collect()
}

/// The fixture: (listing as c55x listing/1 TSV, .text address, .text bytes, symbols).
fn fixture() -> (String, u32, Vec<u8>, Vec<(String, u32)>) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures/observe");
    let read = |f: &str| fs::read_to_string(dir.join(f)).unwrap();
    let insns = c55x_core::frontend::listing::parse(&read("prog.lst"), true).ok().unwrap();
    let mut tsv = String::from("# c55x listing/1 (synthetic, rust/fixtures/observe)\n");
    for i in insns.values() {
        let end = i.block_end.map_or("-".to_string(), |e| format!("{:06x}", e));
        tsv += &format!("{:06x}\t{}\t{}\t{}\n", i.addr, i.size, end, i.text);
    }
    let (mut base, mut code) = (0, Vec::new());
    for line in read("prog.hex").lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() == 3 {
            base = u32::from_str_radix(f[1].trim_start_matches("0x"), 16).unwrap();
        } else {
            code.extend((0..line.len()).step_by(2).map(|i| u8::from_str_radix(&line[i..i + 2], 16).unwrap()));
        }
    }
    let symbols = read("prog.sym").lines().map(|l| {
        let (n, v) = l.split_once(' ').unwrap();
        (n.to_owned(), u32::from_str_radix(v.trim_start_matches("0x"), 16).unwrap())
    }).collect();
    (tsv, base, code, symbols)
}

fn sym(name: &str) -> u32 {
    fixture().3.into_iter().find(|(n, _)| n == name).unwrap().1
}

fn initial_state() -> Value {
    let mut s = serde_json::Map::new();
    for name in ["xcdp", "xdp", "tc1", "tc2", "carry", "st3", "ier0", "ier1", "ifr0", "ifr1", "dbstat", "brc0",
                 "brc1", "brs1", "csr", "rsa0", "rsa1", "rea0", "rea1", "reta", "cfct", "pc", "cycles"]
        .into_iter().chain(PLAIN) {
        s.insert(name.into(), json!(0));
    }
    let more = json!({"ac": [0, 0, 0, 0], "t": [0, 0, 0, 0], "xar": [0, 0, 0, 0, 0, 0, 0, 0], "acov": [0, 0, 0, 0],
                      "loop_active": [null, null], "xsp": 0x1000, "ssp": 0x0800, "st1": 0x4900, "st2": 0x8000,
                      "ivpd": 0xFFFF, "ivph": 0xFFFF, "skip_next": false, "skip_data_next": false, "in_idle": false});
    s.extend(more.as_object().unwrap().clone());
    Value::Object(s)
}

/// One call of entry with regs, observed with observe (plus final_state and a memory diff).
fn script(entry: &str, regs: Value, observe: Value) -> Value {
    let mut o = observe;
    o["final_state"] = json!(true);
    o["memory_diff_since"] = json!("start");
    json!({
        "schema": "c55x-call-script/2", "image": "image.json", "listing": "code.listing.tsv",
        "return_sentinel": format!("0x{:x}", SENTINEL),
        "limits": {"max_steps_per_call": 1000, "chunk_steps": 16, "max_seconds_per_call": 10},
        "on_unmodeled_port": "error", "on_code_page_write": "error", "register_carry_over": true,
        "initial_state": initial_state(),
        "setup": [{"op": "mark_memory", "name": "start"},
                  {"op": "call", "label": entry, "entry": format!("0x{:x}", sym(entry)), "regs": regs, "observe": o}],
        "output": {"format": "s16le", "scaling": "none"},
    })
}

/// A fresh directory under TMPDIR (the job work dir).
fn input_dir() -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let d = std::env::temp_dir().join(format!("c55x-observe-test-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
    fs::create_dir_all(&d).unwrap();
    d
}

fn write_inputs(d: &Path, script: &Value) {
    let (tsv, base, code, _) = fixture();
    let mut mem = vec![0u8; MEM_BYTES];
    mem[base as usize..base as usize + code.len()].copy_from_slice(&code);
    fs::write(d.join("code.bin"), &code).unwrap();
    fs::write(d.join("image.json"), json!({
        "format": "c55x-memory-image/1", "size_bytes": MEM_BYTES, "snapshot_sha256": sha(&mem),
        "segments": [{"file": "code.bin", "byte_address": base, "bytes": code.len(), "sha256": sha(&code)}],
    }).to_string()).unwrap();
    fs::write(d.join("code.listing.tsv"), tsv).unwrap();
    fs::write(d.join("test.call.json"), script.to_string()).unwrap();
}

/// Run a script: (exit status, run.json).
fn run(script: &Value) -> (i32, Value) {
    let d = input_dir();
    write_inputs(&d, script);
    let out = d.join("out");
    let p = Command::new(env!("CARGO_BIN_EXE_c55x-run")).arg("--script").arg(d.join("test.call.json"))
        .arg("--out").arg(&out).output().unwrap();
    let run = fs::read(out.join("run.json")).map(|b| serde_json::from_slice(&b).unwrap())
        .unwrap_or_else(|_| panic!("no run.json: {}", String::from_utf8_lossy(&p.stderr)));
    (p.status.code().unwrap_or(-1), run)
}

const NEW_KEYS: [&str; 3] = ["write_events", "pc_states", "entry_snapshots"];

/// Run entry observed and unobserved; the numeric results must agree. -> the observed call.
fn observed(entry: &str, regs: Value, observe: Value) -> Value {
    let (rc_off, off) = run(&script(entry, regs.clone(), json!({})));
    let (rc_on, on) = run(&script(entry, regs, observe));
    assert_eq!(rc_on, rc_off);
    let (a, b) = (&off["calls"][0], &on["calls"][0]);
    for k in NEW_KEYS {
        assert!(a.get(k).is_none(), "unobserved call has {}", k);
    }
    for k in ["instructions", "cycles", "status", "final_state", "memory_diff"] {
        assert_eq!(a[k], b[k], "{} differs with observers on", k);
    }
    assert_eq!(off["totals"], on["totals"]);
    b.clone()
}

/// Write events as (pc, origin, word, value) with the given iteration and committed flags.
fn writes(call: &Value) -> Vec<(Value, String, u64, u64)> {
    let w = &call["write_events"];
    assert_eq!(w["fields"], json!(["seq", "ordinal", "pc", "iteration", "origin", "word", "value", "mmr", "committed"]));
    w["events"].as_array().unwrap().iter().map(|e| {
        (e[2].clone(), e[4].as_str().unwrap().to_string(), e[5].as_u64().unwrap(), e[6].as_u64().unwrap())
    }).collect()
}

fn store_regs() -> Value {
    json!({"xar0": "0x1600", "xar1": "0x1700", "xar2": "0x1800", "xar3": "0x1900", "t0": 7, "t1": 9,
           "ac0": "0x12345678", "reta": "0x00abcd", "cfct": 0})
}

#[test]
fn write_events_keep_order_duplicates_and_the_call_prologue() {
    let call = observed("stores", store_regs(), json!({"write_events": {}}));
    let w = &call["write_events"];
    assert_eq!((w["complete"].as_bool(), w["dropped"].as_u64(), w["mmr_included"].as_bool()), (Some(true), Some(0), Some(false)));
    let pc = |a: u32| json!(sym("stores") + a);
    assert_eq!(writes(&call), vec![
        // the call prologue: RETA(15-0) on the data stack, CFCT:RETA(23-16) on the system stack
        (Value::Null, "call_entry".into(), 0x0FFF, 0xABCD),
        (Value::Null, "call_entry".into(), 0x07FF, 0x0000),
        (pc(0), "instruction".into(), 0x1600, 7),
        (pc(2), "instruction".into(), 0x1600, 7),      // same value again: still an event
        (pc(4), "instruction".into(), 0x1700, 9),
        (pc(6), "instruction".into(), 0x1800, 0x1234), // dbl(): MSW at the even word first
        (pc(6), "instruction".into(), 0x1801, 0x5678),
        (pc(9), "instruction".into(), 0x1900, 7),
    ]);
    let ev = w["events"].as_array().unwrap();
    let seq: Vec<u64> = ev.iter().map(|e| e[0].as_u64().unwrap()).collect();
    assert_eq!(seq, (0..8).collect::<Vec<_>>());
    let ordinal: Vec<u64> = ev.iter().map(|e| e[1].as_u64().unwrap()).collect();
    assert_eq!(ordinal, [0, 0, 1, 2, 3, 4, 4, 5]);
    assert!(ev.iter().all(|e| e[3].is_null() && e[7] == json!(false) && e[8] == json!(true)));
    // the net difference names 0x1600 once; the events name both stores
    let diff_words: Vec<u64> = call["memory_diff"].as_array().unwrap().iter().map(|d| d[0].as_u64().unwrap()).collect();
    assert_eq!(diff_words.iter().filter(|&&x| x == 0x1600).count(), 1);
}

#[test]
fn mmr_writes_are_recorded_only_on_request() {
    let mut regs = store_regs();
    regs["xar1"] = json!("0x0021");                   // T1 as an MMR: writes T1 = T1
    let off = observed("stores", regs.clone(), json!({"write_events": {}}));
    assert!(writes(&off).iter().all(|(_, _, word, _)| *word >= 0x60));
    let on = observed("stores", regs, json!({"write_events": {"mmr": true}}));
    let mmr: Vec<&Value> = on["write_events"]["events"].as_array().unwrap().iter().filter(|e| e[7] == json!(true)).collect();
    assert_eq!(mmr.len(), 1);
    assert_eq!((mmr[0][5].as_u64(), mmr[0][6].as_u64(), &mmr[0][8]), (Some(0x21), Some(9), &json!(true)));
}

#[test]
fn repeated_store_events_carry_the_body_pc_and_iteration() {
    let call = observed("rpt_stores", json!({"xar0": "0x1600", "t0": 3}), json!({"write_events": {}}));
    let ev: Vec<(u64, u64, u64, u64)> = call["write_events"]["events"].as_array().unwrap().iter().skip(2)
        .map(|e| (e[1].as_u64().unwrap(), e[2].as_u64().unwrap(), e[3].as_u64().unwrap(), e[5].as_u64().unwrap())).collect();
    let body = (sym("rpt_stores") + 2) as u64;
    assert_eq!(ev, vec![(1, body, 0, 0x1600), (1, body, 1, 0x1601), (1, body, 2, 0x1602)]);
}

#[test]
fn pc_states_give_the_state_before_the_instruction_and_its_condition() {
    let x = sym("xcc");
    let pcs = json!([x, x + 2, x + 4]);
    for (tc1, tc2) in [(0, 1), (1, 0)] {
        let regs = json!({"xar0": "0x1600", "xar1": "0x1700", "t0": 7, "t1": 9, "tc1": tc1, "tc2": tc2,
                          "acov": [0, 1, 0, 0], "st1": "0x4b00"});
        let call = observed("xcc", regs, json!({"pc_states": {"pcs": pcs}, "write_events": {}}));
        let p = &call["pc_states"];
        assert_eq!((p["complete"].as_bool(), p["recorded"].as_u64()), (Some(true), Some(3)));
        let ev = p["events"].as_array().unwrap();
        let got: Vec<(u64, &str, &Value)> = ev.iter().map(|e| (e["pc"].as_u64().unwrap(), e["mode"].as_str().unwrap(),
                                                             &e["condition"])).collect();
        let next = if tc1 == 1 { "execute" } else { "skipped" };
        assert_eq!(got, vec![(x as u64, "execute", &json!(tc1 == 1)), ((x + 2) as u64, next, &Value::Null),
                             ((x + 4) as u64, "execute", &json!(tc2 == 1))]);
        // incoming state, before any instruction: the call's registers and ACOV
        let s = &ev[0]["state"];
        assert_eq!((&s["acov"], s["st1"].as_u64(), s["tc2"].as_u64(), s["pc"].as_u64()),
                   (&json!([0, 1, 0, 0]), Some(0x4B00), Some(tc2), Some(x as u64)));
        // the XCCPART store: AR1 advances either way, the write happens only when TC2 = 1
        assert_eq!(ev[2]["state"]["xar"][1].as_u64(), Some(0x1700));
        assert_eq!(call["final_state"]["xar"][1].as_u64(), Some(0x1701));
        let words: Vec<u64> = writes(&call).into_iter().skip(2).map(|w| w.2).collect();
        let mut want = Vec::new();
        if tc1 == 1 { want.push(0x1600) }
        if tc2 == 1 { want.push(0x1700) }
        assert_eq!(words, want);
        let ordinals: Vec<u64> = ev.iter().map(|e| e["ordinal"].as_u64().unwrap()).collect();
        assert_eq!(ordinals, if tc1 == 1 { [1, 2, 3] } else { [1, 1, 2] });
    }
}

#[test]
fn entry_snapshots_at_the_public_and_a_nested_call() {
    let regs = json!({"xar0": "0x1600", "xar1": "0x1601", "xar2": "0x1602", "t0": 7, "t1": 9});
    let observe = json!({"entry_snapshots": {"pcs": [sym("outer"), sym("inner")], "windows": [["0x1600", 3]],
                                             "patch_since_call": true}});
    let call = observed("outer", regs, observe);
    let e = &call["entry_snapshots"];
    assert_eq!((e["complete"].as_bool(), e["recorded"].as_u64(), &e["pending_entry"]), (Some(true), Some(2), &Value::Null));
    let s = e["snapshots"].as_array().unwrap();
    // at outer: before its first instruction, after the public call prologue
    assert_eq!((s[0]["pc"].as_u64(), s[0]["ordinal"].as_u64()), (Some(sym("outer") as u64), Some(0)));
    assert_eq!((s[0]["state"]["reta"].as_u64(), s[0]["state"]["xsp"].as_u64(), s[0]["state"]["ssp"].as_u64()),
               (Some(SENTINEL as u64), Some(0x0FFF), Some(0x07FF)));
    assert_eq!(s[0]["windows"], json!([{"word": 0x1600, "values": [0, 0, 0]}]));
    assert_eq!(s[0]["patch_since_call"], json!([]));  // the prologue stored zeros over zeros
    // at inner: entered by the guest call after outer's first store
    let ret = (sym("outer") + 5) as u64;
    assert_eq!((s[1]["pc"].as_u64(), s[1]["ordinal"].as_u64(), s[1]["state"]["reta"].as_u64()),
               (Some(sym("inner") as u64), Some(2), Some(ret)));
    assert_eq!((s[1]["state"]["xsp"].as_u64(), s[1]["state"]["ssp"].as_u64()), (Some(0x0FFE), Some(0x07FE)));
    assert_eq!(s[1]["windows"], json!([{"word": 0x1600, "values": [7, 0, 0]}]));
    // the patch against the memory at the call: the store and the guest call's two stack words
    // (the public prologue stored zeros over zeros)
    assert_eq!(s[1]["patch_since_call"], json!([[0x07FE, 0, (SENTINEL >> 16) as u64], [0x0FFE, 0, SENTINEL as u64 & 0xFFFF],
                                                [0x1600, 0, 7]]));
}

#[test]
fn caps_truncate_explicitly_without_changing_the_run() {
    let call = observed("stores", store_regs(), json!({"write_events": {"max_events": 3},
                                                      "pc_states": {"pcs": [sym("stores")], "max_events": 1}}));
    let w = &call["write_events"];
    assert_eq!((w["complete"].as_bool(), w["cap"].as_u64(), w["recorded"].as_u64(), w["dropped"].as_u64()),
               (Some(false), Some(3), Some(3), Some(5)));
    let p = &call["pc_states"];
    assert_eq!((p["complete"].as_bool(), p["recorded"].as_u64(), p["dropped"].as_u64()), (Some(true), Some(1), Some(0)));
    let e = observed("outer", json!({"xar0": "0x1600", "xar1": "0x1601", "xar2": "0x1602"}),
                     json!({"entry_snapshots": {"pcs": [sym("inner")], "windows": [["0x1600", 1]], "max_bytes": 100}}));
    let e = &e["entry_snapshots"];
    assert_eq!((e["complete"].as_bool(), e["recorded"].as_u64(), e["omitted_for_bytes"].as_u64()), (Some(false), Some(0), Some(1)));
}

#[test]
fn a_refused_write_is_an_uncommitted_event() {
    let mut regs = store_regs();
    regs["xar0"] = json!(format!("0x{:x}", sym("stores") / 2));      // a code page
    let (rc, run) = run(&script("stores", regs, json!({"write_events": {}})));
    assert_eq!((rc, run["calls"][0]["status"].as_str()), (1, Some("execution_error")));
    let ev = run["calls"][0]["write_events"]["events"].as_array().unwrap();
    assert_eq!(ev.len(), 3);
    assert_eq!((ev[2][5].as_u64(), &ev[2][8]), (Some((sym("stores") / 2) as u64), &json!(false)));
}

#[test]
fn invalid_observer_options_are_refused() {
    for observe in [json!({"write_events": {"max_events": 0}}), json!({"pc_states": {"pcs": []}}),
                    json!({"write_events": {"mmr": 1}}), json!({"entry_snapshots": {"pcs": [1], "windows": [["0x10", 2]]}}),
                    json!({"write_events": {"max_events": (1u64 << 22) + 1}})] {
        let (rc, run) = run(&script("stores", store_regs(), observe.clone()));
        assert_eq!((rc, run["status"].as_str(), run["error"]["type"].as_str()), (1, Some("failed"), Some("ValueError")), "{}", observe);
    }
}
