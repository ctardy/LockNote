// Auto-Lock module
//
// Locks the application after a configurable period of inactivity.
// When triggered: auto-save if dirty -> clear editor -> clear password -> minimize to tray.
// Reset on: mouse move, key press.
//
// Dirty check (fix #035):
//   The legacy `should_lock() -> bool` API is preserved for the current
//   editor wiring (src/ui/editor/), but a richer `lock_decision() ->
//   LockDecision` method is now the preferred entry point. It distinguishes:
//     - `Wait`         : the timeout is not reached yet.
//     - `LockNow`      : timeout reached, nothing dirty -> lock without saving.
//     - `SaveThenLock` : timeout reached, content is dirty -> caller must
//                        persist first, then lock.
//
//   The dirty status is provided by an injected callback registered via
//   `set_dirty_callback`. When no callback is registered, the behaviour stays
//   backward-compatible: timeout reached -> `LockNow` (as if nothing were
//   dirty, matching the legacy bool-based behaviour).
//
// TODO (wiring, out of scope for this change):
//   src/ui/editor/ should:
//     1. Register a dirty callback at startup, e.g.
//        `autolock.set_dirty_callback(Box::new({
//             let flag = f.is_modified.clone();
//             move || *flag.borrow()
//         }))`.
//        (the closure must be Send+Sync, so a shared flag behind a Mutex or
//         AtomicBool is preferable to Rc<RefCell<_>>.)
//     2. Replace the `should_lock()` boolean branch in the autolock timer
//        handler with a `match autolock.lock_decision()` that handles
//        `SaveThenLock` by saving first, then locking.
//
//   Without that wiring, the new enum falls back to the legacy behaviour and
//   the #035 fix is inert.

use std::time::{Duration, Instant};

/// Decision returned by [`AutoLock::lock_decision`].
pub enum LockDecision {
    /// The inactivity timeout has not been reached yet.
    Wait,
    /// The timeout is reached and there is nothing dirty to save — lock
    /// immediately.
    LockNow,
    /// The timeout is reached and the editor has unsaved modifications — the
    /// caller must persist the content first, then lock.
    SaveThenLock,
}

/// Callback type: returns `true` when the editor has unsaved changes.
type DirtyCallback = Box<dyn Fn() -> bool + Send + Sync>;

/// Auto-lock state tracker.
pub struct AutoLock {
    /// Timeout in minutes (0 = disabled).
    timeout_minutes: u32,
    /// Last activity timestamp.
    last_activity: Instant,
    /// Whether auto-lock is currently armed (not locked).
    enabled: bool,
    /// Optional callback used to detect whether the editor has unsaved
    /// changes. When unset, `lock_decision()` defaults to `LockNow` once the
    /// timeout is reached (legacy behaviour).
    dirty_callback: Option<DirtyCallback>,
}

/// Default: 15 minutes.
impl Default for AutoLock {
    fn default() -> Self {
        Self::new(15)
    }
}

impl AutoLock {
    /// Creates a new auto-lock tracker with the given timeout in minutes.
    /// A timeout of 0 disables auto-lock.
    pub fn new(timeout_minutes: u32) -> Self {
        Self {
            timeout_minutes,
            last_activity: Instant::now(),
            enabled: timeout_minutes > 0,
            dirty_callback: None,
        }
    }

    /// Disabled auto-lock.
    pub fn disabled() -> Self {
        Self::new(0)
    }

    /// Reset the inactivity timer (call on mouse move / key press).
    pub fn reset(&mut self) {
        self.last_activity = Instant::now();
    }

    /// Register a dirty-state callback. The callback is invoked by
    /// `lock_decision()` to decide between `LockNow` and `SaveThenLock`.
    ///
    /// Calling this twice replaces the previous callback.
    pub fn set_dirty_callback(&mut self, cb: DirtyCallback) {
        self.dirty_callback = Some(cb);
    }

    /// Remove a previously registered dirty callback (reverts to legacy
    /// behaviour where `lock_decision()` never returns `SaveThenLock`).
    pub fn clear_dirty_callback(&mut self) {
        self.dirty_callback = None;
    }

