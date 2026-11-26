// Window state and close flow.
// Part of the EditorForm implementation, split out of the former editor.rs.

#[allow(unused_imports)]
use super::*;

impl EditorForm {
    // =====================================================================
    // Close flow
    // =====================================================================

    pub(super) fn save_window_state(&self) {
        use nwg::ControlHandle;
        if let ControlHandle::Hwnd(hwnd) = self.window.handle {
            let hwnd = windows::Win32::Foundation::HWND(hwnd as *mut _);
            let mut wp = windows::Win32::UI::WindowsAndMessaging::WINDOWPLACEMENT::default();
            wp.length = std::mem::size_of::<windows::Win32::UI::WindowsAndMessaging::WINDOWPLACEMENT>() as u32;
            let ok = unsafe {
                windows::Win32::UI::WindowsAndMessaging::GetWindowPlacement(hwnd, &mut wp)
            };
            match ok {
                Ok(()) => {
                    let rc = wp.rcNormalPosition;
                    let g = crate::ui::window_geometry::geometry_from_placement(
                        wp.showCmd,
                        wp.flags.0,
                        crate::ui::window_geometry::Rect {
                            left: rc.left,
                            top: rc.top,
                            right: rc.right,
                            bottom: rc.bottom,
                        },
                    );
                    let mut settings = self.settings.borrow_mut();
                    settings.window_width = g.w;
                    settings.window_height = g.h;
                    settings.window_x = g.x;
                    settings.window_y = g.y;
                    settings.window_maximized = g.maximized;
                }
                Err(e) => eprintln!("window: GetWindowPlacement failed err={:?}", e),
            }
        }
    }

    /// Handle close request. Returns `true` if the app should close,
    /// `false` if the user cancelled.
    pub(super) fn on_close(&self) -> bool {
        // A modal dialog runs its own message loop: closing now would wipe
        // the password and the text, then stop that loop instead of the
        // application's, leaving an empty editor that cannot save.
        if self.modal_dialog_open.get() {
            eprintln!("INFO close: refused while a modal dialog is open");
            return false;
        }

        // If locked, data was already saved by on_autolock — just exit.
        // Password is zeroized, so saving now would corrupt the file.
        if *self.is_locked.borrow() {
            // Retry what the lock could not remove; the list cannot be saved
            // (password wiped) and is retried at the next unlock.
            self.unload_agent_keys(false);
            return true;
        }

        // Capture window position/size before any save so it is included
        self.save_window_state();

        let modified = self.is_modified.load(Ordering::Relaxed);
        let save_on_close = self.settings.borrow().save_on_close;

        let outcome = run_close(modified, save_on_close, &mut FormCloseSteps { form: self });

        // The swap helper that moves the .tmp over the .exe is not launched
        // here: `ui::run` does it once, after the message loop, for every
        // way out of the application (storage::promote_pending_on_exit).

        outcome == CloseOutcome::Close
    }
}

/// Whether the application closes at the end of the close flow.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum CloseOutcome {
    Close,
    /// Cancelled, or a save failed: the text and the password stay.
    StayOpen,
}

/// The side effects of the close flow, behind a trait so `run_close`, which
/// decides, is tested without a window (issue #22).
pub(super) trait CloseSteps {
    /// The "unsaved changes" prompt.
    fn ask(&mut self) -> crate::ui::dialogs::close_confirm::CloseConfirmResult;
    /// Store the "Remember my choice" answer in `save_on_close` (memory; the
    /// next save writes it).
    fn remember(&mut self, action: crate::settings::CloseAction);
    /// Tell the user that "Remember my choice" with Don't save cannot be
    /// stored (see `run_close`).
    fn tell_choice_not_remembered(&mut self);
    /// `EditorForm::on_save`: it shows its own error message.
    fn save(&mut self) -> Result<(), String>;
    fn unload_agent_keys(&mut self);
    /// Wipe the password and the editor text.
    fn wipe(&mut self);
}

/// The `save_on_close` value a prompt answer asks to remember (issue #27):
/// Save -> Always, Don't save -> Never, only with "Remember my choice"
/// checked; Cancel never stores anything.
pub(super) fn remembered_close_action(
    r: crate::ui::dialogs::close_confirm::CloseConfirmResult,
) -> Option<crate::settings::CloseAction> {
    use crate::settings::CloseAction;
    use crate::ui::dialogs::close_confirm::CloseAction as Choice;
    if !r.remember {
        return None;
    }
    match r.action {
        Choice::Save => Some(CloseAction::Always),
        Choice::DontSave => Some(CloseAction::Never),
        Choice::Cancel => None,
    }
}

