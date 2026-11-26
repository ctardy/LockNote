// Attachments and TOTP panels.
// Part of the EditorForm implementation, split out of the former editor.rs.

#[allow(unused_imports)]
use super::*;

impl EditorForm {
    // =====================================================================
    // Attachments panel
    // =====================================================================

    /// Toggle the attachments panel.
    pub(super) fn on_toggle_attachments(&self) {
        self.attach_panel.toggle_visible();
        let visible = self.attach_panel.is_visible();
        self.menu_tools_attachments.set_checked(visible);
        if visible {
            self.close_other_panels("attachments");
            self.refresh_attach_list();
        }
        self.layout_controls();
        self.repaint_toolbar_buttons();
    }

    /// Apply the current theme to the attachments panel frame and controls.
    pub(super) fn theme_attach_panel(&self) {
        // The palette is read on every message (and the brush comes from the
        // colour-keyed cache), so a theme switch needs no re-binding.
        let handler = nwg::bind_raw_event_handler(
            &self.attach_frame.handle,
            0x40001,
            move |_hwnd, msg, wparam, _lparam| {
                let pal = crate::theme::current();
                let bg_colorref = pal.background.to_colorref();
                let text_colorref = pal.text_primary.to_colorref();
                let bg_brush = windows::Win32::Graphics::Gdi::HBRUSH(
                    crate::ui::lock_card::cached_brush(pal.background) as *mut _,
                );
                match msg {
                    0x0138 | 0x0134 => { // WM_CTLCOLORSTATIC | WM_CTLCOLORLISTBOX
                        let hdc = windows::Win32::Graphics::Gdi::HDC(wparam as *mut _);
                        unsafe {
                            windows::Win32::Graphics::Gdi::SetTextColor(hdc, windows::Win32::Foundation::COLORREF(text_colorref));
                            windows::Win32::Graphics::Gdi::SetBkColor(hdc, windows::Win32::Foundation::COLORREF(bg_colorref));
                        }
                        Some(bg_brush.0 as isize)
                    }
                    0x0014 => { // WM_ERASEBKGND
                        let hdc = windows::Win32::Graphics::Gdi::HDC(wparam as *mut _);
                        unsafe {
                            let hwnd = windows::Win32::Foundation::HWND(_hwnd as *mut _);
                            let mut rect = std::mem::zeroed();
                            let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rect);
                            windows::Win32::Graphics::Gdi::FillRect(hdc, &rect, bg_brush);
                        }
                        Some(1)
                    }
                    _ => None,
                }
            },
        );
        std::mem::forget(handler);
    }

    /// Refresh the attachments list box.
    pub(super) fn refresh_attach_list(&self) {
        let attachments = self.attachments.borrow();
        let items = crate::ui::attachments_panel::AttachmentsPanel::formatted_list(&attachments);
        let status = crate::ui::attachments_panel::AttachmentsPanel::status_text(&attachments);
        self.attach_list.set_collection(items);
        self.attach_lbl_status.set_text(&status);
    }

    /// Get the currently selected attachment index.
    pub(super) fn selected_attach_index(&self) -> Option<usize> {
        self.attach_list.selection()
    }

    /// Add a file attachment via file dialog.
    pub(super) fn on_attach_add(&self) {
        let mut dialog = nwg::FileDialog::default();
        nwg::FileDialog::builder()
            .title(&t("attach.add.title"))
            .action(nwg::FileDialogAction::Open)
            .build(&mut dialog)
            .expect("Failed to build file dialog");

        if !dialog.run(Some(&self.window)) {
            return;
        }

        let path = match dialog.get_selected_item() {
            Ok(p) => p,
            Err(_) => return,
        };

        self.attach_file_from_path(std::path::Path::new(&path));
    }

    /// Attach a file from a given path (shared by file-dialog and drag-drop flows).
    /// Reads, checks size limits, and appends to the attachments panel.
    /// Shows a modal error on failure.
    pub(super) fn attach_file_from_path(&self, path: &std::path::Path) {
        let file_name = path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "file".to_string());

        let settings = self.settings.borrow();
        let max_file = (settings.max_attachment_size_mb as u64) * 1024 * 1024;
        let max_total = (settings.max_total_attachments_mb as u64) * 1024 * 1024;
        drop(settings);

        // Size check BEFORE reading so a dropped 10 GB file doesn't allocate
        // 10 GB of RAM just to be rejected. We still have to read the data
        // to compute SHA-256, but only after the size is known to fit.
        let file_size = match std::fs::metadata(path) {
            Ok(m) => m.len(),
            Err(e) => {
                let msg = t("attach.error.read").replace("{}", &e.to_string());
                nwg::modal_info_message(&self.window, &t("attach.error.title"), &msg);
                return;
            }
        };
        {
            let attachments = self.attachments.borrow();
            if let Err(e) = crate::features::attachments::check_limits(&attachments, file_size, max_file, max_total) {
                nwg::modal_info_message(&self.window, &t("attach.limit.title"), &attach_limit_message(&e));
                return;
            }
        }

        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(e) => {
                let msg = t("attach.error.read").replace("{}", &e.to_string());
                nwg::modal_info_message(&self.window, &t("attach.error.title"), &msg);
                return;
            }
        };

        let att = crate::features::attachments::Attachment::new(&file_name, data);
        self.attachments.borrow_mut().push(att);
        self.refresh_attach_list();
        self.mark_modified();
    }

    /// Show a save-file dialog with pre-filled filename and extension filter.
    /// Returns the chosen path, or None if cancelled.
    pub(super) fn show_save_dialog(&self, filename: &str, title: &str) -> Option<String> {
        let ext = std::path::Path::new(filename)
            .extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_default();

        // Build filter string: "Extension Files (*.ext)\0*.ext\0All Files (*.*)\0*.*\0\0"
        let filter = if ext.is_empty() {
            format!("{}\0*.*\0\0", t("filter.all"))
        } else {
            let label = t("filter.ext")
                .replace("{EXT}", &ext.to_uppercase())
                .replace("{ext}", &ext);
            format!("{}\0*.{}\0{}\0*.*\0\0", label, ext, t("filter.all"))
        };
        let filter_wide: Vec<u16> = filter.encode_utf16().collect();

        // Pre-fill filename in a buffer (MAX_PATH)
        let mut file_buf = [0u16; 260];
        let name_wide: Vec<u16> = filename.encode_utf16().collect();
        let copy_len = name_wide.len().min(file_buf.len() - 1);
        file_buf[..copy_len].copy_from_slice(&name_wide[..copy_len]);

        let title_wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();

        let hwnd = if let nwg::ControlHandle::Hwnd(h) = self.window.handle {
            windows::Win32::Foundation::HWND(h as *mut _)
        } else {
            windows::Win32::Foundation::HWND::default()
        };

        let mut ofn = windows::Win32::UI::Controls::Dialogs::OPENFILENAMEW {
            lStructSize: std::mem::size_of::<windows::Win32::UI::Controls::Dialogs::OPENFILENAMEW>() as u32,
            hwndOwner: hwnd,
            lpstrFilter: windows::core::PCWSTR(filter_wide.as_ptr()),
            lpstrFile: windows::core::PWSTR(file_buf.as_mut_ptr()),
            nMaxFile: file_buf.len() as u32,
            lpstrTitle: windows::core::PCWSTR(title_wide.as_ptr()),
            Flags: windows::Win32::UI::Controls::Dialogs::OFN_OVERWRITEPROMPT
                | windows::Win32::UI::Controls::Dialogs::OFN_PATHMUSTEXIST,
            ..Default::default()
        };

        let result = unsafe {
            windows::Win32::UI::Controls::Dialogs::GetSaveFileNameW(&mut ofn)
        };

        if result.0 != 0 {
            let path = String::from_utf16_lossy(
                &file_buf[..file_buf.iter().position(|&c| c == 0).unwrap_or(file_buf.len())]
            );
            Some(path)
        } else {
            None
        }
    }

    /// Extract the selected attachment to a chosen location and open it.
    pub(super) fn on_attach_open(&self) {
        let idx = match self.selected_attach_index() {
            Some(i) => i,
            None => return,
        };

        let attachments = self.attachments.borrow();
        let att = match attachments.get(idx) {
            Some(a) => a,
            None => return,
        };

        if !att.verify() {
            nwg::modal_info_message(&self.window, &t("attach.error.title"), &t("attach.error.corrupt"));
            return;
        }

        let title = t("attach.save.title").replace("{}", &att.name);
        let path = match self.show_save_dialog(&att.name, &title) {
            Some(p) => p,
            None => return,
        };

        if let Err(e) = std::fs::write(&path, &att.data) {
            let msg = t("attach.error.save").replace("{}", &e.to_string());
            nwg::modal_info_message(&self.window, &t("attach.error.title"), &msg);
            return;
        }

        // Ask the user to confirm before we ShellExecute an executable
        // extension. A vault shared by a third party could otherwise ship
        // a ".exe"/".bat"/".lnk" and earn arbitrary code execution with a
        // single click. We never refuse outright — mature apps (PDF, DOCX,
        // ZIP) are safe — but exe-like extensions get an explicit prompt.
        let ext_lower = std::path::Path::new(&path)
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default();
        const RISKY_EXTS: &[&str] = &[
            "exe", "bat", "cmd", "com", "scr", "pif",
            "ps1", "psm1", "vbs", "vbe", "js", "jse",
            "wsf", "wsh", "hta", "lnk", "url", "msi",
            "msp", "mst", "cpl", "reg",
        ];
        if RISKY_EXTS.contains(&ext_lower.as_str()) {
            let params = nwg::MessageParams {
                title: &t("attach.risky.title"),
                content: &t("attach.risky.body"),
                buttons: nwg::MessageButtons::YesNo,
                icons: nwg::MessageIcons::Warning,
            };
            if nwg::message(&params) != nwg::MessageChoice::Yes {
                return;
            }
        }

        // Open the saved file with the default app
        let wide_path: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
        let wide_open: Vec<u16> = "open".encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            windows::Win32::UI::Shell::ShellExecuteW(
                None,
                windows::core::PCWSTR(wide_open.as_ptr()),
                windows::core::PCWSTR(wide_path.as_ptr()),
                None,
                None,
                windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
            );
        }
    }

    /// Save the selected attachment to disk.
    pub(super) fn on_attach_save(&self) {
        let idx = match self.selected_attach_index() {
            Some(i) => i,
            None => return,
        };

        let attachments = self.attachments.borrow();
        let att = match attachments.get(idx) {
            Some(a) => a,
            None => return,
        };

        let title = t("attach.save.title").replace("{}", &att.name);
        let path = match self.show_save_dialog(&att.name, &title) {
            Some(p) => p,
            None => return,
        };

        if let Err(e) = std::fs::write(&path, &att.data) {
            let msg = t("attach.error.save").replace("{}", &e.to_string());
            nwg::modal_info_message(&self.window, &t("attach.error.title"), &msg);
        }
    }

    /// Delete the selected attachment.
    pub(super) fn on_attach_delete(&self) {
        let idx = match self.selected_attach_index() {
            Some(i) => i,
            None => return,
        };

        let name = {
            let attachments = self.attachments.borrow();
            match attachments.get(idx) {
                Some(a) => a.name.clone(),
                None => return,
            }
        };

        let msg = t("attach.confirm.delete").replace("{}", &name);
        let title = t("attach.confirm.title");
        let result = nwg::message(&nwg::MessageParams {
            title: &title,
            content: &msg,
            buttons: nwg::MessageButtons::YesNo,
            icons: nwg::MessageIcons::Question,
        });

        if result == nwg::MessageChoice::Yes {
            self.attachments.borrow_mut().remove(idx);
            self.refresh_attach_list();
            self.mark_modified();
        }
    }

    /// Toggle TOTP panel visibility.
    pub(super) fn on_toggle_totp(&self) {
        self.totp_panel.toggle_visible();
        let visible = self.totp_panel.is_visible();
        self.menu_tools_totp.set_checked(visible);
        if visible {
            self.close_other_panels("totp");
            self.refresh_totp_list();
            self.totp_timer.start();
        } else {
            self.totp_timer.stop();
        }
        self.layout_controls();
        self.repaint_toolbar_buttons();
    }

    /// Apply the current theme to the TOTP panel frame.
    pub(super) fn theme_totp_panel(&self) {
        // The palette is read on every message (and the brush comes from the
        // colour-keyed cache), so a theme switch needs no re-binding.
        let handler = nwg::bind_raw_event_handler(
            &self.totp_frame.handle,
            0x40006,
            move |_hwnd, msg, wparam, _lparam| {
                let pal = crate::theme::current();
                let bg_colorref = pal.background.to_colorref();
                let text_colorref = pal.text_primary.to_colorref();
                let bg_brush = windows::Win32::Graphics::Gdi::HBRUSH(
                    crate::ui::lock_card::cached_brush(pal.background) as *mut _,
                );
                match msg {
                    0x0138 | 0x0134 => { // WM_CTLCOLORSTATIC | WM_CTLCOLORLISTBOX
                        let hdc = windows::Win32::Graphics::Gdi::HDC(wparam as *mut _);
                        unsafe {
                            windows::Win32::Graphics::Gdi::SetTextColor(hdc, windows::Win32::Foundation::COLORREF(text_colorref));
                            windows::Win32::Graphics::Gdi::SetBkColor(hdc, windows::Win32::Foundation::COLORREF(bg_colorref));
                        }
                        Some(bg_brush.0 as isize)
                    }
                    0x0014 => { // WM_ERASEBKGND
                        let hdc = windows::Win32::Graphics::Gdi::HDC(wparam as *mut _);
                        unsafe {
                            let hwnd = windows::Win32::Foundation::HWND(_hwnd as *mut _);
                            let mut rect = std::mem::zeroed();
                            let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rect);
                            windows::Win32::Graphics::Gdi::FillRect(hdc, &rect, bg_brush);
                        }
                        Some(1)
                    }
                    _ => None,
                }
            },
        );
        std::mem::forget(handler);
    }

    /// Refresh the TOTP list box with current codes.
    pub(super) fn refresh_totp_list(&self) {
        // Sync entries from settings into the panel
        let settings_entries = self.settings.borrow().totp_entries.clone();
        let entries: Vec<crate::features::totp::TotpEntry> = settings_entries
            .iter()
            .map(|e| crate::features::totp::TotpEntry { name: e.name.clone(), secret: e.secret.clone() })
            .collect();
        self.totp_panel.set_entries(entries);

        let sel = self.totp_list.selection();
        let items = self.totp_panel.formatted_list();
        self.totp_list.set_collection(items);
        if let Some(idx) = sel {
            self.totp_list.set_selection(Some(idx));
        }
        self.totp_lbl_countdown.set_text(&self.totp_panel.countdown_text());
    }

    /// Handle TOTP timer tick — refresh codes every second.
    pub(super) fn on_totp_timer_tick(&self) {
        if !self.totp_panel.is_visible() {
            return;
        }
        let sel = self.totp_list.selection();
        let items = self.totp_panel.formatted_list();
        self.totp_list.set_collection(items);
        if let Some(idx) = sel {
            self.totp_list.set_selection(Some(idx));
        }
        self.totp_lbl_countdown.set_text(&self.totp_panel.countdown_text());
    }

    /// Copy the selected TOTP code to the clipboard.
    pub(super) fn on_totp_copy(&self) {
        if let Some(idx) = self.totp_list.selection() {
            if let Some(name) = self.totp_panel.copy_code(idx) {
                self.totp_lbl_status.set_text(&t("totp.copied").replace("{name}", &name));
            }
        } else {
            self.totp_lbl_status.set_text(&t("totp.select_first"));
        }
    }

    /// Add a new TOTP entry.
    pub(super) fn on_totp_add(&self) {
        // Use the existing TotpSetupDialog for adding
        if let Some(name) = crate::ui::dialogs::totp_manager::simple_input_dialog(
            &self.window.handle, &t("totp.add.title"), &t("totp.add.prompt"),
        ) {
            if !name.is_empty() {
                if let Some(entry) = crate::ui::dialogs::totp_setup::TotpSetupDialog::show_add_key(&name) {
                    // Add to settings
                    let settings_entry = crate::settings::TotpEntry {
                        name: entry.name.clone(),
                        secret: entry.secret.clone(),
                    };
                    self.settings.borrow_mut().totp_entries.push(settings_entry);
                    self.mark_modified();
                    self.refresh_totp_list();
                    self.totp_lbl_status.set_text(&t("totp.added"));
                }
            }
        }
    }

    /// Remove the selected TOTP entry.
    pub(super) fn on_totp_remove(&self) {
        if let Some(idx) = self.totp_list.selection() {
            let name = {
                let entries = self.settings.borrow().totp_entries.clone();
                entries.get(idx).map(|e| e.name.clone())
            };
            if let Some(name) = name {
                let confirm = nwg::modal_message(
                    &self.window.handle,
                    &nwg::MessageParams {
                        title: &t("totp.remove.title"),
                        content: &t("totp.remove.msg").replace("{name}", &name),
                        buttons: nwg::MessageButtons::YesNo,
                        icons: nwg::MessageIcons::Warning,
                    },
                );
                if confirm == nwg::MessageChoice::Yes {
                    self.settings.borrow_mut().totp_entries.remove(idx);
                    self.mark_modified();
                    self.refresh_totp_list();
                    self.totp_lbl_status.set_text(&t("totp.removed"));
                }
            }
        } else {
            self.totp_lbl_status.set_text(&t("totp.select_first"));
        }
    }

    /// Sync TOTP panel entries back to settings (called before save).
    pub(super) fn sync_totp_to_settings(&self) {
        let entries = self.totp_panel.get_entries();
        let converted: Vec<crate::settings::TotpEntry> = entries
            .iter()
            .map(|e| crate::settings::TotpEntry { name: e.name.clone(), secret: e.secret.clone() })
            .collect();
        self.settings.borrow_mut().totp_entries = converted;
    }
}
