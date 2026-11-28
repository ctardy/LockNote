// Steganography: hide and extract.
// Part of the EditorForm implementation, split out of the former editor.rs.

#[allow(unused_imports)]
use super::*;

impl EditorForm {
    /// Hide the current note content inside a carrier image.
    pub(super) fn on_stego_hide(&self) {
        let epoch = self.lock_epoch();
        // Step 1: Choose carrier image
        let mut open_dialog = nwg::FileDialog::default();
        nwg::FileDialog::builder()
            .title(&t("stego.dlg.open_carrier"))
            .action(nwg::FileDialogAction::Open)
            .filters(&format!("{}|{}", t("filter.images"), t("filter.all")))
            .build(&mut open_dialog)
            .expect("Failed to build file open dialog");

        if !open_dialog.run(Some(&self.window)) {
            return;
        }
        let carrier_path = match open_dialog.get_selected_item() {
            Ok(p) => std::path::PathBuf::from(p),
            Err(_) => return,
        };

        // Read carrier and validate format
        let image_data = match std::fs::read(&carrier_path) {
            Ok(data) => data,
            Err(e) => {
                self.inform(&t("stego.error"),
                    &format!("{}: {}", t("stego.error.read_image"), e),
                );
                return;
            }
        };
        // Detect format by magic bytes, fallback to file extension
        if crate::features::stego::detect_format(&image_data).is_none()
            && crate::features::stego::detect_format_by_extension(&carrier_path).is_none()
        {
            self.inform(&t("stego.error.format_title"),
                &t("stego.error.format"),
            );
            return;
        }

        // Step 2: Choose output path
        let ext = carrier_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("jpg");
        let filter = format!("{}|{}", t("filter.image").replace("{ext}", ext), t("filter.all"));

        let mut save_dialog = nwg::FileDialog::default();
        nwg::FileDialog::builder()
            .title(&t("stego.dlg.save"))
            .action(nwg::FileDialogAction::Save)
            .filters(&filter)
            .build(&mut save_dialog)
            .expect("Failed to build file save dialog");

        if !save_dialog.run(Some(&self.window)) {
            return;
        }
        let output_path = match save_dialog.get_selected_item() {
            Ok(p) => p,
            Err(_) => return,
        };

        // Step 3: Ask for a dedicated password for this image
        let password = match stego_password_dialog(&self.window.handle, false) {
            Some(pw) => pw,
            None => return,
        };

        // Nothing is read nor written if a lock came during the dialogs.
        if !self.still_valid(epoch) {
            eprintln!("INFO stego: hide dropped, the note locked meanwhile");
            return;
        }
        // Step 4: Encrypt current tab content and embed
        let content = match self.get_content() {
            Ok(text) => Zeroizing::new(text),
            Err(e) => return self.tell_unreadable("stego_hide", &e),
        };
        let encrypted = match crate::crypto::encrypt(
            &content,
            &password,
            crate::crypto::DEFAULT_M_COST,
            crate::crypto::DEFAULT_T_COST,
            crate::crypto::DEFAULT_P_LANES,
        ) {
            Ok(bytes) => bytes,
            Err(e) => {
                self.inform(&t("stego.error"),
                    &format!("{}: {}", t("stego.error.encrypt"), e),
                );
                return;
            }
        };

        match crate::features::stego::embed_with_path(&image_data, &encrypted, Some(&carrier_path)) {
            Ok(result) => {
                match std::fs::write(&output_path, &result) {
                    Ok(_) => {
                        let image_kb = image_data.len() / 1024;
                        let result_kb = result.len() / 1024;
                        let msg = format!(
                            "{}\n{}: {} KB \u{2192} {} KB\n\n{}",
                            t("stego.success"),
                            t("stego.size"),
                            image_kb,
                            result_kb,
                            t("stego.warning_recompress"),
                        );
                        self.inform(&t("stego.title"),
                            &msg,
                        );
                    }
                    Err(e) => {
                        self.inform(&t("stego.error"),
                            &format!("{}: {}", t("stego.error.write"), e),
                        );
                    }
                }
            }
            Err(crate::features::stego::StegoError::PayloadTooLarge { ratio_percent }) => {
                self.inform(&t("stego.error.toolarge_title"),
                    &format!(
                        "{} ({}%)\n{}",
                        t("stego.error.toolarge"),
                        ratio_percent,
                        t("stego.error.toolarge_hint"),
                    ),
                );
            }
            Err(e) => {
                self.inform(&t("stego.error"),
                    &format!("{}: {}", t("stego.error.embed"), e),
                );
            }
        }
    }

