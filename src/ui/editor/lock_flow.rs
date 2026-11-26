// The lock and close sequences, behind a trait (pass 7).
//
// Passes 5-6 tested the pure step functions only; the defects were in how
// they were chained (a committed auto lock turned manual, "Keep open" left
// the curtain up, the stuck-close offer cancelled itself, a queued request
// replayed under the persistence guard). The sequences now live here and
// run against `LockSteps`: `FormLockSteps` (modal_guard.rs) on the real
// window, a recording fake in the tests below, which drive whole sequences.

#[allow(unused_imports)]
use super::*;
use super::modal_guard::{close_step, lock_goes_on, lock_step, merged_manual, CloseStep, Curtain, LockStep};

/// A lock waiting for dialogs to return.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct PendingLock {
    /// Who asked: the Lock button / Ctrl+L (true) or the inactivity.
    pub(super) manual: bool,
    /// The curtain is up for it: it will lock even if the user comes back
    /// (a mouse move cannot lift it). Not the same as `manual`.
    pub(super) committed: bool,
}

/// What the sequences need from the window.
pub(super) trait LockSteps {
    fn modal_open(&self) -> bool;
    /// One of LockNote's own decision boxes is shown (the lock's box, the
    /// stuck-close offer, the close prompt, the save-failed question): never
    /// cancelled, never hidden, and nothing is cancelled under it.
    fn own_box_open(&self) -> bool;
    /// Ask the other dialogs to close as No / Cancel.
    fn cancel_dialogs(&mut self);
    fn locked(&self) -> bool;
    fn curtain(&self) -> Curtain;
    fn show_curtain(&mut self, mode: Curtain);
    /// Lift the curtain and give the full layout back.
    fn hide_curtain(&mut self);
    /// The inactivity is still there (auto lock).
    fn still_due(&self) -> bool;
    fn modified(&self) -> bool;
    fn save(&mut self) -> Result<(), String>;
    /// Manual lock after a failed save: true = "Lock without saving".
    fn ask_lock_without_saving(&mut self, reason: &str) -> bool;
    fn set_save_failed_reason(&mut self, reason: String);
    /// The lock itself (epoch, keys, wipe, lock screen).
    fn lock_now(&mut self);
    fn reset_autolock(&mut self);
    fn pending(&self) -> Option<PendingLock>;
    fn set_pending(&mut self, p: Option<PendingLock>);
    /// The lock's own box is up: lock requests are queued.
    fn in_box(&self) -> bool;
    fn set_in_box(&mut self, v: bool);
    /// A manual lock asked during the lock's box.
    fn queue_manual(&mut self);
    fn take_queued_manual(&mut self) -> bool;
}

/// A lock request (timer due, Lock button, Ctrl+L).
pub(super) fn request<S: LockSteps>(s: &mut S, manual: bool) {
    // The lock's own box: queued, run after it (never cancelled).
    if s.in_box() {
        if manual {
            s.queue_manual();
        }
        return;
    }
    if !s.modal_open() {
        run_lock(s, manual, false);
        return;
    }
    // One pending lock; a manual request wins. Under one of LockNote's own
    // decision boxes nothing is cancelled: the lock waits for it.
    let mut p = s.pending().unwrap_or(PendingLock { manual, committed: false });
    p.manual = merged_manual(p.manual, manual);
    s.set_pending(Some(p));
    if !s.own_box_open() {
        s.cancel_dialogs();
    }
}

