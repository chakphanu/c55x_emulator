//! The command-line host c55x-run (from tests/test_c55x_run.py): a call script over a
//! memory image and a listing, run without Python, giving output.pcm and run.json
//! (c55x-cli-run/1). The routines come from the fixture rust/fixtures/cli. The binary
//! refuses to build a core outside an emulator job of bin/c55x-contained, so these tests
//! run there (scripts/cargo-test-contained); each test starts one process.

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

/// The fixture: (listing rows as c55x listing/1 TSV, .text bytes at their address, symbols).
fn fixture() -> (String, u32, Vec<u8>, Vec<(String, u32)>) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures/cli");
    let read = |f: &str| fs::read_to_string(dir.join(f)).unwrap();
    let insns = c55x_core::frontend::listing::parse(&read("prog.lst"), true).ok().unwrap();
    let mut tsv = String::from("# c55x listing/1 (synthetic, rust/fixtures/cli)\n");
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

/// Image (the fixture code in zero memory), listing, call script and input in dir.
fn write_inputs(d: &Path, steps: Value, words: &[u16]) {
    let (tsv, base, code, _) = fixture();
    let mut mem = vec![0u8; MEM_BYTES];
    mem[base as usize..base as usize + code.len()].copy_from_slice(&code);
    fs::write(d.join("code.bin"), &code).unwrap();
    fs::write(d.join("image.json"), json!({
        "format": "c55x-memory-image/1", "size_bytes": MEM_BYTES, "snapshot_sha256": sha(&mem),
        "segments": [{"file": "code.bin", "byte_address": base, "bytes": code.len(), "sha256": sha(&code)}],
    }).to_string()).unwrap();
    fs::write(d.join("code.listing.tsv"), tsv).unwrap();
    fs::write(d.join("test.call.json"), json!({
        "schema": "c55x-call-script/1", "image": "image.json", "listing": "code.listing.tsv",
        "return_sentinel": format!("0x{:x}", SENTINEL),
        "limits": {"max_steps_per_call": 1000, "chunk_steps": 16, "max_seconds_per_call": 10},
        "on_unmodeled_port": "error", "on_code_page_write": "error", "register_carry_over": true,
        "initial_state": initial_state(),
        "setup": [{"op": "write", "word_address": "0x1601", "words": [5]}],
        "records": {"record_bytes": 2, "partial_record": "error", "steps": steps},
        "output": {"format": "s16le", "scaling": "none"},
    }).to_string()).unwrap();
    let input: Vec<u8> = words.iter().flat_map(|w| w.to_be_bytes()).collect();
    fs::write(d.join("input.bin"), input).unwrap();
}

fn call(label: &str, entry: &str, regs: Value) -> Value {
    json!({"op": "call", "label": label, "entry": format!("0x{:x}", sym(entry)), "regs": regs})
}

fn record_write() -> Value {
    json!({"op": "write", "word_address": "0x1600", "source": "record", "unpack": "words_be"})
}

fn read_output() -> Value {
    json!({"op": "read", "word_address": "0x1700", "words": 1, "format": "s16", "to": "output"})
}

fn add_regs() -> Value {
    json!({"xar0": "0x1600", "xar1": "0x1601", "xar2": "0x1700"})
}

