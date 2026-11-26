// Modal dialogs of the main window and the timers (issue #30).
//
// Every dialog of LockNote runs its own message loop: the main window's
// timers keep firing under it. Auto-save could write changes the user is
// about to decline: it is suspended while a modal is open and its delay
// restarts when the last one closes. Auto-lock (owner decision on #30,
// pass 5): when it is due with dialogs open, they are cancelled (their input
// is lost, the note is not) and the lock runs from the application's loop
// once they are gone. A dialog that does not close (busy, a box without
// Cancel / No) cannot postpone the hiding of the note: at `LOCK_DEADLINE` a
// curtain covers the window and hides the LockNote dialogs; the wipe runs
// when the last dialog is gone (`lock_step`). The manual lock and the close
// requested under a dialog go through the same path. A modal is "open" when an RAII `ModalGuard`
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

/// A lock still waiting after this shows the curtain (WARN once, with what
/// is still open).
pub(super) const LOCK_DEADLINE: std::time::Duration = std::time::Duration::from_secs(2);
/// How often the dialogs that did not close are asked again.
pub(super) const RECANCEL_EVERY: std::time::Duration = std::time::Duration::from_secs(1);

/// What a tick does with a pending lock (pass 5, owner decision): past
/// `LOCK_DEADLINE` with a dialog still open, a curtain hides the note on
/// time; the wipe itself still waits until no dialog remains.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum LockStep {
    Wait,
    CancelAgain,
    /// Hide the note now (overlay without unlock field, LockNote dialogs
    /// hidden); the lock becomes a manual one, it cannot be undone by a
    /// mouse move.
    ShowCurtain,
    /// No dialog left: lock from the application's own loop.
    Run,
}

pub(super) fn lock_step(modal_open: bool, since_cancel: std::time::Duration, waited: std::time::Duration, curtain_shown: bool) -> LockStep {
    if !modal_open {
        LockStep::Run
    } else if waited >= LOCK_DEADLINE && !curtain_shown {
        LockStep::ShowCurtain
    } else if since_cancel >= RECANCEL_EVERY {
        LockStep::CancelAgain
    } else {
        LockStep::Wait
    }
}

/// A second lock request while one is pending: a manual one wins (pass 5:
/// Ctrl+L under a dialog was lost behind a pending auto lock).
pub(super) fn merged_manual(pending_manual: bool, new_manual: bool) -> bool {
    pending_manual || new_manual
}

/// Whether a pending lock still locks when it runs: a manual one always;
/// an automatic one only if the inactivity is still there (`still_due`).
pub(super) fn lock_goes_on(manual: bool, still_due: bool) -> bool {
    manual || still_due
}

/// How long a close waits for a dialog that does not close before LockNote
/// offers to close without saving (pass 6: tray Quit hung silently on a
/// busy dialog).
pub(super) const CLOSE_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

/// What a pending close does on a tick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum CloseStep {
    Wait,
    CancelAgain,
    /// Ask once: keep waiting, or close without saving.
    Offer,
    Run,
}

pub(super) fn close_step(modal_open: bool, since_cancel: std::time::Duration, waited: std::time::Duration, asked: bool, curtain: bool) -> CloseStep {
    if !modal_open {
        CloseStep::Run
    } else if waited >= CLOSE_WAIT && !asked && !curtain {
        CloseStep::Offer
    } else if since_cancel >= RECANCEL_EVERY {
        CloseStep::CancelAgain
    } else {
        CloseStep::Wait
    }
}

/// The curtain over the note (pass 5/6): the lock screen while the note is
/// not locked yet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Curtain {
    None,
    /// A lock waits for dialogs that did not close.
    Waiting,
    /// The save before an automatic lock failed: lifted only with the
    /// password, the unsaved changes kept (owner decision, pass 6).
    SaveFailed,
}

/// A lock or a close waiting for the dialogs to return.
#[derive(Debug, Clone, Copy)]
pub(super) struct Pending {
    pub(super) since: std::time::Instant,
    pub(super) last_cancel: std::time::Instant,
    /// Lock only: the Lock button / Ctrl+L.
    pub(super) manual: bool,
    pub(super) error_logged: bool,
    /// The curtain hides the note (lock only).
    pub(super) curtain: bool,
}

