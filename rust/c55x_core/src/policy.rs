//! Resource containment policy: the core refuses to construct or execute
//! outside the managed cgroup. This protects against forgotten instructions
//! and accidental direct calls; it is not a security boundary against a
//! hostile same-UID process.
//!
//! The guard reads kernel state (/proc/self/cgroup, cgroup v2 files,
//! /proc/self/limits) and rejects execution when the required limits are
//! not present. It is checked once at construction and re-validated on
//! each execution entry, outside guest instruction semantics.

use std::fs;
use std::path::{Path, PathBuf};

use crate::err::{emu, R};

/// The env var the launcher sets when a job is admitted. Its presence is
/// necessary but not sufficient; the kernel limits are authoritative.
pub const ENV_JOB: &str = "C55X_CONTAINED_JOB";

/// Check that this process is inside a cgroup v2 hierarchy with the
/// required limits. Called at Core construction and on each execution
/// entry. Returns Ok(()) if the policy is satisfied, Err otherwise.
pub fn check_containment() -> R<()> {
    let env = std::env::var(ENV_JOB).unwrap_or_default();
    if env.is_empty() {
        return emu("resource policy: not inside a managed job \
                   (C55X_CONTAINED_JOB not set); use bin/c55x-contained".into());
    }

    // resolve the cgroup v2 path from /proc/self/cgroup
    let cg = match read_cgroup_v2_path() {
        Some(p) => p,
        None => return emu("resource policy: cannot resolve cgroup v2 \
                            membership from /proc/self/cgroup".into()),
    };

    // verify required limits at the job level and the shared slice level
    check_limit(&cg, "memory.max", |v| v > 0 && v <= 2 * 1024 * 1024 * 1024)?;
    check_limit(&cg, "memory.swap.max", |v| v == 0)?;
    check_limit(&cg, "pids.max", |v| v > 0 && v <= 32)?;

    // verify the emulator's own allocation caps are active (set by the
    // Python side: bus MAX_LOAD_BYTES, decoder MAX_CACHED_INSNS). The
    // Python Bus reports its allocation log through the cpu object.
    // The Rust guard checks that the Python process is not running with
    // an overriden allocation limit (the env var is advisory only).
    if let Ok(skip) = std::env::var("C55X_NO_ALLOC_LIMIT") {
        if !skip.is_empty() {
            return emu("resource policy: C55X_NO_ALLOC_LIMIT is set; \
                        emulator allocation caps are disabled".into());
        }
    }

    // the shared ancestor (c55x.slice) must also have finite limits
    let slice = find_ancestor_slice(&cg);
    if let Some(sp) = slice {
        check_limit(&sp, "memory.max", |v| v > 0 && v <= 4 * 1024 * 1024 * 1024)?;
        check_limit(&sp, "memory.swap.max", |v| v == 0)?;
    }

    // verify process rlimits
    check_rlimits()?;

    Ok(())
}

fn read_cgroup_v2_path() -> Option<PathBuf> {
    let text = fs::read_to_string("/proc/self/cgroup").ok()?;
    for line in text.lines() {
        // cgroup v2 lines look like "0::/user.slice/user-1000.slice/..."
        if line.starts_with("0::") {
            let path = line.trim_start_matches("0::").trim();
            if path.is_empty() {
                return None;
            }
            // strip leading '/' so join appends instead of replacing the base
            let path = path.strip_prefix('/').unwrap_or(path);
            return Some(PathBuf::from("/sys/fs/cgroup").join(path));
        }
    }
    None
}

fn read_cgroup_value(cg: &Path, name: &str) -> Option<u64> {
    let p = cg.join(name);
    let text = fs::read_to_string(&p).ok()?;
    let text = text.trim();
    if text == "max" {
        return None; // unlimited
    }
    text.parse().ok()
}

fn check_limit(cg: &Path, name: &str, ok: impl Fn(u64) -> bool) -> R<()> {
    match read_cgroup_value(cg, name) {
        Some(v) if ok(v) => Ok(()),
        Some(v) => emu(format!(
            "resource policy: {}={} at {} does not satisfy the policy",
            name, v, cg.display()
        )),
        None => emu(format!(
            "resource policy: {} is unlimited (max) at {} — not contained",
            name, cg.display()
        )),
    }
}

fn find_ancestor_slice(cg: &Path) -> Option<PathBuf> {
    // walk up the cgroup path to find the c55x.slice ancestor
    for ancestor in cg.ancestors() {
        if ancestor.to_string_lossy().ends_with("c55x.slice") {
            return Some(ancestor.to_path_buf());
        }
    }
    None
}

fn check_rlimits() -> R<()> {
    let limits = match fs::read_to_string("/proc/self/limits") {
        Ok(s) => s,
        Err(_) => return emu("resource policy: cannot read /proc/self/limits".into()),
    };
    // parse: "Limit Name     Soft   Hard   Units" — names contain spaces,
    // so extract numbers from the right side of each matched line
    let mut cpu_ok = false;
    let mut fd_ok = false;
    let mut core_ok = false;
    for line in limits.lines() {
        let nums: Vec<u64> = line.split_whitespace()
            .filter_map(|w| w.parse::<u64>().ok())
            .collect();
        if line.starts_with("Max cpu time") {
            // hard limit is nums[1] (soft, hard, ...); both must be <= 600
            if nums.len() >= 2 && nums[0] <= 600 && nums[1] <= 600 {
                cpu_ok = true;
            }
        }
        if line.starts_with("Max open files") {
            if nums.len() >= 2 && nums[0] <= 512 && nums[1] <= 512 {
                fd_ok = true;
            }
        }
        if line.starts_with("Max core file size") {
            if nums.len() >= 2 && nums[0] == 0 && nums[1] == 0 {
                core_ok = true;
            }
        }
    }
    if !cpu_ok {
        return emu("resource policy: RLIMIT_CPU not set or exceeds 600s".into());
    }
    if !fd_ok {
        return emu("resource policy: RLIMIT_NOFILE not set or exceeds 512".into());
    }
    if !core_ok {
        return emu("resource policy: RLIMIT_CORE not zero".into());
    }
    Ok(())
}
