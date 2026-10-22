//! Resource containment policy for the native core.
//!
//! The core refuses to be constructed, and refuses to execute, unless the
//! process runs inside the managed cgroup v2 hierarchy created by
//! `bin/c55x-contained`:
//!
//!   <cgroup2 mount>/.../c55x.slice/<slot unit>[/nested...]
//!
//! where <slot unit> is one of the emulator slot units: c55x-emu.service
//! (slot 0) or c55x-emu-<k>.service for k = 1..7, spelled exactly.
//!
//! Kernel state is the proof; the environment marker is only a hint that
//! produces a clear error for plain direct calls. This module has no
//! dependency on pyo3 or the rest of the crate so it can be unit tested in
//! isolation with fake cgroup trees:
//!
//!   rustc --edition 2021 --test src/policy.rs -o /path/to/policy_tests
//!
//! Trust boundary: this protects against forgotten instructions and
//! accidental direct calls. A hostile process of the same user could edit its
//! own delegated cgroups or run an old binary; stopping that needs a
//! separate execution identity and OS permissions.

use std::fs;
use std::path::{Component, Path, PathBuf};

pub const ENV_JOB: &str = "C55X_CONTAINED_JOB";
pub const SLICE_NAME: &str = "c55x.slice";
pub const JOB_UNIT: &str = "c55x-emu.service";
pub const JOB_SLOTS: u32 = 8;
const LAUNCHER_HINT: &str = "use bin/c55x-contained -- CMD...";

const GIB: u64 = 1 << 30;
const JOB_MEM_MAX: u64 = 2 * GIB;
const JOB_PIDS_MAX: u64 = 32;
const JOB_CPU_RATIO_MAX: f64 = 1.0;
const SLICE_MEM_MAX: u64 = 18 * GIB;
const SLICE_MEM_HIGH: u64 = 17 * GIB;
const SLICE_PIDS_MAX: u64 = 448;
const SLICE_CPU_RATIO_MAX: f64 = 10.0;
const RLIMIT_CPU_MAX: u64 = 600;
const RLIMIT_NOFILE_MAX: u64 = 512;
const RLIMIT_AS_MAX: u64 = 16 * GIB;

/// Everything the policy reads, so tests can substitute fake content.
pub struct Source<'a> {
    pub proc_cgroup: &'a str,
    pub mountinfo: &'a str,
    pub proc_limits: &'a str,
    pub job_env: Option<&'a str>,
}

/// True for exactly the emulator slot unit names: JOB_UNIT (slot 0) and
/// c55x-emu-<k>.service for k = 1..JOB_SLOTS-1 (no sign, no leading zero).
pub fn is_job_unit(name: &str) -> bool {
    if name == JOB_UNIT {
        return true;
    }
    match name.strip_prefix("c55x-emu-").and_then(|s| s.strip_suffix(".service")) {
        Some(k) => (1..JOB_SLOTS).any(|i| k == i.to_string()),
        None => false,
    }
}

fn refuse<T>(why: String) -> Result<T, String> {
    Err(format!("resource policy: {} ({})", why, LAUNCHER_HINT))
}

/// Check the live process. Ok(()) only when every requirement holds.
pub fn check_containment() -> Result<(), String> {
    let read = |p: &str| {
        fs::read_to_string(p).map_err(|e| format!("resource policy: cannot read {}: {}", p, e))
    };
    let cg = read("/proc/self/cgroup")?;
    let mi = read("/proc/self/mountinfo")?;
    let lim = read("/proc/self/limits")?;
    let env = std::env::var(ENV_JOB).ok();
    evaluate(&Source {
        proc_cgroup: &cg,
        mountinfo: &mi,
        proc_limits: &lim,
        job_env: env.as_deref(),
    })
}