impl Pending {
    pub(super) fn new(manual: bool) -> Pending {
        let now = std::time::Instant::now();
        Pending { since: now, last_cancel: now, manual, error_logged: false, curtain: false }
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
        // The sequence lives in lock_flow (tested with a fake, pass 7).
        if self.lock_in_progress.get() && self.lock_box_queue.request(manual) {
            eprintln!("INFO lock: requests wait for the lock's box manual={}", manual);
        }
        super::lock_flow::request(&mut FormLockSteps { form: self }, manual);
        if let Some(p) = self.lock_pending.get() {
            if p.last_cancel == p.since && !self.lock_in_progress.get() {
                eprintln!("INFO lock: due with dialogs open manual={}", p.manual);
            }
        }
    }

    /// Close requested under a dialog (tray Quit, title bar): cancel the
    /// dialogs, then run the close flow from the application's own loop.
    pub(super) fn request_close_after_dialogs(&self) {
        if self.close_pending.get().is_none() {
            self.close_pending.set(Some(Pending::new(false)));
        }
        // The lock's own box (reason of a failed save) is never cancelled:
        // the close waits for it (pass 6).
        if self.lock_in_progress.get() {
            eprintln!("INFO close: queued after the lock's box");
            return;
        }
        let n = cancel_open_dialogs();
        eprintln!("INFO close: requested with dialogs open, cancelled={}", n);
    }

    /// Run the pending lock or close once every dialog has returned
    /// (highlight tick, every 200 ms, from the application's own loop: the
    /// tick of a nested loop finds `modal_open()` true and only waits).
    pub(super) fn run_pending_requests(&self) {
        if let Some(p) = self.lock_pending.get() {
            let asked_again = super::lock_flow::tick(&mut FormLockSteps { form: self }, p.since.elapsed(), p.last_cancel.elapsed());
            if let Some(mut p) = self.lock_pending.get() {
                if asked_again {
                    p.last_cancel = std::time::Instant::now();
                    self.lock_pending.set(Some(p));
                    eprintln!("INFO lock: dialogs asked to close again");
                }
                if p.curtain {
                    // Dialogs opened meanwhile must not show over it; the
                    // subtitle names the one it waits for.
                    hide_registered_dialogs();
                    self.set_curtain_waiting_for(&first_dialog_title());
                }
            }
        }
        if let Some(mut p) = self.close_pending.get() {
            let curtain = self.curtain.get() != Curtain::None;
            let own_box = self.lock_in_progress.get() || crate::ui::dialogs::style::protected_dialog_open();
            match super::lock_flow::close_tick(self.modal_open(), own_box, p.last_cancel.elapsed(), p.since.elapsed(), p.error_logged, curtain) {
                CloseStep::Wait => {
                    // Under the curtain a quit waiting says so (pass 7).
                    if self.curtain.get() == Curtain::Waiting && p.since.elapsed() >= CLOSE_WAIT {
                        self.set_curtain_quit_waiting(&first_dialog_title());
                    }
                }
                CloseStep::CancelAgain => {
                    if !self.lock_in_progress.get() {
                        cancel_open_dialogs();
                    }
                    p.last_cancel = std::time::Instant::now();
                    self.close_pending.set(Some(p));
                }
                CloseStep::Offer => {
                    p.error_logged = true;
                    self.close_pending.set(Some(p));
                    self.offer_close_without_saving();
                }
                CloseStep::Run => {
                    self.close_pending.set(None);
                    if self.on_close() {
                        nwg::stop_thread_dispatch();
                    }
                }
            }
        }
    }

    /// A close stuck behind a dialog that does not close: say which, and
    /// offer to close without saving (never a silent hang, pass 6). That
    /// exit runs from inside the stuck dialog's loop, so it is the one of
    /// the end of a session (nothing saved, keys removed, stores wiped,
    /// pending save promoted), then the process ends.
    fn offer_close_without_saving(&self) {
        let window = first_dialog_title();
        eprintln!("WARN close: still waiting for \"{}\" after {} s", window, CLOSE_WAIT.as_secs());
        let msg = t("close.stuck").replace("{window}", if window.is_empty() { "?" } else { &window });
        let answer = {
            let _modal = self.modal();
            crate::ui::dialogs::close_confirm::SaveFailedDialog::show_with(
                Some(self.window.handle), &msg, &t("close.keep_waiting"), &t("close.close_unsaved"),
            )
        };
        if !crate::ui::dialogs::close_confirm::closes_without_saving(answer) {
            eprintln!("INFO close: the user keeps waiting");
            return;
        }
        eprintln!("WARN close: closing without saving while \"{}\" is stuck (user choice)", window);
        use super::end_session::EndSessionSteps;
        let mut steps = super::end_session::FormEndSessionSteps { form: self };
        steps.mark_exiting();
        steps.unload_agent_keys();
        steps.wipe();
        steps.finish_exit();
        std::process::exit(0);
    }

