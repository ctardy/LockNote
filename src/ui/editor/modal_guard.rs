// Modal dialogs of the main window and the timers (issue #30).
//
// Every dialog of LockNote runs its own message loop: the main window's
// timers keep firing under it. Auto-save could write changes the user is
// about to decline: it is suspended while a modal is open and its delay
// restarts when the last one closes. The lock follows one rule (owner
// decision on #30, pass 8, see lock_flow.rs): curtain at once, dialogs
// answered harmlessly, real lock only with no window open and the note
// saved. A modal is "open" when an RAII `ModalGuard` is alive, or when a
// LockNote dialog or message box of this thread is shown (the probe in
// `EditorForm::modal_open` catches the dialogs opened without a guard).

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
    /// `locked`: locked or under the curtain (`input_blocked`, pass 9).
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

/// How long a close waits for a dialog that does not close before LockNote
/// offers to close without saving (pass 6: tray Quit hung silently on a
/// busy dialog).
pub(super) const CLOSE_WAIT: std::time::Duration = std::time::Duration::from_secs(10);
use super::lock_flow::RECANCEL_EVERY;

/// What a pending close does on a tick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum CloseStep {
    Wait,
    CancelAgain,
    /// Ask once: keep waiting, or close without saving.
    Offer,
    Run,
}

/// `own_box_open`: one of the close flow's own boxes is shown (pass 9):
/// the close waits for its answer and never cancels it (the lock does).
pub(super) fn close_step(modal_open: bool, own_box_open: bool, since_cancel: std::time::Duration, waited: std::time::Duration, asked: bool, curtain: bool) -> CloseStep {
    if !modal_open {
        CloseStep::Run
    } else if own_box_open {
        CloseStep::Wait
    } else if waited >= CLOSE_WAIT && !asked && !curtain {
        CloseStep::Offer
    } else if since_cancel >= RECANCEL_EVERY {
        CloseStep::CancelAgain
    } else {
        CloseStep::Wait
    }
}

/// What a pending close needs from the window (pass 9: the fake of
/// lock_flow drives it with the lock).
pub(super) trait CloseWaitSteps {
    /// A dialog or message box is open (the close flow's own included).
    fn dialogs_open(&self) -> bool;
    /// One of the close flow's own boxes is open: the close prompt, its
    /// save-failed question or the stuck-close offer.
    fn own_close_box_open(&self) -> bool;
    fn curtain_is_up(&self) -> bool;
    /// Ask the open dialogs to close (No / Cancel).
    fn cancel_for_close(&mut self);
    /// Remember the offer was made, before it is shown (its own loop ticks).
    fn mark_offered(&mut self);
    fn offer_close_without_saving(&mut self);
    /// No dialog left: run the close flow.
    fn close(&mut self);
}

/// A tick of a pending close: `since_cancel` since the dialogs were last
/// asked, `waited` since the close was requested, `asked` once the offer
/// was made.
pub(super) fn close_tick<S: CloseWaitSteps>(s: &mut S, since_cancel: std::time::Duration, waited: std::time::Duration, asked: bool) -> CloseStep {
    let step = close_step(s.dialogs_open(), s.own_close_box_open(), since_cancel, waited, asked, s.curtain_is_up());
    match step {
        CloseStep::Wait => {}
        CloseStep::CancelAgain => s.cancel_for_close(),
        CloseStep::Offer => {
            s.mark_offered();
            s.offer_close_without_saving();
        }
        CloseStep::Run => s.close(),
    }
    step
}

/// The curtain over the note: the lock screen while the note is not locked
/// (wiped) yet; only the password lifts it (pass 8: one kind).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Curtain {
    None,
    Up,
}

/// A close waiting for the dialogs to return.
#[derive(Debug, Clone, Copy)]
pub(super) struct Pending {
    pub(super) since: std::time::Instant,
    pub(super) last_cancel: std::time::Instant,
    /// The stuck-close offer was shown.
    pub(super) error_logged: bool,
}

impl Pending {
    pub(super) fn new() -> Pending {
        let now = std::time::Instant::now();
        Pending { since: now, last_cancel: now, error_logged: false }
    }
}

/// The real lock waiting under the curtain (pass 8).
#[derive(Debug, Clone, Copy)]
pub(super) struct LockPending {
    pub(super) last_cancel: std::time::Instant,
    pub(super) last_try: Option<std::time::Instant>,
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

    /// A lock is due (auto-lock delay, Lock button, Ctrl+L): the single
    /// rule of lock_flow (tested there with a fake).
    pub(super) fn request_lock(&self) {
        super::lock_flow::request(&mut FormLockSteps { form: self });
        self.update_curtain_subtitle();
    }

