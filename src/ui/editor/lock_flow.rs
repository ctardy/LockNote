// The lock sequence, behind a trait (pass 7), reduced to one rule (pass 8,
// owner decision on #30):
//
// 1. As soon as a lock is due (auto-lock delay, Lock button, Ctrl+L) the
//    note goes under the curtain at once; only the password lifts it.
// 2. At that moment every open LockNote dialog gets its harmless answer
//    (cancelled as No / Cancel), and any answer that returns after the
//    curtain went up is treated as the harmless one (`answer_applies`).
// 3. The real lock (wipe) runs from the main loop only when no window is
//    open and the save succeeded (or there was nothing to save); otherwise
//    the curtain stays, nothing is wiped or discarded, and the save is
//    retried. Lifting the curtain with the password before that brings the
//    note back with its changes, and the reason of a failed save.
// 4. There is no "Lock without saving": to drop changes, lift the curtain
//    and close without saving.
//
// `FormLockSteps` (modal_guard.rs) implements the steps on the window; the
// tests below drive whole sequences with a recording fake.

#[allow(unused_imports)]
use super::*;

/// How often the dialogs that did not close are asked again.
pub(super) const RECANCEL_EVERY: std::time::Duration = std::time::Duration::from_secs(1);
/// How often a failed save under the curtain is retried.
pub(super) const SAVE_RETRY: std::time::Duration = std::time::Duration::from_secs(10);

/// What the sequence needs from the window.
pub(super) trait LockSteps {
    /// A LockNote dialog or message box is open.
    fn modal_open(&self) -> bool;
    /// Give every open dialog its harmless answer (No / Cancel) and hide
    /// the LockNote dialogs behind the curtain.
    fn cancel_dialogs(&mut self);
    fn locked(&self) -> bool;
    /// The Windows session ended (nothing may change the note any more).
    fn exiting(&self) -> bool;
    fn curtain_up(&self) -> bool;
    fn show_curtain(&mut self);
    /// Lift the curtain: full layout, hidden dialogs shown again.
    fn hide_curtain(&mut self);
    fn epoch(&self) -> u64;
    /// Every answer of a dialog opened before is now stale.
    fn bump_epoch(&mut self);
    fn modified(&self) -> bool;
    /// A save attempt starts now: the next one waits `SAVE_RETRY`.
    fn mark_tried(&mut self);
    fn save(&mut self) -> Result<(), String>;
    fn save_failed_reason(&self) -> Option<String>;
    fn set_save_failed_reason(&mut self, reason: Option<String>);
    /// The real lock: keys, wipe, lock screen (the curtain becomes it).
    fn lock_now(&mut self);
    fn pending(&self) -> bool;
    fn set_pending(&mut self, pending: bool);
    /// The reason of a failed save, shown after the curtain is lifted.
    fn inform_save_failed(&mut self, reason: &str);
    fn reset_autolock(&mut self);
    /// Activity seen at `at` (input in a LockNote dialog, issue #32): the
    /// auto-lock delay counts from then.
    fn note_activity(&mut self, at: std::time::Instant);
    /// The auto-lock delay ran out (always false when auto-lock is off).
    fn autolock_due(&self) -> bool;
}

/// A tick of the auto-lock timer (every 10 s, also inside a dialog's own
/// loop), not locked. `dialog_input`: when the last input in a LockNote
/// dialog happened since the previous tick (`dialogs::activity::take_input`, issue #32);
/// it counts as activity like input in the main window, except under the
/// curtain: there a lock is already due and only the password lifts it.
/// Then a lock is requested if the delay ran out. Returns whether one was.
pub(super) fn autolock_tick<S: LockSteps>(s: &mut S, dialog_input: Option<std::time::Instant>) -> bool {
    if s.locked() {
        return false;
    }
    if let Some(at) = dialog_input {
        if s.curtain_up() {
            eprintln!("INFO autolock: input in a dialog under the curtain ignored, the password lifts it");
        } else {
            let was_due = s.autolock_due();
            s.note_activity(at);
            if was_due && !s.autolock_due() {
                eprintln!("INFO autolock: input in a LockNote dialog counted, lock postponed");
            }
        }
    }
    if s.autolock_due() {
        request(s);
        true
    } else {
        false
    }
}