    /// A message of the main window (pass 5: counted as a modal, owned by
    /// the main window), for every information box of the editor.
    pub(super) fn inform(&self, title: &str, content: &str) {
        let _modal = self.modal();
        // The only direct call of the editor (scan test).
        nwg::modal_info_message(&self.window, title, content);
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

/// `LockSteps` on the real window (pass 7).
pub(super) struct FormLockSteps<'a> {
    pub(super) form: &'a EditorForm,
}

impl super::lock_flow::LockSteps for FormLockSteps<'_> {
    fn modal_open(&self) -> bool {
        self.form.modal_open()
    }
    fn own_box_open(&self) -> bool {
        self.form.lock_in_progress.get() || crate::ui::dialogs::style::protected_dialog_open()
    }
    fn cancel_dialogs(&mut self) {
        let n = cancel_open_dialogs();
        eprintln!("INFO lock: dialogs asked to close count={}", n);
    }
    fn locked(&self) -> bool {
        *self.form.is_locked.borrow()
    }
    fn curtain(&self) -> Curtain {
        self.form.curtain.get()
    }
    fn show_curtain(&mut self, mode: Curtain) {
        if mode == Curtain::Waiting && self.form.curtain.get() == Curtain::None {
            eprintln!("WARN lock: dialogs still open, note hidden, lock when they close: {}", describe_open_dialogs());
        }
        self.form.show_curtain(mode);
        if mode == Curtain::Waiting {
            self.form.set_curtain_waiting_for(&first_dialog_title());
            hide_registered_dialogs();
        }
    }
    fn hide_curtain(&mut self) {
        self.form.hide_curtain();
    }
    fn still_due(&self) -> bool {
        !matches!(self.form.autolock.borrow().lock_decision(), crate::features::autolock::LockDecision::Wait)
    }
    fn modified(&self) -> bool {
        self.form.is_modified.load(Ordering::Relaxed)
    }
    fn save(&mut self) -> Result<(), String> {
        // Short: the guard is not held across the question that may follow.
        let _persistence_guard = crate::features::persistence_lock::acquire();
        self.form.save_note().map_err(|f| f.message().to_string())
    }
    fn ask_lock_without_saving(&mut self, reason: &str) -> bool {
        let msg = t("lock.save_failed.manual").replace("{error}", reason);
        let answer = {
            let _modal = self.form.modal();
            crate::ui::dialogs::close_confirm::SaveFailedDialog::show_with(
                Some(self.form.window.handle), &msg, &t("lock.keep_open"), &t("lock.lock_unsaved"),
            )
        };
        crate::ui::dialogs::close_confirm::closes_without_saving(answer)
    }
    fn set_save_failed_reason(&mut self, reason: String) {
        *self.form.lock_save_failed_reason.borrow_mut() = reason;
    }
    fn lock_now(&mut self) {
        let _persistence_guard = crate::features::persistence_lock::acquire();
        self.form.do_lock();
    }
    fn reset_autolock(&mut self) {
        self.form.autolock.borrow_mut().reset();
    }
    fn pending(&self) -> Option<super::lock_flow::PendingLock> {
        self.form.lock_pending.get().map(|p| super::lock_flow::PendingLock { manual: p.manual, committed: p.curtain })
    }
    fn set_pending(&mut self, p: Option<super::lock_flow::PendingLock>) {
        let merged = p.map(|pl| {
            let mut q = self.form.lock_pending.get().unwrap_or_else(|| Pending::new(pl.manual));
            q.manual = pl.manual;
            q.curtain = pl.committed;
            q
        });
        self.form.lock_pending.set(merged);
    }
    fn in_box(&self) -> bool {
        self.form.lock_in_progress.get()
    }
    fn set_in_box(&mut self, v: bool) {
        self.form.lock_in_progress.set(v);
    }
    fn queue_manual(&mut self) {
        // Recorded by request_lock (LockBoxQueue logs once per box).
    }
    fn take_queued_manual(&mut self) -> bool {
        self.form.lock_box_queue.take_manual()
    }
}

/// Whether a dialog result may be applied: same lock epoch as when it was
/// opened, and the note neither locked nor exiting.
pub(super) fn result_applies(epoch_before: u64, epoch_now: u64, locked: bool, exiting: bool) -> bool {
    epoch_before == epoch_now && !locked && !exiting
}

