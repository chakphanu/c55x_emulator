//! c55x-run: run a C55x call script (c55x-call-script/1 or /2) over a memory image
//! (c55x-memory-image/1) and a precomputed listing without Python, and write the run
//! (c55x-cli-run/1 or /2: output.pcm and run.json). The listing is decoded by the core's
//! frontend; its block_end column is checked against the image bytes. Each call is run
//! like the reference runner's bounded native batches: enter like a call instruction
//! returning to the sentinel, run chunks of steps until the pc reaches it, check limits
//! between chunks. The core is built only inside the managed containment
//! (bin/c55x-contained).
//!
//! Usage: c55x-run --script S.json [--input FILE] --out DIR (no --input: script without records)

mod state;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use c55x_core::core::{Core, MAX_IMAGE_BYTES, MEM_BYTES};
use c55x_core::err::Error;
use c55x_core::guard;
use c55x_core::frontend::Insn;
use c55x_core::icache::{PcFilter, PcSet};
use c55x_core::native::NativeHost;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use state::num;

const USAGE: &str = "usage: c55x-run --script S.json [--input FILE] --out DIR";

fn sha(data: &[u8]) -> String {
    Sha256::digest(data).iter().map(|b| format!("{:02x}", b)).collect()
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("{}: {}", path.display(), e))
}

fn abs(path: &Path) -> String {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()).display().to_string()
}

fn field<'a>(v: &'a Value, key: &str) -> Result<&'a Value, String> {
    v.get(key).ok_or_else(|| format!("missing {:?}", key))
}

fn int_of(v: &Value, key: &str) -> Result<i128, String> {
    num(field(v, key)?).map_err(|e| format!("{}: {}", key, e))
}

struct Args { script: PathBuf, input: Option<PathBuf>, out: PathBuf }

fn args() -> Result<Args, String> {
    let mut a = std::env::args().skip(1);
    let (mut script, mut input, mut out) = (None, None, None);
    while let Some(k) = a.next() {
        let v = a.next().ok_or_else(|| format!("{} needs a value", k))?;
        match k.as_str() {
            "--script" => script = Some(PathBuf::from(v)),
            "--input" => input = Some(PathBuf::from(v)),
            "--out" => out = Some(PathBuf::from(v)),
            _ => return Err(format!("unknown option {}", k)),
        }
    }
    match (script, out) {
        (Some(script), Some(out)) => Ok(Args { script, input, out }),
        _ => Err("--script and --out are required".into()),
    }
}

/// The memory image: segments loaded onto a zeroed array must give the snapshot.
fn load_image(path: &Path) -> Result<(Vec<u8>, String), String> {
    let image: Value = serde_json::from_slice(&read(path)?).map_err(|e| format!("{}: {}", path.display(), e))?;
    if image["format"] != "c55x-memory-image/1" || int_of(&image, "size_bytes")? != MEM_BYTES as i128 {
        return Err(format!("{}: not a {}-byte c55x-memory-image/1", path.display(), MEM_BYTES));
    }
    let dir = path.parent().unwrap_or(Path::new("."));
    let segments = field(&image, "segments")?.as_array().ok_or("segments is not a list")?;
    let total: i128 = segments.iter().map(|s| int_of(s, "bytes")).sum::<Result<_, _>>()?;
    if total > MAX_IMAGE_BYTES as i128 {
        return Err(format!("image of {} bytes exceeds the program address space ({})", total, MAX_IMAGE_BYTES));
    }
    let mut mem = vec![0u8; MEM_BYTES];
    for seg in segments {
        let file = dir.join(field(seg, "file")?.as_str().ok_or("segment file")?);
        let data = read(&file)?;
        if sha(&data) != field(seg, "sha256")?.as_str().unwrap_or("") || data.len() as i128 != int_of(seg, "bytes")? {
            return Err(format!("segment hash or size mismatch: {}", file.display()));
        }
        let at = int_of(seg, "byte_address")? as usize;
        if at + data.len() > MEM_BYTES {
            return Err(format!("segment outside memory: {}", file.display()));
        }
        mem[at..at + data.len()].copy_from_slice(&data);
    }
    let snapshot = field(&image, "snapshot_sha256")?.as_str().unwrap_or("").to_string();
    if sha(&mem) != snapshot {
        return Err("segments do not rebuild the snapshot".into());
    }
    Ok((mem, snapshot))
}

