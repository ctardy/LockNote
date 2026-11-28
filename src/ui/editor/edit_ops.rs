// View, edit, search/replace and go-to-line operations.
// Part of the EditorForm implementation, split out of the former editor.rs.

#[allow(unused_imports)]
use super::*;
use crate::features::case_convert::CaseOp;

/// Replace the selection of the RichEdit `hwnd` with `s` as one undoable
/// edit, the caret after it: EM_REPLACESEL (wParam = 1). A text that is a
/// lone form feed (what Trim or Shift+Tab leaves of " \f " or "\t\f\t") goes
/// through TOM instead: EM_REPLACESEL (and EM_SETTEXTEX) turn it into a
/// paragraph break (CRLF), ITextSelection::SetText keeps it, in one undo
/// step too (live tests in line_ops.rs). Should TOM fail, the error is
/// logged and EM_REPLACESEL still applies the edit: the line count shown is
/// the same, only that break becomes a paragraph break.
pub(super) fn replace_selection_on(hwnd: windows::Win32::Foundation::HWND, s: &str) {
    if s == "\u{c}" {
        match super::tom::replace_selection_text(hwnd, s) {
            Ok(()) => return,
            Err(e) => eprintln!("ERROR edit: lone form feed not inserted through TOM ({e}); EM_REPLACESEL makes it a paragraph break"),
        }
    }
    let wide: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::SendMessageW(
            hwnd,
            0x00C2, // EM_REPLACESEL
            Some(windows::Win32::Foundation::WPARAM(1)), // undoable
            Some(windows::Win32::Foundation::LPARAM(wide.as_ptr() as isize)),
        );
    }
}

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

    /// Put the View check marks, the word wrap and the editor font back in
    /// line with the settings, after they were replaced (unlock, backup
    /// import); the gutter follows at the next `layout_controls`. `Err`:
    /// the font of the settings was refused and the editor keeps its own
    /// (the caller decides whether to tell the user).
    pub(super) fn sync_view_menu(&self) -> Result<(), String> {
        let (wrap, numbers) = view_menu_checks(&self.settings.borrow());
        self.menu_view_word_wrap.set_checked(wrap);
        self.menu_view_line_numbers.set_checked(numbers);
        self.apply_word_wrap(wrap);
        // An imported font applies at once, like one from the preferences;
        // the same settings (unlock) keep the zoom.
        self.apply_settings_font().inspect_err(|e| {
            eprintln!("ERROR font: the font of the replaced settings was not applied: {e}");
        })
    }

    /// Zoom the editor font by one step (Ctrl+= / Ctrl+- / Ctrl+0 and their
    /// numpad keys, Ctrl+wheel). Session only: the settings keep their size.
    pub(super) fn on_zoom(&self, step: zoom::ZoomStep) {
        let current = self.editor_font_state.borrow().clone();
        let target = current.zoomed(step);
        match zoom::switch_font(&current, target, &mut FormFont(self)) {
            Ok(true) => eprintln!("INFO zoom: {step:?} -> {} pt", self.editor_font_state.borrow().size_pt),
            Ok(false) => {}
            Err(e) => eprintln!("ERROR zoom: {step:?} not applied, the editor keeps its font: {e}"),
        }
    }

    /// Ctrl+wheel over the editor: one zoom step per notch (`wheel_steps`).
    pub(super) fn on_wheel_zoom(&self, delta: i32) {
        let (steps, rest) = zoom::wheel_steps(self.zoom_wheel_rest.get(), delta);
        self.zoom_wheel_rest.set(rest);
        let step = if steps > 0 { zoom::ZoomStep::In } else { zoom::ZoomStep::Out };
        for _ in 0..steps.unsigned_abs() {
            self.on_zoom(step);
        }
    }

    /// Give the editor the font of the settings unless it already shows it
    /// (zoomed or not): a new family or size ends the zoom (A2).
    pub(super) fn apply_settings_font(&self) -> Result<(), String> {
        let (family, size) = {
            let s = self.settings.borrow();
            (s.font_family.clone(), s.font_size)
        };
        let current = self.editor_font_state.borrow().clone();
        if current.follows(&family, size) {
            return Ok(());
        }
        let target = zoom::EditorFontState::from_settings(&family, size);
        zoom::switch_font(&current, target, &mut FormFont(self))?;
        eprintln!("INFO font: editor font {family} {size} pt applied");
        Ok(())
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
        // Only a selection is set: the display text keeps every position.
        let text = match self.display_content() {
            Ok(text) => text,
            Err(e) => {
                eprintln!("ERROR dblclick: native selection kept, editor text unreadable err={:?}", e);
                return;
            }
        };
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

        // The main-row zoom keys are found by the character they type in
        // the active layout (`-` is the 6 key on a French keyboard).
        let mapped = unsafe {
            windows::Win32::UI::Input::KeyboardAndMouse::MapVirtualKeyW(
                vk,
                windows::Win32::UI::Input::KeyboardAndMouse::MAPVK_VK_TO_CHAR,
            )
        };
        if let Some(step) = zoom::zoom_key(vk, zoom::key_char(mapped), ctrl, shift, alt) {
            self.on_zoom(step);
            return true;
        }

        if ctrl && !shift {
            match vk {
                0x51 => { if self.on_close() { nwg::stop_thread_dispatch(); } return true; } // Ctrl+Q
                0x46 => { self.on_show_find(); return true; }   // Ctrl+F
                0x48 => { self.on_show_replace(); return true; }// Ctrl+H
                0x47 => { self.on_goto_line(); return true; }   // Ctrl+G
                0x44 => { self.on_duplicate_line(); return true; } // Ctrl+D
                0x55 => { self.on_convert_case(CaseOp::Lower); return true; } // Ctrl+U
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
                0x55 => { self.on_convert_case(CaseOp::Upper); return true; } // Ctrl+Shift+U
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

    /// Insert a UTF-16 string at the current selection (`replace_selection_on`).
    /// Going through the Windows message (instead of `editor.set_text`) keeps
    /// the undo history and avoids rebuilding the whole document.
    pub(super) fn replace_selection_utf16(&self, s: &str) {
        if let Some(hwnd) = super::tom::hwnd_of(self.editor.handle) {
            replace_selection_on(hwnd, s);
        }
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
        // RichEdit positions are UTF-16 units; nwg's len() counts chars and
        // would stop short of the end after an emoji.
        let len = match self.display_content() {
            Ok(text) => utf16_len(&text) as u32,
            Err(e) => {
                self.tell_unreadable("select_all", &e);
                return;
            }
        };
        self.editor.set_selection(0..len);
    }

    /// Duplicate the current line (Ctrl+D): one undoable EM_REPLACESEL.
    pub(super) fn on_duplicate_line(&self) {
        self.on_current_line_edit("duplicate", super::line_ops::duplicate_line_edit);
    }

    /// Delete the current line (Ctrl+Shift+K): one undoable EM_REPLACESEL.
    pub(super) fn on_delete_line(&self) {
        self.on_current_line_edit("delete", super::line_ops::delete_line_edit);
    }

    /// Toggle a "• " bullet prefix on the current line (Ctrl+.).
    pub(super) fn on_toggle_bullet(&self) {
        const BULLET: &str = "• ";
        const BULLET_U16: u32 = 2; // "• " is 1 code unit for the bullet + 1 for the space

        // Positions only; what is inserted or removed is the bullet itself,
        // never text read here: the display text is enough.
        let text = match self.display_content() {
            Ok(text) => text,
            Err(e) => {
                self.tell_unreadable("toggle_bullet", &e);
                return;
            }
        };
        let sel = self.editor.selection();
        let (byte_caret, _) = utf16_range_to_byte_range(&text, sel.start as usize, sel.start as usize);

        // The line as RichEdit shows it: a form feed ends it too.
        let line_start = crate::features::text_lines::line_start_at(&text, byte_caret);
        let line_end = crate::features::text_lines::line_end_at(&text, byte_caret);
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

    /// Insert a timestamp at the cursor position: the local time of the
    /// machine, read by the same clock as the prompt variables. It replaces
    /// the selection as one undoable EM_REPLACESEL.
    pub(super) fn on_insert_timestamp(&self) {
        let timestamp = local_now();
        super::line_ops::insert_at_selection(&super::line_ops::EditorLineSteps(self), &timestamp);
        self.reset_occurrence_highlights();
        self.send_editor_message(0x00B7, 0, 0); // EM_SCROLLCARET
    }

    // =====================================================================
    // Search / Replace
    // =====================================================================

    /// Open the Find popup dialog.
    pub(super) fn on_show_find(&self) {
        use nwg::ControlHandle;
        if let ControlHandle::Hwnd(hwnd) = self.editor.handle {
            let initial = match self.current_selection_text() {
                Ok(initial) => initial,
                Err(e) => return self.tell_unreadable("find", &e),
            };
            crate::ui::dialogs::find_replace::FindReplaceDialog::show(hwnd as isize, false, &initial);
        }
    }

    /// Open the Find & Replace popup dialog.
    pub(super) fn on_show_replace(&self) {
        use nwg::ControlHandle;
        if let ControlHandle::Hwnd(hwnd) = self.editor.handle {
            let initial = match self.current_selection_text() {
                Ok(initial) => initial,
                Err(e) => return self.tell_unreadable("replace", &e),
            };
            crate::ui::dialogs::find_replace::FindReplaceDialog::show(hwnd as isize, true, &initial);
        }
    }

    /// Escape key fallback (the popup dialog handles its own close).
    pub(super) fn on_escape(&self) {
        self.editor.set_focus();
    }

    /// Return the currently selected text in the editor, or empty if nothing
    /// is selected. Display text (a lone surrogate shows as U+FFFD): it
    /// seeds a search box or is printed, never stored nor written back.
    pub(super) fn current_selection_text(&self) -> Result<String, EditorTextError> {
        let text = self.display_content()?;
        let sel = self.editor.selection();
        let (start_b, end_b) = utf16_range_to_byte_range(&text, sel.start as usize, sel.end as usize);
        if start_b < end_b && end_b <= text.len() {
            Ok(text[start_b..end_b].to_string())
        } else {
            Ok(String::new())
        }
    }

    // =====================================================================
    // Go to Line
    // =====================================================================

    /// Snapshot the per-session state (active tab + caret line) to persist
    /// in the payload on the next save. The caret line is counted on '\n'
    /// (what is saved, read back by `session_line_position`), not on the
    /// lines RichEdit shows: a form feed is not counted. It is counted on
    /// the display text (positions only).
    pub(super) fn capture_session_state(&self) -> Result<crate::features::session_state::SessionState, EditorTextError> {
        let active_tab = self.tab_bar.borrow().active_index();

        let text = self.display_content()?;
        let sel = self.editor.selection();
        let (byte_pos, _) =
            utf16_range_to_byte_range(&text, sel.start as usize, sel.start as usize);
        let cursor_line = text[..byte_pos.min(text.len())]
            .matches('\n')
            .count() as u32;

        Ok(crate::features::session_state::SessionState {
            active_tab,
            cursor_line,
            agent_keys: self.ssh_agent_keys.borrow().clone(),
            ..Default::default()
        })
    }

    /// Load a decrypted payload (after decompression and `Settings::parse`)
    /// into every store and the tab bar. `in_memory` is the session
    /// captured at lock time; it wins over the one read from disk.
    /// Returns the active tab's content (to put in the editor; wrapped, wiped
    /// when the caller drops it, issue #42) and the caret line to restore
    /// (`None` when there is no session at all).
    /// Used by `run`, `restore_after_unlock` and `unlock_from_disk`.
    pub(super) fn load_payload(
        &self,
        note_text: &str,
        in_memory: Option<crate::features::session_state::SessionState>,
    ) -> (zeroize::Zeroizing<String>, Option<u32>) {
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
        let content = zeroize::Zeroizing::new(ui_tabs[view.active].content.clone());
        let tb = crate::ui::tab_bar::TabBar::with_tabs(ui_tabs);
        tb.set_active(view.active);
        *self.tab_bar.borrow_mut() = tb;
        {
            *self.ssh_agent_keys.borrow_mut() = view.agent_keys;
        }

        (content, has_session.then_some(view.cursor_line))
    }

    /// Restore the caret line saved in the session (`capture_session_state`,
    /// counted on '\n'): first column of that line, scrolled into view.
    /// Out-of-range lines are clamped to the document.
    pub(super) fn apply_cursor_line(&self, line: u32) {
        let text = match self.display_content() {
            Ok(text) => text,
            Err(e) => {
                eprintln!("ERROR goto: caret not moved to session line={}, editor text unreadable err={:?}", line, e);
                return;
            }
        };
        let pos = session_line_position(&text, line);
        self.move_caret_to(pos);
    }

    /// Ctrl+G and Go to Heading: the caret on the first column of the
    /// zero-based `line` as RichEdit shows the lines (a form feed ends one),
    /// scrolled into view. Out-of-range lines are clamped to the document.
    pub(super) fn go_to_shown_line(&self, line: u32) {
        let text = match self.display_content() {
            Ok(text) => text,
            Err(e) => {
                eprintln!("ERROR goto: caret not moved to line={}, editor text unreadable err={:?}", line, e);
                return;
            }
        };
        let pos = line_start_position(&text, line);
        self.move_caret_to(pos);
    }

    fn move_caret_to(&self, pos: u32) {
        self.editor.set_selection(pos..pos);
        self.editor.set_focus();
        self.send_editor_message(0x00B7, 0, 0); // EM_SCROLLCARET
    }

    pub(super) fn on_goto_line(&self) {
        use crate::ui::dialogs::goto_line::GoToLineDialog;

        let text = match self.display_content() {
            Ok(text) => text,
            Err(e) => return self.tell_unreadable("goto_line", &e),
        };
        let max_lines = shown_line_count(&text);
        let current = current_line(&text, self.editor.selection().start);

        // The dialog guarantees 1 <= target <= max_lines; the text and the
        // caret are read again by go_to_shown_line, after the dialog.
        if let Some(target_line) = GoToLineDialog::show(current, max_lines) {
            self.go_to_shown_line(target_line - 1);
        }
    }

    /// Edit > Go to Heading (Ctrl+Shift+O): the sequence of heading_flow.
    pub(super) fn on_goto_heading(&self) {
        use super::heading_flow::{go_to_heading, HeadingOutcome};
        match go_to_heading(&mut FormHeadingSteps { form: self }) {
            HeadingOutcome::BadIndex { index, count } => {
                eprintln!("ERROR outline: dialog returned index={} for count={}, caret not moved", index, count);
            }
            HeadingOutcome::Unreadable(e) => self.tell_unreadable("goto_heading", &e),
            outcome => eprintln!("INFO outline: go to heading {:?}", outcome),
        }
    }
}

/// Ctrl+G: the 1-based line of the RichEdit position `caret` (UTF-16
/// units, a line break = 1), as the status bar counts it.
pub(super) fn current_line(text: &str, caret: u32) -> u32 {
    let p = caret as usize;
    caret_status(text, p, p, p).line as u32
}

/// Ctrl+G: the lines RichEdit shows (features::text_lines), the empty one
/// after a final break included, as the gutter numbers them.
pub(super) fn shown_line_count(text: &str) -> u32 {
    crate::features::text_lines::line_ranges(text).len() as u32
}

/// RichEdit position of the first column of the zero-based `line` of
/// `text` (the `get_content` text), lines as RichEdit shows them: "\r\n"
/// and a form feed each end one. A line past the end gives the end of
/// the text.
pub(super) fn line_start_position(text: &str, line: u32) -> u32 {
    let lines = crate::features::text_lines::line_ranges(text);
    let byte_offset = lines.get(line as usize).map_or(text.len(), |&(start, _)| start);
    build_utf16_offset_map(text)[byte_offset] as u32
}

/// RichEdit position of the first column of the zero-based session caret
/// `line`, counted on '\n' as `capture_session_state` saved it (a form
/// feed is not counted: the saved number keeps its meaning). A line past
/// the end gives the end of the text.
pub(super) fn session_line_position(text: &str, line: u32) -> u32 {
    // Each line counted with its "\r" and its "\n": a char boundary.
    let byte_offset: usize = text.split('\n').take(line as usize).map(|l| l.len() + 1).sum::<usize>().min(text.len());
    build_utf16_offset_map(text)[byte_offset] as u32
}

/// The steps of `heading_flow` on the window.
struct FormHeadingSteps<'a> {
    form: &'a EditorForm,
}

impl super::heading_flow::HeadingSteps for FormHeadingSteps<'_> {
    /// Headings and their lines only: the display text.
    fn text(&self) -> Result<String, EditorTextError> {
        self.form.display_content()
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
        self.form.go_to_shown_line(line);
    }
}

/// The window side of `zoom::switch_font`. No borrow of the font state is
/// held across a step: `refresh` repaints the gutter, which reads it.
struct FormFont<'a>(&'a EditorForm);

