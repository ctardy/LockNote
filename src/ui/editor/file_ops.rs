// File operations: save and plain-text export.
// Part of the EditorForm implementation, split out of the former editor.rs.

#[allow(unused_imports)]
use super::*;

impl EditorForm {
    // =====================================================================
    // File operations
    // =====================================================================

    /// Save the current note.
    ///
    /// Returns `Ok(())` when the file was written successfully, or
    /// `Err(msg)` with a human-readable error. Callers that do not care about
    /// the outcome can discard the result with `let _ = ...`; callers that
    /// need to act on failure (e.g. the autolock timer's `SaveThenLock`
    /// branch) inspect it.
    pub(super) fn on_save(&self) -> Result<(), String> {
        // Fail fast before touching the payload: no modal here, the caller
        // (possibly the autolock timer, see issue 049) gets the Err.
        let precondition = save_precondition(&self.password.borrow());
        if let Err(msg) = precondition {
            eprintln!("save: refused reason=empty_password");
            return Err(msg);
        }

        let note_text = self.get_content();

        // The payload is every tab, the prompts, the vault, the
        // attachments and the session state, assembled by
        // `features::payload::compose` (the session state last, so it is the first
        // thing parsed on reload).
        let note_text = {
            // Sync current editor content into the active tab
            let tb = self.tab_bar.borrow_mut();
            let active = tb.active_index();
            tb.set_tab_content(active, &note_text);
            let tabs = tb.all_tabs();
            drop(tb);

            // Convert UI tabs to NoteTab for serialization
            let mut note_tabs: Vec<crate::features::tabs::NoteTab> = tabs.iter().map(|t| {
                crate::features::tabs::NoteTab::with_content(&t.name, &t.content, t.color.as_deref())
            }).collect();
            if note_tabs.is_empty() {
                // The tab bar always holds one tab; without any, the editor
                // text is saved as the default single tab (`compose` decides
                // between plain text and the tabs block).
                note_tabs.push(crate::features::tabs::NoteTab::with_content(
                    crate::features::payload::DEFAULT_TAB_NAME,
                    &note_text,
                    None,
                ));
            }

            let prompts = self.prompts_panel.get_prompts();
            let secrets = self.vault_panel.get_secrets();
            let state = self.capture_session_state();
            let attachments = self.attachments.borrow();
            crate::features::payload::compose(&note_tabs, &prompts, &secrets, &attachments, &state)
        };

        let settings = self.settings.borrow();
        // The `combined` buffer holds the full plaintext being encrypted:
        // settings + tabs + vault + attachments + state. It's
        // the single most sensitive string in the save path. Wrapping in
        // Zeroizing guarantees the buffer is wiped when this scope exits,
        // instead of staying in the allocator's free list after save.
        let combined: Zeroizing<String> = Zeroizing::new(settings.serialize(&note_text));
        // Compress before encryption
        let combined: Zeroizing<String> =
            Zeroizing::new(crate::features::compression::compress(&combined));
        let password = self.password.borrow();

        let encrypted = match crate::crypto::encrypt(
            &combined,
            &password,
            crate::crypto::DEFAULT_M_COST,
            crate::crypto::DEFAULT_T_COST,
            crate::crypto::DEFAULT_P_LANES,
        ) {
            Ok(bytes) => bytes,
            Err(e) => {
                let msg = t("save.error.encrypt").replace("{}", &e.to_string());
                nwg::modal_info_message(
                    &self.window,
                    &t("save.error.title"),
                    &msg,
                );
                return Err(msg);
            }
        };

        match crate::storage::write_data(&self.exe_path, &encrypted) {
            Ok(()) => {
                self.mark_clean();
                Ok(())
            }
            Err(e) => {
                let msg = t("save.error.msg").replace("{}", &e.to_string());
                nwg::modal_info_message(
                    &self.window,
                    &t("save.error.title"),
                    &msg,
                );
                Err(msg)
            }
        }
    }

    /// Export the current note as a text file with encoding selection.
    pub(super) fn on_export(&self) {
        use crate::ui::dialogs::export_encoding::ExportEncodingDialog;
        use crate::ui::dialogs::confirm_password::ConfirmPasswordDialog;

        // Require password confirmation before export
        if !ConfirmPasswordDialog::show(&self.password.borrow(), &t("confirmpwd.export")) {
            return;
        }

        // Step 1: Choose encoding
        let encoding = match ExportEncodingDialog::show() {
            Some(enc) => enc,
            None => return,
        };

        // Step 2: File save dialog
        let mut dialog = nwg::FileDialog::default();
        nwg::FileDialog::builder()
            .title(&t("export.dlg.title"))
            .action(nwg::FileDialogAction::Save)
            .filters(&format!("{}|{}", t("filter.text"), t("filter.all")))
            .build(&mut dialog)
            .expect("Failed to build export dialog");

        if dialog.run(Some(&self.window)) {
            if let Ok(path) = dialog.get_selected_item() {
                let content = self.get_content();
                let (bytes, had_replacements) = encoding.encode(&content);

                // Warn if some characters were replaced
                if had_replacements {
                    let params = nwg::MessageParams {
                        title: &t("export.warning_title"),
                        content: &t("export.warning_lossy"),
                        buttons: nwg::MessageButtons::YesNo,
                        icons: nwg::MessageIcons::Warning,
                    };
                    if nwg::message(&params) != nwg::MessageChoice::Yes {
                        return;
                    }
                }

                match std::fs::write(&path, &bytes) {
                    Ok(_) => {
                        nwg::modal_info_message(
                            &self.window,
                            &t("export.success.title"),
                            &t("export.success.encoding")
                                .replace("{encoding}", encoding.label())
                                .replace("{path}", &path.to_string_lossy()),
                        );
                    }
                    Err(e) => {
                        nwg::modal_info_message(
                            &self.window,
                            &t("export.error.title"),
                            &t("export.error.msg").replace("{}", &e.to_string()),
                        );
                    }
                }
            }
        }
    }
}
