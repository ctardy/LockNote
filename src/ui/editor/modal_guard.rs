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

/// What a tick does with a lock or a close requested while dialogs were
/// open. Pass 4: there is no "run it under the dialog" any more: a lock
/// inside a dialog's nested loop wiped stores its caller still used.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum PendingStep {
    /// The dialogs are closing: check again at the next tick.
    Wait,
    /// Still open after `RECANCEL_EVERY`: ask them to close again.
    CancelAgain,
    /// No dialog left: run it now, from the application's own loop, once
    /// every dialog has returned.
    Run,
}

/// A lock still waiting after this is reported as an ERROR (once), with
/// what is still open.
pub(super) const LOCK_DEADLINE: std::time::Duration = std::time::Duration::from_secs(2);
/// How often the dialogs that did not close are asked again.
pub(super) const RECANCEL_EVERY: std::time::Duration = std::time::Duration::from_secs(1);

/// The step of a pending request; `since_cancel` is the time since the
/// dialogs were last asked to close.
pub(super) fn pending_step(modal_open: bool, since_cancel: std::time::Duration) -> PendingStep {
    if !modal_open {
        PendingStep::Run
    } else if since_cancel >= RECANCEL_EVERY {
        PendingStep::CancelAgain
    } else {
        PendingStep::Wait
    }
}

/// Whether a lock waiting for `waited` is overdue (ERROR, once).
pub(super) fn lock_overdue(waited: std::time::Duration) -> bool {
    waited >= LOCK_DEADLINE
}

/// A lock or a close waiting for the dialogs to return.
#[derive(Debug, Clone, Copy)]
pub(super) struct Pending {
    pub(super) since: std::time::Instant,
    pub(super) last_cancel: std::time::Instant,
    /// Lock only: the Lock button / Ctrl+L.
    pub(super) manual: bool,
    pub(super) error_logged: bool,
}

