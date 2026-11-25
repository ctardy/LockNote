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
        let current = self.settings.borrow().clone();
        if let Some(changes) = crate::ui::dialogs::preferences_dialog::PreferencesDialog::show(&current) {
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

            // The theme applies live: set the mode and re-apply everything
            // themed (no restart).
            if changes.theme != current.theme {
                self.apply_theme_live(crate::theme::mode_for_choice(changes.theme));
            }

            {
                let current_lang = self.settings.borrow().language;
                if current_lang != current.language {
                    nwg::modal_info_message(
                        &self.window,
                        "LockNote",
                        &t("settings.restart"),
                    );
                }
            }
        }
    }

    /// Open the temporary files dialog (issue 057). Returns `true` when the
    /// user asked to close LockNote and the close flow agreed (the caller
    /// stops the message loop, as for File > Quit).
    pub(super) fn on_temp_files(&self) -> bool {
        use crate::ui::dialogs::temp_files_dialog::{TempFilesAction, TempFilesDialog};
        // The tray menu still reaches the disabled editor: `on_close` refuses
        // while this is set, since the message loop it would stop is the
        // dialog's, not the application's.
        self.modal_dialog_open.set(true);
        let action = TempFilesDialog::show(self.window.handle, &self.exe_path);
        self.modal_dialog_open.set(false);
        match action {
            TempFilesAction::CloseApp => self.on_close(),
            TempFilesAction::Close => false,
        }
    }

    /// Open security dialog (password, encryption, locks).
    pub(super) fn on_security(&self) {
        let current = self.settings.borrow().clone();
        if let Some(changes) = crate::ui::dialogs::security_dialog::SecurityDialog::show(&current) {
            let mut settings = self.settings.borrow_mut();
            settings.min_password_length = changes.min_password_length;
            {
                settings.totp_2fa = changes.totp_2fa;
                settings.totp_2fa_secret = changes.totp_2fa_secret.clone();
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
