// Windows logoff / shutdown (issue #29).
//
// WM_QUERYENDSESSION: Windows asks whether the session may end. No dialog
// may be shown then, so the close setting decides alone: "Ask" saves like
// "Always" (losing the text is worse than saving it), "Never" saves nothing.
// A close prompt still open saves too (not answered yet), but a close the
// user already answered "Don't save" / "Close without saving", whose later
// box (notice, ssh-agent error) is still shown, saves nothing (#33,
// `close_save_declined`). A failed save of a modified note blocks the
// shutdown with a reason Windows shows (ShutdownBlockReasonCreate) instead
// of losing the text; an unmodified note only risks its window state: the
// save is tried, a failure is logged (WARN) and never blocks (#33). Once
// LockNote is exiting (the close flow wiped the note, which its set_text("")
// marks modified, before the loop stops) nothing is saved (#33).
// WM_ENDSESSION (wParam TRUE): the session ends; the process is killed after
// the handler returns, so the exit of a normal close runs here: ssh-agent
// keys removed, password and text wiped, pending save promoted
// (`ui::finish_exit_once`, also used by `ui::run`, so it runs once).

#[allow(unused_imports)]
use super::*;

/// What the end of the session saves.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum EndSessionSave {
    Nothing,
    /// The note with its changes: a failed save blocks the shutdown.
    Note,
    /// An unmodified note: only the window state is at stake, a failed save
    /// is logged (WARN) and does not block the shutdown (issue #33).
    WindowState,
}

/// What the end of the session saves: as a close whose prompt would have
/// answered Save, except that a locked note is never saved (its password is
/// wiped; the lock already saved it) and a close already answered "Don't
/// save" / "Close without saving" (`close_declined`) is not overridden.
/// Once LockNote is exiting (`exiting`: the close flow wiped the text and
/// the password, or the session already ended) nothing is left to save.
pub(super) fn end_session_save(
    locked: bool,
    modified: bool,
    save_on_close: crate::settings::CloseAction,
    close_declined: bool,
    exiting: bool,
) -> EndSessionSave {
    if locked || close_declined || exiting {
        EndSessionSave::Nothing
    } else if !modified {
        EndSessionSave::WindowState
    } else if save_on_close == crate::settings::CloseAction::Never {
        EndSessionSave::Nothing
    } else {
        EndSessionSave::Note
    }
}

/// The reason Windows shows next to LockNote when a save blocks the shutdown.
pub(super) fn block_reason(error: &str) -> String {
    t("endsession.blocked").replace("{error}", error)
}

/// The side effects of the end of the session (tested without a window).
pub(super) trait EndSessionSteps {
    /// `save_note`: never a message box.
    fn save(&mut self) -> Result<(), String>;
    /// ShutdownBlockReasonCreate with the translated reason.
    fn block(&mut self, reason: &str);
    /// `unload_agent_keys(false)`: logs only.
    fn unload_agent_keys(&mut self);
    fn wipe(&mut self);
    /// `ui::finish_exit_once`: promote the pending save, release the lock.
    fn finish_exit(&mut self);
    /// From now on every save and every UI command is refused.
    fn mark_exiting(&mut self);
    /// Stop the message loop: nothing is left to run.
    fn stop_loop(&mut self);
    /// ShutdownBlockReasonDestroy, when a reason was set.
    fn destroy_block(&mut self);
}

/// WM_ENDSESSION: `ending` is its wParam, `blocked` whether a reason is set.
pub(super) fn end_session_message(ending: bool, blocked: bool, steps: &mut impl EndSessionSteps) {
    if blocked {
        steps.destroy_block();
    }
    if ending {
        end_session(steps);
    } else {
        eprintln!("INFO endsession: cancelled, LockNote goes on");
    }
}

/// WM_QUERYENDSESSION: true lets the session end.
pub(super) fn query_end_session(
    locked: bool,
    modified: bool,
    save_on_close: crate::settings::CloseAction,
    close_declined: bool,
    exiting: bool,
    steps: &mut impl EndSessionSteps,
) -> bool {
    match end_session_save(locked, modified, save_on_close, close_declined, exiting) {
        EndSessionSave::Nothing => {
            eprintln!(
                "INFO endsession: no save locked={} modified={} setting={:?} close_declined={} exiting={}",
                locked, modified, save_on_close, close_declined, exiting
            );
            true
        }
        EndSessionSave::Note => match steps.save() {
            Ok(()) => {
                eprintln!("INFO endsession: saved");
                true
            }
            Err(msg) => {
                // The reason is logged by the save, in English.
                eprintln!("WARN endsession: save failed, shutdown blocked");
                steps.block(&block_reason(&msg));
                false
            }
        },
        EndSessionSave::WindowState => match steps.save() {
            Ok(()) => {
                eprintln!("INFO endsession: saved (note unmodified, window state)");
                true
            }
            Err(_) => {
                // The reason is logged by the save, in English. Only the
                // window position is lost: the shutdown goes on.
                eprintln!("WARN endsession: window state not saved (note unmodified), shutdown not blocked");
                true
            }
        },
    }
}

