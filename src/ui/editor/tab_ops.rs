// Tab operations and tab context menu.
// Part of the EditorForm implementation, split out of the former editor.rs.

#[allow(unused_imports)]
use super::*;

impl EditorForm {
    // =====================================================================
    // Tab operations
    // =====================================================================

    /// Client width of the tab strip (the reference for every tab layout).
    pub(super) fn tab_bar_client_width(&self) -> Option<i32> {
        let hwnd_val = *self.tab_bar_hwnd.borrow();
        if hwnd_val == 0 {
            return None;
        }
        let mut rect = windows::Win32::Foundation::RECT::default();
        let hwnd = windows::Win32::Foundation::HWND(hwnd_val as *mut _);
        match unsafe { windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rect) } {
            Ok(()) => Some(rect.right),
            Err(e) => {
                eprintln!("tab_bar: GetClientRect failed err={:?}", e);
                None
            }
        }
    }

    /// Tabs, active index and layout of the tab strip for a given client
    /// width. Single source for painting and both click handlers (#047).
    pub(super) fn current_tab_layout(
        &self,
        client_w: i32,
    ) -> Option<(Vec<crate::ui::tab_bar::Tab>, usize, crate::ui::tab_layout::TabLayout)> {
        use crate::ui::tab_layout as tl;
        let tb = self.tab_bar.borrow();
        let tabs = tb.all_tabs();
        let active = tb.active_index();
        drop(tb);
        let right_limit = tl::tabs_right_limit(client_w, &tl::toolbar_buttons());
        let measure = TabTextMeasurer::new();
        let widths = tl::tab_widths(&tabs, &|t| measure.width(t));
        if measure.failed() {
            eprintln!("ERROR tab_bar: text measurement failed, tab strip not laid out tabs={}", tabs.len());
            return None;
        }
        match tl::compute_tab_layout(&widths, right_limit, active) {
            Ok(layout) => Some((tabs, active, layout)),
            Err(e) => {
                eprintln!("tab_bar: layout error err={:?}", e);
                None
            }
        }
    }

    /// Show the drop-down listing the tabs (the active one checked).
    pub(super) fn show_tab_overflow_menu(&self, hidden: &[usize]) {
        let tb = self.tab_bar.borrow();
        let tabs = tb.all_tabs();
        let active_idx = tb.active_index();
        drop(tb);

        if hidden.is_empty() {
            return;
        }
        // The drop-down lists every tab, not only the ones that did not fit.

        unsafe {
            use windows::Win32::UI::WindowsAndMessaging::*;

            let hmenu = CreatePopupMenu().unwrap();
            for i in 0..tabs.len() {
                let text = menu_label(&tabs[i].display_text());
                let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
                let mut flags = MF_STRING;
                if i == active_idx {
                    flags |= MF_CHECKED;
                }
                let _ = AppendMenuW(hmenu, flags, i + 1, windows::core::PCWSTR(wide.as_ptr()));
            }

            // Get cursor position for the popup
            let mut pt = windows::Win32::Foundation::POINT::default();
            let _ = windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut pt);

            let hwnd = if let nwg::ControlHandle::Hwnd(h) = self.window.handle {
                windows::Win32::Foundation::HWND(h as *mut _)
            } else {
                windows::Win32::Foundation::HWND::default()
            };

            let cmd = TrackPopupMenu(
                hmenu,
                TPM_RETURNCMD | TPM_NONOTIFY,
                pt.x, pt.y,
                None,
                hwnd,
                None,
            );

            let _ = DestroyMenu(hmenu);

            // The auto-lock timer keeps running inside TrackPopupMenu's modal
            // loop (issue 049): never switch tabs on a locked, wiped state.
            if *self.is_locked.borrow() {
                return;
            }

            if cmd.0 > 0 {
                let idx = (cmd.0 as usize) - 1;
                if idx != active_idx && idx < tabs.len() {
                    let content = self.get_content();
                    let mut tb = self.tab_bar.borrow_mut();
                    tb.set_tab_content(active_idx, &content);
                    tb.set_active(idx);
                    let new_content = tb.get_tab_content(idx);
                    drop(tb);
                    self.editor.set_text(&new_content);
                    *self.detected_encoding.borrow_mut() = "UTF-8".to_string();
                    self.update_title();
                    self.update_status_bar();
                    self.paint_tab_bar();
                    self.layout_controls();
                }
            }
        }
    }

    /// Drop of a dragged tab: reorder, keep the same tab active, mark the
    /// document modified (the tab order is saved in the payload).
    pub(super) fn on_tab_drop(&self, from: usize, to: usize) {
        // Keep the active tab's stored text current before the reorder.
        let active = self.tab_bar.borrow().active_index();
        let content = self.get_content();
        self.tab_bar.borrow_mut().set_tab_content(active, &content);
        let moved = self.tab_bar.borrow().move_tab(from, to);
        match moved {
            Ok(true) => {
                self.mark_modified();
                self.update_title();
                self.paint_tab_bar();
                self.layout_controls();
            }
            Ok(false) => {}
            Err(e) => eprintln!("tab_bar: drop refused err={:?}", e),
        }
    }

    pub(super) fn on_new_tab(&self) {
        let mut tb = self.tab_bar.borrow_mut();
        if tb.count() >= crate::ui::tab_bar::MAX_TABS {
            drop(tb);
            nwg::modal_info_message(
                &self.window,
                &t("tab.limit.title"),
                &t("tab.limit.msg"),
            );
            return;
        }
        // Generate name based on count
        let name = format!("Note {}", tb.count() + 1);
        // Save current tab content
        let active = tb.active_index();
        let content = self.get_content();
        tb.set_tab_content(active, &content);
        // Add new tab and switch to it
        let idx = tb.add_tab(&name);
        tb.set_active(idx);
        drop(tb);
        self.editor.set_text("");
        self.mark_modified();
        self.update_title();
        self.update_status_bar();
        self.paint_tab_bar();
        self.layout_controls();
    }

    pub(super) fn on_close_tab(&self) {
        let active = self.tab_bar.borrow().active_index();
        self.close_tab_at(active);
    }

    pub(super) fn close_tab_at(&self, index: usize) {
        let active = self.tab_bar.borrow().active_index();
        // Save current editor content to the active tab before closing
        let content = self.get_content();
        self.tab_bar.borrow_mut().set_tab_content(active, &content);

        let tb = self.tab_bar.borrow();
        if tb.count() <= 1 {
            return;
        }

        // Check if the tab has content — ask for confirmation
        let tab_content = tb.get_tab_content(index);
        let tab_name = tb.all_tabs().get(index).map(|t| t.name.clone()).unwrap_or_default();
        drop(tb);

        if !tab_content.trim().is_empty() {
            let msg = t("tab.close.msg")
                .replace("{name}", &tab_name);
            let confirmed = self.confirm(&nwg::MessageParams {
                title: &t("tab.close.title"),
                content: &msg,
                buttons: nwg::MessageButtons::YesNo,
                icons: nwg::MessageIcons::Warning,
            });
            if !confirmed {
                return;
            }
        }

        let mut tb = self.tab_bar.borrow_mut();
        tb.close_tab(index);
        let new_active = tb.active_index();
        let new_content = tb.get_tab_content(new_active);
        drop(tb);
        self.editor.set_text(&new_content);
        self.mark_modified();
        self.update_title();
        self.update_status_bar();
        self.paint_tab_bar();
        self.layout_controls();
    }

    pub(super) fn on_rename_tab(&self) {
        let tb = self.tab_bar.borrow();
        let current_name = tb.active_tab().name.clone();
        drop(tb);
        let epoch = self.lock_epoch();
        let renamed = {
            let _modal = self.modal();
            crate::ui::dialogs::rename_tab::RenameTabDialog::show(&current_name)
        };
        if let Some(new_name) = renamed {
            if !self.still_valid(epoch) {
                eprintln!("INFO tabs: rename dropped, the note locked meanwhile");
                return;
            }
            let mut tb = self.tab_bar.borrow_mut();
            let active = tb.active_index();
            tb.rename_tab(active, &new_name);
            drop(tb);
            self.mark_modified();
            self.update_title();
            self.paint_tab_bar();
        }
    }

    pub(super) fn on_next_tab(&self) {
        let mut tb = self.tab_bar.borrow_mut();
        let active = tb.active_index();
        let content = self.get_content();
        tb.set_tab_content(active, &content);
        tb.next_tab();
        let new_content = tb.get_tab_content(tb.active_index());
        drop(tb);
        self.editor.set_text(&new_content);
        *self.detected_encoding.borrow_mut() = "UTF-8".to_string();
        self.update_title();
        self.update_status_bar();
        self.paint_tab_bar();
        self.layout_controls();
    }

    pub(super) fn on_prev_tab(&self) {
        let mut tb = self.tab_bar.borrow_mut();
        let active = tb.active_index();
        let content = self.get_content();
        tb.set_tab_content(active, &content);
        tb.prev_tab();
        let new_content = tb.get_tab_content(tb.active_index());
        drop(tb);
        self.editor.set_text(&new_content);
        *self.detected_encoding.borrow_mut() = "UTF-8".to_string();
        self.update_title();
        self.update_status_bar();
        self.paint_tab_bar();
        self.layout_controls();
    }

    // =====================================================================
    // Tab context menu
    // =====================================================================

    /// Show a right-click context menu for the tab at the given index.
    pub(super) fn show_tab_context_menu(&self, tab_index: usize) {
        use windows::Win32::UI::WindowsAndMessaging::*;
        use windows::Win32::Foundation::*;
        use windows::core::*;

        unsafe {
            let hmenu = CreatePopupMenu().unwrap();

            // Menu IDs: 5000 = Rename, 5001 = Close, 5100..5107 = colors, 5199 = no color
            let rename_label: Vec<u16> = t("tab.ctx.rename").encode_utf16().chain(std::iter::once(0)).collect();
            let _ = AppendMenuW(hmenu, MENU_ITEM_FLAGS(0), 5000, PCWSTR(rename_label.as_ptr()));

            // Color submenu
            let hcolor = CreatePopupMenu().unwrap();
            let color_names = crate::theme::tab_color_names();
            let current_color = {
                let tb = self.tab_bar.borrow();
                let tabs = tb.all_tabs();
                tabs.get(tab_index).and_then(|t| t.color.clone())
            };

            // "No color" option
            let no_color_flags = if current_color.is_none() {
                MENU_ITEM_FLAGS(MF_CHECKED.0)
            } else {
                MENU_ITEM_FLAGS(0)
            };
            let no_color_label: Vec<u16> = t("tab.ctx.no_color").encode_utf16().chain(std::iter::once(0)).collect();
            let _ = AppendMenuW(hcolor, no_color_flags, 5199, PCWSTR(no_color_label.as_ptr()));
            let _ = AppendMenuW(hcolor, MF_SEPARATOR, 0, None);

            for (i, name) in color_names.iter().enumerate() {
                let flags = if current_color.as_deref() == Some(name) {
                    MENU_ITEM_FLAGS(MF_CHECKED.0)
                } else {
                    MENU_ITEM_FLAGS(0)
                };
                // The stored name stays "blue"...; the menu shows its translation.
                let label: Vec<u16> = tab_color_label(name).encode_utf16().chain(std::iter::once(0)).collect();
                let label_pcwstr = PCWSTR(label.as_ptr());
                let _ = AppendMenuW(hcolor, flags, 5100 + i, label_pcwstr);
            }

            let color_label: Vec<u16> = t("tab.ctx.color").encode_utf16().chain(std::iter::once(0)).collect();
            let _ = AppendMenuW(hmenu, MF_POPUP, hcolor.0 as usize, PCWSTR(color_label.as_ptr()));
            let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, None);
            let close_label: Vec<u16> = t("tab.ctx.close").encode_utf16().chain(std::iter::once(0)).collect();
            let _ = AppendMenuW(hmenu, MENU_ITEM_FLAGS(0), 5001, PCWSTR(close_label.as_ptr()));

            // Get screen position of cursor
            let mut pt = POINT::default();
            let _ = windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut pt);

            if let nwg::ControlHandle::Hwnd(win_hwnd) = self.window.handle {
                let result = TrackPopupMenu(
                    hmenu,
                    TPM_LEFTALIGN | TPM_TOPALIGN | TPM_RETURNCMD | TPM_RIGHTBUTTON,
                    pt.x,
                    pt.y,
                    None,
                    HWND(win_hwnd as *mut _),
                    None,
                );
                if result.as_bool() {
                    let cmd = result.0 as usize;
                    match cmd {
                        5000 => self.on_rename_tab_at(tab_index),
                        5001 => self.on_close_tab_at(tab_index),
                        5199 => self.on_set_tab_color(tab_index, None),
                        id if id >= 5100 && id < 5100 + color_names.len() => {
                            let color = color_names[id - 5100];
                            self.on_set_tab_color(tab_index, Some(color));
                        }
                        _ => {}
                    }
                }
            }

            let _ = DestroyMenu(hmenu);
        }
    }

    /// Set the color of a tab at the given index.
    pub(super) fn on_set_tab_color(&self, index: usize, color: Option<&str>) {
        let tb = self.tab_bar.borrow();
        tb.set_tab_color(index, color);
        drop(tb);
        self.mark_modified();
        self.paint_tab_bar();
    }

    /// Rename the tab at a specific index (for context menu).
    pub(super) fn on_rename_tab_at(&self, index: usize) {
        // Only the name: no copy of the tabs' text kept across the dialog.
        let current_name = {
            let tb = self.tab_bar.borrow();
            tb.all_tabs().get(index).map(|t| t.name.clone())
        };
        if let Some(current_name) = current_name {
            let epoch = self.lock_epoch();
            let renamed = {
                let _modal = self.modal();
                crate::ui::dialogs::rename_tab::RenameTabDialog::show(&current_name)
            };
            if let Some(new_name) = renamed {
                if !self.still_valid(epoch) {
                    eprintln!("INFO tabs: rename dropped, the note locked meanwhile");
                    return;
                }
                let tb = self.tab_bar.borrow();
                tb.rename_tab(index, &new_name);
                drop(tb);
                self.mark_modified();
                self.update_title();
                self.paint_tab_bar();
            }
        }
    }

    /// Close the tab at a specific index (for context menu).
    pub(super) fn on_close_tab_at(&self, index: usize) {
        self.close_tab_at(index);
    }
}

