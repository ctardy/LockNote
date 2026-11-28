// Full backups: export, import, restore, safety backups and their window.
// Part of the EditorForm implementation, split out of the former editor.rs.

#[allow(unused_imports)]
use super::*;

impl EditorForm {
    /// Export every tab, snapshot, vault secret, attachment and setting to an
    /// encrypted backup file. The file is protected by the note password.
    /// The written file is read back and decoded before it is kept.
    pub(super) fn on_export_backup(&self) {
        use crate::ui::dialogs::confirm_password::ConfirmPasswordDialog;
        use crate::features::backup;

        if *self.is_locked.borrow() {
            return;
        }
        let epoch = self.lock_epoch();
        // Require password confirmation before export
        if !ConfirmPasswordDialog::show(&self.password_copy(), &t("confirmpwd.export")) {
            return;
        }

        let mut dialog = nwg::FileDialog::default();
        nwg::FileDialog::builder()
            .title(&t("backup.export.save_title"))
            .action(nwg::FileDialogAction::Save)
            .filters(&t("backup.filter"))
            .build(&mut dialog)
            .expect("Failed to build backup export dialog");
        if !dialog.run(Some(&self.window)) {
            return;
        }
        let Ok(selected) = dialog.get_selected_item() else {
            return;
        };
        let mut path = std::path::PathBuf::from(selected);
        if path.extension().is_none() {
            path.set_extension(backup::FILE_EXTENSION);
            // The save dialog asked about the name that was typed, not about
            // this one: do not replace an older backup without asking.
            if path.exists() {
                let params = nwg::MessageParams {
                    title: &t("backup.export.save_title"),
                    content: &t("backup.export.overwrite")
                        .replace("{path}", &path.to_string_lossy()),
                    buttons: nwg::MessageButtons::YesNo,
                    icons: nwg::MessageIcons::Warning,
                };
                if !self.confirm(&params) {
                    return;
                }
            }
        }

        // Every dialog above is modal while the auto-lock timer keeps ticking.
        // If it fired, the password and the contents are already wiped: a
        // backup built now would be empty and encrypted with "".
        // Under the curtain (not wiped yet) the answers are stale too (#30).
        if !self.still_valid(epoch) || save_precondition(&self.password.borrow()).is_err() {
            eprintln!("backup: export refused reason=locked");
            return;
        }

        let mut snapshot = match self.build_current_backup() {
            Ok(snapshot) => snapshot,
            Err(e) => return self.tell_unreadable("backup_export", &e),
        };
        let counts = (
            snapshot.tabs.len(),
            snapshot.snapshots.len(),
            snapshot.secrets.len(),
            snapshot.attachments.len(),
            snapshot.prompts.len(),
        );

        let outcome = self.write_backup_file(&snapshot, &path);
        snapshot.wipe();

        match outcome {
            Ok(written) => {
                eprintln!(
                    "backup: export ok format={} tabs={} snapshots={} secrets={} attachments={} prompts={} bytes={}",
                    backup::FORMAT,
                    counts.0,
                    counts.1,
                    counts.2,
                    counts.3,
                    counts.4,
                    written
                );
                self.inform("LockNote",
                    &t("backup.export.done")
                        .replace("{path}", &path.to_string_lossy()),
                );
            }
            Err((stage, detail)) => {
                eprintln!("backup: export failed stage={}", stage);
                self.inform("LockNote",
                    &t("backup.export.error").replace("{msg}", &detail),
                );
            }
        }
    }

    /// File > Safety backups...: the history of the automatic backups, with
    /// restore, "create one now", delete and open folder.
    pub(super) fn on_backups(&self) {
        use crate::ui::dialogs::backups_dialog::{BackupsAction, BackupsDialog};

        if *self.is_locked.borrow() {
            return;
        }
        let dir = crate::features::auto_backup::dir_path();
        let epoch = self.lock_epoch();
        let action = BackupsDialog::show(&dir);
        // An answer returning after a curtain went up does nothing (#30).
        if !matches!(action, BackupsAction::Close) && !self.still_valid(epoch) {
            eprintln!("INFO backups: action dropped, the note locked meanwhile");
            return;
        }
        match action {
            BackupsAction::Close => {}
            BackupsAction::CreateNow => match self.write_auto_backup(crate::features::auto_backup::Kind::Manual) {
                Ok(()) => {
                    self.inform("LockNote",
                        &t("autobackup.manual.done"),
                    );
                }
                Err(detail) => {
                    self.inform("LockNote",
                        &t("autobackup.manual.failed")
                            .replace("{msg}", &detail),
                    );
                }
            },
            // The name comes from `auto_backup::list`, so it is one of ours.
            // The restore is the normal import: password, confirmation, and a
            // new safety backup of the current note before it is replaced.
            BackupsAction::Restore(name) => self.import_backup_file(&dir.join(name)),
        }
    }

