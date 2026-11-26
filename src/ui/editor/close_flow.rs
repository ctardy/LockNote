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
        // Under the curtain only the password gives the note back; quitting
        // could lose unsaved changes behind it (owner decision, pass 8).
        // Logoff / shutdown goes through WM_QUERYENDSESSION (#29).
        // Closing now would stop that dialog's loop, not the application's:
        // the dialogs are cancelled and the close runs once they are gone
        // (run_pending_requests). A box of the close flow itself already
        // asks: a second close (title bar, tray Quit) leaves it open instead
        // of cancelling and reopening it (pass 9).
        let curtain = self.curtain.get() != super::modal_guard::Curtain::None;
        match close_request(curtain, self.close_box_open.get(), self.modal_open()) {
            CloseRequest::RefusedUnderCurtain => {
                eprintln!("INFO close: refused under the curtain (password needed)");
                self.tell_quit_refused_under_curtain();
                return false;
            }
            CloseRequest::AlreadyAsking => {
                eprintln!("INFO close: requested again while the close flow asks, its box stays open");
                return false;
            }
            CloseRequest::AfterDialogs => {
                self.request_close_after_dialogs();
                return false;
            }
            CloseRequest::Now => {}
        }

        // If locked, the lock saved the note before wiping it — just exit.
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

/// What a close request does (pass 9).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum CloseRequest {
    /// Under the curtain only the password gives the note back.
    RefusedUnderCurtain,
    /// The close flow's own box (prompt, save-failed question, stuck-close
    /// offer) is open: it already asks, it is neither cancelled nor reopened.
    AlreadyAsking,
    /// Other dialogs are open: cancel them, close once they are gone.
    AfterDialogs,
    Now,
}

