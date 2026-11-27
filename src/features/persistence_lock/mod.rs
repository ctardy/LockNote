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
    //
    // Contention on the autosave (try_acquire) path is forced, not left to the
    // scheduler (#37): the test thread holds the lock while the autosave
    // worker runs its first FORCED_ROUNDS rounds, so each of them must skip,
    // whatever the machine load. The lock is then released and the three
    // workers run freely for the stress part.
    #[test]
    fn stress_mutual_exclusion_three_roles() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::mpsc;
        use std::thread;
        use std::time::Duration;

        const FORCED_ROUNDS: usize = 20;

        let _s = serial();

        let active = Arc::new(AtomicUsize::new(0));
        let max_seen = Arc::new(AtomicUsize::new(0));

        // Simulated critical section, run only while holding a guard.
        fn critical_section(active: &AtomicUsize, max_seen: &AtomicUsize) {
            let now = active.fetch_add(1, Ordering::SeqCst) + 1;
            max_seen.fetch_max(now, Ordering::SeqCst);
            thread::sleep(Duration::from_micros(50));
            active.fetch_sub(1, Ordering::SeqCst);
        }

        // The test thread is a holder too: it counts itself in `active`, so a
        // worker entering while it holds the lock raises `max_seen` to 2.
        let held = acquire();
        let now = active.fetch_add(1, Ordering::SeqCst) + 1;
        max_seen.fetch_max(now, Ordering::SeqCst);

        let spawn_waiting_worker = |iterations: u32| {
            let active = Arc::clone(&active);
            let max_seen = Arc::clone(&max_seen);
            thread::spawn(move || {
                for _ in 0..iterations {
                    let guard = acquire();
                    critical_section(&active, &max_seen);
                    drop(guard);
                }
            })
        };

        let (forced_tx, forced_rx) = mpsc::channel::<usize>();
        let autosave = {
            let active = Arc::clone(&active);
            let max_seen = Arc::clone(&max_seen);
            thread::spawn(move || {
                // Forced phase: the test thread holds the lock.
                let mut forced_skips = 0;
                for _ in 0..FORCED_ROUNDS {
                    match try_acquire() {
                        Some(guard) => {
                            critical_section(&active, &max_seen);
                            drop(guard);
                        }
                        None => forced_skips += 1,
                    }
                }
                forced_tx.send(forced_skips).expect("test thread gone");
                // Free phase: contention depends on scheduling, skips are
                // allowed but not required.
                for _ in 0..80 {
                    if let Some(guard) = try_acquire() {
                        critical_section(&active, &max_seen);
                        drop(guard);
                    }
                }
            })
        };

        let handles = vec![
            spawn_waiting_worker(40), // unlock role: must wait
            spawn_waiting_worker(40), // autolock role: must wait
        ];

        // Bounded wait: a try_acquire that blocks instead of skipping would
        // otherwise hang the test forever.
        let forced_skips = forced_rx
            .recv_timeout(Duration::from_secs(60))
            .expect("autosave worker did not finish its rounds while the lock was held: try_acquire blocked");
        active.fetch_sub(1, Ordering::SeqCst);
        drop(held);

        autosave.join().expect("autosave worker panicked");
        for h in handles {
            h.join().expect("worker panicked");
        }

        assert_eq!(
            forced_skips, FORCED_ROUNDS,
            "try_acquire must skip every round while another holder has the lock"
        );
        assert_eq!(
            max_seen.load(Ordering::SeqCst),
            1,
            "mutual exclusion violated: two threads held the persistence lock simultaneously"
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