    /// The whole note as a `Backup`, for an export or an automatic safety
    /// backup. The current editor text goes into the active tab first, like
    /// on_save; a text that cannot be read strictly (a lone surrogate) is an
    /// error before anything is synced. The caller wipes the result
    /// (`Backup::wipe`).
    pub(super) fn build_current_backup(&self) -> Result<crate::features::backup::Backup, EditorTextError> {
        let current = Zeroizing::new(self.get_content()?);
        let tabs: Vec<crate::features::tabs::NoteTab> = {
            let tb = self.tab_bar.borrow_mut();
            let active = tb.active_index();
            tb.set_tab_content(active, &current);
            tb.all_tabs()
                .iter()
                .map(|t| crate::features::tabs::NoteTab::with_content(&t.name, &t.content, t.color.as_deref()))
                .collect()
        };
        drop(current);
        Ok(crate::features::backup::Backup {
            settings: self.settings.borrow().clone(),
            tabs,
            // Snapshots no longer exist; the backup format keeps the field.
            snapshots: Vec::new(),
            prompts: self.prompts_panel.get_prompts(),
            secrets: self.vault_panel.get_secrets(),
            attachments: self.attachments.borrow().clone(),
            created_unix: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            app_version: crate::updater::CURRENT_VERSION.to_string(),
        })
    }

    /// Write a safety backup of the current note before a destructive action
    /// (import, update), then apply the retention policy of `auto_backup`.
    /// Fail-fast: the caller must stop on `Err`; the text is shown to the user.
    ///
    /// Logged without any path or content: kind, size, purge counts.
    pub(super) fn write_auto_backup(&self, kind: crate::features::auto_backup::Kind) -> Result<(), String> {
        use crate::features::{auto_backup, import_log};

        // The caller's dialogs are modal while the auto-lock timer keeps ticking.
        if *self.is_locked.borrow() || save_precondition(&self.password.borrow()).is_err() {
            import_log::note(&format!("auto_backup: {} failed stage=locked", kind.tag()));
            return Err("the note is locked".to_string());
        }

        let mut snapshot = match self.build_current_backup() {
            Ok(snapshot) => snapshot,
            Err(e) => {
                import_log::note(&format!("auto_backup: {} failed stage=read_editor", kind.tag()));
                eprintln!("WARN auto_backup: refused stage=read_editor err={:?}", e);
                return Err(e.user_message());
            }
        };
        let now = crate::features::snapshots::current_timestamp();
        let outcome = auto_backup::store(&auto_backup::dir_path(), kind, now, |path| {
            self.write_backup_file(&snapshot, path)
        });
        snapshot.wipe();

        match outcome {
            Ok((path, report)) => {
                let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                import_log::note(&format!(
                    "auto_backup: {} ok bytes={} purged={} purge_failed={} over_cap={}",
                    kind.tag(),
                    bytes,
                    report.removed,
                    report.failed,
                    report.over_cap
                ));
                if report.failed > 0 {
                    import_log::note(&format!(
                        "WARN auto_backup: {} purge left {} file(s) it could not delete",
                        kind.tag(),
                        report.failed
                    ));
                }
                if report.over_cap {
                    import_log::note(&format!(
                        "WARN auto_backup: {} protected backups alone exceed the size cap",
                        kind.tag()
                    ));
                }
                Ok(())
            }
            Err((stage, detail)) => {
                import_log::note(&format!("auto_backup: {} failed stage={}", kind.tag(), stage));
                Err(detail)
            }
        }
    }