/// A tick of a pending lock; `waited` since the request, `since_cancel`
/// since the dialogs were last asked to close. Returns true when the
/// dialogs were asked to close again (the caller restarts `since_cancel`).
pub(super) fn tick<S: LockSteps>(s: &mut S, waited: std::time::Duration, since_cancel: std::time::Duration) -> bool {
    let Some(mut p) = s.pending() else { return false };
    match lock_step(s.modal_open(), since_cancel, waited, p.committed) {
        LockStep::Wait => false,
        LockStep::CancelAgain => {
            if s.own_box_open() {
                return false;
            }
            s.cancel_dialogs();
            true
        }
        LockStep::ShowCurtain => {
            // Committed (a mouse move cannot lift it), but its origin stays:
            // an auto lock stays automatic (finding 1, pass 7).
            p.committed = true;
            s.set_pending(Some(p));
            s.show_curtain(Curtain::Waiting);
            false
        }
        LockStep::Run => {
            s.set_pending(None);
            run_lock(s, p.manual, p.committed);
            false
        }
    }
}

/// The lock once no dialog is left (or at once): save, then lock, ask, or
/// hide until the password. Runs under the persistence guard: it never
/// replays a request itself, a queued manual lock becomes a pending one for
/// the next tick.
pub(super) fn run_lock<S: LockSteps>(s: &mut S, manual: bool, committed: bool) {
    if s.locked() {
        return;
    }
    // A committed lock runs even if the user came back; an auto lock that
    // is not committed is dropped then, and any curtain lifted.
    if !lock_goes_on(manual || committed, s.still_due()) {
        eprintln!("INFO lock: pending auto lock dropped, the user is active again");
        if s.curtain() == Curtain::Waiting {
            s.hide_curtain();
        }
        return;
    }
    let modified = s.modified();
    let saved = if modified { s.save() } else { Ok(()) };
    let reason = saved.as_ref().err().cloned().unwrap_or_default();
    match failed_lock_save_step(modified, saved.is_ok(), manual) {
        FailedLockSave::Proceed => s.lock_now(),
        FailedLockSave::AskManual => {
            eprintln!("WARN lock: save before a manual lock failed, asking");
            s.set_in_box(true);
            let lock_anyway = s.ask_lock_without_saving(&reason);
            s.set_in_box(false);
            // Not replayed here (under the persistence guard, finding 5): a
            // manual lock asked during the box is pending for the next tick.
            if s.take_queued_manual() && !lock_anyway {
                s.set_pending(Some(PendingLock { manual: true, committed: false }));
            }
            if !lock_anyway {
                eprintln!("INFO lock: kept open after the failed save");
                // Every exit that does not lock leaves no curtain (finding 2).
                if s.curtain() != Curtain::None {
                    s.hide_curtain();
                }
                s.reset_autolock();
                return;
            }
            eprintln!("WARN lock: locking without saving (user choice)");
            s.lock_now();
        }
        FailedLockSave::HideUntilPassword => {
            if s.curtain() != Curtain::SaveFailed {
                eprintln!("WARN lock: save before an automatic lock failed, note hidden until the password");
            }
            s.set_save_failed_reason(reason);
            s.show_curtain(Curtain::SaveFailed);
        }
    }
}