    /// Close requested under a dialog (tray Quit, title bar): cancel the
    /// dialogs, then run the close flow from the application's own loop.
    pub(super) fn request_close_after_dialogs(&self) {
        if self.close_pending.get().is_none() {
            self.close_pending.set(Some(Pending::new()));
        }
        let n = cancel_open_dialogs();
        eprintln!("INFO close: requested with dialogs open, cancelled={}", n);
    }

    /// Run the pending lock or close (highlight tick, every 200 ms). The
    /// tick also fires inside a dialog's own loop: it then sees
    /// `modal_open()` and asks that dialog again (lock: always; close: not
    /// the close flow's own boxes, `close_box_open`), and the real lock or
    /// the close only runs once no dialog is left.
    pub(super) fn run_pending_requests(&self) {
        if let Some(p) = self.lock_pending.get() {
            // The save attempt is recorded by the step itself (mark_tried).
            let done = super::lock_flow::tick(&mut FormLockSteps { form: self }, p.last_cancel.elapsed(), p.last_try.map(|t| t.elapsed()));
            if let Some(mut now) = self.lock_pending.get() {
                if done.cancelled {
                    now.last_cancel = std::time::Instant::now();
                    self.lock_pending.set(Some(now));
                }
                // Dialogs opened meanwhile must not show over the curtain.
                hide_registered_dialogs();
            }
            self.update_curtain_subtitle();
        }
        if let Some(p) = self.close_pending.get() {
            close_tick(&mut FormCloseWaitSteps { form: self }, p.last_cancel.elapsed(), p.since.elapsed(), p.error_logged);
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
        let epoch = self.lock_epoch();
        let answer = self.close_box(|| {
            let _modal = self.modal();
            crate::ui::dialogs::close_confirm::SaveFailedDialog::show_with(
                Some(self.window.handle), &msg, &t("close.keep_waiting"), &t("close.close_unsaved"),
            )
        });
        // An answer returning after a curtain went up is "Keep waiting".
        if !crate::ui::dialogs::close_confirm::closes_without_saving(answer) || !self.still_valid(epoch) {
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

    /// Show one of the close flow's own boxes (close prompt, its
    /// save-failed question, stuck-close offer): while it is open the close
    /// path waits for its answer instead of cancelling it (pass 9). Reset
    /// on every way out, a panic included.
    pub(super) fn close_box<R>(&self, show: impl FnOnce() -> R) -> R {
        struct Reset<'a>(&'a Cell<bool>, bool);
        impl Drop for Reset<'_> {
            fn drop(&mut self) {
                self.0.set(self.1);
            }
        }
        let _reset = Reset(&self.close_box_open, self.close_box_open.replace(true));
        show()
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
        // The one shared check of every answer (pass 8): the curtain moved
        // the epoch, and none may be up.
        super::lock_flow::answer_applies(epoch, self.lock_epoch.get(), self.curtain.get() != Curtain::None, *self.is_locked.borrow())
            && !self.exiting.get()
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
    fn cancel_dialogs(&mut self) {
        let n = cancel_open_dialogs();
        hide_registered_dialogs();
        eprintln!("INFO lock: open dialogs answered No / Cancel count={}", n);
    }
    fn locked(&self) -> bool {
        *self.form.is_locked.borrow()
    }
    fn curtain_up(&self) -> bool {
        self.form.curtain.get() != Curtain::None
    }
    fn show_curtain(&mut self) {
        self.form.show_curtain();
    }
    fn hide_curtain(&mut self) {
        self.form.hide_curtain();
        show_registered_dialogs();
    }
    fn epoch(&self) -> u64 {
        self.form.lock_epoch.get()
    }
    fn bump_epoch(&mut self) {
        self.form.lock_epoch.set(self.form.lock_epoch.get() + 1);
    }
    fn modified(&self) -> bool {
        self.form.is_modified.load(Ordering::Relaxed)
    }
    fn mark_tried(&mut self) {
        if let Some(mut p) = self.form.lock_pending.get() {
            p.last_try = Some(std::time::Instant::now());
            self.form.lock_pending.set(Some(p));
        }
    }
    fn save(&mut self) -> Result<(), String> {
        // The persistence guard only here and in lock_now (scan test).
        let _persistence_guard = crate::features::persistence_lock::acquire();
        self.form.save_note().map_err(|f| f.message().to_string())
    }
    fn save_failed_reason(&self) -> Option<String> {
        self.form.lock_save_failed_reason.borrow().clone()
    }
    fn set_save_failed_reason(&mut self, reason: Option<String>) {
        *self.form.lock_save_failed_reason.borrow_mut() = reason;
    }
    fn lock_now(&mut self) {
        let _persistence_guard = crate::features::persistence_lock::acquire();
        self.form.do_lock();
    }
    fn pending(&self) -> bool {
        self.form.lock_pending.get().is_some()
    }
    fn set_pending(&mut self, pending: bool) {
        if !pending {
            self.form.lock_pending.set(None);
        } else if self.form.lock_pending.get().is_none() {
            self.form.lock_pending.set(Some(LockPending { last_cancel: std::time::Instant::now(), last_try: None }));
        }
    }
    fn inform_save_failed(&mut self, reason: &str) {
        self.form.inform(&t("save.error.title"), &lock_save_failed_text(reason));
    }
    fn reset_autolock(&mut self) {
        self.form.autolock.borrow_mut().reset();
    }
}

/// `CloseWaitSteps` on the real window (pass 9).
struct FormCloseWaitSteps<'a> {
    form: &'a EditorForm,
}

impl CloseWaitSteps for FormCloseWaitSteps<'_> {
    fn dialogs_open(&self) -> bool {
        self.form.modal_open()
    }
    fn own_close_box_open(&self) -> bool {
        self.form.close_box_open.get()
    }
    fn curtain_is_up(&self) -> bool {
        self.form.curtain.get() != Curtain::None
    }
    fn cancel_for_close(&mut self) {
        cancel_open_dialogs();
        if let Some(mut p) = self.form.close_pending.get() {
            p.last_cancel = std::time::Instant::now();
            self.form.close_pending.set(Some(p));
        }
    }
    fn mark_offered(&mut self) {
        if let Some(mut p) = self.form.close_pending.get() {
            p.error_logged = true;
            self.form.close_pending.set(Some(p));
        }
    }
    fn offer_close_without_saving(&mut self) {
        self.form.offer_close_without_saving();
    }
    fn close(&mut self) {
        self.form.close_pending.set(None);
        if self.form.on_close() {
            nwg::stop_thread_dispatch();
        }
    }
}