    /// After a password change: the automatic backups were encrypted with the
    /// previous password and still open with it. Offer to delete them; keeping
    /// them is the user's explicit choice.
    pub(super) fn offer_purge_auto_backups(&self) {
        use crate::features::{auto_backup, import_log};

        let dir = auto_backup::dir_path();
        let n = auto_backup::count(&dir);
        if n == 0 {
            return;
        }
        let params = nwg::MessageParams {
            title: "LockNote",
            content: &t("autobackup.purge.ask")
                .replace("{n}", &n.to_string()),
            buttons: nwg::MessageButtons::YesNo,
            icons: nwg::MessageIcons::Warning,
        };
        if !self.confirm(&params) {
            import_log::note(&format!("auto_backup: kept {} after password change reason=user", n));
            return;
        }
        let left = match auto_backup::purge_all(&dir) {
            Ok(report) => {
                import_log::note(&format!(
                    "auto_backup: purge_all removed={} failed={}",
                    report.removed, report.failed
                ));
                report.failed
            }
            Err(_) => {
                import_log::note("WARN auto_backup: purge_all failed stage=list");
                n
            }
        };
        if left > 0 {
            self.inform("LockNote",
                &t("autobackup.purge.partial")
                    .replace("{n}", &left.to_string())
                    .replace("{dir}", &dir.to_string_lossy()),
            );
        }
    }

    /// Encrypt `snapshot` with the note password and put it at `path`: written
    /// to a temporary file next to it, read back and decoded, then moved into
    /// place, so a crash or a failed check never leaves a half-written file
    /// under the final name. Returns the size written, or `(stage, detail)`.
    ///
    /// The password borrow ends with this function, before any modal message:
    /// while one is open the auto-lock timer needs `password.borrow_mut()`.
    pub(super) fn write_backup_file(
        &self,
        snapshot: &crate::features::backup::Backup,
        path: &std::path::Path,
    ) -> Result<usize, (&'static str, String)> {
        use crate::features::backup;
        use std::io::Write;

        let password = self.password.borrow();
        let bytes = backup::encode(snapshot, &password, backup::DEFAULT_KDF)
            .map_err(|e| ("encode", e.to_string()))?;

        // Unique name, created with create_new: never touches an existing file.
        let mut tmp_name = path.as_os_str().to_owned();
        tmp_name.push(format!(".{}.tmp", std::process::id()));
        let tmp = std::path::PathBuf::from(tmp_name);
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .map_err(|e| ("write", e.to_string()))?;
        let write_result = file.write_all(&bytes).and_then(|_| file.sync_all());
        drop(file);

        let result = write_result
            .map_err(|e| ("write", e.to_string()))
            .and_then(|_| std::fs::read(&tmp).map_err(|e| ("verify", e.to_string())))
            .and_then(|written| {
                let back = backup::decode(&written, &password).map_err(|e| ("verify", e.to_string()))?;
                if back.tabs.len() == snapshot.tabs.len()
                    && back.snapshots.len() == snapshot.snapshots.len()
                    && back.secrets.len() == snapshot.secrets.len()
                    && back.attachments.len() == snapshot.attachments.len()
                    && back.prompts.len() == snapshot.prompts.len()
                {
                    Ok(())
                } else {
                    Err(("verify", "the written file does not match the note".to_string()))
                }
            })
            .and_then(|_| std::fs::rename(&tmp, path).map_err(|e| ("rename", e.to_string())));
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        result.map(|_| bytes.len())
    }

    /// Show the message matching a backup error.
    pub(super) fn show_backup_error(&self, err: &crate::features::backup::BackupError) {
        use crate::features::backup::BackupError;
        let text = match err {
            BackupError::NotABackup => t("backup.import.not_backup"),
            BackupError::NewerFormat { found, supported } => t("backup.import.newer")
                .replace("{found}", &found.to_string())
                .replace("{supported}", &supported.to_string()),
            BackupError::WrongPasswordOrCorrupt => {
                t("backup.import.wrong_password")
            }
            BackupError::FormatMismatch
            | BackupError::Corrupt(_)
            | BackupError::Crypto(_)
            | BackupError::TooLarge => {
                t("backup.import.corrupt").replace("{msg}", &err.to_string())
            }
        };
        self.inform("LockNote", &text);
    }