/// The title of the first LockNote dialog still open (the window the
/// curtain waits for), or of a standard dialog.
fn first_dialog_title() -> String {
    use windows::Win32::UI::WindowsAndMessaging::GetWindowTextW;
    for h in crate::ui::dialogs::style::registered_dialogs() {
        let mut title = [0u16; 128];
        let n = unsafe { GetWindowTextW(windows::Win32::Foundation::HWND(h as *mut _), &mut title) } as usize;
        if n > 0 {
            return String::from_utf16_lossy(&title[..n]);
        }
    }
    String::new()
}

/// Hide the LockNote dialogs under the curtain (pass 5). They stay
/// registered, so `modal_open()` stays true and the lock still waits for
/// them; standard dialogs (#32770) are not hidden: they are counted by their
/// visibility.
fn hide_registered_dialogs() {
    use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
    for h in crate::ui::dialogs::style::registered_dialogs() {
        // The return value is the previous visibility, not an error.
        let _was_visible = unsafe { ShowWindow(windows::Win32::Foundation::HWND(h as *mut _), SW_HIDE) };
    }
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

/// Ask every open LockNote dialog (not the protected decision boxes, see
/// `style::protect`), standard dialog and popup menu of this thread to
/// close as No / Cancel: WM_CLOSE to LockNote's dialogs; IDCANCEL,
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

    /// Pass 5: a dialog that never closes no longer postpones the lock
    /// forever: the curtain hides the note at the deadline, the lock runs
    /// once the dialog is gone.
    #[test]
    fn past_the_deadline_a_curtain_hides_the_note_then_the_lock_runs() {
        use std::time::Duration;
        let s = Duration::from_millis;
        assert_eq!(lock_step(true, s(200), s(200), false), LockStep::Wait);
        assert_eq!(lock_step(true, RECANCEL_EVERY, s(1000), false), LockStep::CancelAgain);
        assert_eq!(lock_step(true, s(200), LOCK_DEADLINE, false), LockStep::ShowCurtain);
        // Curtain up: keep asking the dialog to close, never run under it.
        assert_eq!(lock_step(true, RECANCEL_EVERY, s(60_000), true), LockStep::CancelAgain);
        assert_eq!(lock_step(true, s(100), s(60_000), true), LockStep::Wait);
        // The dialog closed: the real lock.
        assert_eq!(lock_step(false, s(100), s(60_000), true), LockStep::Run);
    }

    /// Pass 5: auto then manual under a dialog keeps the manual one, and
    /// user activity cancels only an automatic lock.
    #[test]
    fn a_manual_request_wins_and_activity_cancels_only_an_auto_lock() {
        assert!(merged_manual(false, true));
        assert!(merged_manual(true, false));
        assert!(!merged_manual(false, false));
        assert!(lock_goes_on(true, false), "manual: always");
        assert!(!lock_goes_on(false, false), "auto, the user came back");
        assert!(lock_goes_on(false, true));
    }

    /// Pass 6: a close waiting for a dialog that stays open offers, once and
    /// never under the curtain, to close without saving.
    #[test]
    fn a_close_stuck_behind_a_dialog_offers_to_close_without_saving() {
        use std::time::Duration;
        let s = Duration::from_millis;
        assert_eq!(close_step(false, s(0), s(0), false, false), CloseStep::Run);
        assert_eq!(close_step(true, s(100), s(500), false, false), CloseStep::Wait);
        assert_eq!(close_step(true, RECANCEL_EVERY, s(2000), false, false), CloseStep::CancelAgain);
        assert_eq!(close_step(true, s(100), CLOSE_WAIT, false, false), CloseStep::Offer);
        assert_eq!(close_step(true, RECANCEL_EVERY, CLOSE_WAIT, true, false), CloseStep::CancelAgain, "asked once");
        assert_eq!(close_step(true, s(100), CLOSE_WAIT, false, true), CloseStep::Wait, "not under the curtain");
    }

    /// Owner decision on #30: the lock is not postponed by a dialog.
    #[test]
    fn a_due_lock_cancels_the_dialogs_then_locks() {
        use std::time::Duration;
        // The dialogs close within a tick or two: then the lock runs, only
        // once no modal is left (the stack has unwound).
        assert_eq!(lock_step(true, Duration::from_millis(200), Duration::from_millis(200), false), LockStep::Wait);
        assert_eq!(lock_step(false, Duration::from_millis(400), Duration::from_millis(400), false), LockStep::Run);
        // Pass 4: never run under a dialog that stays open; ask it again.
        assert_eq!(lock_step(true, RECANCEL_EVERY, Duration::from_millis(1000), false), LockStep::CancelAgain);
        assert_eq!(lock_step(true, Duration::from_secs(60), Duration::from_secs(60), true), LockStep::CancelAgain);
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