/// The close flow once the window state is captured: prompt (setting `Ask`),
/// saves, ssh-agent unload, wipe. A failed save stops it before the wipe, so
/// the unsaved changes stay in the open window (issue #22: they were lost).
pub(super) fn run_close(
    modified: bool,
    save_on_close: crate::settings::CloseAction,
    steps: &mut impl CloseSteps,
) -> CloseOutcome {
    use crate::settings::CloseAction;
    use crate::ui::dialogs::close_confirm::CloseAction as Choice;

    let mut user_declined_save = false;

    if modified {
        let save_now = match save_on_close {
            CloseAction::Always => true,
            CloseAction::Never => false,
            CloseAction::Ask => {
                let answer = steps.ask();
                if answer.action == Choice::Cancel {
                    eprintln!("INFO close: cancelled at the prompt");
                    return CloseOutcome::StayOpen;
                }
                match remembered_close_action(answer) {
                    // Set before the save below, which writes it.
                    Some(CloseAction::Always) => steps.remember(CloseAction::Always),
                    // Settings live in the encrypted note: storing Never
                    // would mean saving the changes the user just declined.
                    Some(CloseAction::Never) => {
                        steps.remember(CloseAction::Never);
                        eprintln!("WARN close: remember=never not stored, the note is not saved");
                        steps.tell_choice_not_remembered();
                    }
                    Some(CloseAction::Ask) | None => {}
                }
                answer.action == Choice::Save
            }
        };
        if save_now {
            if let Err(msg) = steps.save() {
                eprintln!("WARN close: save failed, window stays open err={}", msg);
                return CloseOutcome::StayOpen;
            }
        } else {
            user_declined_save = true;
        }
    }

    // Past the Cancel branch and before the final save, so the saved list
    // matches the agent.
    steps.unload_agent_keys();

    // Always do a final save to persist settings (window state, etc.)
    // unless the user explicitly declined saving.
    if !user_declined_save {
        if let Err(msg) = steps.save() {
            eprintln!("WARN close: final save failed, window stays open err={}", msg);
            return CloseOutcome::StayOpen;
        }
    }

    steps.wipe();
    CloseOutcome::Close
}

/// `CloseSteps` on the real window.
struct FormCloseSteps<'a> {
    form: &'a EditorForm,
}