/// A lock is due (auto-lock delay, Lock button, Ctrl+L): curtain at once,
/// dialogs answered harmlessly, real lock if possible now. Never dropped,
/// whatever box is open. A lock already pending is not tried again here:
/// its retries follow `tick` at `SAVE_RETRY` (pass 9).
pub(super) fn request<S: LockSteps>(s: &mut S) {
    if s.locked() {
        return;
    }
    let already_pending = s.pending();
    if !s.curtain_up() {
        s.bump_epoch();
        eprintln!("INFO lock: due, note hidden under the curtain epoch={}", s.epoch());
        s.show_curtain();
    }
    s.set_pending(true);
    if s.modal_open() {
        s.cancel_dialogs();
    } else if !already_pending {
        try_lock(s);
    }
}

/// What a tick of a pending lock did.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(super) struct TickDone {
    pub(super) cancelled: bool,
    pub(super) tried: bool,
}

/// A tick of a pending lock (every 200 ms, from the main loop):
/// `since_cancel` since the dialogs were last asked to close,
/// `since_try` since the last save attempt (`None`: never tried).
pub(super) fn tick<S: LockSteps>(s: &mut S, since_cancel: std::time::Duration, since_try: Option<std::time::Duration>) -> TickDone {
    let mut done = TickDone::default();
    if !s.pending() || s.locked() {
        return done;
    }
    if s.modal_open() {
        if since_cancel >= RECANCEL_EVERY {
            s.cancel_dialogs();
            done.cancelled = true;
        }
        return done;
    }
    if since_try.map_or(true, |t| t >= SAVE_RETRY) {
        try_lock(s);
        done.tried = true;
    }
    done
}

/// The real lock, only after a successful save (or nothing to save) and
/// with no window open; otherwise the curtain stays and nothing is lost.
fn try_lock<S: LockSteps>(s: &mut S) {
    s.mark_tried();
    let saved = if s.modified() { s.save() } else { Ok(()) };
    match saved {
        Ok(()) => {
            s.set_save_failed_reason(None);
            s.set_pending(false);
            s.lock_now();
        }
        Err(reason) => {
            if s.save_failed_reason().is_none() {
                eprintln!("WARN lock: save before the lock failed, note kept under the curtain, retried every {} s", SAVE_RETRY.as_secs());
            }
            s.set_save_failed_reason(Some(reason));
        }
    }
}

/// The password typed on the curtain was checked (`password_ok`): lift it,
/// the note comes back with its changes; the reason of a failed save is
/// shown then. Returns whether it was lifted.
pub(super) fn lift<S: LockSteps>(s: &mut S, password_ok: bool) -> bool {
    if !s.curtain_up() || s.locked() || !password_ok {
        return false;
    }
    eprintln!("INFO lock: curtain lifted with the password");
    s.set_pending(false);
    s.hide_curtain();
    s.reset_autolock();
    if let Some(reason) = s.save_failed_reason() {
        s.set_save_failed_reason(None);
        s.inform_save_failed(&reason);
    }
    true
}

/// Whether the answer of a dialog opened at `epoch_before` may act: no
/// curtain went up meanwhile (it moves the epoch) and none is up, not
/// locked. Otherwise the caller applies the harmless answer.
pub(super) fn answer_applies(epoch_before: u64, epoch_now: u64, curtain_up: bool, locked: bool) -> bool {
    epoch_before == epoch_now && !curtain_up && !locked
}

