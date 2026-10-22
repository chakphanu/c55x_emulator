//! The containment guard at core construction and execution entries (policy.rs holds
//! the check itself). Every host calls it: the core must not run outside the managed
//! cgroup.

use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// When the containment check last succeeded in this process.
static LAST_OK: Mutex<Option<Instant>> = Mutex::new(None);

/// A relaxed policy or a moved process is detected on the first execution
/// entry after this interval. The full check reads about 14 cgroup and /proc
/// files (~120 us), which would make single-stepping 25x slower if repeated on
/// every call; construction and the first entry after any failure always
/// check in full.
const REVALIDATE_EVERY: Duration = Duration::from_millis(50);

fn last_ok() -> MutexGuard<'static, Option<Instant>> {
    LAST_OK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Full containment check (constructor, and the first entry after a failure).
pub fn guard() -> Result<(), String> {
    match crate::policy::check_containment() {
        Ok(()) => {
            *last_ok() = Some(Instant::now());
            Ok(())
        }
        Err(e) => {
            *last_ok() = None;
            Err(e)
        }
    }
}

/// Containment check at an execution entry of an existing core.
pub fn guard_entry() -> Result<(), String> {
    if matches!(*last_ok(), Some(t) if t.elapsed() < REVALIDATE_EVERY) {
        return Ok(());
    }
    guard()
}