    /// Import a full backup file, replacing all current content.
    /// Nothing is written to disk here: the result is kept in memory and
    /// follows the normal save path.
    pub(super) fn on_import_backup(&self) {
        if *self.is_locked.borrow() {
            return;
        }

        let mut dialog = nwg::FileDialog::default();
        nwg::FileDialog::builder()
            .title(&t("backup.import.open_title"))
            .action(nwg::FileDialogAction::Open)
            .filters(&t("backup.filter"))
            .build(&mut dialog)
            .expect("Failed to build backup import dialog");
        if !dialog.run(Some(&self.window)) {
            return;
        }
        let Ok(selected) = dialog.get_selected_item() else {
            return;
        };
        self.import_backup_file(&std::path::PathBuf::from(selected));
    }

    /// Everything after the file is chosen: size, header, password, decode,
    /// confirmation. Shared by "Import full backup" and the safety backups
    /// window ("Restore").
    pub(super) fn import_backup_file(&self, path: &std::path::Path) {
        use crate::ui::dialogs::backup_password::BackupPasswordDialog;
        use crate::features::backup::{self, BackupError};

        let read_error = |detail: String| {
            eprintln!("backup: import refused reason=read");
            self.inform("LockNote",
                &t("backup.import.read_error").replace("{msg}", &detail),
            );
        };
        match std::fs::metadata(path) {
            Ok(md) if md.len() > backup::MAX_FILE_SIZE => {
                eprintln!("backup: import refused reason=too_large");
                self.inform("LockNote",
                    &t("backup.import.too_large"),
                );
                return;
            }
            Ok(_) => {}
            Err(e) => return read_error(e.to_string()),
        }
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => return read_error(e.to_string()),
        };

        // Header first: a file from a newer LockNote is refused without
        // asking for a password that could not help.
        if let Err(e) = backup::peek_format(&bytes) {
            eprintln!("backup: import refused reason=header");
            self.show_backup_error(&e);
            return;
        }

        let imported = loop {
            let Some(password) = BackupPasswordDialog::show(
                &t("backup.import.pw_title"),
                &t("backup.import.pw_label"),
                &t("goto.ok"),
                &t("goto.cancel"),
                &t("unlock.empty"),
            ) else {
                return;
            };
            match backup::decode(&bytes, &password) {
                Ok(b) => break b,
                Err(BackupError::WrongPasswordOrCorrupt) => {
                    eprintln!("backup: import refused reason=wrong_password");
                    self.show_backup_error(&BackupError::WrongPasswordOrCorrupt);
                }
                Err(e) => {
                    eprintln!("backup: import refused reason=invalid");
                    self.show_backup_error(&e);
                    return;
                }
            }
        };