/// The listing: "pc(hex) TAB size TAB block_end(hex or -) TAB text", '#' comments.
fn load_listing(bytes: &[u8]) -> Result<Vec<Insn>, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "listing is not UTF-8".to_string())?;
    let mut out = Vec::new();
    for line in text.lines() {
        if line.is_empty() || line.starts_with('#') { continue; }
        let mut f = line.splitn(4, '\t');
        let (pc, size, end, text) = match (f.next(), f.next(), f.next(), f.next()) {
            (Some(a), Some(b), Some(c), Some(d)) => (a, b, c, d),
            _ => return Err(format!("listing line {:?}", line)),
        };
        let bad = || format!("listing line {:?}", line);
        out.push(Insn {
            addr: u32::from_str_radix(pc, 16).map_err(|_| bad())?,
            size: size.parse().map_err(|_| bad())?,
            block_end: if end == "-" { None } else { Some(u32::from_str_radix(end, 16).map_err(|_| bad())?) },
            text: text.to_string(),
        });
    }
    Ok(out)
}

fn unpack(data: &[u8], how: &str) -> Result<Vec<u16>, String> {
    Ok(match how {
        "bits_msb_first" => data.iter().flat_map(|b| (0..8).rev().map(move |i| ((b >> i) & 1) as u16)).collect(),
        "words_be" => data.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect(),
        "words_le" => data.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect(),
        other => return Err(format!("unknown unpack {}", other)),
    })
}

fn core_error(e: Error) -> (String, String) {
    match e {
        Error::Emu(m) | Error::Unsupported(m) => ("EmulationError".into(), m),
        Error::Policy(m) => ("ResourcePolicyError".into(), m),
        Error::Host(_) => ("HostError".into(), "host error".into()),
    }
}

struct Limits { steps: u64, seconds: f64, chunk: u64 }