    /// Legacy boolean API (do NOT remove — still consumed by
    /// `src/ui/editor/`). Returns `true` when the autolock should fire.
    ///
    /// New callers should prefer [`AutoLock::lock_decision`], which also
    /// tells them whether a save is needed before locking (fix #035).
    pub fn should_lock(&self) -> bool {
        matches!(
            self.lock_decision(),
            LockDecision::LockNow | LockDecision::SaveThenLock
        )
    }

    /// Decide what the autolock timer should do now.
    ///
    /// See [`LockDecision`] for the semantics of each variant. This is the
    /// preferred entry point; `should_lock()` is kept only for backward
    /// compatibility with the current editor wiring.
    pub fn lock_decision(&self) -> LockDecision {
        if !self.enabled || self.timeout_minutes == 0 {
            return LockDecision::Wait;
        }
        let timeout = Duration::from_secs(self.timeout_minutes as u64 * 60);
        if self.last_activity.elapsed() < timeout {
            return LockDecision::Wait;
        }
        // Timeout reached: consult the dirty callback when available.
        match &self.dirty_callback {
            Some(cb) if cb() => LockDecision::SaveThenLock,
            _ => LockDecision::LockNow,
        }
    }

    /// Get the timeout in minutes.
    pub fn timeout_minutes(&self) -> u32 {
        self.timeout_minutes
    }

    /// Set a new timeout (0 = disabled).
    pub fn set_timeout(&mut self, minutes: u32) {
        self.timeout_minutes = minutes;
        self.enabled = minutes > 0;
        self.reset();
    }

    /// Timer interval in milliseconds for the UI timer, or None if disabled.
    pub fn timer_interval_ms(&self) -> Option<u32> {
        if self.timeout_minutes == 0 {
            None
        } else {
            Some(self.timeout_minutes * 60 * 1000)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_15_minutes() {
        let al = AutoLock::default();
        assert_eq!(al.timeout_minutes(), 15);
        assert!(!al.should_lock());
        assert!(matches!(al.lock_decision(), LockDecision::Wait));
    }

    #[test]
    fn disabled_never_locks() {
        let al = AutoLock::disabled();
        assert_eq!(al.timeout_minutes(), 0);
        assert!(!al.should_lock());
        assert!(matches!(al.lock_decision(), LockDecision::Wait));
    }

    #[test]
    fn set_timeout_changes_value() {
        let mut al = AutoLock::new(5);
        assert_eq!(al.timeout_minutes(), 5);
        al.set_timeout(30);
        assert_eq!(al.timeout_minutes(), 30);
        al.set_timeout(0);
        assert!(!al.should_lock());
    }

    #[test]
    fn timer_interval() {
        let al = AutoLock::new(15);
        assert_eq!(al.timer_interval_ms(), Some(900_000));
        let al = AutoLock::disabled();
        assert_eq!(al.timer_interval_ms(), None);
    }

    #[test]
    fn reset_prevents_lock() {
        let mut al = AutoLock::new(1);
        al.reset();
        assert!(!al.should_lock());
    }

    #[test]
    fn dirty_callback_chooses_save_then_lock() {
        // Construct a tracker whose timeout is already elapsed.
        let mut al = AutoLock::new(1);
        al.last_activity = Instant::now() - Duration::from_secs(120);

        // Without callback -> LockNow.
        assert!(matches!(al.lock_decision(), LockDecision::LockNow));
        assert!(al.should_lock());

        // With dirty=true -> SaveThenLock.
        al.set_dirty_callback(Box::new(|| true));
        assert!(matches!(al.lock_decision(), LockDecision::SaveThenLock));
        assert!(al.should_lock());

        // With dirty=false -> LockNow.
        al.set_dirty_callback(Box::new(|| false));
        assert!(matches!(al.lock_decision(), LockDecision::LockNow));

        // Clearing reverts to legacy behaviour.
        al.clear_dirty_callback();
        assert!(matches!(al.lock_decision(), LockDecision::LockNow));
    }

    // Regression guard for issue #035: when the timeout has NOT elapsed we must
    // return Wait regardless of the dirty state — the dirty callback must only
    // influence the decision *at* the timeout boundary, never trigger an
    // autosave earlier than scheduled.
    #[test]
    fn dirty_but_within_timeout_returns_wait() {
        let mut al = AutoLock::new(5);
        al.set_dirty_callback(Box::new(|| true));
        assert!(matches!(al.lock_decision(), LockDecision::Wait));
        assert!(!al.should_lock());
    }

    // Disabled autolock must never fire, even with an elapsed last_activity
    // and a dirty-true callback. Guards against accidental re-enablement when
    // users set timeout=0.
    #[test]
    fn disabled_autolock_ignores_dirty_callback() {
        let mut al = AutoLock::disabled();
        // Instant::now() - Duration panics if the monotonic clock hasn't
        // advanced that far yet (freshly booted CI runners). 120 s is well
        // beyond any disabled-autolock timeout and never underflows.
        al.last_activity = Instant::now() - Duration::from_secs(120);
        al.set_dirty_callback(Box::new(|| true));
        assert!(matches!(al.lock_decision(), LockDecision::Wait));
        assert!(!al.should_lock());
    }

    // The dirty callback must NOT be invoked before the timeout fires,
    // otherwise every timer tick would call into the editor state (which may
    // lock a mutex or perform IO). Use a counter to confirm zero calls while
    // the timer is still within its window.
    #[test]
    fn dirty_callback_not_invoked_before_timeout() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let calls = Arc::new(AtomicUsize::new(0));
        let calls_cb = Arc::clone(&calls);

        let mut al = AutoLock::new(10);
        al.set_dirty_callback(Box::new(move || {
            calls_cb.fetch_add(1, Ordering::SeqCst);
            true
        }));

        // Poll lock_decision a few times while timer is still far from expiry.
        for _ in 0..5 {
            assert!(matches!(al.lock_decision(), LockDecision::Wait));
        }

        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "dirty callback must not be consulted before the timeout expires"
        );
    }

