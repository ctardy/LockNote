// Windows logoff / shutdown (issue #29).
//
// WM_QUERYENDSESSION: Windows asks whether the session may end. No dialog
// may be shown then, so the close setting decides alone: "Ask" saves like
// "Always" (losing the text is worse than saving it), "Never" saves nothing.
// A failed save blocks the shutdown with a reason Windows shows
// (ShutdownBlockReasonCreate) instead of losing the text.
// WM_ENDSESSION (wParam TRUE): the session ends; the process is killed after
// the handler returns, so the exit of a normal close runs here: ssh-agent
// keys removed, password and text wiped, pending save promoted
// (`ui::finish_exit_once`, also used by `ui::run`, so it runs once).

#[allow(unused_imports)]
use super::*;

/// Whether the end of the session saves the note: as a close whose prompt
/// would have answered Save, except that a locked note is never saved (its
/// password is wiped; the lock already saved it).
pub(super) fn end_session_saves(
    locked: bool,
    modified: bool,
    save_on_close: crate::settings::CloseAction,
) -> bool {
    if locked {
        return false;
    }
    !(modified && save_on_close == crate::settings::CloseAction::Never)
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
    steps: &mut impl EndSessionSteps,
) -> bool {
    if !end_session_saves(locked, modified, save_on_close) {
        eprintln!("INFO endsession: no save locked={} modified={} setting={:?}", locked, modified, save_on_close);
        return true;
    }
    match steps.save() {
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
        if !locked {
            self.save_window_state();
        }
        let modified = self.is_modified.load(Ordering::Relaxed);
        let setting = self.settings.borrow().save_on_close;
        query_end_session(locked, modified, setting, &mut FormEndSessionSteps { form: self })
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
mod tests {
    use super::*;
    use crate::settings::CloseAction::*;

    struct Fake {
        save: Result<(), String>,
        log: Vec<String>,
    }

    impl Fake {
        fn new(save: Result<(), String>) -> Fake {
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
        for (locked, modified, setting, saves) in [
            (false, true, Ask, true),
            (false, true, Always, true),
            (false, true, Never, false),
            // Not modified: the final save of a normal close (window state).
            (false, false, Ask, true),
            (false, false, Never, true),
            // Locked: the password is wiped, nothing can be encrypted.
            (true, true, Ask, false),
            (true, true, Always, false),
        ] {
            assert_eq!(end_session_saves(locked, modified, setting), saves, "{} {} {:?}", locked, modified, setting);
        }
    }

    #[test]
    fn a_saved_note_lets_the_session_end() {
        let mut f = Fake::new(Ok(()));
        assert!(query_end_session(false, true, Ask, &mut f));
        assert_eq!(f.log, ["save"]);
        let mut f = Fake::new(Ok(()));
        assert!(query_end_session(false, true, Never, &mut f));
        assert!(f.log.is_empty());
    }

    #[test]
    fn a_failed_save_blocks_the_shutdown_with_its_reason() {
        let mut f = Fake::new(Err("disk full".into()));
        assert!(!query_end_session(false, true, Always, &mut f));
        assert_eq!(f.log.len(), 2);
        assert_eq!(f.log[0], "save");
        assert!(f.log[1].starts_with("block:") && f.log[1].contains("disk full"), "{:?}", f.log);
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
}