pub fn evaluate(src: &Source) -> Result<(), String> {
    match src.job_env {
        Some("emulator") => {}
        Some("") | None => {
            return refuse("not inside a managed emulator job \
                           (C55X_CONTAINED_JOB is not set)".to_string())
        }
        Some(other) => {
            return refuse(format!(
                "job class {:?} may not run the emulator core", other))
        }
    }

    let job_dir = resolve_job_dir(src.proc_cgroup, src.mountinfo)?;
    let slice_dir = match job_dir.parent() {
        Some(p) if p.file_name().map_or(false, |n| n == SLICE_NAME) => p.to_path_buf(),
        _ => return refuse(format!(
            "cgroup {} is not directly below {}", job_dir.display(), SLICE_NAME)),
    };

    // job level
    let mem = need_finite(&job_dir, "memory.max")?;
    if mem == 0 || mem > JOB_MEM_MAX {
        return refuse(format!("job memory.max={} outside 1..={}", mem, JOB_MEM_MAX));
    }
    let swap = need_finite(&job_dir, "memory.swap.max")?;
    if swap != 0 {
        return refuse(format!("job memory.swap.max={} (must be 0)", swap));
    }
    let pids = need_finite(&job_dir, "pids.max")?;
    if pids == 0 || pids > JOB_PIDS_MAX {
        return refuse(format!("job pids.max={} outside 1..={}", pids, JOB_PIDS_MAX));
    }
    let oom = need_finite(&job_dir, "memory.oom.group")?;
    if oom != 1 {
        return refuse(format!("job memory.oom.group={} (must be 1)", oom));
    }
    let cpu = need_cpu_ratio(&job_dir)?;
    if cpu > JOB_CPU_RATIO_MAX {
        return refuse(format!("job cpu.max ratio {:.2} exceeds {:.2}", cpu, JOB_CPU_RATIO_MAX));
    }

    // shared ancestor
    let smem = need_finite(&slice_dir, "memory.max")?;
    if smem == 0 || smem > SLICE_MEM_MAX {
        return refuse(format!("shared memory.max={} outside 1..={}", smem, SLICE_MEM_MAX));
    }
    let shigh = need_finite(&slice_dir, "memory.high")?;
    if shigh == 0 || shigh > SLICE_MEM_HIGH {
        return refuse(format!("shared memory.high={} outside 1..={}", shigh, SLICE_MEM_HIGH));
    }
    let sswap = need_finite(&slice_dir, "memory.swap.max")?;
    if sswap != 0 {
        return refuse(format!("shared memory.swap.max={} (must be 0)", sswap));
    }
    let spids = need_finite(&slice_dir, "pids.max")?;
    if spids == 0 || spids > SLICE_PIDS_MAX {
        return refuse(format!("shared pids.max={} outside 1..={}", spids, SLICE_PIDS_MAX));
    }
    let scpu = need_cpu_ratio(&slice_dir)?;
    if scpu > SLICE_CPU_RATIO_MAX {
        return refuse(format!("shared cpu.max ratio {:.2} exceeds {:.2}", scpu, SLICE_CPU_RATIO_MAX));
    }

    check_rlimits(src.proc_limits)
}

/// Locate the cgroup2 mount in mountinfo and map this process's cgroup path
/// onto it; refuse anything but a plain descending path.
fn resolve_job_dir(proc_cgroup: &str, mountinfo: &str) -> Result<PathBuf, String> {
    let rel = proc_cgroup
        .lines()
        .find_map(|l| l.strip_prefix("0::"))
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "resource policy: no cgroup v2 membership in /proc/self/cgroup \
                        (cgroup v2 is required)".to_string())?;

    // mountinfo: id parent maj:min root mountpoint opts [optional...] - fstype src sopts
    let mut found: Option<(String, String)> = None;
    for line in mountinfo.lines() {
        let (left, right) = match line.split_once(" - ") {
            Some(x) => x,
            None => continue,
        };
        if right.split_whitespace().next() != Some("cgroup2") {
            continue;
        }
        let f: Vec<&str> = left.split_whitespace().collect();
        if f.len() >= 5 {
            found = Some((f[3].to_string(), f[4].to_string()));
            break;
        }
    }
    let (mnt_root, mnt_point) =
        found.ok_or_else(|| "resource policy: no cgroup2 mount found".to_string())?;

    // path of this process relative to the mount's root
    let rel_path = Path::new(rel);
    let under = rel_path
        .strip_prefix(&mnt_root)
        .map_err(|_| format!("resource policy: cgroup {} is outside the mount root {}", rel, mnt_root))?;
    for c in under.components() {
        match c {
            Component::Normal(_) => {}
            _ => return refuse(format!("cgroup path {:?} is not a plain descending path", rel)),
        }
    }
    // the job directory is the first slot unit component (nested cgroups
    // below it are allowed and only tighten limits)
    let mut dir = PathBuf::from(&mnt_point);
    let mut found_job = false;
    for c in under.components() {
        dir.push(c.as_os_str());
        if c.as_os_str().to_str().map_or(false, is_job_unit) {
            found_job = true;
            break;
        }
    }
    if !found_job {
        return refuse(format!(
            "cgroup {} is not inside an emulator slot unit ({} or c55x-emu-<1..{}>.service)",
            rel, JOB_UNIT, JOB_SLOTS - 1));
    }
    Ok(dir)
}