/// Measures tab names with the font the tab strip paints them in (Segoe UI,
/// bold: the active tab is bold and the layout must not jump on activation).
pub(super) struct TabTextMeasurer {
    hdc: windows::Win32::Graphics::Gdi::HDC,
    font: windows::Win32::Graphics::Gdi::HFONT,
    bitmap: windows::Win32::Graphics::Gdi::HBITMAP,
    old_font: windows::Win32::Graphics::Gdi::HGDIOBJ,
    old_bitmap: windows::Win32::Graphics::Gdi::HGDIOBJ,
    /// A GDI call failed: the widths are not trustworthy.
    failed: std::cell::Cell<bool>,
}

impl TabTextMeasurer {
    pub(super) fn new() -> Self {
        use windows::Win32::Graphics::Gdi::*;
        unsafe {
            // Colour DC like the painter's (a bare compatible DC holds a 1x1
            // monochrome bitmap, which can map the font differently).
            let screen = GetDC(None);
            let hdc = CreateCompatibleDC(Some(screen));
            let bitmap = CreateCompatibleBitmap(screen, 1, 1);
            ReleaseDC(None, screen);
            let family: Vec<u16> = "Segoe UI".encode_utf16().chain(std::iter::once(0)).collect();
            let font = CreateFontW(
                -15, 0, 0, 0, FW_BOLD.0 as i32, 0, 0, 0,
                DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY, 0,
                windows::core::PCWSTR(family.as_ptr()),
            );
            let old_bitmap = SelectObject(hdc, bitmap.into());
            let old_font = SelectObject(hdc, font.into());
            let failed = hdc.is_invalid() || bitmap.is_invalid() || font.is_invalid();
            if failed {
                eprintln!("ERROR tab_bar: measurer GDI setup failed");
            }
            Self { hdc, font, bitmap, old_font, old_bitmap, failed: std::cell::Cell::new(failed) }
        }
    }

