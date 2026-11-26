// View, edit, search/replace and go-to-line operations.
// Part of the EditorForm implementation, split out of the former editor.rs.

#[allow(unused_imports)]
use super::*;

impl EditorForm {
    // =====================================================================
    // View operations
    // =====================================================================

    /// Toggle always-on-top.
    pub(super) fn on_toggle_always_on_top(&self) {
        let mut on_top = self.always_on_top.borrow_mut();
        *on_top = !*on_top;
        self.menu_view_always_on_top.set_checked(*on_top);
        unsafe {
            let raw_hwnd = self.window.handle.hwnd().unwrap();
            let hwnd = windows::Win32::Foundation::HWND(raw_hwnd as *mut _);
            let insert_after = if *on_top {
                windows::Win32::UI::WindowsAndMessaging::HWND_TOPMOST
            } else {
                windows::Win32::UI::WindowsAndMessaging::HWND_NOTOPMOST
            };
            let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowPos(
                hwnd,
                Some(insert_after),
                0, 0, 0, 0,
                windows::Win32::UI::WindowsAndMessaging::SWP_NOMOVE
                    | windows::Win32::UI::WindowsAndMessaging::SWP_NOSIZE,
            );
        }
    }

    /// Toggle word wrap.
    pub(super) fn on_toggle_word_wrap(&self) {
        let mut settings = self.settings.borrow_mut();
        settings.word_wrap = !settings.word_wrap;
        self.menu_view_word_wrap.set_checked(settings.word_wrap);
        self.apply_word_wrap(settings.word_wrap);
    }

    /// Show or hide the line-number gutter (issue #26). Saved with the note
    /// like word wrap: by the next save, the final save at close included.
    pub(super) fn on_toggle_line_numbers(&self) {
        let show = {
            let mut settings = self.settings.borrow_mut();
            settings.show_line_numbers = !settings.show_line_numbers;
            settings.show_line_numbers
        };
        eprintln!("INFO view: show_line_numbers={}", show);
        self.menu_view_line_numbers.set_checked(show);
        // The editor takes the gutter's width, or gives it back.
        self.layout_controls();
    }

    /// Put the View check marks and the word wrap back in line with the
    /// settings, after they were replaced (unlock, backup import); the
    /// gutter follows at the next `layout_controls`.
    pub(super) fn sync_view_menu(&self) {
        let (wrap, numbers) = view_menu_checks(&self.settings.borrow());
        self.menu_view_word_wrap.set_checked(wrap);
        self.menu_view_line_numbers.set_checked(numbers);
        self.apply_word_wrap(wrap);
    }

    /// Apply word wrap setting to the editor control.
    pub(super) fn apply_word_wrap(&self, wrap: bool) {
        // EM_SETTARGETDEVICE: wParam=0 (HDC), lParam=0 wraps to window, lParam=1 disables wrap
        let lparam = if wrap { 0 } else { 1 };
        self.send_editor_message(0x0448, 0, lparam); // EM_SETTARGETDEVICE
    }

    // =====================================================================
    // Edit operations
    // =====================================================================

    pub(super) fn on_cut(&self) {
        // WM_CUT = 0x0300
        self.send_editor_message(0x0300, 0, 0);
    }

    pub(super) fn on_copy(&self) {
        // WM_COPY = 0x0301
        self.send_editor_message(0x0301, 0, 0);
    }

    pub(super) fn on_paste(&self) {
        // WM_PASTE = 0x0302
        self.send_editor_message(0x0302, 0, 0);
    }

    /// Paste as plain text (strip RTF formatting).
    pub(super) fn on_paste_plain(&self) {
        // Read plain text from clipboard and insert via EM_REPLACESEL
        if let Ok(text) = clipboard_win::get_clipboard_string() {
            let sel = self.editor.selection();
            self.editor.set_selection(sel.start..sel.end);
            // EM_REPLACESEL = 0x00C2, wParam=1 (can undo)
            let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
            self.send_editor_message(0x00C2, 1, wide.as_ptr() as isize);
        }
    }

    /// After a double-click in the editor, widen the native word-select to
    /// the whole whitespace-delimited word (see `expand_word_selection`).
    /// Nothing is copied: copying stays an explicit user action (Ctrl+C).
    pub(super) fn on_dblclick_expand_selection(&self) {
        let sel = self.editor.selection();
        let text = self.get_content();
        match dblclick_target(&text, sel.start, sel.end) {
            Some((start, end)) => {
                eprintln!("DEBUG dblclick: native={}..{} widened={}..{}", sel.start, sel.end, start, end);
                self.editor.set_selection(start..end);
            }
            None => {
                eprintln!("DEBUG dblclick: native={}..{} kept", sel.start, sel.end);
            }
        }
    }