/// WM_ENDSESSION with wParam TRUE: the exit of a normal close.
pub(super) fn end_session(steps: &mut impl EndSessionSteps) {
    eprintln!("INFO endsession: exiting");
    // First, so nothing below (the ssh-agent list save included) and no
    // message dispatched later can save or act.
    steps.mark_exiting();
    steps.unload_agent_keys();
    steps.wipe();
    steps.finish_exit();
    steps.stop_loop();
}

/// `EndSessionSteps` on the real window.
pub(super) struct FormEndSessionSteps<'a> {
    pub(super) form: &'a EditorForm,
}

impl EndSessionSteps for FormEndSessionSteps<'_> {
    fn save(&mut self) -> Result<(), String> {
        self.form.save_note().map_err(|f| f.message().to_string())
    }

    fn block(&mut self, reason: &str) {
        let wide: Vec<u16> = reason.encode_utf16().chain(std::iter::once(0)).collect();
        let Some(hwnd) = self.form.window.handle.hwnd() else {
            eprintln!("WARN endsession: no window handle, shutdown not blocked");
            return;
        };
        unsafe {
            if let Err(e) = windows::Win32::System::Shutdown::ShutdownBlockReasonCreate(
                windows::Win32::Foundation::HWND(hwnd as *mut _),
                windows::core::PCWSTR(wide.as_ptr()),
            ) {
                eprintln!("WARN endsession: ShutdownBlockReasonCreate failed err={:?}", e);
            }
        }
        self.form.shutdown_blocked.set(true);
    }

    fn unload_agent_keys(&mut self) {
        // No box: nothing may be shown while the session ends.
        self.form.unload_agent_keys(false);
    }

    fn wipe(&mut self) {
        // Every plaintext store, as the lock (TOTP secrets included). Never
        // a panic: a store still borrowed is retried on the next tick, and
        // finish_exit below runs anyway.
        self.form.wipe_plaintext_stores();
    }

    fn finish_exit(&mut self) {
        // Nothing may be shown now: no restart message.
        let _already_done = crate::ui::finish_exit_once(&self.form.exe_path, |_queued| {});
    }

    fn mark_exiting(&mut self) {
        self.form.exiting.set(true);
        // Locked too: shortcuts, menus (except Quit / About), the timers and
        // the dialogs all refuse to act on a locked note.
        *self.form.is_locked.borrow_mut() = true;
        self.form.set_menus_for_lock(true);
    }

    fn stop_loop(&mut self) {
        nwg::stop_thread_dispatch();
    }

    fn destroy_block(&mut self) {
        self.form.clear_shutdown_block();
    }
}

impl EditorForm {
    /// WM_QUERYENDSESSION: the answer for Windows (true = may end).
    pub(super) fn on_query_end_session(&self) -> bool {
        let locked = *self.is_locked.borrow();
        let exiting = self.exiting.get();
        if !locked && !exiting {
            self.save_window_state();
        }
        let modified = self.is_modified.load(Ordering::Relaxed);
        let setting = self.settings.borrow().save_on_close;
        let close_declined = self.close_save_declined.get();
        query_end_session(locked, modified, setting, close_declined, exiting, &mut FormEndSessionSteps { form: self })
    }

    /// WM_ENDSESSION: `ending` is its wParam.
    pub(super) fn on_end_session(&self, ending: bool) {
        let blocked = self.shutdown_blocked.get();
        end_session_message(ending, blocked, &mut FormEndSessionSteps { form: self });
    }

    /// The TOTP secrets of the settings and of the panel (lock and end of
    /// session; the unlock reads them back from the file).
    pub(super) fn wipe_totp_secrets(&self) {
        self.totp_panel.set_entries(Vec::new());
        match self.settings.try_borrow_mut() {
            Ok(mut s) => zeroize_settings_secrets(&mut s),
            Err(_) => self.defer_wipe("settings (TOTP secrets)"),
        }
    }

