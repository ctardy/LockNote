// Persistence lock
//
// Global singleton mutex that serializes any operation mutating the on-disk
// persistent state (unlock, autosave, autolock save-then-lock, manual save).
// This avoids race conditions such as:
//   - autosave writing at the same time as autolock clears the in-memory state
//   - unlock rehydrating while autosave is still flushing
//
// Usage:
//   - Callers that MUST wait (unlock, autolock) use `acquire()` (blocking).
//   - Callers that can skip a cycle (autosave) use `try_acquire()` and simply
//     skip the tick if another thread holds the lock.
//
// Poison recovery: the mutex guards no real data (unit type), so if a thread
// panics while holding the lock, recovering the inner guard is safe.

use std::sync::{Mutex, MutexGuard, OnceLock, TryLockError};

static PERSISTENCE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn get_lock() -> &'static Mutex<()> {
    PERSISTENCE_LOCK.get_or_init(|| Mutex::new(()))
}

/// Blocking acquire of the persistence lock.
///
/// Poison recovery: the mutex protects a `()` value, so there is no invalid
/// inner state to worry about; we transparently recover the guard.
pub fn acquire() -> MutexGuard<'static, ()> {
    match get_lock().lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Try to acquire without blocking.
///
/// Returns `None` if another thread currently holds the lock. Poisoned state
/// is recovered transparently (see `acquire` for the rationale).
pub fn try_acquire() -> Option<MutexGuard<'static, ()>> {
    match get_lock().try_lock() {
        Ok(g) => Some(g),
        Err(TryLockError::Poisoned(p)) => Some(p.into_inner()),
        Err(TryLockError::WouldBlock) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // These tests all exercise the same global `PERSISTENCE_LOCK` singleton,
    // so cargo's parallel test runner would race them against each other. We
    // serialize the whole suite with a test-only mutex — same pattern as the
    // i18n tests that share CURRENT_LANG.
    use std::sync::{Mutex, MutexGuard};
    static SERIAL: Mutex<()> = Mutex::new(());
    fn serial() -> MutexGuard<'static, ()> {
        SERIAL.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn acquire_then_try_acquire_blocks() {
        let _s = serial();
        let _g = acquire();
        assert!(try_acquire().is_none());
    }

    #[test]
    fn try_acquire_succeeds_when_free() {
        let _s = serial();
        {
            let g = try_acquire();
            drop(g);
        }
        let g = try_acquire();
        assert!(g.is_some());
    }

    #[test]
    fn sequential_acquires_release() {
        let _s = serial();
        {
            let _g = acquire();
        }
        let _g2 = acquire();
    }

    // Mutual exclusion under contention: the three persistence-mutating
    // workflows (autosave tick, autolock save-then-lock, unlock decrypt) must
    // never run simultaneously. We simulate each as a worker and assert that
    // the observed-concurrent-holders counter never exceeds 1.
    #[test]
    fn stress_mutual_exclusion_three_roles() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::thread;
        use std::time::Duration;

        let _s = serial();

        // Drain any leftover guard from previous tests.
        while try_acquire().is_none() {
            thread::yield_now();
        }

        let active = Arc::new(AtomicUsize::new(0));
        let max_seen = Arc::new(AtomicUsize::new(0));
        let try_skips = Arc::new(AtomicUsize::new(0));

        let spawn_worker = |iterations: u32, use_try: bool| {
            let active = Arc::clone(&active);
            let max_seen = Arc::clone(&max_seen);
            let try_skips = Arc::clone(&try_skips);
            thread::spawn(move || {
                for _ in 0..iterations {
                    let guard = if use_try {
                        match try_acquire() {
                            Some(g) => g,
                            None => {
                                try_skips.fetch_add(1, Ordering::Relaxed);
                                continue;
                            }
                        }
                    } else {
                        acquire()
                    };

                    let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                    max_seen.fetch_max(now, Ordering::SeqCst);
                    thread::sleep(Duration::from_micros(50));
                    active.fetch_sub(1, Ordering::SeqCst);
                    drop(guard);
                }
            })
        };

        let handles = vec![
            spawn_worker(40, false), // unlock role: must wait
            spawn_worker(40, false), // autolock role: must wait
            spawn_worker(80, true),  // autosave role: skips when busy
        ];
        for h in handles {
            h.join().expect("worker panicked");
        }

        assert_eq!(
            max_seen.load(Ordering::SeqCst),
            1,
            "mutual exclusion violated: two threads held the persistence lock simultaneously"
        );
        // try_acquire must have skipped at least once under contention — otherwise
        // the test did not exercise the contended path.
        assert!(
            try_skips.load(Ordering::Relaxed) > 0,
            "no contention observed: try_acquire never skipped, test is not meaningful"
        );
    }

    // try_acquire must not deadlock when the caller already released its guard.
    // Also validates that a held guard blocks try_acquire from another thread.
    #[test]
    fn try_acquire_blocks_when_held_by_other_thread() {
        use std::sync::mpsc;
        use std::thread;

        let _s = serial();

        while try_acquire().is_none() {
            thread::yield_now();
        }

        let (ready_tx, ready_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel::<()>();

        let holder = thread::spawn(move || {
            let _g = acquire();
            ready_tx.send(()).unwrap();
            // Keep holding until the main thread signals us to release.
            release_rx.recv().unwrap();
        });

        ready_rx.recv().unwrap();
        assert!(
            try_acquire().is_none(),
            "try_acquire must return None while another thread holds the guard"
        );

        release_tx.send(()).unwrap();
        holder.join().unwrap();

        // Now the guard is released; try_acquire must succeed.
        let g = try_acquire();
        assert!(g.is_some(), "try_acquire must succeed after the other thread releases");
    }
}