    /// Keyboard-shortcut dispatch. Called from both the main-window and the
    /// editor-control subclass handlers so shortcuts work regardless of which
    /// control has focus. Returns true if the shortcut was handled and the
    /// WM_KEYDOWN should be consumed.
    pub(super) fn handle_shortcut(&self, vk: u32) -> bool {
        // While locked, no shortcut may act on the (wiped) editor state:
        // Ctrl+S would re-encrypt empty tabs, Ctrl+W/T/Tab would mutate
        // tabs, Ctrl+B would show the vault.
        // The curtain counts as locked (pass 6).
        if self.input_blocked() {
            return false;
        }

        self.autolock.borrow_mut().reset();

        let ctrl = unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(0x11) } < 0;
        let shift = unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(0x10) } < 0;
        let alt = unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(0x12) } < 0;

        // Dynamic save shortcut (configurable modifier+key)
        {
            let s = self.settings.borrow();
            let sm = s.save_modifiers;
            let sk = s.save_key;
            drop(s);
            let want_ctrl = sm & 0x02 != 0;
            let want_shift = sm & 0x04 != 0;
            let want_alt = sm & 0x01 != 0;
            if ctrl == want_ctrl && shift == want_shift && alt == want_alt && vk == sk {
                let _ = self.on_save();
                return true;
            }
        }

        if ctrl && !shift {
            match vk {
                0x51 => { if self.on_close() { nwg::stop_thread_dispatch(); } return true; } // Ctrl+Q
                0x46 => { self.on_show_find(); return true; }   // Ctrl+F
                0x48 => { self.on_show_replace(); return true; }// Ctrl+H
                0x47 => { self.on_goto_line(); return true; }   // Ctrl+G
                0x44 => { self.on_duplicate_line(); return true; } // Ctrl+D
                0xBE => { self.on_toggle_bullet(); return true; } // Ctrl+.
                0x0D => { self.on_toggle_checkbox(); return true; } // Ctrl+Enter
                0x41 => { self.on_select_all(); return true; }  // Ctrl+A
                0x5A => { self.on_undo(); return true; }        // Ctrl+Z
                0x59 => { self.on_redo(); return true; }        // Ctrl+Y
                0x50 => { self.on_print(); return true; }       // Ctrl+P
                0x42 => { self.on_toggle_vault(); return true; }
                0x4A => { self.on_toggle_attachments(); return true; } // Ctrl+J
                0x4C => { self.on_manual_lock(); return true; } // Ctrl+L
                0x54 => { self.on_new_tab(); return true; }     // Ctrl+T
                0x57 => { self.on_close_tab(); return true; }   // Ctrl+W
                0x09 => { self.on_next_tab(); return true; }    // Ctrl+Tab
                _ => {}
            }
        } else if ctrl && shift {
            match vk {
                0x56 => { self.on_paste_plain(); return true; } // Ctrl+Shift+V
                0x46 => { self.on_search_all_tabs(); return true; } // Ctrl+Shift+F
                0x4B => { self.on_delete_line(); return true; } // Ctrl+Shift+K
                0x4F => { self.on_goto_heading(); return true; } // Ctrl+Shift+O
                0x09 => { self.on_prev_tab(); return true; }    // Ctrl+Shift+Tab
                0x50 => { self.on_command_palette(); return true; } // Ctrl+Shift+P
                _ => {}
            }
        } else if !ctrl && !shift {
            // Alt+key arrives as WM_SYSKEYDOWN (events.rs), only for these keys.
            if alt {
                match alt_key_action(vk) {
                    Some(AltKey::MoveLinesUp) => { self.on_move_lines(true); return true; }
                    Some(AltKey::MoveLinesDown) => { self.on_move_lines(false); return true; }
                    None => {}
                }
            }
            match plain_key_action(vk) {
                Some(PlainKey::InsertTimestamp) => { self.on_insert_timestamp(); return true; }
                Some(PlainKey::Escape) => { self.on_escape(); return true; }
                Some(PlainKey::RenameTab) => { self.on_rename_tab(); return true; }
                None => {}
            }
        }
        false
    }

    /// Send a Windows message to the editor's HWND.
    pub(super) fn send_editor_message(&self, msg: u32, wparam: usize, lparam: isize) {
        use nwg::ControlHandle;
        if let ControlHandle::Hwnd(hwnd) = self.editor.handle {
            unsafe {
                windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                    windows::Win32::Foundation::HWND(hwnd as *mut _),
                    msg,
                    Some(windows::Win32::Foundation::WPARAM(wparam)),
                    Some(windows::Win32::Foundation::LPARAM(lparam)),
                );
            }
        }
    }

    /// Insert a UTF-16 string at the current selection via EM_REPLACESEL.
    /// Going through the Windows message (instead of `editor.set_text`) keeps
    /// the undo history and avoids rebuilding the whole document.
    pub(super) fn replace_selection_utf16(&self, s: &str) {
        let wide: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
        self.send_editor_message(0x00C2, 1, wide.as_ptr() as isize); // EM_REPLACESEL, wParam=1 (undoable)
    }

    /// Undo last edit. EM_UNDO = 0x00C7.
    pub(super) fn on_undo(&self) {
        self.send_editor_message(0x00C7, 0, 0);
    }

    /// Redo last undone edit. EM_REDO = WM_USER + 84 = 0x0454 (RichEdit only).
    pub(super) fn on_redo(&self) {
        self.send_editor_message(0x0454, 0, 0);
    }

    pub(super) fn on_select_all(&self) {
        let len = self.editor.len() as u32;
        self.editor.set_selection(0..len);
    }

    /// Duplicate the current line.
    pub(super) fn on_duplicate_line(&self) {
        let text = self.get_content();
        let sel = self.editor.selection();
        let sel_start = (sel.start as usize).min(text.len());

        // Find line boundaries
        let line_start = text[..sel_start]
            .rfind('\n')
            .map(|p| p + 1)
            .unwrap_or(0);
        let line_end = text[sel_start..]
            .find('\n')
            .map(|p| sel_start + p)
            .unwrap_or(text.len());

        let line = &text[line_start..line_end];
        let insertion = format!("\n{}", line);

        // Insert after the current line
        self.editor.set_selection(line_end as u32..line_end as u32);
        // Use the Windows EM_REPLACESEL approach — for now, rebuild text
        let new_text = format!("{}{}{}", &text[..line_end], insertion, &text[line_end..]);
        self.editor.set_text(&new_text);
        let new_pos = (line_end + insertion.len()) as u32;
        self.editor.set_selection(new_pos..new_pos);
        self.mark_modified();
        self.layout_controls();
    }

    /// Delete the current line.
    pub(super) fn on_delete_line(&self) {
        let text = self.get_content();
        let sel = self.editor.selection();
        let sel_start = (sel.start as usize).min(text.len());

        let line_start = text[..sel_start]
            .rfind('\n')
            .map(|p| p + 1)
            .unwrap_or(0);
        let line_end = text[sel_start..]
            .find('\n')
            .map(|p| sel_start + p + 1) // include the newline
            .unwrap_or(text.len());

        // If deleting last line (no trailing \n), also remove preceding \n
        let del_start = if line_end == text.len() && line_start > 0 {
            line_start - 1
        } else {
            line_start
        };

        let new_text = format!("{}{}", &text[..del_start], &text[line_end..]);
        self.editor.set_text(&new_text);
        let new_pos = del_start.min(new_text.len()) as u32;
        self.editor.set_selection(new_pos..new_pos);
        self.mark_modified();
        self.layout_controls();
    }

    /// Toggle a "• " bullet prefix on the current line (Ctrl+.).
    pub(super) fn on_toggle_bullet(&self) {
        const BULLET: &str = "• ";
        const BULLET_U16: u32 = 2; // "• " is 1 code unit for the bullet + 1 for the space

        let text = self.get_content();
        let sel = self.editor.selection();
        let (byte_caret, _) = utf16_range_to_byte_range(&text, sel.start as usize, sel.start as usize);

        let line_start = text[..byte_caret].rfind('\n').map(|p| p + 1).unwrap_or(0);
        let line_end = text[byte_caret..].find('\n').map(|p| byte_caret + p).unwrap_or(text.len());
        let line = &text[line_start..line_end];

        let u16_map = build_utf16_offset_map(&text);
        let line_start_u16 = u16_map[line_start] as u32;

        if line.starts_with(BULLET) {
            let bullet_end_byte = line_start + BULLET.len();
            let bullet_end_u16 = u16_map[bullet_end_byte] as u32;
            self.editor.set_selection(line_start_u16..bullet_end_u16);
            self.replace_selection_utf16("");
            let new_caret = sel.start.saturating_sub(BULLET_U16).max(line_start_u16);
            self.editor.set_selection(new_caret..new_caret);
        } else {
            self.editor.set_selection(line_start_u16..line_start_u16);
            self.replace_selection_utf16(BULLET);
            let new_caret = sel.start + BULLET_U16;
            self.editor.set_selection(new_caret..new_caret);
        }
        self.mark_modified();
    }

    /// Insert a timestamp at the cursor position.
    pub(super) fn on_insert_timestamp(&self) {
        use std::time::SystemTime;
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default();
        // Format as a simple ISO-like timestamp
        let secs = now.as_secs();
        // Simple formatting without chrono dependency
        let timestamp = format_timestamp(secs);

        let text = self.get_content();
        let sel = self.editor.selection();
        let sel_start = (sel.start as usize).min(text.len());
        let sel_end = (sel.end as usize).min(text.len());
        let new_text = format!(
            "{}{}{}",
            &text[..sel_start],
            timestamp,
            &text[sel_end..]
        );
        self.editor.set_text(&new_text);
        let new_pos = sel_start as u32 + timestamp.len() as u32;
        self.editor.set_selection(new_pos..new_pos);
        self.mark_modified();
    }

    // =====================================================================
    // Search / Replace
    // =====================================================================

    /// Open the Find popup dialog.
    pub(super) fn on_show_find(&self) {
        use nwg::ControlHandle;
        if let ControlHandle::Hwnd(hwnd) = self.editor.handle {
            let initial = self.current_selection_text();
            crate::ui::dialogs::find_replace::FindReplaceDialog::show(hwnd as isize, false, &initial);
        }
    }

    /// Open the Find & Replace popup dialog.
    pub(super) fn on_show_replace(&self) {
        use nwg::ControlHandle;
        if let ControlHandle::Hwnd(hwnd) = self.editor.handle {
            let initial = self.current_selection_text();
            crate::ui::dialogs::find_replace::FindReplaceDialog::show(hwnd as isize, true, &initial);
        }
    }

    /// Escape key fallback (the popup dialog handles its own close).
    pub(super) fn on_escape(&self) {
        self.editor.set_focus();
    }

    /// Return the currently selected text in the editor, or empty if nothing is selected.
    pub(super) fn current_selection_text(&self) -> String {
        let text = self.get_content();
        let sel = self.editor.selection();
        let (start_b, end_b) = utf16_range_to_byte_range(&text, sel.start as usize, sel.end as usize);
        if start_b < end_b && end_b <= text.len() {
            text[start_b..end_b].to_string()
        } else {
            String::new()
        }
    }

    // =====================================================================
    // Go to Line
    // =====================================================================

    /// Snapshot the per-session state (active tab + caret line) to persist
    /// in the payload on the next save.
    pub(super) fn capture_session_state(&self) -> crate::features::session_state::SessionState {
        let active_tab = self.tab_bar.borrow().active_index();

        let text = self.get_content();
        let sel = self.editor.selection();
        let (byte_pos, _) =
            utf16_range_to_byte_range(&text, sel.start as usize, sel.start as usize);
        let cursor_line = text[..byte_pos.min(text.len())]
            .matches('\n')
            .count() as u32;

        crate::features::session_state::SessionState {
            active_tab,
            cursor_line,
            agent_keys: self.ssh_agent_keys.borrow().clone(),
            ..Default::default()
        }
    }

    /// Load a decrypted payload (after decompression and `Settings::parse`)
    /// into every store and the tab bar. `in_memory` is the session
    /// captured at lock time; it wins over the one read from disk.
    /// Returns the active tab's content (to put in the editor) and the caret
    /// line to restore (`None` when there is no session at all).
    /// Used by `run`, `restore_after_unlock` and `unlock_from_disk`.
    pub(super) fn load_payload(
        &self,
        note_text: &str,
        in_memory: Option<crate::features::session_state::SessionState>,
    ) -> (String, Option<u32>) {
        // Settings of this payload must already be in self.settings.
        let categories = self.vault_categories();
        let parsed = crate::features::payload::parse_payload(note_text, &categories);
        let has_session = in_memory.is_some() || parsed.session.is_some();
        *self.attachments.borrow_mut() = parsed.attachments;
        self.vault_panel.set_secrets(parsed.secrets);
        self.refresh_vault_tree(None);
        self.prompts_panel.set_prompts(parsed.prompts);
        self.refresh_prompts_list(None);

        let view = crate::features::payload::build_view(parsed.tabs, in_memory, parsed.session);
        let ui_tabs: Vec<crate::ui::tab_bar::Tab> = view.tabs.into_iter().map(|nt| {
            let mut t = crate::ui::tab_bar::Tab::new(&nt.name);
            // What was just read is what the last save wrote.
            t.saved_content = nt.content.clone();
            t.content = nt.content;
            t.color = nt.color;
            t
        }).collect();
        // tabs::parse always yields at least one tab, so `view.active` is a
        // valid index (build_view invariant).
        let content = ui_tabs[view.active].content.clone();
        let tb = crate::ui::tab_bar::TabBar::with_tabs(ui_tabs);
        tb.set_active(view.active);
        *self.tab_bar.borrow_mut() = tb;
        {
            *self.ssh_agent_keys.borrow_mut() = view.agent_keys;
        }

        (content, has_session.then_some(view.cursor_line))
    }

    /// Move the caret to the first column of the given zero-based line and
    /// scroll it into view. Out-of-range lines are clamped to the document.
    pub(super) fn apply_cursor_line(&self, line: u32) {
        let text = self.get_content();
        let byte_offset: usize = if line == 0 {
            0
        } else {
            text.split('\n')
                .take(line as usize)
                .map(|l| l.len() + 1)
                .sum::<usize>()
                .min(text.len())
        };
        let u16_map = build_utf16_offset_map(&text);
        let u16_off = u16_map[byte_offset] as u32;
        self.editor.set_selection(u16_off..u16_off);
        self.editor.set_focus();
        self.send_editor_message(0x00B7, 0, 0); // EM_SCROLLCARET
    }

    pub(super) fn on_goto_line(&self) {
        use crate::ui::dialogs::goto_line::GoToLineDialog;

        let text = self.get_content();
        let lines: Vec<&str> = text.split('\n').collect();
        let max_lines = lines.len().max(1) as u32;

        // Determine the current line from cursor position
        let sel = self.editor.selection();
        let cursor_pos = sel.start as usize;
        let current_line = text[..cursor_pos.min(text.len())]
            .matches('\n')
            .count() as u32
            + 1;

        if let Some(target_line) = GoToLineDialog::show(current_line, max_lines) {
            // Calculate character offset of the target line
            let offset: usize = lines
                .iter()
                .take((target_line - 1) as usize)
                .map(|l| l.len() + 1) // +1 for the '\n'
                .sum();
            let offset = offset.min(text.len()) as u32;

            self.editor.set_selection(offset..offset);
            self.editor.set_focus();

            // EM_SCROLLCARET (0x00B7) — scroll the caret into view
            self.send_editor_message(0x00B7, 0, 0);
        }
    }

    /// Edit > Go to Heading (Ctrl+Shift+O): the sequence of heading_flow.
    pub(super) fn on_goto_heading(&self) {
        use super::heading_flow::{go_to_heading, HeadingOutcome};
        match go_to_heading(&mut FormHeadingSteps { form: self }) {
            HeadingOutcome::BadIndex { index, count } => {
                eprintln!("ERROR outline: dialog returned index={} for count={}, caret not moved", index, count);
            }
            outcome => eprintln!("INFO outline: go to heading {:?}", outcome),
        }
    }
}

/// The steps of `heading_flow` on the window.
struct FormHeadingSteps<'a> {
    form: &'a EditorForm,
}

impl super::heading_flow::HeadingSteps for FormHeadingSteps<'_> {
    fn text(&self) -> String {
        self.form.get_content()
    }
    fn lock_epoch(&self) -> u64 {
        self.form.lock_epoch()
    }
    fn pick(&mut self, headings: &[crate::features::outline::Heading]) -> Option<usize> {
        let _modal = self.form.modal();
        crate::ui::dialogs::goto_heading::GoToHeadingDialog::show(headings, Some(self.form.window.handle))
    }
    fn still_valid(&self, epoch: u64) -> bool {
        self.form.still_valid(epoch)
    }
    fn move_caret(&mut self, line: u32) {
        self.form.apply_cursor_line(line);
    }
}
