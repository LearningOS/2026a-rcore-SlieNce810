//! Synchronization and interior mutability primitives

mod condvar;
mod mutex;
mod semaphore;
mod up;

use core::sync::atomic::{AtomicBool, Ordering};

pub use condvar::Condvar;
pub use mutex::{Mutex, MutexBlocking, MutexSpin};
pub use semaphore::Semaphore;
pub use up::UPSafeCell;

static DEADLOCK_DETECT: AtomicBool = AtomicBool::new(false);

/// Enable or disable deadlock detection.
pub fn set_deadlock_detect(enabled: bool) {
    DEADLOCK_DETECT.store(enabled, Ordering::Relaxed);
}

/// Whether deadlock detection is enabled.
pub fn deadlock_detect_enabled() -> bool {
    DEADLOCK_DETECT.load(Ordering::Relaxed)
}