    /// Remove the shutdown block reason, if one is set: at WM_ENDSESSION, or
    /// after a later save succeeded (the reason no longer holds).
    pub(super) fn clear_shutdown_block(&self) {
        if !self.shutdown_blocked.replace(false) {
            return;
        }
        if let Some(hwnd) = self.window.handle.hwnd() {
            unsafe {
                if let Err(e) = windows::Win32::System::Shutdown::ShutdownBlockReasonDestroy(
                    windows::Win32::Foundation::HWND(hwnd as *mut _),
                ) {
                    eprintln!("WARN endsession: ShutdownBlockReasonDestroy failed err={:?}", e);
                }
            }
        }
        eprintln!("INFO endsession: shutdown block reason removed");
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::settings::CloseAction::*;

    /// Also used by the close-flow sequence tests (a logoff inside one of
    /// the close flow's boxes, #33).
    pub(in crate::ui::editor) struct Fake {
        pub(in crate::ui::editor) save: Result<(), String>,
        pub(in crate::ui::editor) log: Vec<String>,
    }

    impl Fake {
        pub(in crate::ui::editor) fn new(save: Result<(), String>) -> Fake {
            Fake { save, log: Vec::new() }
        }
    }

    impl EndSessionSteps for Fake {
        fn save(&mut self) -> Result<(), String> {
            self.log.push("save".into());
            self.save.clone()
        }
        fn block(&mut self, reason: &str) {
            self.log.push(format!("block:{}", reason));
        }
        fn unload_agent_keys(&mut self) {
            self.log.push("unload".into());
        }
        fn wipe(&mut self) {
            self.log.push("wipe".into());
        }
        fn finish_exit(&mut self) {
            self.log.push("finish_exit".into());
        }
        fn mark_exiting(&mut self) {
            self.log.push("exiting".into());
        }
        fn stop_loop(&mut self) {
            self.log.push("stop".into());
        }
        fn destroy_block(&mut self) {
            self.log.push("destroy_block".into());
        }
    }

    /// Pass 3, M3: after the end of the session nothing stays readable or
    /// usable: everything wiped, saves and commands refused, loop stopped.
    #[test]
    fn the_end_of_the_session_wipes_refuses_and_stops() {
        let mut f = Fake::new(Ok(()));
        end_session_message(true, false, &mut f);
        assert_eq!(f.log, ["exiting", "unload", "wipe", "finish_exit", "stop"]);
    }

    #[test]
    fn a_cancelled_shutdown_keeps_everything_and_drops_the_block() {
        let mut f = Fake::new(Ok(()));
        end_session_message(false, true, &mut f);
        assert_eq!(f.log, ["destroy_block"]);
        let mut f = Fake::new(Ok(()));
        end_session_message(false, false, &mut f);
        assert!(f.log.is_empty());
    }

    #[test]
    fn the_setting_and_the_state_decide_the_save() {
        use EndSessionSave::*;
        for (locked, modified, setting, declined, saves) in [
            (false, true, Ask, false, Note),
            (false, true, Always, false, Note),
            (false, true, Never, false, Nothing),
            // Locked: the password is wiped, nothing can be encrypted.
            (true, true, Ask, false, Nothing),
            (true, true, Always, false, Nothing),
            (true, false, Never, false, Nothing),
            (true, true, Ask, true, Nothing),
        ] {
            assert_eq!(end_session_save(locked, modified, setting, declined, false), saves, "{} {} {:?} {}", locked, modified, setting, declined);
        }
    }

    /// Issue #33, case 1: a close already answered "Don't save" (or "Close
    /// without saving") is not overridden by the logoff, whatever the
    /// setting (Ask would otherwise save like Always).
    #[test]
    fn a_close_answered_dont_save_is_not_overridden_by_the_logoff() {
        for (modified, setting) in [(true, Ask), (true, Always), (true, Never), (false, Ask), (false, Never)] {
            assert_eq!(
                end_session_save(false, modified, setting, true, false),
                EndSessionSave::Nothing,
                "{} {:?}", modified, setting
            );
        }
    }

    /// Issue #33, case 2: an unmodified note only saves the window state,
    /// whatever the setting (the final save of a normal close).
    #[test]
    fn an_unmodified_note_only_saves_the_window_state() {
        for setting in [Ask, Always, Never] {
            assert_eq!(end_session_save(false, false, setting, false, false), EndSessionSave::WindowState, "{:?}", setting);
        }
    }

    #[test]
    fn a_saved_note_lets_the_session_end() {
        let mut f = Fake::new(Ok(()));
        assert!(query_end_session(false, true, Ask, false, false, &mut f));
        assert_eq!(f.log, ["save"]);
        let mut f = Fake::new(Ok(()));
        assert!(query_end_session(false, true, Never, false, false, &mut f));
        assert!(f.log.is_empty());
        let mut f = Fake::new(Ok(()));
        assert!(query_end_session(false, false, Never, false, false, &mut f));
        assert_eq!(f.log, ["save"], "window state");
    }

    #[test]
    fn a_failed_save_blocks_the_shutdown_with_its_reason() {
        for setting in [Ask, Always] {
            let mut f = Fake::new(Err("disk full".into()));
            assert!(!query_end_session(false, true, setting, false, false, &mut f));
            assert_eq!(f.log.len(), 2);
            assert_eq!(f.log[0], "save");
            assert!(f.log[1].starts_with("block:") && f.log[1].contains("disk full"), "{:?}", f.log);
        }
    }

    /// Issue #33, case 2: the window-state save is tried, its failure lets
    /// the session end (no block reason).
    #[test]
    fn an_unmodified_note_whose_save_fails_never_blocks_the_shutdown() {
        for setting in [Ask, Always, Never] {
            let mut f = Fake::new(Err("disk full".into()));
            assert!(query_end_session(false, false, setting, false, false, &mut f), "{:?}", setting);
            assert_eq!(f.log, ["save"], "{:?}", setting);
        }
    }

    /// Issue #33, case 1: no save at all, so nothing can block either.
    #[test]
    fn a_close_answered_dont_save_lets_the_session_end_without_saving() {
        for (modified, setting) in [(true, Ask), (true, Always), (false, Ask)] {
            let mut f = Fake::new(Err("disk full".into()));
            assert!(query_end_session(false, modified, setting, true, false, &mut f), "{} {:?}", modified, setting);
            assert!(f.log.is_empty(), "{} {:?}: {:?}", modified, setting, f.log);
        }
    }

    #[test]
    fn the_end_of_the_session_runs_the_exit_of_a_normal_close() {
        let mut f = Fake::new(Ok(()));
        end_session(&mut f);
        // Same order as on_close + ui::run: agent keys, wipe, promote.
        assert_eq!(f.log, ["exiting", "unload", "wipe", "finish_exit", "stop"]);
    }

    #[test]
    fn the_reason_is_translated_and_names_the_error() {
        let _g = crate::features::i18n::test_support::lang_guard();
        crate::features::i18n::set_language(crate::features::i18n::Language::French);
        let r = block_reason("disque plein");
        crate::features::i18n::set_language(crate::features::i18n::Language::English);
        assert!(r.contains("disque plein") && r.starts_with("LockNote"), "{}", r);
        assert!(!r.contains('{'), "{}", r);
    }

    /// Issue #33: once the close flow wiped the note (or the session ended),
    /// the logoff saves nothing, whatever the state the wipe left (its
    /// `set_text("")` marks the note modified) and never blocks.
    #[test]
    fn nothing_is_saved_once_lock_note_is_exiting() {
        for (modified, setting, declined) in [(true, Ask, false), (true, Always, false), (false, Ask, false), (false, Never, false), (true, Ask, true)] {
            assert_eq!(end_session_save(false, modified, setting, declined, true), EndSessionSave::Nothing, "{} {:?} {}", modified, setting, declined);
            let mut f = Fake::new(Err("Failed to save: empty password".into()));
            assert!(query_end_session(false, modified, setting, declined, true, &mut f), "{} {:?}", modified, setting);
            assert!(f.log.is_empty(), "{:?}", f.log);
        }
    }

    /// The window feeds the decision with the real state (#33): a mutant
    /// passing a constant instead survives every test with the fakes.
    #[test]
    fn the_end_of_session_state_is_wired() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("ui").join("editor");
        let read = |f: &str| crate::read_source(dir.join(f));
        let body = |src: &str, fun: &str| -> String {
            let start = src.find(fun).unwrap_or_else(|| panic!("{}", fun));
            let end = src[start..].find("
    }
").map_or(src.len(), |e| start + e);
            src[start..end].to_string()
        };
        let end = read("end_session.rs");
        let query = body(&end, "pub(super) fn on_query_end_session(&self) -> bool {");
        assert!(query.contains("let close_declined = self.close_save_declined.get();"), "{}", query);
        assert!(query.contains("let exiting = self.exiting.get();"), "{}", query);
        assert!(
            query.contains("query_end_session(locked, modified, setting, close_declined, exiting, &mut FormEndSessionSteps { form: self })"),
            "{}", query
        );
        let close = read("close_flow.rs");
        let declined = body(&close, "    fn mark_save_declined(&mut self) {
        eprintln!");
        assert!(declined.contains("self.form.close_save_declined.set(true);"), "{}", declined);
        let wipe = body(&close, "    fn wipe(&mut self) {
        // ");
        let exiting = wipe.find("self.form.exiting.set(true);").unwrap_or_else(|| panic!("{}", wipe));
        let password = wipe.find("self.form.wipe_password();").expect("password wiped");
        let text = wipe.find("self.form.editor.set_text(\"\");").expect("text wiped");
        assert!(exiting < password && exiting < text, "exiting set before the wipe: {}", wipe);
    }
}
