//! The containment guard of the core and the trace length cap (from
//! tests/test_c55x_emu_containment.py, scope A).
//! The tests in `inside` run in an emulator job (scripts/cargo-test-contained runs this
//! binary on one thread: some tests change the job's limits and environment and restore
//! them). The tests in `outside` check the refusal and run only by hand, outside the
//! launcher: cargo test --test containment -- --ignored outside
//! The launcher itself is tested in tests/test_c55x_launcher.py.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::thread::sleep;
use std::time::Duration;

use c55x_core::core::{Core, MAX_TRACE_LEN};
use c55x_core::guard;
use c55x_core::icache::PcFilter;
use common::{message, run, try_run, try_step};

const GIB: u64 = 1 << 30;
const EMU_UNITS: [&str; 8] = ["c55x-emu.service", "c55x-emu-1.service", "c55x-emu-2.service", "c55x-emu-3.service",
                              "c55x-emu-4.service", "c55x-emu-5.service", "c55x-emu-6.service", "c55x-emu-7.service"];
/// The revalidation interval of guard.rs is 50 ms.
const PAST_REVALIDATION: Duration = Duration::from_millis(100);

fn machine(name: &str) -> Core {
    fixture!("containment").core(&format!("t_{}", name))
}

fn job_cgroup() -> PathBuf {
    let cgroup = fs::read_to_string("/proc/self/cgroup").unwrap();
    let rel = cgroup.lines().find_map(|l| l.strip_prefix("0::")).unwrap().trim();
    Path::new("/sys/fs/cgroup").join(rel.trim_start_matches('/'))
}

fn read(dir: &Path, name: &str) -> String {
    fs::read_to_string(dir.join(name)).unwrap().trim().to_owned()
}

mod outside {
    use super::*;

    #[test]
    #[ignore = "run outside the launcher"]
    fn direct_construction_is_refused() {
        let e = guard::guard().unwrap_err();
        assert!(e.contains("resource policy"), "{}", e);
        assert!(e.contains("bin/c55x-contained"), "{}", e);
    }

    #[test]
    #[ignore = "run outside the launcher"]
    fn fake_environment_marker_alone_is_refused() {
        std::env::set_var(c55x_core::policy::ENV_JOB, "emulator");
        let r = guard::guard();
        std::env::remove_var(c55x_core::policy::ENV_JOB);
        assert!(r.is_err(), "the marker alone admitted the core");
    }
}

mod inside {
    use super::*;

    #[test]
    fn core_constructs_and_runs() {
        let mut cpu = machine("core_constructs_and_runs");
        run(&mut cpu);
        assert_eq!(cpu.st.ac[0], 0x100 << 16);
    }

    #[test]
    fn kernel_values_seen_by_the_job() {
        let job = job_cgroup();
        let slc = job.parent().unwrap();
        assert!(EMU_UNITS.contains(&job.file_name().unwrap().to_str().unwrap()));
        assert_eq!(slc.file_name().unwrap(), "c55x.slice");
        let mem: u64 = read(&job, "memory.max").parse().unwrap();
        assert!(0 < mem && mem <= 2 * GIB);
        assert_eq!(read(&job, "memory.swap.max"), "0");
        assert_eq!(read(&job, "pids.max"), "32");
        assert_eq!(read(&job, "memory.oom.group"), "1");
        assert_eq!(read(&job, "cpu.max"), "100000 100000");
        assert_eq!(read(slc, "memory.max"), (18 * GIB).to_string());
        assert_eq!(read(slc, "memory.high"), (17 * GIB).to_string());
        assert_eq!(read(slc, "memory.swap.max"), "0");
        assert_eq!(read(slc, "pids.max"), "448");
        assert_eq!(read(slc, "cpu.max"), "1000000 100000");
    }