fn read_trim(dir: &Path, name: &str) -> Result<String, String> {
    let p = dir.join(name);
    fs::read_to_string(&p)
        .map(|s| s.trim().to_string())
        .map_err(|_| format!(
            "resource policy: controller file {} missing or unreadable ({})",
            p.display(), LAUNCHER_HINT))
}

/// A numeric limit that must be finite ("max" or garbage refuses).
fn need_finite(dir: &Path, name: &str) -> Result<u64, String> {
    let t = read_trim(dir, name)?;
    if t == "max" {
        return refuse(format!("{} at {} is unlimited", name, dir.display()));
    }
    t.parse::<u64>().or_else(|_| refuse(format!("{} at {} is not a number: {:?}", name, dir.display(), t)))
}

/// cpu.max quota/period as a ratio of one CPU; "max" quota refuses.
fn need_cpu_ratio(dir: &Path) -> Result<f64, String> {
    let t = read_trim(dir, "cpu.max")?;
    let mut it = t.split_whitespace();
    let (q, p) = (it.next(), it.next());
    match (q, p) {
        (Some("max"), _) => refuse(format!("cpu.max at {} is unlimited", dir.display())),
        (Some(q), Some(p)) => match (q.parse::<f64>(), p.parse::<f64>()) {
            (Ok(q), Ok(p)) if p > 0.0 && q > 0.0 => Ok(q / p),
            _ => refuse(format!("cpu.max at {} unparsable: {:?}", dir.display(), t)),
        },
        _ => refuse(format!("cpu.max at {} unparsable: {:?}", dir.display(), t)),
    }
}

/// (soft, hard) of a /proc/self/limits row; None means "unlimited".
fn limit_row(limits: &str, name: &str) -> Option<(Option<u64>, Option<u64>)> {
    for line in limits.lines() {
        if let Some(rest) = line.strip_prefix(name) {
            let mut it = rest.split_whitespace();
            let parse = |s: Option<&str>| match s {
                Some("unlimited") => Some(None),
                Some(n) => n.parse::<u64>().ok().map(Some),
                None => None,
            };
            let soft = parse(it.next())?;
            let hard = parse(it.next())?;
            return Some((soft, hard));
        }
    }
    None
}

