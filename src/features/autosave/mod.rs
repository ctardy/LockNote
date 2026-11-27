// Auto-Save module
//
// Periodically saves the document if any tab has been modified.
// Timer interval is configurable (0 = disabled, default 30 seconds).
//
// Persistence mutex (fix #036):
//   Autosave must NEVER write concurrently with unlock or autolock. To avoid
//   blocking the UI thread when another persistence operation is running, the
//   autosave tick uses `AutoSave::acquire_lock()` which is a thin, non-blocking
//   wrapper around `features::persistence_lock::try_acquire()`.
//
//   Expected usage from the UI timer / editor:
//
//   ```ignore
//   if autosave.should_save() {
//       if let Some(_guard) = AutoSave::acquire_lock() {
//           // Safe to persist: no other thread holds the persistence lock.
//           perform_save();
//           autosave.mark_saved();
//           // _guard drops here, releasing the mutex.
//       } else {
//           // Another thread (unlock, autolock, manual save) is mid-flight;
//           // skip this tick and retry at the next one.
//       }
//   }
//   ```

use std::time::{Duration, Instant};

use std::sync::MutexGuard;

/// Auto-save state tracker.
pub struct AutoSave {
    /// Interval in seconds (0 = disabled).
    interval_seconds: u32,
    /// Last save timestamp.
    last_save: Instant,
}

/// Default: 30 seconds.
impl Default for AutoSave {
    fn default() -> Self {
        Self::new(30)
    }
}

impl AutoSave {
    /// Creates a new auto-save tracker with the given interval in seconds.
    pub fn new(interval_seconds: u32) -> Self {
        Self {
            interval_seconds,
            last_save: Instant::now(),
        }
    }

    /// Disabled auto-save.
    pub fn disabled() -> Self {
        Self::new(0)
    }

    /// Check whether enough time has passed for an auto-save.
    pub fn should_save(&self) -> bool {
        if self.interval_seconds == 0 {
            return false;
        }
        let interval = Duration::from_secs(self.interval_seconds as u64);
        self.last_save.elapsed() >= interval
    }

    /// Mark that a save just occurred.
    pub fn mark_saved(&mut self) {
        self.last_save = Instant::now();
    }

    /// Get the interval in seconds.
    pub fn interval_seconds(&self) -> u32 {
        self.interval_seconds
    }

    /// Set a new interval (0 = disabled).
    pub fn set_interval(&mut self, seconds: u32) {
        self.interval_seconds = seconds;
        self.mark_saved();
    }

    /// Timer interval in milliseconds for the UI timer, or None if disabled.
    pub fn timer_interval_ms(&self) -> Option<u32> {
        if self.interval_seconds == 0 {
            None
        } else {
            Some(self.interval_seconds * 1000)
        }
    }

    /// Try to acquire the global persistence lock without blocking.
    ///
    /// Returns `None` when another persistence operation (unlock, autolock,
    /// manual save) is currently running. In that case the autosave tick
    /// should simply be skipped and retried at the next interval.
    ///
    /// The returned guard MUST be kept alive for the whole duration of the
    /// save and then dropped to release the mutex.
    pub fn acquire_lock() -> Option<MutexGuard<'static, ()>> {
        crate::features::persistence_lock::try_acquire()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_30_seconds() {
        let asv = AutoSave::default();
        assert_eq!(asv.interval_seconds(), 30);
    }

    #[test]
    fn disabled_never_saves() {
        let asv = AutoSave::disabled();
        assert!(!asv.should_save());
    }

    #[test]
    fn fresh_tracker_not_ready() {
        let asv = AutoSave::new(60);
        assert!(!asv.should_save());
    }

    #[test]
    fn mark_saved_resets_timer() {
        let mut asv = AutoSave::new(1);
        asv.mark_saved();
        assert!(!asv.should_save());
    }

    #[test]
    fn timer_interval() {
        let asv = AutoSave::new(30);
        assert_eq!(asv.timer_interval_ms(), Some(30_000));
        let asv = AutoSave::disabled();
        assert_eq!(asv.timer_interval_ms(), None);
    }

    #[test]
    fn acquire_lock_returns_some_when_free() {
        // Same global lock as the persistence_lock tests: serialize with them.
        let _s = crate::features::persistence_lock::test_serial();
        // Drop any stale guard first.
        {
            let g = AutoSave::acquire_lock();
            drop(g);
        }
        let g = AutoSave::acquire_lock();
        assert!(g.is_some());
    }
}