/// Run c55x-run over the inputs: (exit status, run.json, output.pcm).
fn run_cli(steps: Value, words: &[u16]) -> (i32, Value, Vec<u8>) {
    static N: AtomicUsize = AtomicUsize::new(0);
    let d = std::env::temp_dir().join(format!("c55x-run-test-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
    fs::create_dir_all(&d).unwrap();
    write_inputs(&d, steps, words);
    let out = d.join("out");
    let status = Command::new(env!("CARGO_BIN_EXE_c55x-run"))
        .arg("--script").arg(d.join("test.call.json")).arg("--input").arg(d.join("input.bin")).arg("--out").arg(&out)
        .output().unwrap().status;
    let run: Value = serde_json::from_slice(&fs::read(out.join("run.json")).unwrap()).unwrap();
    let pcm = fs::read(out.join("output.pcm")).unwrap_or_default();
    fs::remove_dir_all(&d).unwrap();
    (status.code().unwrap_or(-1), run, pcm)
}

fn samples(pcm: &[u8]) -> Vec<i16> {
    pcm.chunks(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect()
}

fn add_steps() -> Value {
    json!([record_write(), call("add", "add", add_regs()), read_output()])
}

#[test]
fn records_run_the_call_and_collect_the_output() {
    let (rc, run, pcm) = run_cli(add_steps(), &[1, 2, 0xFFFE]);
    assert_eq!((rc, run["status"].as_str(), &run["error"]), (0, Some("completed"), &Value::Null));
    assert_eq!(samples(&pcm), [6, 7, 3]);
    assert_eq!((run["output"]["samples"].as_u64(), run["output"]["sha256"].as_str()), (Some(3), Some(sha(&pcm).as_str())));
    let calls: Vec<(u64, &str, u64, &str)> = run["calls"].as_array().unwrap().iter().map(|c| (
        c["index"].as_u64().unwrap(), c["name"].as_str().unwrap(), c["instructions"].as_u64().unwrap(),
        c["status"].as_str().unwrap())).collect();
    assert_eq!(calls, (0..3).map(|i| (i, "add", 4, "returned")).collect::<Vec<_>>());
    let t = &run["totals"];
    assert_eq!([&t["calls"], &t["instructions"], &t["unmodeled_port_accesses"], &t["code_page_writes"]],
               [&json!(3), &json!(12), &json!(0), &json!(0)]);
    let cycles: u64 = run["calls"].as_array().unwrap().iter().map(|c| c["cycles"].as_u64().unwrap()).sum();
    assert_eq!(t["cycles"].as_u64(), Some(cycles));
    assert_eq!(run["input"]["records"].as_u64(), Some(3));
}

#[test]
fn registers_carry_over_between_calls() {
    // the second call names no registers: AR0..AR2 keep their values
    let steps = json!([record_write(), call("add", "add", add_regs()), call("again", "add", json!({})), read_output()]);
    let (rc, run, pcm) = run_cli(steps, &[1]);
    assert_eq!((rc, samples(&pcm)), (0, vec![6]));
    let names: Vec<&str> = run["calls"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["add", "again"]);
}

#[test]
fn port_access_fails_the_run() {
    let steps = json!([record_write(), call("add", "read_port", add_regs()), read_output()]);
    let (rc, run, _) = run_cli(steps, &[1]);
    assert_eq!((rc, run["status"].as_str(), run["calls"][0]["status"].as_str()), (1, Some("failed"), Some("execution_error")));
    assert_eq!(run["totals"]["unmodeled_port_accesses"].as_u64(), Some(1));
}

#[test]
fn code_page_write_fails_the_run() {
    let regs = json!({"xar0": "0x1600", "xar1": "0x1601", "xar2": format!("0x{:x}", sym("add") / 2)});
    let (rc, run, _) = run_cli(json!([record_write(), call("add", "add", regs)]), &[1]);
    assert_eq!((rc, run["status"].as_str(), run["calls"][0]["status"].as_str()), (1, Some("failed"), Some("execution_error")));
    assert_eq!(run["totals"]["code_page_writes"].as_u64(), Some(1));
}

#[test]
fn call_without_return_fails_the_run() {
    let steps = json!([record_write(), call("add", "spin", add_regs()), read_output()]);
    let (rc, run, _) = run_cli(steps, &[1, 2]);
    assert_eq!((rc, run["status"].as_str(), run["calls"].as_array().unwrap().len()), (1, Some("failed"), 1));
    let status = run["calls"][0]["status"].as_str().unwrap();
    assert!(["idle_without_return", "step_limit"].contains(&status), "{}", status);
}