/// A tick of a pending close. `asked`: the stuck-close offer was shown.
pub(super) fn close_tick(modal_open: bool, own_box: bool, since_cancel: std::time::Duration, waited: std::time::Duration, asked: bool, curtain: bool) -> CloseStep {
    // Under LockNote's own box (the offer itself, the lock's box) a close
    // neither cancels nor offers: it waits (finding 3, pass 7).
    if own_box {
        return if modal_open { CloseStep::Wait } else { CloseStep::Run };
    }
    close_step(modal_open, since_cancel, waited, asked, curtain)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// The window, recorded.
    struct Fake {
        dialog_open: bool,
        own_box: bool,
        locked: bool,
        curtain: Curtain,
        still_due: bool,
        modified: bool,
        save_ok: bool,
        lock_without_saving: bool,
        pending: Option<PendingLock>,
        in_box: bool,
        queued_manual: bool,
        log: Vec<String>,
    }

    impl Fake {
        fn new() -> Fake {
            Fake {
                dialog_open: false,
                own_box: false,
                locked: false,
                curtain: Curtain::None,
                still_due: true,
                modified: true,
                save_ok: true,
                lock_without_saving: false,
                pending: None,
                in_box: false,
                queued_manual: false,
                log: Vec::new(),
            }
        }
        fn has(&self, e: &str) -> bool {
            self.log.iter().any(|l| l == e)
        }
    }

    impl LockSteps for Fake {
        fn modal_open(&self) -> bool {
            self.dialog_open || self.own_box || self.in_box
        }
        fn own_box_open(&self) -> bool {
            self.own_box || self.in_box
        }
        fn cancel_dialogs(&mut self) {
            self.log.push("cancel".into());
        }
        fn locked(&self) -> bool {
            self.locked
        }
        fn curtain(&self) -> Curtain {
            self.curtain
        }
        fn show_curtain(&mut self, mode: Curtain) {
            self.curtain = mode;
            self.log.push(format!("curtain:{:?}", mode));
        }
        fn hide_curtain(&mut self) {
            self.curtain = Curtain::None;
            self.log.push("hide_curtain".into());
        }
        fn still_due(&self) -> bool {
            self.still_due
        }
        fn modified(&self) -> bool {
            self.modified
        }
        fn save(&mut self) -> Result<(), String> {
            self.log.push("save".into());
            if self.save_ok {
                self.modified = false;
                Ok(())
            } else {
                Err("disk full".into())
            }
        }
        fn ask_lock_without_saving(&mut self, _reason: &str) -> bool {
            self.log.push("ask".into());
            assert!(self.in_box, "the question is the lock's own box");
            self.lock_without_saving
        }
        fn set_save_failed_reason(&mut self, reason: String) {
            self.log.push(format!("reason:{}", reason));
        }
        fn lock_now(&mut self) {
            self.locked = true;
            self.curtain = Curtain::None;
            self.log.push("lock".into());
        }
        fn reset_autolock(&mut self) {
            self.log.push("reset".into());
        }
        fn pending(&self) -> Option<PendingLock> {
            self.pending
        }
        fn set_pending(&mut self, p: Option<PendingLock>) {
            self.pending = p;
        }
        fn in_box(&self) -> bool {
            self.in_box
        }
        fn set_in_box(&mut self, v: bool) {
            self.in_box = v;
        }
        fn queue_manual(&mut self) {
            self.queued_manual = true;
        }
        fn take_queued_manual(&mut self) -> bool {
            std::mem::replace(&mut self.queued_manual, false)
        }
    }

    const MS: fn(u64) -> Duration = Duration::from_millis;

    /// Finding 1: an AUTO lock committed under the curtain stays automatic:
    /// its failed save hides the note until the password, it never offers
    /// "Lock without saving" to a passer-by.
    #[test]
    fn a_committed_auto_lock_with_a_failed_save_hides_until_the_password() {
        let mut f = Fake::new();
        f.dialog_open = true; // a dialog that does not close
        f.save_ok = false;
        request(&mut f, false);
        assert!(f.has("cancel"));
        tick(&mut f, LOCK_DEADLINE_FOR_TESTS, MS(200));
        assert_eq!(f.curtain, Curtain::Waiting);
        // The user comes back (mouse): the committed lock still runs.
        f.still_due = false;
        f.dialog_open = false;
        tick(&mut f, MS(5000), MS(200));
        assert!(!f.has("ask"), "Lock without saving offered for an auto lock: {:?}", f.log);
        assert_eq!(f.curtain, Curtain::SaveFailed, "{:?}", f.log);
        assert!(!f.locked);
    }

    /// Finding 2: Ctrl+L under a stuck dialog, failed save, "Keep open":
    /// no curtain is left, nothing locked.
    #[test]
    fn keep_open_after_a_failed_manual_lock_leaves_no_curtain() {
        let mut f = Fake::new();
        f.dialog_open = true;
        f.save_ok = false;
        request(&mut f, true);
        tick(&mut f, LOCK_DEADLINE_FOR_TESTS, MS(200));
        assert_eq!(f.curtain, Curtain::Waiting);
        f.dialog_open = false;
        f.lock_without_saving = false; // "Keep open"
        tick(&mut f, MS(5000), MS(200));
        assert!(f.has("ask"));
        assert_eq!(f.curtain, Curtain::None, "{:?}", f.log);
        assert!(!f.locked);
        assert!(f.pending.is_none());
    }

    /// "Lock without saving" locks.
    #[test]
    fn lock_without_saving_locks() {
        let mut f = Fake::new();
        f.save_ok = false;
        f.lock_without_saving = true;
        request(&mut f, true);
        assert!(f.locked && f.curtain == Curtain::None, "{:?}", f.log);
    }

    /// An auto lock no longer due when it runs (user back, not committed)
    /// is dropped; a manual one always locks.
    #[test]
    fn an_auto_lock_the_user_came_back_from_is_dropped() {
        let mut f = Fake::new();
        f.dialog_open = true;
        request(&mut f, false);
        f.still_due = false;
        f.dialog_open = false;
        tick(&mut f, MS(400), MS(200));
        assert!(!f.locked && f.pending.is_none(), "{:?}", f.log);
        let mut f = Fake::new();
        f.still_due = false;
        request(&mut f, true);
        assert!(f.locked);
    }

    /// Finding 5: a manual lock asked during the lock's box is not replayed
    /// inside run_lock (under the persistence guard): it becomes a pending
    /// manual lock, run by the next tick.
    #[test]
    fn a_manual_lock_queued_during_the_box_runs_at_the_next_tick() {
        let mut f = Fake::new();
        f.save_ok = false;
        f.lock_without_saving = false;
        // During the question, Ctrl+L.
        struct Hook;
        let _ = Hook;
        f.in_box = true;
        request(&mut f, true);
        assert!(f.queued_manual && !f.locked);
        f.in_box = false;
        // The run of a lock whose box gets the answer "Keep open".
        run_lock(&mut f, true, false);
        assert!(!f.locked, "replayed under the guard: {:?}", f.log);
        assert_eq!(f.pending, Some(PendingLock { manual: true, committed: false }));
        f.save_ok = true;
        tick(&mut f, MS(200), MS(200));
        assert!(f.locked, "{:?}", f.log);
    }

    /// Finding 3: while one of LockNote's own boxes is shown (the
    /// stuck-close offer itself, the lock's box), a pending close neither
    /// cancels nor offers again; once it is gone the close runs.
    #[test]
    fn the_stuck_close_offer_is_not_cancelled_by_its_own_close() {
        let w = Duration::from_secs(10);
        assert_eq!(close_tick(true, false, MS(100), w, false, false), CloseStep::Offer);
        // The offer is up (own box): no cancel, no second offer.
        assert_eq!(close_tick(true, true, Duration::from_secs(3), w, true, false), CloseStep::Wait);
        assert_eq!(close_tick(true, true, Duration::from_secs(3), w, false, false), CloseStep::Wait, "not over the lock's box");
        assert_eq!(close_tick(true, false, Duration::from_secs(3), w, true, false), CloseStep::CancelAgain);
        assert_eq!(close_tick(false, false, MS(100), w, true, false), CloseStep::Run);
    }

    /// No lock request cancels LockNote's own box.
    #[test]
    fn a_lock_request_never_cancels_an_own_box() {
        let mut f = Fake::new();
        f.own_box = true; // e.g. the close prompt
        request(&mut f, false);
        assert!(!f.has("cancel"), "{:?}", f.log);
        tick(&mut f, MS(400), Duration::from_secs(2));
        assert!(!f.has("cancel"), "{:?}", f.log);
        assert!(f.pending.is_some());
    }

    const LOCK_DEADLINE_FOR_TESTS: Duration = super::super::modal_guard::LOCK_DEADLINE;
}
