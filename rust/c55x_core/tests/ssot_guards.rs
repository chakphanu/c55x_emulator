//! Static guards for the single source of truth (AGENTS.md): the workspace has no
//! Python binding, the repository no Python besides the launcher and its tests, and only
//! the core's native host implements its host interface. These tests only read files;
//! they build no core and need no TI tools.

use std::fs;
use std::path::{Path, PathBuf};

fn rust_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("workspace directory").to_path_buf()
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {}", path.display(), e))
}

/// Every .rs file below dir, sorted.
fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in fs::read_dir(&d).unwrap_or_else(|e| panic!("{}: {}", d.display(), e)) {
            let p = entry.expect("directory entry").path();
            if p.is_dir() {
                if p.file_name().is_some_and(|n| n != "target") {
                    stack.push(p);
                }
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// The code of a Rust source without // comments (doc comments included).
fn code_lines(text: &str) -> Vec<(usize, &str)> {
    text.lines().enumerate()
        .map(|(i, l)| (i + 1, l.split("//").next().unwrap_or("")))
        .filter(|(_, l)| !l.trim().is_empty())
        .collect()
}

/// The keys of one [section] of a Cargo.toml.
fn section_keys(toml: &str, section: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut inside = false;
    for line in toml.lines().map(str::trim) {
        if line.starts_with('[') {
            inside = line == format!("[{}]", section);
        } else if inside && !line.is_empty() && !line.starts_with('#') {
            if let Some((k, _)) = line.split_once('=') {
                keys.push(k.trim().to_string());
            }
        }
    }
    keys
}

#[test]
fn core_depends_on_regex_only() {
    let toml = read(&rust_dir().join("c55x_core/Cargo.toml"));
    assert_eq!(section_keys(&toml, "dependencies"), ["regex"], "c55x_core [dependencies]");
    for line in toml.lines().map(str::trim) {
        assert!(!line.starts_with("[build-dependencies") && !line.starts_with("[target."),
                "c55x_core has another dependency table: {}", line);
    }
}

#[test]
fn lock_file_core_depends_on_regex_only() {
    let lock = read(&rust_dir().join("Cargo.lock"));
    let package = lock.split("[[package]]")
        .find(|p| p.lines().any(|l| l.trim() == "name = \"c55x_core\""))
        .expect("c55x_core in Cargo.lock");
    let deps: Vec<&str> = match package.split_once("dependencies = [") {
        Some((_, rest)) => rest.split(']').next().unwrap_or("").split(',')
            .map(|d| d.trim().trim_matches('"')).filter(|d| !d.is_empty()).collect(),
        None => Vec::new(),
    };
    assert_eq!(deps, ["regex"], "c55x_core dependencies in Cargo.lock");
}

#[test]
fn core_source_has_no_python_binding() {
    let banned = ["pyo3", "PyObject", "Py<", "PyAny", "Python<", "Bound<'"];
    let mut hits = Vec::new();
    for path in rust_files(&rust_dir().join("c55x_core/src")) {
        let text = read(&path);
        for (n, line) in code_lines(&text) {
            if banned.iter().any(|b| line.contains(b)) {
                hits.push(format!("{}:{}: {}", path.display(), n, line.trim()));
            }
        }
    }
    assert!(hits.is_empty(), "Python binding code in c55x_core:\n{}", hits.join("\n"));
}

#[test]
fn only_rust_hosts_implement_host() {
    let allowed = ["c55x_core/src/native.rs"];
    let root = rust_dir();
    let mut hits = Vec::new();
    for path in rust_files(&root) {
        let rel = path.strip_prefix(&root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
        if rel.ends_with("ssot_guards.rs") {
            continue;
        }
        let text = read(&path);
        for (n, line) in code_lines(&text) {
            let words: Vec<&str> = line.split_whitespace().collect();
            let implements = words.windows(3).any(|w| {
                w[0] == "impl" && (w[1] == "Host" || w[1].ends_with("::Host")) && w[2] == "for"
            });
            if implements && !allowed.contains(&rel.as_str()) {
                hits.push(format!("{}:{}: {}", rel, n, line.trim()));
            }
        }
    }
    assert!(hits.is_empty(), "Host implemented outside the allowed hosts:\n{}", hits.join("\n"));
}

fn repo_dir() -> PathBuf {
    rust_dir().parent().expect("repository directory").to_path_buf()
}

#[test]
fn workspace_has_no_pyo3() {
    let root = rust_dir();
    let lock = read(&root.join("Cargo.lock"));
    assert!(!lock.contains("pyo3"), "pyo3 in rust/Cargo.lock");
    for entry in fs::read_dir(&root).expect("rust directory") {
        let manifest = entry.expect("directory entry").path().join("Cargo.toml");
        if manifest.is_file() {
            assert!(!read(&manifest).contains("pyo3"), "pyo3 in {}", manifest.display());
        }
    }
}

#[test]
fn no_python_binding_or_package() {
    for dir in ["rust/c55x_py", "c55x_emu"] {
        assert!(!repo_dir().join(dir).exists(), "{} exists", dir);
    }
}

/// Every file of the repository outside build output, caches and .git.
fn repo_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {}", dir.display(), e)) {
        let p = entry.expect("directory entry").path();
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if p.is_dir() {
            if !matches!(name.as_str(), ".git" | "build" | "target" | "__pycache__") {
                repo_files(&p, out);
            }
        } else {
            out.push(p);
        }
    }
}

#[test]
fn no_python_besides_the_launcher() {
    // The launcher and its tests are the explicit Python exception (AGENTS.md).
    let allowed = ["bin/c55x-contained", "tests/test_c55x_launcher.py", "tests/__init__.py"];
    let root = repo_dir();
    let mut files = Vec::new();
    repo_files(&root, &mut files);
    let mut hits = Vec::new();
    for path in files {
        let rel = path.strip_prefix(&root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
        let head = fs::read(&path).map(|b| b.into_iter().take(128).collect::<Vec<u8>>()).unwrap_or_default();
        let first = String::from_utf8_lossy(&head);
        let first = first.lines().next().unwrap_or("");
        let python = rel.ends_with(".py") || (first.starts_with("#!") && first.contains("python"));
        if python && !allowed.contains(&rel.as_str()) {
            hits.push(rel);
        }
    }
    assert!(hits.is_empty(), "Python files outside the launcher exception:\n{}", hits.join("\n"));
}