    /// Extract hidden notes from a steganographic image.
    pub(super) fn on_stego_extract(&self) {
        let epoch = self.lock_epoch();
        // Step 1: Choose image file
        let mut open_dialog = nwg::FileDialog::default();
        nwg::FileDialog::builder()
            .title(&t("stego.dlg.open_extract"))
            .action(nwg::FileDialogAction::Open)
            .filters(&format!("{}|{}", t("filter.images"), t("filter.all")))
            .build(&mut open_dialog)
            .expect("Failed to build file open dialog");

        if !open_dialog.run(Some(&self.window)) {
            return;
        }
        let image_path = match open_dialog.get_selected_item() {
            Ok(p) => std::path::PathBuf::from(p),
            Err(_) => return,
        };

        // Step 2: Read image and check for hidden data
        let file_data = match std::fs::read(&image_path) {
            Ok(data) => data,
            Err(e) => {
                self.inform(&t("stego.error"),
                    &format!("{}: {}", t("stego.error.read_image"), e),
                );
                return;
            }
        };

        if !crate::features::stego::has_hidden_data(&file_data) {
            self.inform(&t("stego.nodata_title"),
                &t("stego.nodata"),
            );
            return;
        }

        // Step 3: Ask for the password used to hide the data
        let password = match stego_password_dialog(&self.window.handle, true) {
            Some(pw) => pw,
            None => return,
        };

        if !self.still_valid(epoch) {
            eprintln!("INFO stego: extract dropped, the note locked meanwhile");
            return;
        }
        match crate::features::stego::extract(&file_data, &password) {
            Ok(plaintext) => {
                // Open extracted notes in a new tab
                {
                    let tb = self.tab_bar.borrow();
                    if tb.count() < crate::ui::tab_bar::MAX_TABS {
                        drop(tb);
                        // Derive tab name from image filename
                        let tab_name = image_path.file_stem()
                            .and_then(|s| s.to_str())
                            .map(str::to_string)
                            .unwrap_or_else(|| t("stego.extracted_tab"));
                        // Save current tab content, switch to the new tab
                        let switched = super::tab_ops::switch_editor_tab(&self.editor, &self.tab_bar, |tb| {
                            let idx = tb.add_tab(&tab_name);
                            tb.set_tab_content(idx, &plaintext);
                            tb.set_active(idx);
                        });
                        if let Err(e) = switched {
                            // No tab was added: the extracted text is not
                            // opened, the user can extract it again.
                            return self.tell_unreadable("stego_open", &e);
                        }
                        self.mark_modified();
                        self.update_title();
                        self.update_status_bar();
                        self.paint_tab_bar();
                        self.layout_controls();
                    } else {
                        drop(tb);
                        self.set_content(&plaintext);
                        self.mark_modified();
                    }
                }
                self.inform(&t("stego.title"),
                    &t("stego.extracted"),
                );
            }
            Err(crate::features::stego::StegoError::DecryptionFailed) => {
                self.inform(&t("stego.error.decrypt_title"),
                    &t("stego.error.decrypt"),
                );
            }
            Err(e) => {
                self.inform(&t("stego.error"),
                    &format!("{}: {}", t("stego.error.extract"), e),
                );
            }
        }
    }
}