    /// Pixel width of `text` on one line.
    pub(super) fn width(&self, text: &str) -> i32 {
        use windows::Win32::Foundation::SIZE;
        use windows::Win32::Graphics::Gdi::GetTextExtentPoint32W;
        let wide: Vec<u16> = text.encode_utf16().collect();
        let mut size = SIZE::default();
        let ok = unsafe { GetTextExtentPoint32W(self.hdc, &wide, &mut size) };
        if !ok.as_bool() {
            eprintln!("ERROR tab_bar: GetTextExtentPoint32W failed name_utf16_len={}", wide.len());
            self.failed.set(true);
            return 0;
        }
        size.cx
    }

    /// True when any setup or measurement failed since creation.
    pub(super) fn failed(&self) -> bool {
        self.failed.get()
    }
}

impl Drop for TabTextMeasurer {
    fn drop(&mut self) {
        use windows::Win32::Graphics::Gdi::*;
        unsafe {
            SelectObject(self.hdc, self.old_font);
            SelectObject(self.hdc, self.old_bitmap);
            let _ = DeleteObject(self.font.into());
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.hdc);
        }
    }
}

/// Text of a tab as a menu item: `&` would otherwise make the next letter a
/// mnemonic and vanish ("R&D" -> "RD").
pub(super) fn menu_label(name: &str) -> String {
    name.replace('&', "&&")
}
