// Prompts panel operations (issue 058).
// Part of the EditorForm implementation. State and pure helpers live in
// `ui::prompts_panel`, the edit dialog in `ui::dialogs::prompt_edit`.
//
// Logs: `prompts: add|edit|delete|copy|insert count=N`, never a prompt text,
// a clipboard text or a tab name.

#[allow(unused_imports)]
use super::*;

use crate::ui::prompts_panel as panel;

impl EditorForm {
    /// Toggle the prompts panel.
    pub(super) fn on_toggle_prompts(&self) {
        self.prompts_panel.toggle_visible();
        let visible = self.prompts_panel.is_visible();
        self.menu_tools_prompts.set_checked(visible);
        if visible {
            self.close_other_panels("prompts");
            self.refresh_prompts_list(None);
        }
        self.layout_controls();
        self.repaint_toolbar_buttons();
    }

    /// Apply the current theme to the prompts panel frame (bound once). The
    /// palette is read on every message, so a theme switch needs no
    /// re-binding: the repaint of every window (`theme_switch`) is enough.
    pub(super) fn theme_prompts_panel(&self) {
        let handler = nwg::bind_raw_event_handler(
            &self.prompts_frame.handle,
            0x40070,
            move |_hwnd, msg, wparam, _lparam| {
                let pal = crate::theme::current();
                let bg_colorref = pal.background.to_colorref();
                let text_colorref = pal.text_primary.to_colorref();
                let bg_brush = windows::Win32::Graphics::Gdi::HBRUSH(
                    crate::ui::lock_card::cached_brush(pal.background) as *mut _,
                );
                match msg {
                    // WM_CTLCOLOREDIT (search field) | WM_CTLCOLORLISTBOX |
                    // WM_CTLCOLORSTATIC (labels, read-only preview)
                    0x0133 | 0x0134 | 0x0138 => {
                        let hdc = windows::Win32::Graphics::Gdi::HDC(wparam as *mut _);
                        unsafe {
                            windows::Win32::Graphics::Gdi::SetTextColor(hdc, windows::Win32::Foundation::COLORREF(text_colorref));
                            windows::Win32::Graphics::Gdi::SetBkColor(hdc, windows::Win32::Foundation::COLORREF(bg_colorref));
                        }
                        Some(bg_brush.0 as isize)
                    }
                    0x0014 => {
                        // WM_ERASEBKGND
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

    /// Rebuild the list from the panel state and the search field: sorted by
    /// category then name, "Category · Name". `select` re-selects that
    /// prompt (storage index) when it is still listed. A failed comparison
    /// shows an explicit message, never a silently empty list.
    pub(super) fn refresh_prompts_list(&self, select: Option<usize>) {
        let prompts = self.prompts_panel.get_prompts();
        let query = self.prompts_search.text();
        match panel::visible_rows(&prompts, &query, &panel::contains_user_locale, &panel::compare_user_locale) {
            Ok(rows) => {
                let labels: Vec<String> = rows.iter().map(|&i| panel::list_label(&prompts[i])).collect();
                let status = if query.trim().is_empty() {
                    t("prompts.count").replace("{count}", &prompts.len().to_string())
                } else {
                    t("prompts.count.filtered")
                        .replace("{shown}", &rows.len().to_string())
                        .replace("{total}", &prompts.len().to_string())
                };
                self.prompts_list.set_collection(labels);
                self.prompts_panel.set_rows(rows);
                self.prompts_list.set_selection(select.and_then(|i| self.prompts_panel.row_of(i)));
                self.prompts_status.set_text(&status);
            }
            Err(e) => {
                eprintln!("WARN prompts: list not built count={} err={}", prompts.len(), e.0);
                self.prompts_list.set_collection(Vec::new());
                self.prompts_panel.set_rows(Vec::new());
                self.prompts_status.set_text(&t("prompts.list.error").replace("{msg}", &e.0));
            }
        }
        self.on_prompts_selection_changed();
    }

    /// Storage index of the prompt selected in the list.
    pub(super) fn selected_prompt(&self) -> Option<usize> {
        self.prompts_list.selection().and_then(|row| self.prompts_panel.index_at_row(row))
    }

    /// Preview and buttons follow the selection.
    pub(super) fn on_prompts_selection_changed(&self) {
        let selected = self.selected_prompt().and_then(|i| self.prompts_panel.get(i));
        self.prompts_preview.set_text(&selected.as_ref().map(|p| panel::to_crlf(&p.text)).unwrap_or_default());
        let has = selected.is_some();
        self.prompts_btn_edit.set_enabled(has);
        self.prompts_btn_delete.set_enabled(has);
        self.prompts_btn_copy.set_enabled(has);
        self.prompts_btn_insert.set_enabled(has);
    }

    /// Categories offered by the dialog: the translated default, then those
    /// in use. `None` (message shown) when they cannot be compared.
    fn prompt_category_choices(&self) -> Option<Vec<String>> {
        let default = t("prompts.category.default");
        match panel::category_choices(&self.prompts_panel.get_prompts(), &default, &panel::same_user_locale, &panel::compare_user_locale) {
            Ok(list) => Some(list),
            Err(e) => {
                eprintln!("WARN prompts: categories not listed err={}", e.0);
                self.prompts_status.set_text(&t("prompts.list.error").replace("{msg}", &e.0));
                None
            }
        }
    }

    pub(super) fn on_prompts_add(&self) {
        let Some(categories) = self.prompt_category_choices() else { return };
        let epoch = self.lock_epoch();
        let Some(entry) = crate::ui::dialogs::prompt_edit::PromptEditDialog::show(None, &categories) else { return };
        if !self.still_valid(epoch) {
            return; // locked or curtain while the dialog was open (issue 049, #30)
        }
        let index = self.prompts_panel.add(entry);
        eprintln!("INFO prompts: add count={}", self.prompts_panel.count());
        self.refresh_prompts_list(Some(index));
        self.mark_modified();
        self.repaint_toolbar_buttons();
    }

    pub(super) fn on_prompts_edit(&self) {
        let Some(index) = self.selected_prompt() else { return };
        let Some(current) = self.prompts_panel.get(index) else { return };
        let Some(categories) = self.prompt_category_choices() else { return };
        let epoch = self.lock_epoch();
        let Some(entry) = crate::ui::dialogs::prompt_edit::PromptEditDialog::show(Some(&current), &categories) else {
            return;
        };
        if !self.still_valid(epoch) {
            return; // locked or curtain while the dialog was open (issue 049, #30)
        }
        // Only the prompt the dialog was opened on: the list may have been
        // replaced while it was open (import).
        if !self.prompts_panel.update_if_unchanged(index, &current, entry) {
            self.report_prompt_gone("edit");
            return;
        }
        eprintln!("INFO prompts: edit count={}", self.prompts_panel.count());
        self.refresh_prompts_list(Some(index));
        self.mark_modified();
    }

    pub(super) fn on_prompts_delete(&self) {
        let Some(index) = self.selected_prompt() else { return };
        let Some(current) = self.prompts_panel.get(index) else { return };
        let title = t("prompts.delete.title");
        let content = t("prompts.delete.confirm").replace("{name}", &current.name);
        let params = nwg::MessageParams {
            title: &title,
            content: &content,
            buttons: nwg::MessageButtons::YesNo,
            icons: nwg::MessageIcons::Warning,
        };
        if !self.confirm(&params) {
            return;
        }
        if *self.is_locked.borrow() {
            return; // locked while the dialog was open (issue 049)
        }
        // The same prompt must still be there: the list may have been
        // replaced while the confirmation was open (import).
        if !self.prompts_panel.delete_if_unchanged(index, &current) {
            self.report_prompt_gone("delete");
            return;
        }
        eprintln!("INFO prompts: delete count={}", self.prompts_panel.count());
        self.refresh_prompts_list(None);
        self.mark_modified();
        self.repaint_toolbar_buttons();
    }

    /// A modal returned on a prompt that is no longer the one it was opened
    /// on: nothing is changed, and the user is told why.
    fn report_prompt_gone(&self, action: &str) {
        eprintln!("INFO prompts: {} dropped, prompt changed meanwhile count={}", action, self.prompts_panel.count());
        self.refresh_prompts_list(None);
        self.inform("LockNote",
            &t("prompts.gone"),
        );
    }

    /// Copy the selected prompt (button or click on its row), variables
    /// expanded. A prompt is not a secret: no clipboard guard, the copy stays
    /// until replaced.
    pub(super) fn on_prompts_copy(&self) {
        let Some(index) = self.selected_prompt() else { return };
        let Some(prompt) = self.prompts_panel.get(index) else { return };
        let text = panel::expanded_text(&FormPromptSteps { form: self }, &prompt.text);
        match clipboard_win::set_clipboard(clipboard_win::formats::Unicode, &text) {
            Ok(()) => {
                eprintln!("INFO prompts: copy count={}", self.prompts_panel.count());
                self.prompts_status.set_text(&panel::copied_message(&t("prompts.copied.named"), &prompt.name));
            }
            Err(e) => {
                eprintln!("WARN prompts: copy failed err={}", e);
                self.prompts_status.set_text(&t("prompts.copy.failed").replace("{err}", &e.to_string()));
            }
        }
    }

    /// "Insert into note" button: the selected prompt, variables expanded,
    /// replaces the editor selection.
    pub(super) fn on_prompts_insert(&self) {
        let Some(index) = self.selected_prompt() else { return };
        self.insert_prompt_at(index);
    }

    /// Insert the prompt at storage index `index` (button or palette). The
    /// status line of the panel says which prompt went in.
    pub(super) fn insert_prompt_at(&self, index: usize) {
        match panel::insert_prompt(&FormPromptSteps { form: self }, index) {
            panel::InsertOutcome::Inserted(name) => {
                eprintln!("INFO prompts: insert count={}", self.prompts_panel.count());
                self.mark_modified();
                self.prompts_status.set_text(&panel::copied_message(&t("prompts.inserted.named"), &name));
            }
            panel::InsertOutcome::Blocked => eprintln!("INFO prompts: insert dropped, the note is locked"),
            panel::InsertOutcome::Gone => {
                eprintln!("WARN prompts: insert dropped, no prompt at index={} count={}", index, self.prompts_panel.count());
            }
        }
    }

    /// Lock: drop the prompts (PromptEntry is ZeroizeOnDrop) and empty the
    /// list, the preview and the search. Unlock reloads them from disk.
    pub(super) fn clear_prompts_for_lock(&self) {
        self.prompts_panel.set_prompts(Vec::new());
        self.prompts_list.set_collection(Vec::new());
        self.prompts_preview.set_text("");
        self.prompts_search.set_text("");
        self.prompts_status.set_text("");
    }
}

/// `PromptSteps` on the real window.
struct FormPromptSteps<'a> {
    form: &'a EditorForm,
}

impl panel::PromptSteps for FormPromptSteps<'_> {
    fn blocked(&self) -> bool {
        self.form.input_blocked()
    }

    fn prompt(&self, index: usize) -> Option<crate::features::prompts::PromptEntry> {
        self.form.prompts_panel.get(index)
    }

    /// The clock and the format of Insert Timestamp (F5): UTC.
    fn now(&self) -> String {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        format_timestamp(secs)
    }

    fn active_tab_name(&self) -> String {
        self.form.tab_bar.borrow().active_tab().name.clone()
    }

    /// No text on the clipboard (empty, an image, held by another program)
    /// expands to nothing, as the variable is documented; the reason is
    /// logged, never the clipboard content.
    fn clipboard_text(&self) -> String {
        match clipboard_win::get_clipboard_string() {
            Ok(text) => text,
            Err(e) => {
                eprintln!("INFO prompts: no clipboard text for {{clipboard}} err={}", e);
                String::new()
            }
        }
    }

    /// One EM_REPLACESEL: one undo step. EM_STOPGROUPTYPING first, so the
    /// typing just before is not merged into it.
    fn insert(&self, text: &str) {
        self.form.editor.set_focus();
        self.form.send_editor_message(0x0458, 0, 0); // EM_STOPGROUPTYPING = WM_USER + 88
        self.form.replace_selection_utf16(text);
    }
}

#[cfg(test)]
mod tests {
    fn source(file: &str) -> String {
        crate::read_source(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("ui").join("editor").join(file))
    }

    fn body<'a>(src: &'a str, head: &str) -> &'a str {
        let start = src.find(head).unwrap_or_else(|| panic!("{head}"));
        let end = src[start..].find("
    }
").unwrap_or_else(|| panic!("end of {head}"));
        &src[start..start + end]
    }

    /// Copy and Insert give the same expanded text (B6).
    #[test]
    fn copy_and_insert_expand_the_variables() {
        let ops = source("prompts_ops.rs");
        assert!(body(&ops, "pub(super) fn on_prompts_copy(").contains("panel::expanded_text(&FormPromptSteps { form: self }, &prompt.text)"));
        assert!(body(&ops, "pub(super) fn insert_prompt_at(").contains("panel::insert_prompt(&FormPromptSteps { form: self }, index)"));
    }

    /// The insertion is one undoable EM_REPLACESEL, never a rebuild of the
    /// whole text (`set_text` loses the undo history).
    #[test]
    fn insert_is_one_undoable_replace_selection() {
        let ops = source("prompts_ops.rs");
        let insert = body(&ops, "    fn insert(&self, text: &str)");
        assert_eq!(insert.matches("replace_selection_utf16(").count(), 1);
        assert!(!insert.contains("set_text"));
        let ed = source("edit_ops.rs");
        assert!(body(&ed, "pub(super) fn replace_selection_utf16(").contains("self.send_editor_message(0x00C2, 1,"));
    }

    /// `{date}`/`{time}` follow Insert Timestamp: the same UTC formatter.
    #[test]
    fn the_clock_is_the_insert_timestamp_one() {
        let ops = source("prompts_ops.rs");
        assert!(body(&ops, "    fn now(&self)").contains("format_timestamp(secs)"));
        let ed = source("edit_ops.rs");
        assert!(body(&ed, "pub(super) fn on_insert_timestamp(").contains("format_timestamp(secs)"));
    }

    #[test]
    fn the_insert_button_is_wired() {
        let ev = source("events.rs");
        assert!(ev.contains("} else if handle == f_pr.prompts_btn_insert.handle {
                            f_pr.on_prompts_insert();"));
        let build = source("build.rs");
        assert!(build.contains(".text(&t(\"prompts.btn.insert\"))"));
    }
}
