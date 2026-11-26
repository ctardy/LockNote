// Modal dialogs of the main window and the timers (issue #30).
//
// Every dialog of LockNote runs its own message loop: the main window's
// timers keep firing under it. Auto-save could write changes the user is
// about to decline: it is suspended while a modal is open and its delay
// restarts when the last one closes. Auto-lock is never postponed past its
// delay (owner decision on #30): when it is due with dialogs open, they are
// cancelled (their input is lost, the note is not) and the lock runs once
// they are gone (`pending_step`); the manual lock and the close requested
// under a dialog do the same. A modal is "open" when an RAII `ModalGuard`
// is alive, or when a LockNote dialog or message box of this thread is shown
// (the probe in `EditorForm::modal_open` catches the dialogs opened without
// a guard).

#[allow(unused_imports)]
use super::*;

/// Number of nested modal dialogs opened through `EditorForm::modal`.
#[derive(Default)]
pub(super) struct ModalDepth {
    depth: Cell<u32>,
}

impl ModalDepth {
    pub(super) fn is_open(&self) -> bool {
        self.depth.get() > 0
    }

    /// Enter a modal: the returned guard leaves it when dropped (also on an
    /// early return or a panic) and calls `on_last_close` when no modal is
    /// left.
    pub(super) fn enter<F: FnMut()>(&self, on_last_close: F) -> ModalGuard<'_, F> {
        self.depth.set(self.depth.get() + 1);
        ModalGuard { depth: &self.depth, on_last_close }
    }
}

pub(super) struct ModalGuard<'a, F: FnMut()> {
    depth: &'a Cell<u32>,
    on_last_close: F,
}

impl<F: FnMut()> Drop for ModalGuard<'_, F> {
    fn drop(&mut self) {
        let left = self.depth.get().saturating_sub(1);
        self.depth.set(left);
        if left == 0 {
            (self.on_last_close)();
        }
    }
}

/// What a tick of the auto-save or auto-lock timer may do.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum TimerTick {
    /// A modal is open or the note is locked: do nothing.
    Skip,
    /// The last modal just closed: restart the delay, do nothing yet.
    RestartDelay,
    /// Normal tick.
    Run,
}

/// Remembers whether the previous tick saw a modal, so the tick after it
/// closes restarts the delay (also for modals opened without a guard).
#[derive(Default)]
pub(super) struct TimerGate {
    saw_modal: Cell<bool>,
}

impl TimerGate {
    pub(super) fn tick(&self, modal_open: bool, locked: bool) -> TimerTick {
        if locked {
            // The unlock restarts the delays itself.
            self.saw_modal.set(false);
            return TimerTick::Skip;
        }
        if modal_open {
            self.saw_modal.set(true);
            return TimerTick::Skip;
        }
        if self.saw_modal.replace(false) {
            return TimerTick::RestartDelay;
        }
        TimerTick::Run
    }
}

/// What to do with a lock or a close requested while dialogs were open.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum PendingStep {
    /// The dialogs are closing: check again at the next tick.
    Wait,
    /// No dialog left: run it now, from the application's own loop.
    Run,
    /// A dialog did not close (a Yes/No message box has no Cancel): run it
    /// anyway, inside that dialog's loop (lock only).
    Force,
}

/// Time a lock waits for the cancelled dialogs before it runs anyway.
pub(super) const LOCK_FORCE_AFTER: std::time::Duration = std::time::Duration::from_secs(2);

/// The step of a pending request: `force_after` is `None` for a close (it
/// waits for the user), `Some(LOCK_FORCE_AFTER)` for a lock.
pub(super) fn pending_step(modal_open: bool, waited: std::time::Duration, force_after: Option<std::time::Duration>) -> PendingStep {
    if !modal_open {
        return PendingStep::Run;
    }
    match force_after {
        Some(limit) if waited >= limit => PendingStep::Force,
        _ => PendingStep::Wait,
    }
}

/// A lock requested now: under dialogs it first cancels them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum LockRequest {
    LockNow,
    CancelDialogsThenLock,
}

pub(super) fn lock_request(modal_open: bool) -> LockRequest {
    if modal_open {
        LockRequest::CancelDialogsThenLock
    } else {
        LockRequest::LockNow
    }
}

impl EditorForm {
    /// Open a modal of the main window: auto-save waits until the guard is
    /// dropped, then its delay restarts (auto-lock never waits, see above).
    pub(super) fn modal(&self) -> ModalGuard<'_, impl FnMut() + '_> {
        self.modal_depth.enter(move || {
            self.autosave.borrow_mut().mark_saved();
            eprintln!("INFO modal: last dialog closed, auto-save delay restarted");
        })
    }