struct Call { name: String, instructions: u64, cycles: u64, status: &'static str, observed: Map<String, Value> }

/// A failed step: (type, message).
type Fail = (String, String);

struct Runner {
    core: Core,
    sentinel: u32,
    stop: PcFilter,
    limits: Limits,
    calls: Vec<Call>,
    output: Vec<i16>,
    image: Vec<u8>,                         // the memory at the start of the run
    marks: HashMap<String, Vec<u8>>,        // mark_memory baselines
}

/// Every data word whose value differs from baseline: [word, before, after], ascending.
fn memory_diff(baseline: &[u8], now: &[u8]) -> Vec<Value> {
    let mut out = Vec::new();
    for (k, (a, b)) in baseline.chunks_exact(8).zip(now.chunks_exact(8)).enumerate() {
        if a == b { continue; }
        for w in 0..4 {
            let (x, y) = (&a[2 * w..2 * w + 2], &b[2 * w..2 * w + 2]);
            if x != y {
                out.push(json!([4 * k + w, u16::from_be_bytes([x[0], x[1]]), u16::from_be_bytes([y[0], y[1]])]));
            }
        }
    }
    out
}

impl Runner {
    /// One call, like the reference runner's bounded native batches.
    fn call(&mut self, label: &str, entry: u32, observe: Option<&Value>) -> Result<(), Fail> {
        let trace = observe.and_then(|o| o["trace_pcs"].as_u64()).unwrap_or(0) as usize;
        let mark = observe.and_then(|o| o["memory_diff_since"].as_str());
        if let Some(n) = mark {
            if !self.marks.contains_key(n) {
                return Err(("ValueError".into(), format!("no memory mark {:?}", n)));
            }
        }
        self.core.trace = if trace > 0 { Some((trace, Default::default())) } else { None };
        let (native0, cycles0) = (self.core.native, self.core.st.cycles);
        let start = Instant::now();
        let mut budget = 0u64;
        let mut status = "step_limit";
        let mut failure: Option<Fail> = None;
        let r = (|| -> Result<(), Fail> {
            guard::guard_entry().map_err(|m| ("ResourcePolicyError".to_string(), m))?;
            let pc = self.core.call(entry, self.sentinel).map_err(core_error)?;
            self.core.st.pc = pc;
            while budget < self.limits.steps {
                if start.elapsed().as_secs_f64() >= self.limits.seconds {
                    return Err(("TimeoutError".into(), "Wall-clock execution bound reached".into()));
                }
                if self.core.st.pc == self.sentinel {
                    status = "returned";
                    return Ok(());
                }
                let requested = self.limits.chunk.min(self.limits.steps - budget);
                budget += requested;
                guard::guard_entry().map_err(|m| ("ResourcePolicyError".to_string(), m))?;
                let idle = self.core.run(requested, &self.stop, None).map_err(core_error)?;
                if start.elapsed().as_secs_f64() >= self.limits.seconds {
                    return Err(("TimeoutError".into(), "Wall-clock execution bound reached".into()));
                }
                if self.core.st.pc == self.sentinel {
                    status = "returned";
                    return Ok(());
                }
                if idle {
                    status = "idle_without_return";
                    return Ok(());
                }
            }
            Ok(())
        })();
        if let Err(f) = r {
            status = if f.0 == "TimeoutError" { "timeout" } else { "execution_error" };
            failure = Some(f);
        }
        let mut observed = Map::new();
        if let Some((_, t)) = self.core.trace.take() {
            observed.insert("trace_pcs".into(), json!(t.into_iter().collect::<Vec<u32>>()));
        }
        if observe.map_or(false, |o| o["final_state"] == true) {
            observed.insert("final_state".into(), Value::Object(state::dump(&self.core.st)));
        }
        if let Some(n) = mark {
            observed.insert("memory_diff".into(), json!(memory_diff(&self.marks[n], self.core.memory())));
        }
        self.calls.push(Call { name: label.into(), instructions: self.core.native - native0,
                               cycles: self.core.st.cycles - cycles0, status, observed });
        if status != "returned" {
            let why = failure.map(|(t, m)| format!("{}: {}", t, m)).unwrap_or_default();
            return Err(("RuntimeError".into(), format!("call {} ended {}: {}", label, status, why)));
        }
        Ok(())
    }

    fn step(&mut self, op: &Value, record: Option<&[u8]>) -> Result<(), Fail> {
        let bad = |m: String| ("ValueError".to_string(), m);
        match op["op"].as_str() {
            Some("call") => {
                let regs = field(op, "regs").map_err(bad)?.as_object().ok_or_else(|| bad("regs".into()))?;
                for (name, v) in regs {
                    state::set(&mut self.core.st, name, v).map_err(bad)?;
                }
                let entry = int_of(op, "entry").map_err(bad)? as u32;
                let label = op["label"].as_str().unwrap_or("").to_string();
                self.call(&label, entry, op.get("observe"))
            }
            Some("restore_image") => {
                self.core.memory_mut().copy_from_slice(&self.image);
                Ok(())
            }
            Some("mark_memory") => {
                let name = op["name"].as_str().ok_or_else(|| bad("mark_memory without a name".into()))?;
                self.marks.insert(name.into(), self.core.memory().to_vec());
                Ok(())
            }
            Some("write") => {
                let words: Vec<u16> = match op["source"].as_str() {
                    Some("record") => unpack(record.ok_or_else(|| bad("record write outside records".into()))?,
                                             op["unpack"].as_str().unwrap_or("")).map_err(bad)?,
                    _ => field(op, "words").map_err(bad)?.as_array().ok_or_else(|| bad("words".into()))?
                        .iter().map(|v| num(v).map(|x| x as u16)).collect::<Result<_, _>>().map_err(bad)?,
                };
                let at = int_of(op, "word_address").map_err(bad)? as u32;
                let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_be_bytes()).collect();
                self.core.load_bytes(at * 2, &bytes).map_err(|e| bad(core_error(e).1))?;
                Ok(())
            }
            Some("read") => {
                if op["format"] != "s16" || op["to"] != "output" {
                    return Err(bad("unsupported read".into()));
                }
                let at = int_of(op, "word_address").map_err(bad)? as u32;
                let n = int_of(op, "words").map_err(bad)? as u32;
                for w in at..at + n {
                    let v = ((self.core.mem_byte(2 * w) as u16) << 8) | self.core.mem_byte(2 * w + 1) as u16;
                    self.output.push(v as i16);
                }
                Ok(())
            }
            other => Err(bad(format!("unknown op {:?}", other))),
        }
    }
}