impl CloseSteps for FormCloseSteps<'_> {
    fn ask(&mut self) -> crate::ui::dialogs::close_confirm::CloseConfirmResult {
        // The translated dialog with "Remember my choice" (issue #27). It runs
        // its own message loop: a second close request meanwhile (tray menu,
        // title bar) is refused by the modal_dialog_open check of on_close.
        self.form.modal_dialog_open.set(true);
        let answer = crate::ui::dialogs::close_confirm::CloseConfirmDialog::show();
        self.form.modal_dialog_open.set(false);
        eprintln!("INFO close: prompt action={:?} remember={}", answer.action, answer.remember);
        answer
    }

    fn remember(&mut self, action: crate::settings::CloseAction) {
        eprintln!("INFO close: remember save_on_close={:?}", action);
        self.form.settings.borrow_mut().save_on_close = action;
    }

    fn tell_choice_not_remembered(&mut self) {
        nwg::modal_info_message(&self.form.window, "LockNote", &t("close.remember_not_saved"));
    }

    fn save(&mut self) -> Result<(), String> {
        self.form.on_save()
    }

    fn unload_agent_keys(&mut self) {
        self.form.unload_agent_keys(true);
    }

    fn wipe(&mut self) {
        use zeroize::Zeroize;
        self.form.password.borrow_mut().zeroize();
        self.form.editor.set_text("");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::CloseAction;
    use crate::ui::dialogs::close_confirm::{CloseAction as Choice, CloseConfirmResult};

    /// Records the steps; `saves` are the results of the successive saves.
    struct Fake {
        answer: CloseConfirmResult,
        saves: Vec<Result<(), String>>,
        log: Vec<&'static str>,
        password: String,
        text: String,
    }

    impl Fake {
        fn new(action: Choice, saves: Vec<Result<(), String>>) -> Fake {
            Fake {
                answer: CloseConfirmResult { action, remember: false },
                saves,
                log: Vec::new(),
                password: "pw".into(),
                text: "unsaved".into(),
            }
        }
    }

    impl CloseSteps for Fake {
        fn ask(&mut self) -> CloseConfirmResult {
            self.log.push("ask");
            self.answer
        }
        fn remember(&mut self, action: CloseAction) {
            self.log.push(match action {
                CloseAction::Always => "remember:always",
                CloseAction::Never => "remember:never",
                CloseAction::Ask => "remember:ask",
            });
        }
        fn tell_choice_not_remembered(&mut self) {
            self.log.push("not_remembered");
        }
        fn save(&mut self) -> Result<(), String> {
            self.log.push("save");
            assert!(!self.saves.is_empty(), "unexpected save");
            self.saves.remove(0)
        }
        fn unload_agent_keys(&mut self) {
            self.log.push("unload");
        }
        fn wipe(&mut self) {
            self.log.push("wipe");
            self.password.clear();
            self.text.clear();
        }
    }

    fn disk_full() -> Result<(), String> {
        Err("Failed to save: disk full".into())
    }

    /// Issue #22: a failed save at close kept closing and lost the text.
    #[test]
    fn failed_save_at_the_prompt_keeps_the_window_open_with_text_and_password() {
        let mut f = Fake::new(Choice::Save, vec![disk_full()]);
        assert_eq!(run_close(true, CloseAction::Ask, &mut f), CloseOutcome::StayOpen);
        assert_eq!(f.log, ["ask", "save"]);
        assert_eq!((f.password.as_str(), f.text.as_str()), ("pw", "unsaved"));
    }

    #[test]
    fn failed_save_with_always_keeps_the_window_open() {
        let mut f = Fake::new(Choice::Cancel, vec![disk_full()]);
        assert_eq!(run_close(true, CloseAction::Always, &mut f), CloseOutcome::StayOpen);
        assert_eq!(f.log, ["save"]);
        assert_eq!((f.password.as_str(), f.text.as_str()), ("pw", "unsaved"));
    }

    #[test]
    fn failed_final_save_keeps_the_window_open() {
        // Not modified: only the final save (window state) runs.
        let mut f = Fake::new(Choice::Cancel, vec![disk_full()]);
        assert_eq!(run_close(false, CloseAction::Ask, &mut f), CloseOutcome::StayOpen);
        assert_eq!(f.log, ["unload", "save"]);
        assert_eq!(f.password, "pw");
    }

    #[test]
    fn saved_close_wipes_and_closes() {
        let mut f = Fake::new(Choice::Save, vec![Ok(()), Ok(())]);
        assert_eq!(run_close(true, CloseAction::Ask, &mut f), CloseOutcome::Close);
        assert_eq!(f.log, ["ask", "save", "unload", "save", "wipe"]);
        assert!(f.password.is_empty() && f.text.is_empty());
    }

    #[test]
    fn declined_save_closes_without_saving() {
        let mut f = Fake::new(Choice::DontSave, vec![]);
        assert_eq!(run_close(true, CloseAction::Ask, &mut f), CloseOutcome::Close);
        assert_eq!(f.log, ["ask", "unload", "wipe"]);
        let mut f = Fake::new(Choice::Cancel, vec![]);
        assert_eq!(run_close(true, CloseAction::Never, &mut f), CloseOutcome::Close);
        assert_eq!(f.log, ["unload", "wipe"]);
    }

    fn answer(action: Choice, remember: bool) -> CloseConfirmResult {
        CloseConfirmResult { action, remember }
    }

    /// Issue #27: dialog result -> `save_on_close`.
    #[test]
    fn remember_maps_save_to_always_and_dont_save_to_never() {
        assert_eq!(remembered_close_action(answer(Choice::Save, true)), Some(CloseAction::Always));
        assert_eq!(remembered_close_action(answer(Choice::DontSave, true)), Some(CloseAction::Never));
        assert_eq!(remembered_close_action(answer(Choice::Save, false)), None);
        assert_eq!(remembered_close_action(answer(Choice::DontSave, false)), None);
    }

    #[test]
    fn cancel_never_remembers_a_choice() {
        assert_eq!(remembered_close_action(answer(Choice::Cancel, true)), None);
        let mut f = Fake::new(Choice::Cancel, vec![]);
        f.answer.remember = true;
        assert_eq!(run_close(true, CloseAction::Ask, &mut f), CloseOutcome::StayOpen);
        assert_eq!(f.log, ["ask"]);
    }

    #[test]
    fn remembered_save_is_stored_before_the_save_that_writes_it() {
        let mut f = Fake::new(Choice::Save, vec![Ok(()), Ok(())]);
        f.answer.remember = true;
        assert_eq!(run_close(true, CloseAction::Ask, &mut f), CloseOutcome::Close);
        assert_eq!(f.log, ["ask", "remember:always", "save", "unload", "save", "wipe"]);
    }

    #[test]
    fn remembered_dont_save_is_not_saved_and_the_user_is_told() {
        let mut f = Fake::new(Choice::DontSave, vec![]);
        f.answer.remember = true;
        assert_eq!(run_close(true, CloseAction::Ask, &mut f), CloseOutcome::Close);
        assert_eq!(f.log, ["ask", "remember:never", "not_remembered", "unload", "wipe"]);
    }

    #[test]
    fn cancel_saves_nothing_and_stays_open() {
        let mut f = Fake::new(Choice::Cancel, vec![]);
        assert_eq!(run_close(true, CloseAction::Ask, &mut f), CloseOutcome::StayOpen);
        assert_eq!(f.log, ["ask"]);
    }
}