    /// Lock now, or cancel the open dialogs and lock once they are gone.
    /// `manual`: the Lock button / Ctrl+L (always locks); otherwise the
    /// auto-lock, whose decision (save first or not) is taken when it runs.
    pub(super) fn request_lock(&self, manual: bool) {
        match lock_request(self.modal_open()) {
            LockRequest::LockNow => self.execute_lock(manual),
            LockRequest::CancelDialogsThenLock => {
                if self.lock_pending.get().is_none() {
                    self.lock_pending.set(Some((std::time::Instant::now(), manual)));
                }
                let n = cancel_open_dialogs();
                eprintln!("INFO lock: due with dialogs open, cancelled={} manual={}", n, manual);
            }
        }
    }

    /// Close requested under a dialog (tray Quit, title bar): cancel the
    /// dialogs, then run the close flow from the application's own loop.
    pub(super) fn request_close_after_dialogs(&self) {
        if self.close_pending.get().is_none() {
            self.close_pending.set(Some(std::time::Instant::now()));
        }
        let n = cancel_open_dialogs();
        eprintln!("INFO close: requested with dialogs open, cancelled={}", n);
    }

    /// Run the pending lock or close once the dialogs are gone (highlight
    /// tick, every 200 ms).
    pub(super) fn run_pending_requests(&self) {
        if let Some((since, manual)) = self.lock_pending.get() {
            match pending_step(self.modal_open(), since.elapsed(), Some(LOCK_FORCE_AFTER)) {
                PendingStep::Wait => {}
                PendingStep::Run => {
                    self.lock_pending.set(None);
                    self.execute_lock(manual);
                }
                PendingStep::Force => {
                    self.lock_pending.set(None);
                    eprintln!("WARN lock: a dialog did not close, locking under it");
                    self.execute_lock(manual);
                }
            }
        }
        if let Some(since) = self.close_pending.get() {
            if pending_step(self.modal_open(), since.elapsed(), None) == PendingStep::Run {
                self.close_pending.set(None);
                if self.on_close() {
                    nwg::stop_thread_dispatch();
                }
            }
        }
    }

    fn execute_lock(&self, manual: bool) {
        if *self.is_locked.borrow() {
            return;
        }
        if manual {
            self.on_autolock();
            return;
        }
        use crate::features::autolock::LockDecision;
        let decision = self.autolock.borrow().lock_decision();
        match decision {
            LockDecision::Wait => {}
            LockDecision::LockNow => self.on_autolock(),
            LockDecision::SaveThenLock => {
                // Try to save first. If the save succeeds we lock; if it
                // fails we keep the data visible and reset the inactivity
                // timer so the user gets another cycle to react.
                match self.on_save() {
                    Ok(()) => self.on_autolock(),
                    Err(msg) => {
                        self.autolock.borrow_mut().reset();
                        eprintln!("autolock: skip due to save failure: {}", msg);
                    }
                }
            }
        }
    }

    /// Whether a modal of LockNote is open: a guard is alive, or a LockNote
    /// dialog (registered by `apply_dialog_theme`) or a message box of this
    /// thread is shown.
    pub(super) fn modal_open(&self) -> bool {
        self.modal_depth.is_open()
            || crate::ui::dialogs::style::open_dialog_count() > 0
            || thread_message_box_visible()
    }
}

/// Ask every open LockNote dialog and standard dialog of this thread to
/// close as Cancel (WM_CLOSE; IDCANCEL for message boxes, which a Yes/No box
/// ignores: `LOCK_FORCE_AFTER` covers it). Returns how many were asked.
fn cancel_open_dialogs() -> usize {
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumThreadWindows, GetClassNameW, IsWindowVisible, PostMessageW, WM_CLOSE, WM_COMMAND,
    };
    let mut targets: Vec<(isize, bool)> = crate::ui::dialogs::style::registered_dialogs()
        .into_iter()
        .map(|h| (h, false))
        .collect();
    unsafe extern "system" fn cb(h: HWND, lp: LPARAM) -> windows::core::BOOL {
        let out = &mut *(lp.0 as *mut Vec<(isize, bool)>);
        if IsWindowVisible(h).as_bool() {
            let mut buf = [0u16; 16];
            let n = GetClassNameW(h, &mut buf) as usize;
            if String::from_utf16_lossy(&buf[..n]) == "#32770" {
                out.push((h.0 as isize, true));
            }
        }
        true.into()
    }
    unsafe {
        let tid = windows::Win32::System::Threading::GetCurrentThreadId();
        let _all_enumerated = EnumThreadWindows(tid, Some(cb), LPARAM(&mut targets as *mut _ as isize));
        for &(h, standard) in &targets {
            let hwnd = HWND(h as *mut _);
            if standard {
                if let Err(e) = PostMessageW(Some(hwnd), WM_COMMAND, WPARAM(2), LPARAM(0)) {
                    eprintln!("WARN modal: IDCANCEL not posted err={:?}", e);
                }
            }
            if let Err(e) = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) {
                eprintln!("WARN modal: WM_CLOSE not posted err={:?}", e);
            }
        }
    }
    targets.len()
}

