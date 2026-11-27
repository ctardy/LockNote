// Password, preferences and security settings.
// Part of the EditorForm implementation, split out of the former editor.rs.

#[allow(unused_imports)]
use super::*;

impl EditorForm {
    /// Change the master password.
    pub(super) fn on_change_password(&self) {
        let min_len = self.settings.borrow().min_password_length;
        if let Some(new_password) = crate::ui::dialogs::create_password::show(min_len) {
            *self.password.borrow_mut() = new_password;
            self.mark_modified();
            self.offer_purge_auto_backups();
        }
    }

    /// Open preferences dialog (appearance + behavior).
    pub(super) fn on_preferences(&self) {
        let mut current = self.settings.borrow().clone();
        let epoch = self.lock_epoch();
        let changes = crate::ui::dialogs::preferences_dialog::PreferencesDialog::show(&current);
        // The copy of the settings held TOTP secrets (pass 6). `current` is
        // still read below for the old theme and language only.
        zeroize_settings_secrets(&mut current);
        if changes.is_some() && !self.still_valid(epoch) {
            eprintln!("INFO preferences: changes dropped, the note locked meanwhile");
            return;
        }
        if let Some(changes) = changes {
            let mut settings = self.settings.borrow_mut();
            settings.save_on_close = changes.save_on_close;
            settings.theme = changes.theme;
            settings.minimize_to_tray = changes.minimize_to_tray;
            settings.font_family = changes.font_family.clone();
            settings.font_size = changes.font_size;
            settings.save_key = changes.save_key;
            settings.save_modifiers = changes.save_modifiers;
            {
                let old_lang = settings.language;
                settings.language = changes.language;
                if old_lang != changes.language {
                    let i18n_lang = match changes.language {
                        crate::settings::Language::En => crate::features::i18n::Language::English,
                        crate::settings::Language::Fr => crate::features::i18n::Language::French,
                        crate::settings::Language::Es => crate::features::i18n::Language::Spanish,
                        crate::settings::Language::De => crate::features::i18n::Language::German,
                    };
                    crate::features::i18n::set_language(i18n_lang);
                }
            }
            {
                settings.auto_save = changes.auto_save;
            }
            {
                settings.hotkey_modifiers = changes.hotkey_modifiers;
                settings.hotkey_key = changes.hotkey_key;
            }
            drop(settings);
            self.mark_modified();

            // A new font applies at once and ends the zoom (A2). A refusal
            // is told at the end, once every change is applied: no state
            // change may follow a message, whose loop can run a lock.
            let font = self.apply_settings_font();
            if let Err(e) = &font {
                eprintln!("ERROR preferences: the new editor font was not applied: {e}");
            }

            // The theme applies live: set the mode and re-apply everything
            // themed (no restart).
            if changes.theme != current.theme {
                self.apply_theme_live(crate::theme::mode_for_choice(changes.theme));
            }

            {
                let current_lang = self.settings.borrow().language;
                if current_lang != current.language {
                    self.inform("LockNote",
                        &t("settings.restart"),
                    );
                }
            }

            if let Err(e) = font {
                self.inform("LockNote", &t("settings.font.failed").replace("{detail}", &e));
            }
        }
    }

    /// Open the temporary files dialog (issue 057). Returns `true` when the
    /// user asked to close LockNote and the close flow agreed (the caller
    /// stops the message loop, as for File > Quit).
    pub(super) fn on_temp_files(&self) -> bool {
        use crate::ui::dialogs::temp_files_dialog::{TempFilesAction, TempFilesDialog};
        // The tray menu still reaches the disabled editor: `on_close` then
        // cancels this dialog and closes once it is gone, since the message
        // loop it would stop is the dialog's, not the application's.
        let epoch = self.lock_epoch();
        let action = {
            let _modal = self.modal();
            TempFilesDialog::show(self.window.handle, &self.exe_path)
        };
        match action {
            // An answer returning after a curtain went up does not close (#30).
            TempFilesAction::CloseApp if !self.still_valid(epoch) => {
                eprintln!("INFO tempfiles: close dropped, the note locked meanwhile");
                false
            }
            TempFilesAction::CloseApp => self.on_close(),
            TempFilesAction::Close => false,
        }
    }

    /// Open security dialog (password, encryption, locks).
    pub(super) fn on_security(&self) {
        let mut current = self.settings.borrow().clone();
        let mut copied = None;
        let epoch = self.lock_epoch();
        let changes = crate::ui::dialogs::security_dialog::SecurityDialog::show(&current, &mut copied);
        // The copy of the settings held TOTP secrets (pass 5).
        zeroize_settings_secrets(&mut current);
        // The 2FA secret copied in its setup is cleared like any copy.
        self.arm_clipboard_guard(copied);
        if changes.is_some() && !self.still_valid(epoch) {
            eprintln!("INFO security: changes dropped, the note locked meanwhile");
            return;
        }
        if let Some(mut changes) = changes {
            let mut settings = self.settings.borrow_mut();
            settings.min_password_length = changes.min_password_length;
            {
                settings.totp_2fa = changes.totp_2fa;
                // Moved, not copied: the emptied wrapper is wiped on drop.
                settings.totp_2fa_secret = std::mem::take(&mut *changes.totp_2fa_secret);
            }
            {
                settings.auto_lock = changes.auto_lock;
                drop(settings);
                self.autolock.borrow_mut().set_timeout(changes.auto_lock);
                if changes.auto_lock > 0 {
                    self.autolock_timer.start();
                } else {
                    self.autolock_timer.stop();
                }
                settings = self.settings.borrow_mut();
            }
            {
                settings.clipboard_clear = changes.clipboard_clear;
            }
            drop(settings);
            let password_changed = changes.new_password.is_some();
            if let Some(new_password) = changes.new_password {
                *self.password.borrow_mut() = new_password;
            }
            self.mark_modified();
            if password_changed {
                self.offer_purge_auto_backups();
            }
        }
    }
}
