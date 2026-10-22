//! c55x-fixtures: rebuild or check the committed test fixtures with the TI C55x tools.
//!
//! A fixture is a directory rust/fixtures/NAME holding a synthetic program and what the
//! tests read instead of running the tools:
//!   prog.asm     the source (algebraic syntax), written by hand
//!   fixture.txt  'origin 0x020000': the program byte address of .text
//!   prog.lst     dis55 -qq -r -g of the linked program (the runtime listing flags)
//!   prog.hex     the loaded sections of the linked program (see write_image)
//! 'write' regenerates prog.lst and prog.hex, 'check' fails when they differ from what
//! the tools produce now. Other files in a fixture directory (expected results) are
//! not touched.
//!
//! Usage: c55x-fixtures (check|write) [--ti DIR] [NAME...]
//! DIR holds asm55, lnk55 and dis55 (default: $C55X_TI_BIN, else the directory of
//! asm55 in PATH). Run it in a work job of bin/c55x-contained.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{exit, Command};

const FILE_HDR: usize = 22;             // magic, nscns, time, symptr, nsyms, opthdr, flags, target
const OPT_HDR: usize = 28;
const SCN_HDR: usize = 48;
const COFF2_MAGIC: u16 = 0x00C2;
const C55X_TARGET: u16 = 0x009C;
const STYP_SKIP: u32 = 0x02 | 0x10 | 0x80;      // NOLOAD, COPY, BSS

fn u16le(d: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([d[o], d[o + 1]])
}