/// A visible standard dialog (`#32770`: MessageBox, common dialogs) of this
/// thread: nwg's message boxes have no guard.
fn thread_message_box_visible() -> bool {
    use windows::Win32::Foundation::{HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{EnumThreadWindows, GetClassNameW, IsWindowVisible};
    unsafe extern "system" fn cb(h: HWND, lp: LPARAM) -> windows::core::BOOL {
        let found = &mut *(lp.0 as *mut bool);
        if IsWindowVisible(h).as_bool() {
            let mut buf = [0u16; 16];
            let n = GetClassNameW(h, &mut buf) as usize;
            if String::from_utf16_lossy(&buf[..n]) == "#32770" {
                *found = true;
                return false.into();
            }
        }
        true.into()
    }
    let mut found = false;
    unsafe {
        let tid = windows::Win32::System::Threading::GetCurrentThreadId();
        // Returns FALSE when the callback stops early: not an error here.
        let _stopped_early = EnumThreadWindows(tid, Some(cb), LPARAM(&mut found as *mut bool as isize));
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_guard_is_open_while_alive_and_nested() {
        let d = ModalDepth::default();
        let mut closed = 0;
        {
            let _a = d.enter(|| closed += 1);
            assert!(d.is_open());
        }
        assert!(!d.is_open());
        assert_eq!(closed, 1);
    }

    #[test]
    fn nested_guards_restart_the_delays_once_at_the_last_close() {
        let d = ModalDepth::default();
        let count = Cell::new(0);
        {
            let _outer = d.enter(|| count.set(count.get() + 1));
            {
                let _inner = d.enter(|| count.set(count.get() + 1));
                assert!(d.is_open());
            }
            assert!(d.is_open(), "the outer modal is still open");
            assert_eq!(count.get(), 0);
        }
        assert!(!d.is_open());
        assert_eq!(count.get(), 1);
    }

    #[test]
    fn the_guard_is_reset_by_an_early_return_or_a_panic() {
        let d = ModalDepth::default();
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _g = d.enter(|| {});
            panic!("dialog failed");
        }));
        assert!(r.is_err());
        assert!(!d.is_open());
    }

    /// Owner decision on #30: the lock is not postponed by a dialog.
    #[test]
    fn a_due_lock_cancels_the_dialogs_then_locks() {
        use std::time::Duration;
        assert_eq!(lock_request(false), LockRequest::LockNow);
        assert_eq!(lock_request(true), LockRequest::CancelDialogsThenLock);
        // The dialogs close within a tick or two: then the lock runs.
        let lock = Some(LOCK_FORCE_AFTER);
        assert_eq!(pending_step(true, Duration::from_millis(200), lock), PendingStep::Wait);
        assert_eq!(pending_step(false, Duration::from_millis(400), lock), PendingStep::Run);
        // A box that cannot be cancelled does not stop it.
        assert_eq!(pending_step(true, LOCK_FORCE_AFTER, lock), PendingStep::Force);
        // A close waits for the user, never forced.
        assert_eq!(pending_step(true, Duration::from_secs(60), None), PendingStep::Wait);
        assert_eq!(pending_step(false, Duration::from_secs(60), None), PendingStep::Run);
    }

    /// Auto-save stays suspended under a dialog.
    #[test]
    fn auto_save_is_still_skipped_under_a_dialog() {
        let g = TimerGate::default();
        assert_eq!(g.tick(true, false), TimerTick::Skip);
        assert_eq!(g.tick(false, false), TimerTick::RestartDelay);
    }

    #[test]
    fn timers_wait_for_the_modal_then_restart_their_delay() {
        let g = TimerGate::default();
        assert_eq!(g.tick(false, false), TimerTick::Run);
        assert_eq!(g.tick(true, false), TimerTick::Skip);
        assert_eq!(g.tick(true, false), TimerTick::Skip);
        assert_eq!(g.tick(false, false), TimerTick::RestartDelay);
        assert_eq!(g.tick(false, false), TimerTick::Run);
        // Locked: nothing, and no restart owed.
        assert_eq!(g.tick(false, true), TimerTick::Skip);
        assert_eq!(g.tick(false, false), TimerTick::Run);
    }
}
