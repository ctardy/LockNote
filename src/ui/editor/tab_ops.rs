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

            let epoch = self.lock_epoch();
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
            // loop (issue 049): never switch tabs on a locked, wiped state,
            // nor after a lock and an unlock (pass 4: lock epoch).
            if !self.still_valid(epoch) {
                return;
            }

            if cmd.0 > 0 {
                let idx = (cmd.0 as usize) - 1;
                if idx < tabs.len() {
                    self.switch_to_tab(idx);
                }
            }
        }
    }

    /// Make the tab at `index` active: the editor's text is kept in the
    /// tab it leaves, the editor shows the new one. Nothing happens for the
    /// active tab or an index past the last tab. Used by the "…" drop-down
    /// and the command palette.
    pub(super) fn switch_to_tab(&self, index: usize) {
        let (active, count) = {
            let tb = self.tab_bar.borrow();
            (tb.active_index(), tb.count())
        };
        if index == active || index >= count {
            return;
        }
        if let Err(e) = switch_editor_tab(&self.editor, &self.tab_bar, |tb| tb.set_active(index)) {
            return self.tell_unreadable("switch_tab", &e);
        }
        *self.detected_encoding.borrow_mut() = "UTF-8".to_string();
        self.update_title();
        self.update_status_bar();
        self.paint_tab_bar();
        self.layout_controls();
    }

    /// Drop of a dragged tab: reorder, keep the same tab active, mark the
    /// document modified (the tab order is saved in the payload).
    pub(super) fn on_tab_drop(&self, from: usize, to: usize) {
        // Keep the active tab's stored text current before the reorder; a
        // text that cannot be stored refuses the drop.
        let active = self.tab_bar.borrow().active_index();
        let content = match self.get_content() {
            Ok(text) => Zeroizing::new(text),
            Err(e) => return self.tell_unreadable("tab_drop", &e),
        };
        self.tab_bar.borrow_mut().set_tab_content(active, &content);
        drop(content);
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
        let tb = self.tab_bar.borrow();
        if tb.count() >= crate::ui::tab_bar::MAX_TABS {
            drop(tb);
            self.inform(&t("tab.limit.title"),
                &t("tab.limit.msg"),
            );
            return;
        }
        // Name based on the count, in the current language (issue #34):
        // stored with the tab, it never changes afterwards.
        let name = crate::features::tabs::default_name(crate::features::i18n::current_language(), tb.count() + 1);
        drop(tb);
        // Save current tab content, add the new tab and switch to it
        let switched = switch_editor_tab(&self.editor, &self.tab_bar, |tb| {
            let idx = tb.add_tab(&name);
            tb.set_active(idx);
        });
        if let Err(e) = switched {
            return self.tell_unreadable("new_tab", &e);
        }
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
        let (active, count) = {
            let tb = self.tab_bar.borrow();
            (tb.active_index(), tb.count())
        };
        // The last tab is never closed: nothing to read nor refuse.
        if count <= 1 {
            return;
        }
        // Save current editor content to the active tab before closing: the
        // editor is reloaded from the tabs afterwards, whichever tab closes,
        // so a text that cannot be stored refuses the close.
        let content = match self.get_content() {
            Ok(text) => Zeroizing::new(text),
            Err(e) => return self.tell_unreadable("close_tab", &e),
        };
        self.tab_bar.borrow_mut().set_tab_content(active, &content);
        drop(content);

        let tb = self.tab_bar.borrow();
        if tb.count() <= 1 {
            return;
        }

        // Check if the tab has content — ask for confirmation. Only whether
        // it is empty crosses the question, not a copy of the text (pass 5).
        let has_content = !Zeroizing::new(tb.get_tab_content(index)).trim().is_empty();
        let tab_name = tb.all_tabs().get(index).map(|t| t.name.clone()).unwrap_or_default();
        drop(tb);

        if has_content {
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
        self.editor.load_tab_text(&new_content);
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
        if let Err(e) = switch_editor_tab(&self.editor, &self.tab_bar, |tb| tb.next_tab()) {
            return self.tell_unreadable("next_tab", &e);
        }
        *self.detected_encoding.borrow_mut() = "UTF-8".to_string();
        self.update_title();
        self.update_status_bar();
        self.paint_tab_bar();
        self.layout_controls();
    }

    pub(super) fn on_prev_tab(&self) {
        if let Err(e) = switch_editor_tab(&self.editor, &self.tab_bar, |tb| tb.prev_tab()) {
            return self.tell_unreadable("prev_tab", &e);
        }
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
                // A popup menu runs its own loop (counted by modal_open,
                // closed by EndMenu at a lock): its command is dropped if a
                // lock came meanwhile (pass 4).
                let epoch = self.lock_epoch();
                let result = TrackPopupMenu(
                    hmenu,
                    TPM_LEFTALIGN | TPM_TOPALIGN | TPM_RETURNCMD | TPM_RIGHTBUTTON,
                    pt.x,
                    pt.y,
                    None,
                    HWND(win_hwnd as *mut _),
                    None,
                );
                if result.as_bool() && !self.still_valid(epoch) {
                    eprintln!("INFO tabs: menu command dropped, the note locked meanwhile");
                } else if result.as_bool() {
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

    /// Tab > Compare with Last Save: the sequence of compare_flow.
    pub(super) fn on_compare_with_last_save(&self) {
        use super::compare_flow::{compare_with_last_save, CompareOutcome};
        let tab = self.tab_bar.borrow().active_index();
        match compare_with_last_save(&mut FormCompareSteps { form: self }) {
            CompareOutcome::NoActiveTab => {
                eprintln!("ERROR compare: no saved text for active tab={}", tab);
            }
            CompareOutcome::Failed(e) => {
                eprintln!("ERROR compare: diff failed tab={} err={:?}", tab, e);
            }
            outcome => eprintln!("INFO compare: tab={} {:?}", tab, outcome),
        }
    }
}

/// The steps of `compare_flow` on the window.
struct FormCompareSteps<'a> {
    form: &'a EditorForm,
}

impl super::compare_flow::CompareSteps for FormCompareSteps<'_> {
    fn texts(&self) -> Option<(Zeroizing<String>, Zeroizing<String>)> {
        // Copies: no tab bar borrow lives across the dialog.
        let tb = self.form.tab_bar.borrow();
        let saved = Zeroizing::new(tb.get_saved_content(tb.active_index())?);
        drop(tb);
        // A diff to read: the display text (a lone surrogate, which no save
        // can have written, shows as U+FFFD on its line).
        match self.form.display_content() {
            Ok(current) => Some((saved, Zeroizing::new(current))),
            Err(e) => {
                eprintln!("ERROR compare: editor text unreadable err={:?}", e);
                None
            }
        }
    }
    fn inform(&mut self, message: &str) {
        self.form.inform(&t("compare.title"), message);
    }
    fn show_diff(&mut self, lines: &[String]) {
        let text = Zeroizing::new(lines.join("\r\n"));
        let (family, size) = {
            let s = self.form.settings.borrow();
            (s.font_family.clone(), font_size_to_nwg(s.font_size))
        };
        let _modal = self.form.modal();
        crate::ui::dialogs::compare_dialog::CompareDialog::show(&text, &family, size, Some(self.form.window.handle));
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

// =========================================================================
// Tab switch (A7)
// =========================================================================

/// What a tab switch needs from the editor (a fake in the sequence tests).
pub(super) trait TabEditor {
    /// The text shown now: the active tab's, as edited. Read strictly: it is
    /// stored in the tab.
    fn current_text(&self) -> Result<String, EditorTextError>;
    /// Show a tab's text in place of the current one, with an empty undo
    /// history.
    fn load_tab_text(&self, text: &str);
}

/// EM_EMPTYUNDOBUFFER.
const EM_EMPTYUNDOBUFFER: u32 = 0x00CD;

impl TabEditor for nwg::RichTextBox {
    fn current_text(&self) -> Result<String, EditorTextError> {
        crate::ui::editor_text::read_strict(crate::ui::editor_text::hwnd_of(self.handle))
    }

    /// All tabs share the one RichEdit, hence one undo history: an entry
    /// left from another tab would bring that tab's text into this one
    /// (Ctrl+Z). WM_SETTEXT drops the history today (measured, `live_*`
    /// tests); emptying it here keeps that true whatever way the text gets
    /// in. The redo history goes with it.
    fn load_tab_text(&self, text: &str) {
        self.set_text(text);
        let nwg::ControlHandle::Hwnd(h) = self.handle else {
            eprintln!("ERROR tabs: the editor has no window handle; its undo history was not emptied");
            return;
        };
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                windows::Win32::Foundation::HWND(h as *mut _),
                EM_EMPTYUNDOBUFFER,
                None,
                None,
            );
        }
    }
}