pub(super) fn close_request(curtain: bool, own_box_open: bool, modal_open: bool) -> CloseRequest {
    if curtain {
        CloseRequest::RefusedUnderCurtain
    } else if own_box_open {
        CloseRequest::AlreadyAsking
    } else if modal_open {
        CloseRequest::AfterDialogs
    } else {
        CloseRequest::Now
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
    /// Store the "Remember my choice" answer (Save -> Always) in
    /// `save_on_close` (memory; the save that follows writes it).
    fn remember(&mut self, action: crate::settings::CloseAction);
    /// Tell the user that "Remember my choice" with Don't save cannot be
    /// stored (see `run_close`).
    fn tell_choice_not_remembered(&mut self);
    /// `EditorForm::save_note`: no message box, the error is the translated
    /// reason shown by `close_without_saving`.
    fn save(&mut self) -> Result<(), String>;
    /// A save at close failed with `reason`: does the user close anyway,
    /// without saving? `false` (keep LockNote open) is the default answer.
    fn close_without_saving(&mut self, reason: &str) -> bool;
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

/// The text of the notice shown when "Remember my choice" goes with Don't
/// save: it names the setting and the menu with the labels the app shows.
pub(super) fn close_remember_notice() -> String {
    // Menu labels without their access-key `&` and trailing "...".
    let plain = |key: &str| t(key).replace('&', "").trim_end_matches("...").trim_end_matches('\u{2026}').to_string();
    let menu = format!(
        "{} > {} > {}",
        plain("menu.file"),
        plain("menu.file.settings"),
        plain("menu.settings.preferences"),
    );
    t("close.remember_not_saved")
        .replace("{onclose}", &t("settings.onclose"))
        .replace("{never}", &t("settings.never"))
        .replace("{menu}", &menu)
}

/// The close flow once the window state is captured: prompt (setting `Ask`),
/// saves, ssh-agent unload, wipe. A failed save stops it before the wipe, so
/// the unsaved changes stay in the open window (issue #22: they were lost),
/// unless the user then explicitly chooses to close without saving: a save
/// that keeps failing (unreachable share, full disk) must not make LockNote
/// impossible to close.
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
                    // Nothing is written, not even in memory (decision of
                    // 2026-11-26): the notice says where to set it.
                    Some(CloseAction::Never) => {
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
                // The reason is logged by the save, in English.
                if !steps.close_without_saving(&msg) {
                    eprintln!("WARN close: save failed, window stays open");
                    return CloseOutcome::StayOpen;
                }
                eprintln!("WARN close: save failed, closing without saving (user choice)");
                user_declined_save = true;
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
            if !steps.close_without_saving(&msg) {
                eprintln!("WARN close: final save failed, window stays open");
                return CloseOutcome::StayOpen;
            }
            eprintln!("WARN close: final save failed, closing without saving (user choice)");
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
        // title bar) leaves it open (`close_box`, pass 9); auto-save waits, a
        // due lock answers it Cancel (issue #30).
        let epoch = self.form.lock_epoch();
        let mut answer = self.form.close_box(|| {
            let _modal = self.form.modal();
            crate::ui::dialogs::close_confirm::CloseConfirmDialog::show(Some(self.form.window.handle))
        });
        // An answer returning after a curtain went up is Cancel (pass 8).
        if !self.form.still_valid(epoch) {
            eprintln!("INFO close: prompt answer dropped, a lock came meanwhile");
            answer = crate::ui::dialogs::close_confirm::CloseConfirmResult {
                action: crate::ui::dialogs::close_confirm::CloseAction::Cancel,
                remember: false,
            };
        }
        eprintln!("INFO close: prompt action={:?} remember={}", answer.action, answer.remember);
        answer
    }

    fn remember(&mut self, action: crate::settings::CloseAction) {
        eprintln!("INFO close: remember save_on_close={:?}", action);
        self.form.settings.borrow_mut().save_on_close = action;
    }

    fn tell_choice_not_remembered(&mut self) {
        // Its own message loop: a close request meanwhile (tray Quit) must be
        // refused by on_close.
        let _modal = self.form.modal();
        self.form.inform("LockNote", &close_remember_notice());
    }

    fn save(&mut self) -> Result<(), String> {
        // Without on_save's "Save Error" box: close_without_saving shows the
        // reason in its question, one message instead of two.
        self.form.save_note().map_err(|f| f.message().to_string())
    }

    fn close_without_saving(&mut self, reason: &str) -> bool {
        // LockNote's own buttons ("Keep LockNote open" / "Close without
        // saving"), not a Yes/No box labelled in the Windows language; keep
        // open is the default (Enter, Esc, title bar). Owned by the main
        // window, so it stays above it even when it is always on top.
        use crate::ui::dialogs::close_confirm::{closes_without_saving, SaveFailedDialog};
        let content = t("close.save_failed").replace("{error}", reason);
        let epoch = self.form.lock_epoch();
        let answer = self.form.close_box(|| {
            let _modal = self.form.modal();
            SaveFailedDialog::show(Some(self.form.window.handle), &content)
        });
        // An answer returning after a curtain went up is "Keep open".
        let close = closes_without_saving(answer) && self.form.still_valid(epoch);
        eprintln!("INFO close: save failed, close_without_saving={}", close);
        close
    }

    fn unload_agent_keys(&mut self) {
        self.form.unload_agent_keys(true);
    }

    fn wipe(&mut self) {
        self.form.wipe_password();
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
        /// Answer to "close without saving?" after a failed save.
        close_anyway: bool,
    }

    impl Fake {
        fn new(action: Choice, saves: Vec<Result<(), String>>) -> Fake {
            Fake {
                answer: CloseConfirmResult { action, remember: false },
                saves,
                log: Vec::new(),
                password: "pw".into(),
                text: "unsaved".into(),
                close_anyway: false,
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
        fn close_without_saving(&mut self, reason: &str) -> bool {
            assert_eq!(reason, "Failed to save: disk full");
            self.log.push(if self.close_anyway { "close_anyway?yes" } else { "close_anyway?no" });
            self.close_anyway
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

    /// Pass 9: a second close while the close flow's own box is open leaves
    /// it open (it was cancelled, then reopened by the pending close).
    #[test]
    fn a_second_close_leaves_the_close_box_open() {
        assert_eq!(close_request(false, true, true), CloseRequest::AlreadyAsking);
        assert_eq!(close_request(false, false, true), CloseRequest::AfterDialogs);
        assert_eq!(close_request(false, false, false), CloseRequest::Now);
        assert_eq!(close_request(true, true, true), CloseRequest::RefusedUnderCurtain);
    }

    /// Pass 2, M1: a save that keeps failing (unreachable share, disk full)
    /// must not make LockNote impossible to close: the user is asked, and
    /// only an explicit Yes to "close without saving" closes.
    #[test]
    fn failed_save_with_ask_offers_to_close_without_saving() {
        let mut f = Fake::new(Choice::Save, vec![disk_full()]);
        f.close_anyway = true;
        assert_eq!(run_close(true, CloseAction::Ask, &mut f), CloseOutcome::Close);
        // No second save after the user chose to close without saving.
        assert_eq!(f.log, ["ask", "save", "close_anyway?yes", "unload", "wipe"]);
    }

    #[test]
    fn failed_save_with_always_offers_to_close_without_saving() {
        let mut f = Fake::new(Choice::Cancel, vec![disk_full()]);
        f.close_anyway = true;
        assert_eq!(run_close(true, CloseAction::Always, &mut f), CloseOutcome::Close);
        assert_eq!(f.log, ["save", "close_anyway?yes", "unload", "wipe"]);
    }

    #[test]
    fn failed_final_save_offers_to_close_without_saving() {
        let mut f = Fake::new(Choice::Cancel, vec![disk_full()]);
        f.close_anyway = true;
        assert_eq!(run_close(false, CloseAction::Ask, &mut f), CloseOutcome::Close);
        assert_eq!(f.log, ["unload", "save", "close_anyway?yes", "wipe"]);
    }

    /// Issue #22: a failed save at close kept closing and lost the text.
    #[test]
    fn failed_save_at_the_prompt_keeps_the_window_open_with_text_and_password() {
        let mut f = Fake::new(Choice::Save, vec![disk_full()]);
        assert_eq!(run_close(true, CloseAction::Ask, &mut f), CloseOutcome::StayOpen);
        assert_eq!(f.log, ["ask", "save", "close_anyway?no"]);
        assert_eq!((f.password.as_str(), f.text.as_str()), ("pw", "unsaved"));
    }

    #[test]
    fn failed_save_with_always_keeps_the_window_open() {
        let mut f = Fake::new(Choice::Cancel, vec![disk_full()]);
        assert_eq!(run_close(true, CloseAction::Always, &mut f), CloseOutcome::StayOpen);
        assert_eq!(f.log, ["save", "close_anyway?no"]);
        assert_eq!((f.password.as_str(), f.text.as_str()), ("pw", "unsaved"));
    }

    #[test]
    fn failed_final_save_keeps_the_window_open() {
        // Not modified: only the final save (window state) runs.
        let mut f = Fake::new(Choice::Cancel, vec![disk_full()]);
        assert_eq!(run_close(false, CloseAction::Ask, &mut f), CloseOutcome::StayOpen);
        assert_eq!(f.log, ["unload", "save", "close_anyway?no"]);
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
        // Decision of 2026-11-26: nothing is written, not even in memory.
        assert_eq!(f.log, ["ask", "not_remembered", "unload", "wipe"]);
    }

    /// Pass 2, M4: the notice names the setting to change, with the labels
    /// the app shows.
    #[test]
    fn the_not_remembered_notice_names_the_setting_and_the_menu() {
        use crate::features::i18n::{set_language, Language};
        let _g = crate::features::i18n::test_support::lang_guard();
        set_language(Language::French);
        let fr = close_remember_notice();
        let mut all = Vec::new();
        for lang in [Language::English, Language::Spanish, Language::German] {
            set_language(lang);
            all.push(close_remember_notice());
        }
        set_language(Language::English);
        assert!(
            fr.ends_with("Pour ne plus \u{ea}tre questionn\u{e9}, choisissez \u{ab} \u{c0} la fermeture : Ne jamais enregistrer \u{bb} dans Fichier > Param\u{e8}tres > Pr\u{e9}f\u{e9}rences, puis enregistrez."),
            "{}", fr
        );
        assert!(all[0].contains("\"On close: Never save\" in File > Settings > Preferences"), "{}", all[0]);
        for text in all.iter().chain(std::iter::once(&fr)) {
            assert!(!text.contains('{') && !text.contains('&'), "{}", text);
        }
    }

    #[test]
    fn cancel_saves_nothing_and_stays_open() {
        let mut f = Fake::new(Choice::Cancel, vec![]);
        assert_eq!(run_close(true, CloseAction::Ask, &mut f), CloseOutcome::StayOpen);
        assert_eq!(f.log, ["ask"]);
    }
}