/// The title of the first LockNote dialog still open (the window the
/// curtain waits for), or of a standard dialog.
pub(super) fn first_dialog_title() -> String {
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

/// Show again the LockNote dialogs the curtain hid, when it is lifted with
/// the password (they were answered Cancel; one that was busy is still
/// open and must not stay invisible).
fn show_registered_dialogs() {
    use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_SHOW};
    for h in crate::ui::dialogs::style::registered_dialogs() {
        // The return value is the previous visibility, not an error.
        let _was_visible = unsafe { ShowWindow(windows::Win32::Foundation::HWND(h as *mut _), SW_SHOW) };
    }
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

/// Ask every open LockNote dialog, standard dialog and popup menu of this
/// thread to close as No / Cancel (their harmless answer, pass 8): WM_CLOSE to LockNote's dialogs; IDCANCEL,
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
        use super::super::lock_flow::answer_applies;
        assert!(answer_applies(3, 3, false, false));
        assert!(!answer_applies(3, 4, false, false), "epoch changed");
        assert!(!answer_applies(3, 3, true, false), "curtain up");
        assert!(!answer_applies(3, 3, false, true), "locked");
    }

    /// Pass 6: a close waiting for a dialog that stays open offers, once and
    /// never under the curtain, to close without saving.
    #[test]
    fn a_close_stuck_behind_a_dialog_offers_to_close_without_saving() {
        use std::time::Duration;
        let s = Duration::from_millis;
        assert_eq!(close_step(false, false, s(0), s(0), false, false), CloseStep::Run);
        assert_eq!(close_step(true, false, s(100), s(500), false, false), CloseStep::Wait);
        assert_eq!(close_step(true, false, RECANCEL_EVERY, s(2000), false, false), CloseStep::CancelAgain);
        assert_eq!(close_step(true, false, s(100), CLOSE_WAIT, false, false), CloseStep::Offer);
        assert_eq!(close_step(true, false, s(100), CLOSE_WAIT, false, true), CloseStep::Wait, "not under the curtain");
        // Pass 9: the offer shown, its own loop ticks: it is not cancelled
        // (it was, by CancelAgain, and answered Keep waiting at once).
        assert_eq!(close_step(true, true, RECANCEL_EVERY, CLOSE_WAIT, true, false), CloseStep::Wait, "offer shown");
        // Asked once, offer answered Keep waiting: the busy dialog is asked again.
        assert_eq!(close_step(true, false, RECANCEL_EVERY, CLOSE_WAIT, true, false), CloseStep::CancelAgain, "asked once");
        // The close prompt or its save-failed question shown: waited for.
        assert_eq!(close_step(true, true, RECANCEL_EVERY, s(100), false, false), CloseStep::Wait, "close prompt shown");
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