impl zoom::FontSteps for FormFont<'_> {
    fn apply_font(&mut self, family: &str, size_pt: f64) -> Result<(), String> {
        let font = build_editor_font(family, size_pt)?;
        let nwg::ControlHandle::Hwnd(raw) = self.0.editor.handle else {
            return Err("the editor has no window handle".to_string());
        };
        let hwnd = windows::Win32::Foundation::HWND(raw as *mut _);
        // On a refusal the editor still uses the old font, which stays;
        // once it took the new one, the old one may go. The font counts as
        // applied even when the undo history did not come back: that is
        // noted for the highlight tick to tell, never shown in the middle
        // of a Ctrl+wheel zoom (undo_watch).
        let applied = surface::set_font(hwnd, font.handle as isize)?;
        *self.0.editor_font.borrow_mut() = font;
        self.0.note_undo(applied.undo.map_err(super::undo_watch::UndoError::NotResumed), "a font change");
        Ok(())
    }

    fn store(&mut self, state: zoom::EditorFontState) {
        *self.0.editor_font_state.borrow_mut() = state;
    }

    fn refresh(&mut self) {
        // The gutter width follows the font size; the band and the numbers
        // follow the new line pitch.
        self.0.layout_controls();
        self.0.paint_gutter();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn goto_current_line_counts_richedit_positions() {
        // "\r\n" is one position: the caret at 2 is on line 2.
        assert_eq!(current_line("a\r\nb", 2), 2);
        assert_eq!(current_line("a\r\nb\r\n", 4), 3);
        // A caret after "é" or an emoji used to slice inside it (panic).
        assert_eq!(current_line("é", 1), 1);
        assert_eq!(current_line("😀\r\nx", 3), 2);
        assert_eq!(current_line("• \r\n• ", 5), 2);
        assert_eq!(current_line("", 0), 1);
    }

    #[test]
    fn goto_target_is_a_richedit_position() {
        // Ten lines "x": line 6 (zero-based 5) starts at position 10, not
        // at byte 15 (which is line 8).
        let ten = vec!["x"; 10].join("\r\n");
        assert_eq!(line_start_position(&ten, 5), 10);
        assert_eq!(line_start_position("a\r\nb\r\nc", 2), 4);
        assert_eq!(line_start_position("é\r\n😀\r\nz", 2), 5);
        assert_eq!(line_start_position("é\r\n😀\r\nz", 1), 2);
        assert_eq!(line_start_position("abc", 0), 0);
        assert_eq!(line_start_position("", 0), 0);
        // Past the last line: the end of the text.
        assert_eq!(line_start_position("a\r\nb", 9), 3);
    }

    #[test]
    fn goto_counts_a_form_feed_as_a_line_break_as_richedit() {
        // Shown as a | b | c (positions a=0, FF=1, b=2, CRLF=3, c=4).
        let t = "a\u{c}b\r\nc";
        assert_eq!(current_line(t, 2), 2);
        assert_eq!(current_line(t, 4), 3);
        assert_eq!(line_start_position(t, 1), 2);
        assert_eq!(line_start_position(t, 2), 4);
        assert_eq!(line_start_position(t, 3), 5);
        assert_eq!(shown_line_count(t), 3);
        // As the gutter: the empty line after a final break counts.
        assert_eq!(shown_line_count(""), 1);
        assert_eq!(shown_line_count("a\r\n"), 2);
        assert_eq!(shown_line_count("a\u{c}"), 2);
    }

    #[test]
    fn session_caret_line_keeps_counting_on_line_feeds() {
        // The saved caret line is a '\n' count (capture_session_state): on
        // "a\fb\r\nc" the caret on "c" was saved as 1, and comes back on
        // "c" (position 4), as before a form feed counted as a line.
        let t = "a\u{c}b\r\nc";
        assert_eq!(session_line_position(t, 1), 4);
        assert_eq!(session_line_position(t, 0), 0);
        assert_eq!(session_line_position("a\r\nb\r\nc", 2), 4);
        assert_eq!(session_line_position("é\r\n😀\r\nz", 2), 5);
        assert_eq!(session_line_position("a\r\nb", 9), 3);
    }
}