    // When the timeout has expired, lock_decision must invoke the dirty
    // callback exactly once per query. Avoids silent double-reads of editor
    // state from the autolock timer.
    #[test]
    fn dirty_callback_invoked_once_per_decision() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let calls = Arc::new(AtomicUsize::new(0));
        let calls_cb = Arc::clone(&calls);

        let mut al = AutoLock::new(1);
        al.last_activity = Instant::now() - Duration::from_secs(120);
        al.set_dirty_callback(Box::new(move || {
            calls_cb.fetch_add(1, Ordering::SeqCst);
            false
        }));

        let _ = al.lock_decision();
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        let _ = al.lock_decision();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    // set_timeout must reset the activity clock, otherwise raising the
    // timeout immediately fires on a previously-elapsed last_activity.
    #[test]
    fn set_timeout_resets_activity_clock() {
        let mut al = AutoLock::new(1);
        al.last_activity = Instant::now() - Duration::from_secs(120);
        assert!(al.should_lock()); // confirm it was armed

        al.set_timeout(30); // user changes the setting -> clock reset
        assert!(
            matches!(al.lock_decision(), LockDecision::Wait),
            "set_timeout must reset last_activity so the new window starts fresh"
        );
    }

    // Switching from dirty=true to dirty=false across two queries must update
    // the decision without keeping a cached "dirty" from the first call.
    #[test]
    fn dirty_callback_reads_latest_state_each_call() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        let dirty = Arc::new(AtomicBool::new(true));
        let dirty_cb = Arc::clone(&dirty);

        let mut al = AutoLock::new(1);
        al.last_activity = Instant::now() - Duration::from_secs(120);
        al.set_dirty_callback(Box::new(move || dirty_cb.load(Ordering::SeqCst)));

        assert!(matches!(al.lock_decision(), LockDecision::SaveThenLock));

        dirty.store(false, Ordering::SeqCst);
        assert!(matches!(al.lock_decision(), LockDecision::LockNow));
    }
}