fn main() {
    let a = match args() {
        Ok(a) => a,
        Err(e) => { eprintln!("c55x-run: {}\n{}", e, USAGE); std::process::exit(2) }
    };
    if let Err(e) = guard::guard() {
        eprintln!("c55x-run: refused outside the managed containment: {}", e);
        std::process::exit(3);
    }
    match run(&a) {
        Ok(true) => {}
        Ok(false) => std::process::exit(1),
        Err(e) => { eprintln!("c55x-run: {}", e); std::process::exit(2) }
    }
}

/// -> Ok(completed); Err for inputs that cannot be run (nothing written).
fn run(a: &Args) -> Result<bool, String> {
    let script_bytes = read(&a.script)?;
    let script: Value = serde_json::from_slice(&script_bytes).map_err(|e| format!("{}: {}", a.script.display(), e))?;
    let v2 = script["schema"] == "c55x-call-script/2";
    if !(v2 || script["schema"] == "c55x-call-script/1") || script["register_carry_over"] != true {
        return Err("unsupported call script".into());
    }
    if script["on_unmodeled_port"] != "error" || script["on_code_page_write"] != "error" {
        return Err("only the \"error\" port and code-page policies are supported".into());
    }
    if script["output"]["format"] != "s16le" || script["output"]["scaling"] != "none" {
        return Err("only s16le output without scaling is supported".into());
    }
    let base = a.script.parent().unwrap_or(Path::new("."));
    let lim = field(&script, "limits")?;
    let limits = Limits { steps: int_of(lim, "max_steps_per_call")? as u64,
                          seconds: int_of(lim, "max_seconds_per_call")? as f64,
                          chunk: int_of(lim, "chunk_steps")? as u64 };
    if !(1..=10_000_000).contains(&limits.steps) || !(1.0..=300.0).contains(&limits.seconds)
        || !(1..=16384).contains(&limits.chunk) {
        return Err("invalid execution bounds".into());
    }
    let records = match script.get("records") {
        Some(r) => Some(r),
        None if v2 => None,
        None => return Err("missing \"records\"".into()),
    };
    let input = match (records, &a.input) {
        (Some(rec), Some(path)) => {
            let data = read(path)?;
            let size = int_of(rec, "record_bytes")? as usize;
            if size == 0 || (data.len() % size != 0 && rec["partial_record"] == "error") {
                return Err("input is not a whole number of records".into());
            }
            Some((path, data, size))
        }
        (Some(_), None) => return Err("the script has records: --input is required".into()),
        (None, Some(_)) => return Err("the script has no records: no --input".into()),
        (None, None) => None,
    };

    let (mem, snapshot) = load_image(&base.join(field(&script, "image")?.as_str().ok_or("image")?))?;
    let listing_path = base.join(field(&script, "listing")?.as_str().ok_or("listing")?);
    let listing_bytes = read(&listing_path)?;
    let listing_sha = sha(&listing_bytes);
    let listing = load_listing(&listing_bytes)?;
    let mut core = NativeHost::from_listing(mem, &listing, &HashMap::new()).map_err(|e| core_error(e).1)?;
    let init = field(&script, "initial_state")?.as_object().ok_or("initial_state")?;
    state::initial(&mut core.st, init)?;
    let sentinel = int_of(&script, "return_sentinel")? as u32;
    let mut stop = PcSet::default();
    stop.insert(sentinel);
    let image = core.memory().to_vec();
    let mut r = Runner { core, sentinel, stop: PcFilter::new(stop), limits, calls: Vec::new(), output: Vec::new(),
                         image, marks: HashMap::new() };

    std::fs::create_dir(&a.out).map_err(|e| format!("{}: {}", a.out.display(), e))?;
    let mut error: Option<Fail> = None;
    let mut done = 0usize;
    let steps = |v: &Value, k: &str| -> Result<Vec<Value>, String> {
        Ok(field(v, k)?.as_array().ok_or_else(|| format!("{} is not a list", k))?.clone())
    };
    let setup = steps(&script, "setup")?;
    let per_record = match records { Some(rec) => steps(rec, "steps")?, None => Vec::new() };
    'outer: {
        for op in &setup {
            if let Err(f) = r.step(op, None) { error = Some(f); break 'outer; }
        }
        let Some((_, data, size)) = &input else { break 'outer };
        for rec in data.chunks_exact(*size) {
            for op in &per_record {
                if let Err(f) = r.step(op, Some(rec)) { error = Some(f); break 'outer; }
            }
            done += 1;
        }
    }

    let pcm: Vec<u8> = r.output.iter().flat_map(|v| v.to_le_bytes()).collect();
    std::fs::write(a.out.join("output.pcm"), &pcm).map_err(|e| e.to_string())?;
    let host: &NativeHost = r.core.host.as_any().downcast_ref().ok_or("native host")?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_sha = sha(&read(&exe)?);
    let calls: Vec<Value> = r.calls.iter().enumerate().map(|(i, c)| {
        let mut e = json!({"index": i, "name": c.name, "instructions": c.instructions, "cycles": c.cycles,
                           "status": c.status});
        e.as_object_mut().unwrap().extend(c.observed.clone());
        e
    }).collect();
    let mut core_info = Map::new();
    core_info.insert("path".into(), json!(exe.display().to_string()));
    core_info.insert("sha256".into(), json!(exe_sha));
    if let Some(rev) = option_env!("C55X_SOURCE_REVISION") {
        core_info.insert("source_revision".into(), json!(rev));
    }
    let run = json!({
        "format": if v2 { "c55x-cli-run/2" } else { "c55x-cli-run/1" },
        "status": if error.is_none() { "completed" } else { "failed" },
        "error": error.as_ref().map(|(t, m)| json!({"type": t, "message": m})),
        "producer": {"name": "c55x-run", "sha256": exe_sha, "version": env!("CARGO_PKG_VERSION")},
        "core": core_info,
        "script": {"path": abs(&a.script), "sha256": sha(&script_bytes)},
        "image": {"snapshot_sha256": snapshot},
        "listing": {"sha256": listing_sha},
        "input": input.as_ref().map(|(p, data, _)| json!({"path": abs(p), "sha256": sha(data), "records": done})),
        "output": {"file": "output.pcm", "format": "s16le", "samples": r.output.len(), "sha256": sha(&pcm)},
        "calls": calls,
        "totals": {"calls": r.calls.len(),
                   "instructions": r.calls.iter().map(|c| c.instructions).sum::<u64>(),
                   "cycles": r.calls.iter().map(|c| c.cycles).sum::<u64>(),
                   "unmodeled_port_accesses": host.unmodeled_ports,
                   "code_page_writes": host.code_page_writes},
    });
    let text = serde_json::to_string_pretty(&run).map_err(|e| e.to_string())? + "\n";
    std::fs::write(a.out.join("run.json"), text).map_err(|e| e.to_string())?;
    if let Some((t, m)) = &error {
        eprintln!("c55x-run: run failed: {}: {}", t, m);
    }
    Ok(error.is_none())
}