fn u32le(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

/// Loaded sections of a TI-COFF2 C55x executable: (name, byte address, bytes), the
/// rules of c55x_emu/coff.py.
fn coff_sections(d: &[u8]) -> Result<Vec<(String, u32, Vec<u8>)>, String> {
    if d.len() < FILE_HDR || u16le(d, 0) != COFF2_MAGIC || u16le(d, 20) != C55X_TARGET {
        return Err("not a C55x TI-COFF2 file".into());
    }
    let (nscns, symptr, nsyms, opthdr) = (u16le(d, 2) as usize, u32le(d, 8) as usize, u32le(d, 12) as usize,
                                          u16le(d, 16) as usize);
    if opthdr != OPT_HDR {
        return Err("executable without optional header".into());
    }
    let strings = &d[symptr + nsyms * 18..];
    let mut out = Vec::new();
    for i in 0..nscns {
        let h = FILE_HDR + opthdr + i * SCN_HDR;
        let raw = &d[h..h + 8];
        let (paddr, size, scnptr, flags) = (u32le(d, h + 8), u32le(d, h + 16) as usize, u32le(d, h + 20) as usize,
                                            u32le(d, h + 40));
        if scnptr == 0 || flags & STYP_SKIP != 0 {
            continue;
        }
        let name = if raw[..4] == [0; 4] {
            let off = u32le(raw, 4) as usize;
            let end = strings[off..].iter().position(|&c| c == 0).ok_or("bad string table")?;
            String::from_utf8_lossy(&strings[off..off + end]).into_owned()
        } else {
            String::from_utf8_lossy(raw).trim_end_matches('\0').to_owned()
        };
        out.push((name, paddr, d[scnptr..scnptr + size].to_vec()));
    }
    Ok(out)
}

/// prog.hex: per section a line 'NAME 0xADDRESS SIZE', then its bytes in hex, 32 per line.
fn write_image(sections: &[(String, u32, Vec<u8>)]) -> String {
    let mut s = String::from("# c55x-fixtures image: section, byte address, size; bytes in hex\n");
    for (name, addr, data) in sections {
        let _ = writeln!(s, "{} 0x{:06x} {}", name, addr, data.len());
        for chunk in data.chunks(32) {
            s.extend(chunk.iter().map(|b| format!("{:02x}", b)));
            s.push('\n');
        }
    }
    s
}

fn run(tool: &Path, args: &[&str], cwd: &Path) -> Result<String, String> {
    let out = Command::new(tool).args(args).current_dir(cwd).output()
        .map_err(|e| format!("{}: {}", tool.display(), e))?;
    if !out.status.success() {
        return Err(format!("{} {} failed:\n{}{}", tool.display(), args.join(" "),
                           String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn origin(dir: &Path) -> Result<u32, String> {
    let text = fs::read_to_string(dir.join("fixture.txt")).map_err(|e| format!("fixture.txt: {}", e))?;
    for line in text.lines() {
        if let Some(v) = line.trim().strip_prefix("origin ") {
            let v = v.trim();
            return u32::from_str_radix(v.trim_start_matches("0x"), 16).map_err(|e| format!("origin {}: {}", v, e));
        }
    }
    Err("fixture.txt has no 'origin' line".into())
}

/// (prog.lst, prog.hex) of a fixture as the tools make them now.
fn build(ti: &Path, dir: &Path, work: &Path) -> Result<(String, String), String> {
    fs::create_dir_all(work).map_err(|e| e.to_string())?;
    fs::copy(dir.join("prog.asm"), work.join("prog.asm")).map_err(|e| format!("prog.asm: {}", e))?;
    fs::write(work.join("prog.cmd"), format!(
        "MEMORY {{ CODE (RX) : origin = 0x{:x}, length = 0x01000000 }}\nSECTIONS {{ .text : {{}} > CODE }}\n",
        origin(dir)?)).map_err(|e| e.to_string())?;
    run(&ti.join("asm55"), &["prog.asm", "prog.obj"], work)?;
    run(&ti.join("lnk55"), &["-q", "prog.obj", "prog.cmd", "-o", "prog.out"], work)?;
    let lst = run(&ti.join("dis55"), &["-qq", "-r", "-g", "prog.out"], work)?;
    let out = fs::read(work.join("prog.out")).map_err(|e| e.to_string())?;
    Ok((lst, write_image(&coff_sections(&out)?)))
}

fn ti_dir(arg: Option<String>) -> Result<PathBuf, String> {
    if let Some(d) = arg.or_else(|| std::env::var("C55X_TI_BIN").ok()) {
        return Ok(PathBuf::from(d));
    }
    std::env::var_os("PATH").and_then(|p| std::env::split_paths(&p).find(|d| d.join("asm55").is_file()))
        .ok_or_else(|| "TI tools not found: pass --ti DIR or set C55X_TI_BIN".into())
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mode = args.next().unwrap_or_default();
    if mode != "check" && mode != "write" {
        eprintln!("usage: c55x-fixtures (check|write) [--ti DIR] [NAME...]");
        exit(2);
    }
    let (mut ti, mut names) = (None, Vec::new());
    while let Some(a) = args.next() {
        if a == "--ti" { ti = args.next(); } else { names.push(a); }
    }
    let ti = ti_dir(ti).unwrap_or_else(|e| { eprintln!("c55x-fixtures: {}", e); exit(2) });
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures");
    if names.is_empty() {
        let mut found: Vec<String> = fs::read_dir(&root).map(|it| it.flatten()
            .filter(|e| e.path().join("prog.asm").is_file())
            .map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
        found.sort();
        names = found;
    }
    let scratch = std::env::temp_dir().join(format!("c55x-fixtures-{}", std::process::id()));
    let mut failed = 0;
    for name in &names {
        let dir = root.join(name);
        let result = build(&ti, &dir, &scratch.join(name)).and_then(|(lst, img)| {
            let mut stale = Vec::new();
            for (file, new) in [("prog.lst", lst), ("prog.hex", img)] {
                let path = dir.join(file);
                if fs::read_to_string(&path).ok().as_deref() == Some(new.as_str()) {
                    continue;
                }
                if mode == "write" {
                    fs::write(&path, new).map_err(|e| e.to_string())?;
                    println!("{}: wrote {}", name, file);
                } else {
                    stale.push(file);
                }
            }
            if stale.is_empty() { Ok(()) } else { Err(format!("differs from the tools: {}", stale.join(", "))) }
        });
        match result {
            Ok(()) => println!("{}: ok", name),
            Err(e) => {
                failed += 1;
                println!("{}: {}", name, e);
            }
        }
    }
    let _ = fs::remove_dir_all(&scratch);
    exit(if failed > 0 { 1 } else { 0 });
}