    #[test]
    fn policy_is_revalidated_when_the_job_ceiling_is_relaxed() {
        // A core built under valid policy must stop executing once the group is relaxed.
        // The job cgroup is a bounded test group: the shared ancestor keeps its 18 GiB cap
        // throughout, and the value is restored.
        let mut cpu = machine("two_steps");
        try_step(&mut cpu).unwrap();                    // valid policy: executes
        let job = job_cgroup();
        let original = read(&job, "memory.max");
        fs::write(job.join("memory.max"), "max\n").unwrap();
        sleep(PAST_REVALIDATION);
        let relaxed = (try_step(&mut cpu), try_run(&mut cpu));
        fs::write(job.join("memory.max"), format!("{}\n", original)).unwrap();
        let e = relaxed.0.unwrap_err();
        assert!(e.contains("unlimited"), "{}", e);
        assert!(relaxed.1.is_err());
        try_step(&mut cpu).unwrap();                    // restored: executes again
    }

    #[test]
    fn policy_is_revalidated_when_the_marker_disappears() {
        let mut cpu = machine("two_steps");
        try_step(&mut cpu).unwrap();
        let saved = std::env::var(c55x_core::policy::ENV_JOB).unwrap();
        std::env::remove_var(c55x_core::policy::ENV_JOB);
        sleep(PAST_REVALIDATION);
        let r = try_step(&mut cpu);
        std::env::set_var(c55x_core::policy::ENV_JOB, saved);
        assert!(r.is_err());
        try_step(&mut cpu).unwrap();
    }

    #[test]
    fn every_execution_entry_of_an_existing_core_is_guarded() {
        // run / step / call / ret / advance execute guest code or advance guest time; the
        // core checks the containment at each of them
        let mut cpu = machine("core_constructs_and_runs");
        let entries: [(&str, fn(&mut Core) -> Result<(), String>); 5] = [
            ("run", |c| c.run(1, &PcFilter::default(), None).map(|_| ()).map_err(message)),
            ("step", |c| c.step().map_err(message)),
            ("call", |c| c.call(0x20000, 0).map(|_| ()).map_err(message)),
            ("ret", |c| c.ret().map(|_| ()).map_err(message)),
            ("advance", |c| c.advance(1).map_err(message)),
        ];
        let saved = std::env::var(c55x_core::policy::ENV_JOB).unwrap();
        std::env::remove_var(c55x_core::policy::ENV_JOB);
        sleep(PAST_REVALIDATION);
        let refused: Vec<(&str, bool)> = entries.iter().map(|(name, f)| (*name, f(&mut cpu).is_err())).collect();
        std::env::set_var(c55x_core::policy::ENV_JOB, saved);
        assert!(refused.iter().all(|(_, r)| *r), "{:?}", refused);
    }

    #[test]
    fn trace_length_cap_is_an_explicit_error() {
        let mut cpu = machine("nop");
        assert!(cpu.set_trace(MAX_TRACE_LEN + 1).is_err());
        assert!(cpu.set_trace(MAX_TRACE_LEN).is_ok());  // the maximum itself is fine
        assert!(cpu.set_trace(0).is_ok());
        assert_eq!(MAX_TRACE_LEN, 1 << 22);
    }

    #[test]
    fn descendants_share_the_task_limit() {
        let mut children: Vec<Child> = Vec::new();
        let mut refused = false;
        for _ in 0..64 {                                // the job limit is 32 tasks
            match Command::new("sleep").arg("30").spawn() {
                Ok(c) => children.push(c),
                Err(_) => { refused = true; break; }
            }
        }
        let n = children.len();
        for c in &mut children {                        // exactly the processes we started
            let _ = c.kill();
        }
        for c in &mut children {
            let _ = c.wait();
        }
        assert!(refused, "64 children started");
        assert!(n < 32 && n > 10, "{} children", n);
    }

    #[test]
    fn small_tmpfs_allocation_is_charged_to_the_job() {
        let job = job_cgroup();
        let shmem = || read(&job, "memory.stat").lines()
            .find_map(|l| l.strip_prefix("shmem ")).map_or(0, |v| v.parse::<u64>().unwrap());
        let path = PathBuf::from(format!("/dev/shm/c55x-test-{}", std::process::id()));
        let before = shmem();
        fs::write(&path, vec![0u8; 8 << 20]).unwrap();  // 8 MiB, deliberately tiny
        let charged = shmem() - before;
        fs::remove_file(&path).unwrap();
        assert!(charged >= 7 << 20, "{} bytes charged", charged);
    }
}