/// Put the editor on another tab: the editor text goes to the active tab,
/// `select` changes the active tab (`set_active`, `next_tab`, `add_tab`...),
/// then that tab's text is loaded with an empty undo history. The tab bar
/// is not borrowed while the editor takes the text: WM_SETTEXT notifies the
/// window, whose handlers read the tabs.
///
/// A text that cannot be read strictly (a lone surrogate) stops the switch
/// before anything changes: the tab bar, the active tab and the editor (its
/// text and its undo history) are left as they were, and the caller tells
/// the user. Storing a substitute would destroy the note at the next load.
pub(super) fn switch_editor_tab(
    editor: &impl TabEditor,
    tabs: &RefCell<crate::ui::tab_bar::TabBar>,
    select: impl FnOnce(&crate::ui::tab_bar::TabBar),
) -> Result<(), EditorTextError> {
    let content = Zeroizing::new(editor.current_text()?);
    let new_content = {
        let tb = tabs.borrow();
        tb.set_tab_content(tb.active_index(), &content);
        select(&tb);
        tb.get_tab_content(tb.active_index())
    };
    drop(content);
    editor.load_tab_text(&new_content);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::tab_bar::TabBar;
    use std::cell::RefCell;

    /// Records what the switch does to the editor, in order.
    struct FakeEditor {
        text: RefCell<String>,
        log: RefCell<Vec<String>>,
        /// When set, reading the text fails with it (a lone surrogate).
        unreadable: Option<EditorTextError>,
    }

    impl FakeEditor {
        fn showing(text: &str) -> Self {
            FakeEditor { text: RefCell::new(text.to_string()), log: RefCell::new(vec![]), unreadable: None }
        }
        fn log(&self) -> Vec<String> {
            self.log.borrow().clone()
        }
    }

    impl TabEditor for FakeEditor {
        fn current_text(&self) -> Result<String, EditorTextError> {
            self.log.borrow_mut().push("read".to_string());
            match &self.unreadable {
                Some(e) => Err(e.clone()),
                None => Ok(self.text.borrow().clone()),
            }
        }
        fn load_tab_text(&self, text: &str) {
            self.log.borrow_mut().push(format!("load {text}"));
            *self.text.borrow_mut() = text.to_string();
        }
    }

    fn two_tabs() -> TabBar {
        let tabs = TabBar::new();
        tabs.set_tab_content(0, "stale A");
        tabs.add_tab("Note 2");
        tabs.set_tab_content(1, "bbb");
        tabs.set_active(0);
        tabs
    }

    #[test]
    fn switch_stores_the_left_tab_then_loads_the_new_active_one() {
        let tabs = RefCell::new(two_tabs());
        let ed = FakeEditor::showing("aaa typed");
        switch_editor_tab(&ed, &tabs, |tb| tb.set_active(1)).expect("readable");
        assert_eq!(ed.log(), ["read", "load bbb"], "read before the change, one load after");
        assert_eq!(tabs.borrow().get_tab_content(0), "aaa typed", "the left tab keeps what was typed");
        assert_eq!(tabs.borrow().active_index(), 1);
    }

    #[test]
    fn switch_with_an_unreadable_text_changes_nothing() {
        // A lone surrogate in the editor: nothing is stored in the tab left,
        // the active tab does not change, the editor is not reloaded (its
        // text and undo history stay), and the error goes to the caller.
        let err = EditorTextError::InvalidUtf16 { unit: 3, line: 1, count: 1 };
        for select in [
            (|tb: &TabBar| tb.set_active(1)) as fn(&TabBar),
            |tb: &TabBar| tb.next_tab(),
            |tb: &TabBar| tb.prev_tab(),
            |tb: &TabBar| {
                let i = tb.add_tab("Note 3");
                tb.set_active(i);
            },
        ] {
            let tabs = RefCell::new(two_tabs());
            let mut ed = FakeEditor::showing("aaa typed");
            ed.unreadable = Some(err.clone());
            assert_eq!(switch_editor_tab(&ed, &tabs, select), Err(err.clone()));
            assert_eq!(ed.log(), ["read"], "no load after a failed read");
            let tb = tabs.borrow();
            assert_eq!((tb.active_index(), tb.count()), (0, 2), "the tab bar changed");
            assert_eq!(tb.get_tab_content(0), "stale A", "something was stored in the tab left");
            assert_eq!(tb.get_tab_content(1), "bbb");
        }
    }

    #[test]
    fn switch_selection_runs_after_the_text_is_stored() {
        // next_tab / prev_tab / add_tab move the active index: the text
        // must go to the tab it was typed in, not to the next one.
        let tabs = RefCell::new(two_tabs());
        let ed = FakeEditor::showing("typed in A");
        switch_editor_tab(&ed, &tabs, |tb| tb.next_tab()).expect("readable");
        assert_eq!(tabs.borrow().get_tab_content(0), "typed in A");
        assert_eq!(tabs.borrow().get_tab_content(1), "bbb");
        switch_editor_tab(&ed, &tabs, |tb| tb.prev_tab()).expect("readable");
        assert_eq!(ed.log(), ["read", "load bbb", "read", "load typed in A"]);
    }

    #[test]
    fn switch_to_a_new_tab_loads_an_empty_editor() {
        let tabs = RefCell::new(two_tabs());
        let ed = FakeEditor::showing("aaa");
        switch_editor_tab(&ed, &tabs, |tb| {
            tb.add_tab("Note 3");
        }).expect("readable");
        assert_eq!(ed.log(), ["read", "load "]);
        assert_eq!(tabs.borrow().active_index(), 2);
        assert_eq!(tabs.borrow().get_tab_content(0), "aaa");
    }

    #[test]
    fn switch_without_a_change_reloads_the_same_tab() {
        // Ctrl+Tab with a single tab: as before, the text goes through the
        // tab and comes back unchanged.
        let tabs = RefCell::new(TabBar::new());
        let ed = FakeEditor::showing("only");
        switch_editor_tab(&ed, &tabs, |tb| tb.next_tab()).expect("readable");
        assert_eq!(ed.log(), ["read", "load only"]);
        assert_eq!(tabs.borrow().get_tab_content(0), "only");
    }

    #[test]
    fn every_tab_switch_path_goes_through_the_shared_helpers() {
        // A path that set the editor text itself would bypass the undo
        // reset (A7). Read from the code.
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ui/editor");
        let tab_ops = crate::read_source(dir.join("tab_ops.rs"));
        let events = crate::read_source(dir.join("events.rs"));
        let ops = tab_ops.split("#[cfg(test)]").next().expect("code part");
        for name in ["switch_to_tab", "on_new_tab", "on_next_tab", "on_prev_tab", "close_tab_at"] {
            let start = ops.find(&format!("fn {name}(")).unwrap_or_else(|| panic!("{name} not found"));
            let body = &ops[start..];
            let body = &body[..body[1..].find("    pub(super) fn ").map_or(body.len(), |i| i + 1)];
            assert!(!body.contains("editor.set_text("), "{name} sets the editor text directly");
            assert!(body.contains("switch_editor_tab(") || body.contains("load_tab_text("), "{name} bypasses the helpers");
        }
        // The "…" drop-down and the command palette switch through switch_to_tab.
        let menu = ops.find("fn show_tab_overflow_menu(").expect("show_tab_overflow_menu not found");
        let menu = &ops[menu..menu + ops[menu..].find("fn switch_to_tab(").expect("end of the overflow menu")];
        assert!(!menu.contains("editor.set_text("), "the overflow menu sets the editor text directly");
        assert!(menu.contains("switch_to_tab("), "the overflow menu bypasses switch_to_tab");
        let click = events.find("msg == 0x8001").expect("tab click handler");
        let click = &events[click..click + events[click..].find("TabHit::Overflow").expect("end of the tab arm")];
        assert!(!click.contains("editor.set_text("), "the tab click sets the editor text directly");
        assert!(click.contains("switch_editor_tab("), "the tab click bypasses the helper");
        // The steganography extraction opens its text in a new tab.
        let stego = crate::read_source(dir.join("stego_ops.rs"));
        let opened = stego.find("Open extracted notes in a new tab").expect("stego new tab");
        let opened = &stego[opened..opened + stego[opened..].find("} else {").expect("end of the new tab branch")];
        assert!(!opened.contains("editor.set_text("), "the stego tab sets the editor text directly");
        assert!(opened.contains("switch_editor_tab("), "the stego tab bypasses the helper");
    }

    // -----------------------------------------------------------------
    // Live RichEdit tests: an nwg RichTextBox with the editor surface, as
    // the application builds it, in the test process (never the app).
    // -----------------------------------------------------------------

    mod live {
        use super::*;

        const EM_SETSEL: u32 = 0x00B1;
        const EM_REPLACESEL: u32 = 0x00C2;
        const EM_CANUNDO: u32 = 0x00C6;
        const EM_UNDO: u32 = 0x00C7;
        const EM_REDO: u32 = 0x0454;
        const EM_CANREDO: u32 = 0x0455;

        struct Rig {
            _window: nwg::Window,
            editor: nwg::RichTextBox,
            // Last: released once the window is destroyed (see live_rich_edit.rs).
            _live: crate::ui::editor::live_rich_edit::LiveRichEdit,
        }

        /// The editor showing tab A ("aaa") of two tabs (B holds "bbb").
        fn rig() -> (Rig, RefCell<TabBar>) {
            let live = crate::ui::editor::live_rich_edit::guard();
            let _ = nwg::init();
            let mut window = nwg::Window::default();
            nwg::Window::builder().size((400, 300)).flags(nwg::WindowFlags::WINDOW).build(&mut window).expect("window");
            let mut editor = nwg::RichTextBox::default();
            nwg::RichTextBox::builder().parent(&window).size((300, 200)).build(&mut editor).expect("editor");
            crate::ui::editor::surface::install(&editor).expect("undo resumed");
            let tabs = TabBar::new();
            tabs.set_tab_content(0, "aaa");
            tabs.add_tab("Note 2");
            tabs.set_tab_content(1, "bbb");
            tabs.set_active(0);
            editor.load_tab_text("aaa");
            (Rig { _window: window, editor, _live: live }, RefCell::new(tabs))
        }

        fn send(ed: &nwg::RichTextBox, msg: u32, w: usize, l: isize) -> isize {
            let nwg::ControlHandle::Hwnd(h) = ed.handle else { panic!("the editor has no window handle") };
            unsafe {
                windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                    windows::Win32::Foundation::HWND(h as *mut _),
                    msg,
                    Some(windows::Win32::Foundation::WPARAM(w)),
                    Some(windows::Win32::Foundation::LPARAM(l)),
                )
                .0
            }
        }

        /// Type `s` at the end, as an undoable edit (like the keyboard).
        fn type_at_end(ed: &nwg::RichTextBox, s: &str) {
            send(ed, EM_SETSEL, usize::MAX >> 1, -1);
            let wide: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
            send(ed, EM_REPLACESEL, 1, wide.as_ptr() as isize);
        }

        /// Ctrl+Z / Ctrl+Y as many times as the user likes right after a
        /// tab switch: the text stays `expected`, never `foreign`.
        fn undo_redo_keep(ed: &nwg::RichTextBox, expected: &str, foreign: &str) {
            assert_eq!(send(ed, EM_CANUNDO, 0, 0), 0, "nothing to undo right after a tab switch");
            assert_eq!(send(ed, EM_CANREDO, 0, 0), 0, "nothing to redo right after a tab switch");
            for _ in 0..3 {
                send(ed, EM_UNDO, 0, 0);
                assert!(!ed.text().contains(foreign), "undo showed the other tab: {:?}", ed.text());
                assert_eq!(ed.text(), expected);
                send(ed, EM_REDO, 0, 0);
                assert_eq!(ed.text(), expected);
            }
        }

        #[test]
        fn live_undo_after_a_tab_switch_never_shows_the_tab_left() {
            let (r, tabs) = rig();
            type_at_end(&r.editor, "XYZ");
            assert_ne!(send(&r.editor, EM_CANUNDO, 0, 0), 0, "the typing is undoable in its tab");

            switch_editor_tab(&r.editor, &tabs, |tb| tb.set_active(1)).expect("readable");
            assert_eq!(r.editor.text(), "bbb");
            undo_redo_keep(&r.editor, "bbb", "XYZ");

            // Edits in B undo within B only, back to the text B was shown with.
            type_at_end(&r.editor, "123");
            send(&r.editor, EM_UNDO, 0, 0);
            send(&r.editor, EM_UNDO, 0, 0);
            assert_eq!(r.editor.text(), "bbb");
            type_at_end(&r.editor, "123");

            // Back to A: both tabs intact, A's history is not restored.
            switch_editor_tab(&r.editor, &tabs, |tb| tb.set_active(0)).expect("readable");
            assert_eq!(r.editor.text(), "aaaXYZ");
            assert_eq!(tabs.borrow().get_tab_content(1), "bbb123");
            undo_redo_keep(&r.editor, "aaaXYZ", "123");
            switch_editor_tab(&r.editor, &tabs, |tb| tb.next_tab()).expect("readable");
            assert_eq!(r.editor.text(), "bbb123");
            assert_eq!(tabs.borrow().get_tab_content(0), "aaaXYZ");
        }

        #[test]
        fn live_undo_after_new_next_and_prev_tab_stays_in_the_tab() {
            let (r, tabs) = rig();
            type_at_end(&r.editor, "XYZ");
            switch_editor_tab(&r.editor, &tabs, |tb| {
                tb.add_tab("Note 3");
            }).expect("readable");
            assert_eq!(r.editor.text(), "");
            undo_redo_keep(&r.editor, "", "aaa");

            type_at_end(&r.editor, "ccc");
            switch_editor_tab(&r.editor, &tabs, |tb| tb.prev_tab()).expect("readable");
            assert_eq!(r.editor.text(), "bbb");
            undo_redo_keep(&r.editor, "bbb", "ccc");

            switch_editor_tab(&r.editor, &tabs, |tb| tb.next_tab()).expect("readable");
            assert_eq!(r.editor.text(), "ccc");
            undo_redo_keep(&r.editor, "ccc", "bbb");
            assert_eq!(tabs.borrow().get_tab_content(0), "aaaXYZ");
        }

        #[test]
        fn live_undo_after_closing_a_tab_never_shows_the_closed_tab() {
            // close_tab_at's sequence: the closed tab's text is gone, the
            // next active tab is loaded with load_tab_text.
            let (r, tabs) = rig();
            type_at_end(&r.editor, "secret");
            tabs.borrow().set_tab_content(0, &r.editor.text());
            assert!(tabs.borrow().close_tab(0));
            let next = tabs.borrow().get_tab_content(tabs.borrow().active_index());
            r.editor.load_tab_text(&next);
            assert_eq!(r.editor.text(), "bbb");
            undo_redo_keep(&r.editor, "bbb", "secret");
        }

        const WM_CHAR: u32 = 0x0102;

        /// The editor's text as the UTF-16 units RichEdit holds, read with
        /// GetWindowTextW (no decoding: a lone surrogate stays visible).
        fn raw_units(ed: &nwg::RichTextBox) -> Vec<u16> {
            let nwg::ControlHandle::Hwnd(h) = ed.handle else { panic!("the editor has no window handle") };
            let hwnd = windows::Win32::Foundation::HWND(h as *mut _);
            unsafe {
                let len = windows::Win32::UI::WindowsAndMessaging::GetWindowTextLengthW(hwnd) as usize;
                let mut buf = vec![0u16; len + 1];
                let n = windows::Win32::UI::WindowsAndMessaging::GetWindowTextW(hwnd, &mut buf) as usize;
                buf.truncate(n);
                buf
            }
        }

        #[test]
        fn live_tab_switch_keeps_a_note_holding_a_lone_surrogate() {
            // A lone high surrogate typed at the end (WM_CHAR, as half of an
            // emoji sent by SendInput / an IME): RichEdit keeps it
            // (measured). The switch must never put a substitute text in
            // the tab left, nor in the editor when coming back.
            let (r, tabs) = rig();
            send(&r.editor, EM_SETSEL, usize::MAX >> 1, -1);
            send(&r.editor, WM_CHAR, 0xD83D, 0);
            let before = raw_units(&r.editor);
            assert_eq!(before, [0x61, 0x61, 0x61, 0xD83D], "RichEdit holds the lone surrogate");

            let refused = switch_editor_tab(&r.editor, &tabs, |tb| tb.set_active(1));
            assert_eq!(refused, Err(EditorTextError::InvalidUtf16 { unit: 3, line: 1, count: 1 }));
            let stored = tabs.borrow().get_tab_content(0);
            assert!(stored == "aaa", "the tab left lost its text: {:?}", stored);
            assert_eq!(tabs.borrow().active_index(), 0, "the active tab changed");
            assert_eq!(raw_units(&r.editor), before, "the editor was reloaded");
            assert_ne!(send(&r.editor, EM_CANUNDO, 0, 0), 0, "the undo history was emptied");

            // The second half arrives (the emoji typed as two WM_CHAR): the
            // same switch now goes through with the exact pair.
            send(&r.editor, WM_CHAR, 0xDE00, 0);
            switch_editor_tab(&r.editor, &tabs, |tb| tb.set_active(1)).expect("a whole emoji is valid");
            assert_eq!(tabs.borrow().get_tab_content(0), "aaa\u{1F600}");
            assert_eq!(raw_units(&r.editor), "bbb".encode_utf16().collect::<Vec<_>>());
        }

        #[test]
        fn live_readers_refuse_half_an_emoji_then_read_the_whole_one() {
            // An emoji typed as two WM_CHAR passes through a lone high
            // surrogate: the strict reader refuses it, the display reader
            // keeps every position, and both read the pair once complete.
            let (r, _tabs) = rig();
            let hwnd = crate::ui::editor_text::hwnd_of(r.editor.handle);
            r.editor.load_tab_text("l1\r\nl2");
            send(&r.editor, EM_SETSEL, usize::MAX >> 1, -1);
            send(&r.editor, WM_CHAR, 0xD83D, 0);
            assert_eq!(
                crate::ui::editor_text::read_strict(hwnd),
                Err(EditorTextError::InvalidUtf16 { unit: 6, line: 2, count: 1 })
            );
            let shown = crate::ui::editor_text::read_display(hwnd).expect("display");
            assert_eq!(shown, "l1\r\nl2\u{FFFD}");
            assert_eq!(shown.encode_utf16().count(), raw_units(&r.editor).len());
            send(&r.editor, WM_CHAR, 0xDE00, 0);
            assert_eq!(crate::ui::editor_text::read_strict(hwnd), Ok("l1\r\nl2\u{1F600}".to_string()));
            // nwg's reader, which no code may use any more, said this.
            send(&r.editor, WM_CHAR, 0xD83D, 0);
            assert_eq!(r.editor.text(), "Decoding error", "nwg 1.0.13 behaviour this fix works around");
        }

        #[test]
        fn live_load_resets_the_undo_history_even_for_the_same_text() {
            let (r, _tabs) = rig();
            type_at_end(&r.editor, "XYZ");
            r.editor.load_tab_text("aaaXYZ");
            undo_redo_keep(&r.editor, "aaaXYZ", "bbb");
        }
    }
}