fn check_rlimits(limits: &str) -> Result<(), String> {
    let finite_le = |name: &str, max: u64| -> Result<(), String> {
        match limit_row(limits, name) {
            Some((Some(s), Some(h))) if s <= max && h <= max => Ok(()),
            Some(_) => refuse(format!("rlimit {} unlimited or above {}", name.trim(), max)),
            None => refuse(format!("rlimit {} not readable", name.trim())),
        }
    };
    finite_le("Max cpu time", RLIMIT_CPU_MAX)?;
    finite_le("Max open files", RLIMIT_NOFILE_MAX)?;
    finite_le("Max address space", RLIMIT_AS_MAX)?;
    match limit_row(limits, "Max core file size") {
        Some((Some(0), Some(0))) => Ok(()),
        _ => refuse("rlimit core file size is not 0".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static N: AtomicU32 = AtomicU32::new(0);

    /// A fake cgroup2 tree plus matching /proc content, with every value
    /// adjustable so each test breaks exactly one requirement.
    struct Fake {
        root: PathBuf,
        job_rel: String,
        env: Option<String>,
        limits: String,
        mountinfo_fstype: String,
    }

    const GOOD_LIMITS: &str = "\
Limit                     Soft Limit           Hard Limit           Units     \n\
Max cpu time              600                  600                  seconds   \n\
Max core file size        0                    0                    bytes     \n\
Max open files            512                  512                  files     \n\
Max address space         2147483648           2147483648           bytes     \n";

    fn put(dir: &Path, name: &str, val: &str) {
        fs::write(dir.join(name), format!("{}\n", val)).unwrap();
    }

    impl Fake {
        fn new() -> Fake {
            let id = N.fetch_add(1, Ordering::SeqCst);
            let root = std::env::temp_dir()
                .join(format!("c55x-policy-test-{}-{}", std::process::id(), id));
            let _ = fs::remove_dir_all(&root);
            let f = Fake {
                root,
                job_rel: format!("/user.slice/{}/{}", SLICE_NAME, JOB_UNIT),
                env: Some("emulator".to_string()),
                limits: GOOD_LIMITS.to_string(),
                mountinfo_fstype: "cgroup2".to_string(),
            };
            f.write_good();
            f
        }
        fn dir(&self, rel: &str) -> PathBuf {
            self.root.join(rel.trim_start_matches('/'))
        }
        fn write_good(&self) {
            let slice = self.dir(&format!("/user.slice/{}", SLICE_NAME));
            let job = self.dir(&self.job_rel);
            fs::create_dir_all(&job).unwrap();
            put(&slice, "memory.max", &(18 * GIB).to_string());
            put(&slice, "memory.high", &(17 * GIB).to_string());
            put(&slice, "memory.swap.max", "0");
            put(&slice, "pids.max", "448");
            put(&slice, "cpu.max", "1000000 100000");
            put(&job, "memory.max", &GIB.to_string());
            put(&job, "memory.swap.max", "0");
            put(&job, "pids.max", "32");
            put(&job, "memory.oom.group", "1");
            put(&job, "cpu.max", "100000 100000");
        }
        fn slice(&self) -> PathBuf { self.dir(&format!("/user.slice/{}", SLICE_NAME)) }
        fn job(&self) -> PathBuf { self.dir(&self.job_rel) }
        fn check(&self) -> Result<(), String> {
            let cg = format!("0::{}\n", self.job_rel);
            let mi = format!(
                "29 22 0:26 / {} rw,nosuid - {} cgroup2 rw\n",
                self.root.display(), self.mountinfo_fstype);
            evaluate(&Source {
                proc_cgroup: &cg, mountinfo: &mi,
                proc_limits: &self.limits, job_env: self.env.as_deref(),
            })
        }
        fn expect_refused(&self, needle: &str) {
            match self.check() {
                Ok(()) => panic!("expected refusal containing {:?}, got Ok", needle),
                Err(e) => assert!(e.contains(needle), "wrong refusal: {}", e),
            }
        }
    }
    impl Drop for Fake {
        fn drop(&mut self) { let _ = fs::remove_dir_all(&self.root); }
    }

    #[test] fn baseline_is_admitted() { assert_eq!(Fake::new().check(), Ok(())); }

    #[test] fn missing_marker_is_refused() {
        let mut f = Fake::new(); f.env = None; f.expect_refused("not inside a managed");
        f.env = Some(String::new()); f.expect_refused("not inside a managed");
    }
    #[test] fn fake_marker_alone_is_not_enough() {
        // marker present but the kernel state is relaxed: still refused
        let f = Fake::new(); put(&f.job(), "memory.max", "max");
        f.expect_refused("unlimited");
    }
    #[test] fn work_class_cannot_run_the_core() {
        let mut f = Fake::new(); f.env = Some("work".to_string()); f.expect_refused("may not run");
    }
    #[test] fn group_outside_the_shared_slice_is_refused() {
        let mut f = Fake::new();
        f.job_rel = "/user.slice/c55xfake.slice/c55x-emu.service".to_string();
        f.write_good();
        f.expect_refused("not directly below");
    }
    #[test] fn wrong_unit_name_is_refused() {
        let mut f = Fake::new();
        f.job_rel = format!("/user.slice/{}/c55x-other.service", SLICE_NAME);
        f.write_good();
        f.expect_refused("not inside");
    }
    #[test] fn every_slot_unit_is_admitted() {
        let names = ["c55x-emu.service", "c55x-emu-1.service", "c55x-emu-2.service",
                     "c55x-emu-3.service", "c55x-emu-4.service", "c55x-emu-5.service",
                     "c55x-emu-6.service", "c55x-emu-7.service"];
        assert_eq!(names.len() as u32, JOB_SLOTS);
        for name in names {
            assert!(is_job_unit(name), "{}", name);
            let mut f = Fake::new();
            f.job_rel = format!("/user.slice/{}/{}", SLICE_NAME, name);
            f.write_good();
            assert_eq!(f.check(), Ok(()), "{}", name);
            // a nested group below the slot unit is admitted too
            f.job_rel = format!("/user.slice/{}/{}/child", SLICE_NAME, name);
            f.write_good();
            assert_eq!(f.check(), Ok(()), "{}/child", name);
        }
    }
    #[test] fn names_outside_the_slot_set_are_refused() {
        for name in ["c55x-emu-8.service", "c55x-emu-9.service", "c55x-emu-0.service",
                     "c55x-emu-x.service", "c55x-emu-01.service", "c55x-emu-10.service",
                     "c55x-emu-+1.service", "c55x-emu-.service", "c55x-emu-1.scope",
                     "c55x-emu-1.service.d", "c55x-emu1.service", "c55x-emus.service",
                     "C55X-EMU-1.service", "c55x-work-1-abcdef12.service", "c55x-other.service"] {
            assert!(!is_job_unit(name), "{}", name);
            let mut f = Fake::new();
            f.job_rel = format!("/user.slice/{}/{}", SLICE_NAME, name);
            f.write_good();
            f.expect_refused("not inside an emulator slot unit");
        }
    }
    #[test] fn slot_unit_outside_the_shared_slice_is_refused() {
        let mut f = Fake::new();
        f.job_rel = "/user.slice/c55xfake.slice/c55x-emu-3.service".to_string();
        f.write_good();
        f.expect_refused("not directly below");
    }
    #[test] fn shared_caps_are_inclusive_and_one_above_is_refused() {
        let f = Fake::new();
        // write_good puts every shared value exactly at its cap
        assert_eq!(f.check(), Ok(()));
        put(&f.slice(), "memory.max", &(18 * GIB + 1).to_string());
        f.expect_refused("shared memory.max");
        put(&f.slice(), "memory.max", &(18 * GIB).to_string());
        put(&f.slice(), "memory.high", &(17 * GIB + 1).to_string());
        f.expect_refused("shared memory.high");
        put(&f.slice(), "memory.high", &(17 * GIB).to_string());
        put(&f.slice(), "pids.max", "449");
        f.expect_refused("shared pids.max");
        put(&f.slice(), "pids.max", "448");
        put(&f.slice(), "cpu.max", "1000001 100000");
        f.expect_refused("shared cpu.max");
        put(&f.slice(), "cpu.max", "1000000 100000");
        assert_eq!(f.check(), Ok(()));
    }
    #[test] fn smaller_shared_values_are_admitted() {
        // tighter than the caps (the earlier single-slot values) is fine
        let f = Fake::new();
        put(&f.slice(), "memory.max", &(4 * GIB).to_string());
        put(&f.slice(), "memory.high", &(3 * GIB).to_string());
        put(&f.slice(), "pids.max", "64");
        put(&f.slice(), "cpu.max", "200000 100000");
        assert_eq!(f.check(), Ok(()));
    }
    #[test] fn relaxed_job_memory_is_refused() {
        let f = Fake::new();
        put(&f.job(), "memory.max", &(3 * GIB).to_string()); f.expect_refused("outside");
        put(&f.job(), "memory.max", "max"); f.expect_refused("unlimited");
    }
    #[test] fn relaxed_shared_ancestor_is_refused() {
        let f = Fake::new();
        put(&f.slice(), "memory.max", "max"); f.expect_refused("unlimited");
        put(&f.slice(), "memory.max", &(32 * GIB).to_string()); f.expect_refused("outside");
        put(&f.slice(), "memory.max", &(18 * GIB).to_string());
        put(&f.slice(), "memory.high", "max"); f.expect_refused("unlimited");
        put(&f.slice(), "memory.high", &(17 * GIB).to_string());
        put(&f.slice(), "pids.max", "max"); f.expect_refused("unlimited");
        put(&f.slice(), "pids.max", "448");
        put(&f.slice(), "cpu.max", "2000000 100000"); f.expect_refused("exceeds");
        put(&f.slice(), "cpu.max", "max 100000"); f.expect_refused("unlimited");
    }
    #[test] fn swap_must_be_zero_at_job_and_slice() {
        let f = Fake::new();
        put(&f.job(), "memory.swap.max", "1048576"); f.expect_refused("swap");
        put(&f.job(), "memory.swap.max", "0");
        put(&f.slice(), "memory.swap.max", "max"); f.expect_refused("unlimited");
        // finite but non-zero swap at the shared ancestor
        put(&f.slice(), "memory.swap.max", "1048576"); f.expect_refused("shared memory.swap.max");
    }
    #[test] fn shared_memory_high_must_be_finite_and_bounded() {
        let f = Fake::new();
        put(&f.slice(), "memory.high", &(18 * GIB).to_string());
        f.expect_refused("shared memory.high");
        put(&f.slice(), "memory.high", "0");
        f.expect_refused("shared memory.high");
    }
    #[test] fn job_task_and_cpu_limits_are_required() {
        let f = Fake::new();
        put(&f.job(), "pids.max", "max"); f.expect_refused("unlimited");
        put(&f.job(), "pids.max", "33"); f.expect_refused("outside");
        put(&f.job(), "pids.max", "32");
        put(&f.job(), "cpu.max", "max 100000"); f.expect_refused("unlimited");
        put(&f.job(), "cpu.max", "200000 100000"); f.expect_refused("exceeds");
    }
    #[test] fn local_oom_grouping_is_required() {
        let f = Fake::new(); put(&f.job(), "memory.oom.group", "0"); f.expect_refused("oom.group");
    }
    #[test] fn missing_controller_file_is_refused() {
        let f = Fake::new();
        fs::remove_file(f.job().join("memory.oom.group")).unwrap();
        f.expect_refused("missing");
        put(&f.job(), "memory.oom.group", "1");
        fs::remove_file(f.slice().join("cpu.max")).unwrap();
        f.expect_refused("missing");
    }
    #[test] fn process_rlimits_are_required() {
        let mut f = Fake::new();
        f.limits = GOOD_LIMITS.replace("600                  600", "unlimited            unlimited");
        f.expect_refused("rlimit Max cpu time");
        f.limits = GOOD_LIMITS.replace("512                  512", "1024                 1024");
        f.expect_refused("rlimit Max open files");
        f.limits = GOOD_LIMITS.replace("Max core file size        0                    0 ",
                                       "Max core file size        0                    unlimited");
        f.expect_refused("core file size");
        f.limits = GOOD_LIMITS.replace("2147483648           2147483648", "unlimited            unlimited");
        f.expect_refused("rlimit Max address space");
        f.limits = String::new();
        f.expect_refused("not readable");
    }
    #[test] fn cgroup_v1_only_and_missing_mount_are_refused() {
        let f = Fake::new();
        let r = evaluate(&Source { proc_cgroup: "12:memory:/x\n", mountinfo: "",
            proc_limits: GOOD_LIMITS, job_env: Some("emulator") });
        assert!(r.unwrap_err().contains("cgroup v2"));
        let r = evaluate(&Source { proc_cgroup: &format!("0::{}\n", f.job_rel), mountinfo: "",
            proc_limits: GOOD_LIMITS, job_env: Some("emulator") });
        assert!(r.unwrap_err().contains("no cgroup2 mount"));
    }
    #[test] fn path_with_dotdot_is_refused() {
        let f = Fake::new();
        let cg = format!("0::/user.slice/../{}/{}\n", SLICE_NAME, JOB_UNIT);
        let mi = format!("29 22 0:26 / {} rw - cgroup2 rw\n", f.root.display());
        let r = evaluate(&Source { proc_cgroup: &cg, mountinfo: &mi,
            proc_limits: GOOD_LIMITS, job_env: Some("emulator") });
        assert!(r.unwrap_err().contains("plain descending"));
    }
    #[test] fn nested_cgroup_below_the_job_is_admitted_and_mount_root_is_honored() {
        let f = Fake::new();
        // process sits in a child of the job; mount root is a sub-path
        fs::create_dir_all(f.job().join("child")).unwrap();
        let cg = format!("0::{}/child\n", f.job_rel);
        let mi = format!("29 22 0:26 / {} rw - cgroup2 rw\n", f.root.display());
        let r = evaluate(&Source { proc_cgroup: &cg, mountinfo: &mi,
            proc_limits: GOOD_LIMITS, job_env: Some("emulator") });
        assert_eq!(r, Ok(()));
        // namespaced mount whose root is /user.slice: path must be under it
        let mi2 = format!("29 22 0:26 /user.slice {} rw - cgroup2 rw\n", f.root.join("user.slice").display());
        let r = evaluate(&Source { proc_cgroup: &cg, mountinfo: &mi2,
            proc_limits: GOOD_LIMITS, job_env: Some("emulator") });
        assert_eq!(r, Ok(()));
        let cg_out = "0::/other.slice/c55x.slice/c55x-emu.service\n";
        let r = evaluate(&Source { proc_cgroup: cg_out, mountinfo: &mi2,
            proc_limits: GOOD_LIMITS, job_env: Some("emulator") });
        assert!(r.unwrap_err().contains("outside the mount root"));
    }
}