        self.confirm_and_apply_import(imported, false);
    }

    /// Last steps shared by "Import full backup" and "Import from another
    /// LockNote": confirm, re-check the lock, apply. `from_exe` only selects
    /// the wording and the log tag.
    pub(super) fn confirm_and_apply_import(&self, mut imported: crate::features::backup::Backup, from_exe: bool) {
        let tag = if from_exe { "import_exe" } else { "backup" };
        let date = if imported.created_unix == 0 {
            "?".to_string()
        } else {
            crate::features::snapshots::format_timestamp(imported.created_unix)
        };
        let confirm_text = if from_exe {
            t("import_exe.confirm")
        } else {
            t("backup.import.confirm")
        }
        .replace("{date}", &date)
        .replace("{tabs}", &imported.tabs.len().to_string())
        .replace("{prompts}", &imported.prompts.len().to_string())
        .replace("{secrets}", &imported.secrets.len().to_string())
        .replace("{attachments}", &imported.attachments.len().to_string());
        crate::features::import_log::note(&format!(
            "{}: confirm shown tabs={} snapshots={} secrets={} attachments={} prompts={}",
            tag,
            imported.tabs.len(),
            imported.snapshots.len(),
            imported.secrets.len(),
            imported.attachments.len(),
            imported.prompts.len()
        ));
        let params = nwg::MessageParams {
            title: &if from_exe {
                t("import_exe.title")
            } else {
                t("backup.import.open_title")
            },
            content: &confirm_text,
            buttons: nwg::MessageButtons::YesNo,
            icons: nwg::MessageIcons::Warning,
        };
        if !self.confirm(&params) {
            crate::features::import_log::note(&format!("{}: import cancelled reason=user", tag));
            imported.wipe();
            return;
        }

        // Every dialog above is modal while the auto-lock timer keeps ticking.
        // If it fired, the note is locked and wiped: applying now would put
        // plaintext back under the lock overlay and the import would vanish
        // at the next unlock.
        if *self.is_locked.borrow() || self.password.borrow().is_empty() {
            crate::features::import_log::note(&format!("{}: import refused reason=locked", tag));
            imported.wipe();
            return;
        }

        // Safety net: the current note goes to a verified .lnbak first. No
        // backup, no import (the Save that follows is the point of no return).
        if let Err(detail) = self.write_auto_backup(crate::features::auto_backup::Kind::PreImport) {
            crate::features::import_log::note(&format!("{}: import refused reason=auto_backup", tag));
            imported.wipe();
            self.inform("LockNote",
                &t("autobackup.failed.import")
                    .replace("{msg}", &detail),
            );
            return;
        }

        crate::features::import_log::note(&format!(
            "{}: import ok tabs={} snapshots={} secrets={} attachments={} prompts={}",
            tag,
            imported.tabs.len(),
            imported.snapshots.len(),
            imported.secrets.len(),
            imported.attachments.len(),
            imported.prompts.len()
        ));
        self.apply_backup(imported);
        let done = if from_exe {
            t("import_exe.done")
        } else {
            t("backup.import.done")
        };
        self.inform("LockNote", &done);
    }

    /// Bring in the data of another LockNote file (an older exe): read what
    /// follows its marker (or its pending staging file), ask the password of
    /// THAT data, then go through the same confirmation and apply path as a
    /// backup import. The source file is only read, never modified.
    pub(super) fn on_import_from_exe(&self) {
        use crate::ui::dialogs::backup_password::BackupPasswordDialog;
        use crate::features::backup::{self, BackupError};

        crate::features::import_log::note("import_exe: start");
        if *self.is_locked.borrow() {
            crate::features::import_log::note("import_exe: refused reason=locked_at_start");
            return;
        }

        let mut dialog = nwg::FileDialog::default();
        nwg::FileDialog::builder()
            .title(&t("import_exe.open_title"))
            .action(nwg::FileDialogAction::Open)
            .filters(&t("import_exe.filter"))
            .build(&mut dialog)
            .expect("Failed to build import-from-exe dialog");
        if !dialog.run(Some(&self.window)) {
            crate::features::import_log::note("import_exe: cancelled reason=file_dialog");
            return;
        }
        let Ok(selected) = dialog.get_selected_item() else {
            crate::features::import_log::note("import_exe: refused reason=no_file_selected");
            return;
        };
        let path = std::path::PathBuf::from(selected);

        // Importing the file we are running from would read our own data.
        let same = match (std::fs::canonicalize(&path), std::fs::canonicalize(&self.exe_path)) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        };
        if same {
            crate::features::import_log::note("import_exe: refused reason=same_file");
            self.inform("LockNote",
                &t("import_exe.same_file"),
            );
            return;
        }

        match std::fs::metadata(&path) {
            Ok(md) if md.len() > backup::MAX_FILE_SIZE => {
                crate::features::import_log::note("import_exe: refused reason=too_large");
                self.inform("LockNote",
                    &t("backup.import.too_large"),
                );
                return;
            }
            Ok(_) => {}
            Err(e) => {
                crate::features::import_log::note("import_exe: refused reason=read");
                self.inform("LockNote",
                    &t("backup.import.read_error")
                        .replace("{msg}", &e.to_string()),
                );
                return;
            }
        }

        // Same source of truth as the application itself: the staging file of
        // that exe wins over the exe when it is the latest state. If that exe
        // no longer holds the notes its staging file was based on (an
        // unresolved conflict, see storage::PendingDecision), the exe is read.
        // A staging file (or its companion) that cannot be read may hold
        // notes more recent than the exe: nothing is imported, and the
        // message names that file.
        let payload = match crate::storage::read_data(&path) {
            Ok(Some(payload)) => payload,
            Ok(None) => {
                crate::features::import_log::note("import_exe: refused reason=no_data");
                self.inform("LockNote",
                    &t("import_exe.no_data"),
                );
                return;
            }
            Err(e) => {
                crate::features::import_log::note("import_exe: refused reason=pending_unreadable");
                self.inform("LockNote",
                    &t("backup.import.read_error").replace("{msg}", &e),
                );
                return;
            }
        };

        crate::features::import_log::note(&format!("import_exe: payload read bytes={}", payload.len()));
        let imported = loop {
            let Some(password) = BackupPasswordDialog::show(
                &t("import_exe.pw_title"),
                &t("import_exe.pw_label"),
                &t("goto.ok"),
                &t("goto.cancel"),
                &t("unlock.empty"),
            ) else {
                crate::features::import_log::note("import_exe: cancelled reason=password_dialog");
                return;
            };
            crate::features::import_log::note("import_exe: password given, decoding");
            let started = std::time::Instant::now();
            let decoded = backup::decode_exe_payload(&payload, &password);
            crate::features::import_log::note(&format!(
                "import_exe: decode finished ms={} ok={}",
                started.elapsed().as_millis(),
                decoded.is_ok()
            ));
            match decoded {
                Ok(b) => break b,
                Err(BackupError::WrongPasswordOrCorrupt) => {
                    crate::features::import_log::note("import_exe: refused reason=wrong_password");
                    self.show_backup_error(&BackupError::WrongPasswordOrCorrupt);
                }
                Err(e) => {
                    crate::features::import_log::note("import_exe: refused reason=invalid");
                    self.show_backup_error(&e);
                    return;
                }
            }
        };
        self.confirm_and_apply_import(imported, true);
    }

    /// Replace the in-memory note with a decoded backup. Window geometry and
    /// side panel widths stay this machine's; keys loaded in ssh-agent are not touched (they are
    /// tracked per session, not part of a backup).
    pub(super) fn apply_backup(&self, backup: crate::features::backup::Backup) {
        let mut settings = backup.settings;
        {
            let current = self.settings.borrow();
            settings.window_x = current.window_x;
            settings.window_y = current.window_y;
            settings.window_width = current.window_width;
            settings.window_height = current.window_height;
            settings.window_maximized = current.window_maximized;
            crate::ui::side_panel::keep_machine_panel_widths(&current, &mut settings);
        }
        settings.active_tab = 0;
        let new_mode = crate::theme::mode_for_choice(settings.theme);
        // Settings first: the vault tree reads the user's categories from them.
        *self.settings.borrow_mut() = settings;
        // The imported theme applies live (the language still needs a restart).
        self.apply_theme_live(new_mode);

        *self.attachments.borrow_mut() = backup.attachments;
        self.vault_panel.set_secrets(backup.secrets);
        // The imported vault shows whole, not through the previous search.
        self.clear_vault_search();
        self.refresh_vault_tree(None);
        self.prompts_panel.set_prompts(backup.prompts);
        self.refresh_prompts_list(None);

        // Same view rule as a load: at least one tab, active index in range.
        let view = crate::features::payload::build_view(backup.tabs, None, None);
        let ui_tabs: Vec<crate::ui::tab_bar::Tab> = view
            .tabs
            .into_iter()
            .map(|nt| {
                // Not saved yet: `saved_content` stays empty until the next
                // save (Compare with Last Save shows every line as added).
                let mut t = crate::ui::tab_bar::Tab::new(&nt.name);
                t.content = nt.content;
                t.color = nt.color;
                t
            })
            .collect();
        let content = ui_tabs[view.active].content.clone();
        let tb = crate::ui::tab_bar::TabBar::with_tabs(ui_tabs);
        tb.set_active(view.active);
        *self.tab_bar.borrow_mut() = tb;

        self.set_content(&content);
        self.mark_modified();
        self.update_title();
        self.update_status_bar();
        self.paint_tab_bar();
        // The imported settings may change word wrap, line numbers and the
        // font; a font that cannot be built is reported, as in the
        // preferences, once the import is fully applied.
        let font = self.sync_view_menu();
        self.layout_controls();
        if let Err(e) = font {
            self.inform("LockNote", &t("settings.font.failed").replace("{detail}", &e));
        }
    }
}