/// The check of every dialog answer (`EditorForm::still_valid`, pass 9:
/// shared with the tests' fake): `answer_applies` on the window's state,
/// and the session not ended.
pub(super) fn answer_still_valid<S: LockSteps>(s: &S, epoch_at_open: u64) -> bool {
    answer_applies(epoch_at_open, s.epoch(), s.curtain_up(), s.locked()) && !s.exiting()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// The window, recorded. `dialogs` are the open LockNote dialogs, with
    /// whether each closes when asked (a busy one does not).
    struct Fake {
        dialogs: Vec<(&'static str, bool)>,
        locked: bool,
        curtain: bool,
        epoch: u64,
        modified: bool,
        save_ok: bool,
        reason: Option<String>,
        pending: bool,
        log: Vec<String>,
        /// The close flow's own box (close prompt, save-failed question,
        /// stuck-close offer), when shown.
        close_box: Option<&'static str>,
        close_asked: bool,
        /// Save attempts recorded (`mark_tried`).
        tries: u32,
        /// The auto-lock clock (issue #32): the fake's "now", the last
        /// activity and the delay.
        now: std::time::Instant,
        last_activity: std::time::Instant,
        delay: Duration,
        /// The epoch when the curtain was shown (issue #35: the order of
        /// `request`, seen from the window).
        curtain_epoch: Option<u64>,
        /// The Windows session ended.
        exiting: bool,
    }

    impl Fake {
        fn new() -> Fake {
            let start = std::time::Instant::now();
            Fake { dialogs: Vec::new(), locked: false, curtain: false, epoch: 0, modified: true, save_ok: true, reason: None, pending: false, log: Vec::new(), close_box: None, close_asked: false, tries: 0, now: start, last_activity: start, delay: Duration::from_secs(15 * 60), curtain_epoch: None, exiting: false }
        }
        /// Time passes on the fake's clock.
        fn wait(&mut self, d: Duration) {
            self.now += d;
        }
        fn saves(&self) -> usize {
            self.log.iter().filter(|l| *l == "save").count()
        }
        fn has(&self, e: &str) -> bool {
            self.log.iter().any(|l| l == e)
        }
        /// A dialog opened at `epoch_at_open` returns its destructive answer
        /// now: the caller acts only if the production check of
        /// `still_valid` lets it; true when it acted destructively.
        fn answer_returns(&mut self, epoch_at_open: u64) -> bool {
            let acts = answer_still_valid(self, epoch_at_open);
            if acts {
                self.log.push("destructive".into());
            }
            acts
        }
    }

    impl LockSteps for Fake {
        fn modal_open(&self) -> bool {
            !self.dialogs.is_empty() || self.close_box.is_some()
        }
        fn cancel_dialogs(&mut self) {
            self.log.push("cancel".into());
            // Harmless answers: the ones that can close, close.
            self.dialogs.retain(|(_, busy)| *busy);
            self.close_box = None;
        }
        fn locked(&self) -> bool {
            self.locked
        }
        fn exiting(&self) -> bool {
            self.exiting
        }
        fn curtain_up(&self) -> bool {
            self.curtain
        }
        fn show_curtain(&mut self) {
            self.curtain = true;
            self.curtain_epoch = Some(self.epoch);
            self.log.push("curtain".into());
        }
        fn hide_curtain(&mut self) {
            self.curtain = false;
            self.log.push("lift".into());
        }
        fn epoch(&self) -> u64 {
            self.epoch
        }
        fn bump_epoch(&mut self) {
            self.epoch += 1;
        }
        fn modified(&self) -> bool {
            self.modified
        }
        fn mark_tried(&mut self) {
            self.tries += 1;
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
        fn save_failed_reason(&self) -> Option<String> {
            self.reason.clone()
        }
        fn set_save_failed_reason(&mut self, reason: Option<String>) {
            self.reason = reason;
        }
        fn lock_now(&mut self) {
            assert!(self.dialogs.is_empty(), "wiped with a window open");
            assert!(!self.modified, "wiped unsaved changes");
            self.locked = true;
            self.curtain = false;
            self.log.push("wipe".into());
        }
        fn pending(&self) -> bool {
            self.pending
        }
        fn set_pending(&mut self, pending: bool) {
            self.pending = pending;
        }
        fn inform_save_failed(&mut self, reason: &str) {
            self.log.push(format!("inform:{}", reason));
        }
        fn reset_autolock(&mut self) {
            self.last_activity = self.now;
        }
        fn note_activity(&mut self, at: std::time::Instant) {
            // As `AutoLock::note_activity_at`: only forward.
            if at > self.last_activity {
                self.last_activity = at;
            }
            self.log.push("activity".into());
        }
        fn autolock_due(&self) -> bool {
            self.now.duration_since(self.last_activity) >= self.delay
        }
    }

    impl super::super::modal_guard::CloseWaitSteps for Fake {
        fn dialogs_open(&self) -> bool {
            !self.dialogs.is_empty() || self.close_box.is_some()
        }
        fn own_close_box_open(&self) -> bool {
            self.close_box.is_some()
        }
        fn curtain_is_up(&self) -> bool {
            self.curtain
        }
        fn cancel_for_close(&mut self) {
            self.log.push("close-cancel".into());
            self.dialogs.retain(|(_, busy)| *busy);
            // A close box asked to close answers Cancel / Keep waiting.
            self.close_box = None;
        }
        fn mark_offered(&mut self) {
            self.close_asked = true;
        }
        fn offer_close_without_saving(&mut self) {
            // Its own loop starts; the ticks below run inside it.
            self.close_box = Some("stuck-close offer");
        }
        fn close(&mut self) {
            self.log.push("close".into());
        }
    }

    const S2: Duration = Duration::from_secs(2);
    const MIN: Duration = Duration::from_secs(60);

    /// Issue #32: someone typing in a dialog (a long vault secret) a minute
    /// before the delay runs out: the tick at the delay does not lock and
    /// does not cancel the dialog; the delay counts from that input.
    #[test]
    fn input_in_a_dialog_before_the_delay_keeps_the_note_open() {
        let mut f = Fake::new();
        f.dialogs.push(("edit secret", false));
        f.wait(14 * MIN);
        let typed = f.now;
        f.wait(MIN);
        assert!(!autolock_tick(&mut f, Some(typed)), "locked although someone typed in the dialog: {:?}", f.log);
        assert!(!f.curtain && !f.has("cancel") && f.dialogs.len() == 1, "{:?}", f.log);
        // Nobody since: the lock comes 15 min after that input.
        f.wait(13 * MIN);
        assert!(!autolock_tick(&mut f, None));
        f.wait(MIN);
        assert!(autolock_tick(&mut f, None), "15 min after the last input");
        assert!(f.curtain && f.has("cancel") && f.dialogs.is_empty(), "{:?}", f.log);
    }

    /// Issue #32, #30: a dialog left open with nobody at it is still
    /// cancelled at the delay, and the note saved and locked.
    #[test]
    fn a_dialog_with_nobody_at_it_is_cancelled_at_the_delay() {
        let mut f = Fake::new();
        f.dialogs.push(("edit secret", false));
        f.wait(14 * MIN);
        assert!(!autolock_tick(&mut f, None));
        assert!(f.dialogs.len() == 1 && !f.curtain);
        f.wait(MIN);
        assert!(autolock_tick(&mut f, None));
        assert!(f.has("cancel") && f.dialogs.is_empty(), "{:?}", f.log);
        tick(&mut f, RECANCEL_EVERY, None);
        assert!(f.locked, "{:?}", f.log);
        assert!(!f.has("activity"));
    }

    /// Issue #32: input under the curtain (in a dialog that did not close,
    /// or a box shown over it) neither lifts it nor postpones the lock
    /// already due: the lock goes on when the window closes.
    #[test]
    fn input_under_the_curtain_does_not_lift_or_postpone_the_lock() {
        let mut f = Fake::new();
        f.dialogs.push(("temporary files (busy)", true));
        f.wait(15 * MIN);
        assert!(autolock_tick(&mut f, None));
        assert!(f.curtain && !f.locked && f.pending);
        f.wait(Duration::from_secs(10));
        let typed = f.now;
        f.wait(Duration::from_secs(10));
        autolock_tick(&mut f, Some(typed));
        assert!(f.curtain && f.pending && !f.locked, "the curtain stays: {:?}", f.log);
        assert!(!f.has("activity"), "input under the curtain counted: {:?}", f.log);
        assert!(!f.has("lift"));
        f.dialogs.clear();
        tick(&mut f, RECANCEL_EVERY, None);
        assert!(f.locked, "the due lock went on: {:?}", f.log);
    }

    /// Issue #32: once locked, a recorded input does nothing.
    #[test]
    fn input_after_the_lock_does_nothing() {
        let mut f = Fake::new();
        f.wait(15 * MIN);
        assert!(autolock_tick(&mut f, None));
        assert!(f.locked);
        let n = f.log.len();
        let now = f.now;
        assert!(!autolock_tick(&mut f, Some(now)));
        assert_eq!(f.log.len(), n, "{:?}", f.log);
    }

    /// Pass 9: a lock already pending under the curtain is not saved again
    /// by a new request: one save, then one per SAVE_RETRY (ticks), each
    /// recorded so the next waits.
    #[test]
    fn a_second_request_does_not_save_again_the_retry_keeps_its_pace() {
        let mut f = Fake::new();
        f.save_ok = false;
        request(&mut f);
        request(&mut f);
        assert_eq!(f.saves(), 1, "{:?}", f.log);
        assert_eq!(f.tries, 1, "the request's attempt is recorded");
        assert!(!tick(&mut f, S2, Some(S2)).tried);
        assert_eq!(f.saves(), 1);
        assert!(tick(&mut f, S2, Some(SAVE_RETRY)).tried);
        assert_eq!((f.saves(), f.tries), (2, 2));
    }

    /// Pass 9: the stuck-close offer is not cancelled by the close path's
    /// own ticks (they run inside its loop), but a due lock cancels it.
    #[test]
    fn the_stuck_close_offer_survives_the_close_ticks_but_not_a_due_lock() {
        use super::super::modal_guard::{close_tick, CloseStep, CLOSE_WAIT};
        let mut f = Fake::new();
        f.dialogs.push(("temporary files (busy)", true));
        assert_eq!(close_tick(&mut f, RECANCEL_EVERY, CLOSE_WAIT, false), CloseStep::Offer);
        assert!(f.close_asked && f.close_box.is_some());
        for _ in 0..3 {
            let asked = f.close_asked;
            close_tick(&mut f, RECANCEL_EVERY, CLOSE_WAIT, asked);
        }
        assert!(f.close_box.is_some(), "the close path cancelled its own offer: {:?}", f.log);
        request(&mut f);
        assert!(f.close_box.is_none(), "a due lock cancels the offer too");
        assert!(f.curtain && !f.locked, "the busy dialog keeps the curtain");
    }

    /// Issue #35 (was a text scan of `request`): the epoch moves before the
    /// curtain shows, so every answer of a dialog opened before is stale
    /// from the moment the note is hidden, and stays stale once the
    /// password lifted the curtain.
    #[test]
    fn the_epoch_moves_before_the_curtain_shows() {
        let mut f = Fake::new();
        f.dialogs.push(("delete this secret?", false));
        let opened_at = f.epoch;
        request(&mut f);
        assert_eq!(f.curtain_epoch, Some(opened_at + 1), "the curtain showed before the epoch moved: {:?}", f.log);
        assert!(f.curtain && !f.locked, "the dialog was open: no lock yet");
        assert!(lift(&mut f, true));
        assert!(!f.answer_returns(opened_at), "an answer from before the curtain acted after the lift");
        // A second lock moves it again before its own curtain.
        let mut f = Fake::new();
        f.save_ok = false;
        request(&mut f);
        assert!(lift(&mut f, true));
        let before_second = f.epoch;
        request(&mut f);
        assert_eq!(f.curtain_epoch, Some(before_second + 1), "{:?}", f.log);
    }

    /// Issue #35 (was a text scan of `answer_still_valid`): once the Windows
    /// session ended, no dialog answer acts, curtain or not.
    #[test]
    fn no_answer_acts_once_the_session_ended() {
        let mut f = Fake::new();
        let opened_at = f.epoch;
        assert!(answer_still_valid(&f, opened_at), "nothing happened: the answer applies");
        f.exiting = true;
        assert!(!f.answer_returns(opened_at), "an answer acted after the end of the session");
        assert!(!f.has("destructive"));
    }

    /// Issue #35 (was a text scan of `answer_still_valid`): an answer never
    /// acts under the curtain or once locked, even with the epoch it was
    /// opened at (each argument of the check has its own case: the other
    /// tests reach the curtain and the lock through `request`, which also
    /// moves the epoch).
    #[test]
    fn no_answer_acts_under_the_curtain_or_locked() {
        let mut f = Fake::new();
        let at = f.epoch;
        f.curtain = true;
        assert!(!f.answer_returns(at), "an answer acted under the curtain");
        f.curtain = false;
        f.locked = true;
        assert!(!f.answer_returns(at), "an answer acted while locked");
        assert!(!f.has("destructive"));
    }

    #[test]
    fn a_due_lock_with_nothing_open_saves_then_locks() {
        let mut f = Fake::new();
        request(&mut f);
        assert_eq!(f.log, ["curtain", "save", "wipe"]);
        assert!(f.locked && !f.curtain);
    }

    #[test]
    fn a_failed_save_keeps_the_curtain_and_the_changes_then_retries() {
        let mut f = Fake::new();
        f.save_ok = false;
        request(&mut f);
        assert!(f.curtain && !f.locked && f.modified && f.pending);
        assert_eq!(tick(&mut f, S2, Some(Duration::from_secs(1))), TickDone::default(), "not before SAVE_RETRY");
        f.save_ok = true;
        assert!(tick(&mut f, S2, Some(SAVE_RETRY)).tried);
        assert!(f.locked, "{:?}", f.log);
    }

    #[test]
    fn the_password_lifts_the_curtain_and_shows_why_the_save_failed() {
        let mut f = Fake::new();
        f.save_ok = false;
        request(&mut f);
        assert!(!lift(&mut f, false), "wrong password");
        assert!(f.curtain);
        assert!(lift(&mut f, true));
        assert!(!f.curtain && !f.pending && f.modified, "changes kept");
        assert!(f.has("inform:disk full"));
    }

    #[test]
    fn a_window_that_does_not_close_keeps_the_curtain_until_it_does() {
        let mut f = Fake::new();
        f.dialogs.push(("temporary files (busy)", true));
        request(&mut f);
        assert!(f.curtain && f.has("cancel") && !f.locked);
        assert!(tick(&mut f, RECANCEL_EVERY, None).cancelled, "asked again");
        f.dialogs.clear();
        tick(&mut f, Duration::from_millis(200), None);
        assert!(f.locked);
    }

    /// The table of every scenario found in passes 5-7. Invariants:
    /// (a) at the deadline the note is hidden under the password curtain;
    /// (b) no destructive action happens without the password;
    /// (c) nothing unsaved is discarded;
    /// (d) the wipe happens only after a save success and with no window
    ///     (asserted inside `Fake::lock_now`).
    #[test]
    fn the_single_rule_holds_for_every_known_scenario() {
        struct Scenario {
            name: &'static str,
            /// Open when the lock is due: (name, busy).
            dialogs: Vec<(&'static str, bool)>,
            save_ok: bool,
            /// A second request while the first is being processed.
            second_request: bool,
            /// The open box is answered destructively after the curtain.
            answered_after: bool,
        }
        let all = vec![
            Scenario { name: "close prompt left open", dialogs: vec![("close prompt", false)], save_ok: true, second_request: false, answered_after: true },
            Scenario { name: "save-failed question open", dialogs: vec![("close without saving?", false)], save_ok: false, second_request: false, answered_after: true },
            Scenario { name: "stuck-close offer open", dialogs: vec![("close without saving (stuck)", false)], save_ok: true, second_request: false, answered_after: true },
            Scenario { name: "Ctrl+L under a stuck dialog + failed save", dialogs: vec![("temporary files (busy)", true)], save_ok: false, second_request: false, answered_after: false },
            Scenario { name: "Ctrl+L, no dialog, failed save", dialogs: vec![], save_ok: false, second_request: false, answered_after: false },
            Scenario { name: "auto lock during the lock's own processing", dialogs: vec![], save_ok: false, second_request: true, answered_after: false },
            Scenario { name: "Yes over the curtain", dialogs: vec![("delete this secret?", false)], save_ok: true, second_request: false, answered_after: true },
            Scenario { name: "About open under the curtain", dialogs: vec![("about", false)], save_ok: true, second_request: false, answered_after: false },
        ];
        for sc in all {
            let mut f = Fake::new();
            f.dialogs = sc.dialogs.clone();
            f.save_ok = sc.save_ok;
            let opened_at = f.epoch;
            request(&mut f);
            if sc.second_request {
                request(&mut f);
                assert_eq!(f.saves(), 1, "{}: one save for both requests", sc.name);
            }
            // (a) at the deadline (at once here) the note is hidden.
            assert!(f.curtain || f.locked, "{}: (a) note visible", sc.name);
            // (b) an answer returning after the curtain does nothing.
            if sc.answered_after {
                assert!(!f.answer_returns(opened_at), "{}: (b) destructive answer over the curtain", sc.name);
            }
            // Time passes; busy windows end; save attempts go on.
            for _ in 0..3 {
                tick(&mut f, RECANCEL_EVERY, Some(SAVE_RETRY));
            }
            f.dialogs.clear();
            tick(&mut f, RECANCEL_EVERY, Some(SAVE_RETRY));
            assert!(!f.has("destructive"), "{}: (b)", sc.name);
            // (c) nothing unsaved discarded: either saved and locked, or
            // still under the curtain with the changes.
            if sc.save_ok {
                assert!(f.locked, "{}: should lock once possible: {:?}", sc.name, f.log);
            } else {
                assert!(f.curtain && f.modified && !f.locked, "{}: (c) {:?}", sc.name, f.log);
            }
        }
    }
}