impl Pending {
    pub(super) fn new(manual: bool) -> Pending {
        let now = std::time::Instant::now();
        Pending { since: now, last_cancel: now, manual, error_logged: false }
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
                    self.lock_pending.set(Some(Pending::new(manual)));
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
            self.close_pending.set(Some(Pending::new(false)));
        }
        let n = cancel_open_dialogs();
        eprintln!("INFO close: requested with dialogs open, cancelled={}", n);
    }

    /// Run the pending lock or close once every dialog has returned
    /// (highlight tick, every 200 ms, from the application's own loop: the
    /// tick of a nested loop finds `modal_open()` true and only waits).
    pub(super) fn run_pending_requests(&self) {
        if let Some(mut p) = self.lock_pending.get() {
            match pending_step(self.modal_open(), p.last_cancel.elapsed()) {
                PendingStep::Wait => {}
                PendingStep::CancelAgain => {
                    let n = cancel_open_dialogs();
                    p.last_cancel = std::time::Instant::now();
                    if lock_overdue(p.since.elapsed()) && !p.error_logged {
                        p.error_logged = true;
                        eprintln!(
                            "ERROR lock: waiting {} ms, dialogs still open: {}",
                            p.since.elapsed().as_millis(),
                            describe_open_dialogs()
                        );
                    }
                    eprintln!("INFO lock: dialogs asked to close again count={}", n);
                    self.lock_pending.set(Some(p));
                }
                PendingStep::Run => {
                    self.lock_pending.set(None);
                    eprintln!("INFO lock: running after the dialogs closed delay_ms={}", p.since.elapsed().as_millis());
                    self.execute_lock(p.manual);
                }
            }
        }
        if let Some(mut p) = self.close_pending.get() {
            match pending_step(self.modal_open(), p.last_cancel.elapsed()) {
                PendingStep::Wait => {}
                PendingStep::CancelAgain => {
                    cancel_open_dialogs();
                    p.last_cancel = std::time::Instant::now();
                    self.close_pending.set(Some(p));
                }
                PendingStep::Run => {
                    self.close_pending.set(None);
                    if self.on_close() {
                        nwg::stop_thread_dispatch();
                    }
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
                        eprintln!("WARN autolock: skip due to save failure: {}", msg);
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

    /// The lock epoch: +1 at every lock (pass 4). A dialog's result is
    /// applied only if the epoch did not change while it was open.
    pub(super) fn lock_epoch(&self) -> u64 {
        self.lock_epoch.get()
    }

    /// Whether what a dialog opened at `epoch` returned may still change the
    /// note.
    pub(super) fn still_valid(&self, epoch: u64) -> bool {
        result_applies(epoch, self.lock_epoch.get(), *self.is_locked.borrow(), self.exiting.get())
    }

    /// Every Yes/No question of the main window (pass 4): true only for Yes,
    /// asked and answered with no lock in between, on an unlocked window.
    /// The box is owned by the main window and counted as a modal.
    pub(super) fn confirm(&self, params: &nwg::MessageParams) -> bool {
        let epoch = self.lock_epoch();
        let choice = {
            let _modal = self.modal();
            nwg::modal_message(&self.window, params)
        };
        let yes = choice == nwg::MessageChoice::Yes;
        let applies = yes && self.still_valid(epoch);
        if yes && !applies {
            eprintln!("INFO confirm: Yes dropped, the note locked meanwhile");
        }
        applies
    }
}

/// Whether a dialog result may be applied: same lock epoch as when it was
/// opened, and the note neither locked nor exiting.
pub(super) fn result_applies(epoch_before: u64, epoch_now: u64, locked: bool, exiting: bool) -> bool {
    epoch_before == epoch_now && !locked && !exiting
}

/// Class and title of the windows of this thread still open, for the
/// ERROR of an overdue lock.
fn describe_open_dialogs() -> String {
    use windows::Win32::Foundation::{HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{EnumThreadWindows, GetClassNameW, GetWindowTextW, IsWindowVisible};
    unsafe extern "system" fn cb(h: HWND, lp: LPARAM) -> windows::core::BOOL {
        let out = &mut *(lp.0 as *mut Vec<String>);
        if IsWindowVisible(h).as_bool() {
            let mut class = [0u16; 64];
            let n = GetClassNameW(h, &mut class) as usize;
            let mut title = [0u16; 128];
            let m = GetWindowTextW(h, &mut title) as usize;
            out.push(format!("{} \"{}\"", String::from_utf16_lossy(&class[..n]), String::from_utf16_lossy(&title[..m])));
        }
        true.into()
    }
    let mut out: Vec<String> = Vec::new();
    unsafe {
        let tid = windows::Win32::System::Threading::GetCurrentThreadId();
        let _all_enumerated = EnumThreadWindows(tid, Some(cb), LPARAM(&mut out as *mut _ as isize));
    }
    out.join(", ")
}

/// Ask every open LockNote dialog, standard dialog and popup menu of this
/// thread to close as No / Cancel: WM_CLOSE to LockNote's dialogs; IDCANCEL,
/// IDNO and WM_CLOSE to standard dialogs (a Yes/No box only accepts IDNO, an
/// overwrite prompt of a file dialog too); EndMenu for a popup menu. Returns
/// how many windows were asked.
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
                // IDCANCEL = 2, IDNO = 7: whichever the box has answers.
                for id in [2usize, 7] {
                    if let Err(e) = PostMessageW(Some(hwnd), WM_COMMAND, WPARAM(id), LPARAM(0)) {
                        eprintln!("WARN modal: command {} not posted err={:?}", id, e);
                    }
                }
            }
            if let Err(e) = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) {
                eprintln!("WARN modal: WM_CLOSE not posted err={:?}", e);
            }
        }
    }
    // A popup menu (tab context menu) runs its own loop too.
    unsafe {
        if let Err(e) = windows::Win32::UI::WindowsAndMessaging::EndMenu() {
            eprintln!("INFO modal: EndMenu err={:?}", e);
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
            // #32770: standard dialog; #32768: a popup menu in its loop.
            let class = String::from_utf16_lossy(&buf[..n]);
            if class == "#32770" || class == "#32768" {
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

    /// Pass 4: a question answered Yes does nothing if a lock came in between.
    #[test]
    fn a_result_applies_only_without_a_lock_in_between() {
        assert!(result_applies(3, 3, false, false));
        assert!(!result_applies(3, 4, false, false), "epoch changed");
        assert!(!result_applies(3, 3, true, false), "locked");
        assert!(!result_applies(3, 3, false, true), "exiting");
    }

    /// Owner decision on #30: the lock is not postponed by a dialog.
    #[test]
    fn a_due_lock_cancels_the_dialogs_then_locks() {
        use std::time::Duration;
        assert_eq!(lock_request(false), LockRequest::LockNow);
        assert_eq!(lock_request(true), LockRequest::CancelDialogsThenLock);
        // The dialogs close within a tick or two: then the lock runs, only
        // once no modal is left (the stack has unwound).
        assert_eq!(pending_step(true, Duration::from_millis(200)), PendingStep::Wait);
        assert_eq!(pending_step(false, Duration::from_millis(400)), PendingStep::Run);
        // Pass 4: never run under a dialog that stays open; ask it again.
        assert_eq!(pending_step(true, RECANCEL_EVERY), PendingStep::CancelAgain);
        assert_eq!(pending_step(true, Duration::from_secs(60)), PendingStep::CancelAgain);
        assert!(!lock_overdue(Duration::from_millis(1999)));
        assert!(lock_overdue(LOCK_DEADLINE));
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
