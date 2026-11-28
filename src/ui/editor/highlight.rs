// Occurrence highlighting.
// Part of the EditorForm implementation, split out of the former editor.rs.

#[allow(unused_imports)]
use super::*;

impl EditorForm {
    // =====================================================================
    // Occurrence highlighting
    // =====================================================================

    /// Update occurrence highlighting based on the current selection.
    /// Called on every highlight_timer tick (200ms debounce).
    pub(super) fn update_occurrence_highlights(&self) {
        let sel = self.editor.selection();
        // Display and positions only; on every timer tick, so never a dialog.
        let text = match self.display_content() {
            Ok(text) => text,
            Err(e) => {
                eprintln!("ERROR highlight: not refreshed, editor text unreadable err={:?}", e);
                return;
            }
        };

        // Convert UTF-16 selection positions to byte offsets in the UTF-8 string.
        // RichTextBox selection uses UTF-16 code unit positions.
        let utf16_start = sel.start as usize;
        let utf16_end = sel.end as usize;
        let (byte_start, byte_end) = utf16_range_to_byte_range(&text, utf16_start, utf16_end);

        // Extract selected text
        let selected = if byte_start < byte_end && byte_end <= text.len() {
            &text[byte_start..byte_end]
        } else {
            ""
        };

        let search_term = highlight_term(selected).unwrap_or("");

        // Skip redundant work if same as last time
        {
            let last = self.last_highlighted.borrow();
            if *last == search_term {
                return;
            }
        }
        *self.last_highlighted.borrow_mut() = search_term.to_string();

        let Some(hwnd) = super::tom::hwnd_of(self.editor.handle) else {
            return;
        };
        // The selection with its direction, put back as it was at the end.
        let (anchor, active) = match super::tom::selection_ends(hwnd) {
            Ok(ends) => ends,
            Err(e) => {
                eprintln!("ERROR highlight: occurrence highlights skipped, selection unreadable: {e}");
                return;
            }
        };

        // Suppress change events and redraw while manipulating selections
        *self.suppress_change.borrow_mut() = true;
        self.send_editor_message(0x000B, 0, 0); // WM_SETREDRAW(FALSE)

        // Save the topmost visible line. Repeated set_selection() calls below
        // make RichEdit auto-scroll internally even with WM_SETREDRAW disabled,
        // which causes a visible jump when redraw is re-enabled.
        let first_visible = self.get_first_visible_line();

        // The backgrounds are presentation: with the undo on, each format
        // change would be an undo step ahead of the user's last edit.
        let painted = super::tom::without_undo(hwnd, || {
            // Clear all previous highlights (reset entire text to auto background)
            let total_len = utf16_len(&text) as u32;
            self.editor.set_selection(0..total_len);
            self.apply_char_format_background(None); // auto background

            if !search_term.is_empty() {
                let highlight_color = crate::theme::current().match_highlight.to_colorref();

                // Build a UTF-16 offset map for converting byte positions to UTF-16 positions.
                let utf16_offsets = build_utf16_offset_map(&text);

                // Find all case-insensitive matches (byte positions in the original text).
                let matches = find_all_case_insensitive(&text, search_term);

                for (abs_byte, abs_byte_end) in matches {
                    let u16_start = utf16_offsets[abs_byte];
                    let u16_end = utf16_offsets[abs_byte_end];

                    // Don't highlight the current selection itself
                    if u16_start != utf16_start || u16_end != utf16_end {
                        self.editor.set_selection(u16_start as u32..u16_end as u32);
                        self.apply_char_format_background(Some(highlight_color));
                    }
                }
            }
        });
        // A history left suspended is told by the tick (undo_watch); this
        // pass only notes it, with the redraw still off.
        self.note_undo(painted, "the occurrence highlights");

        // Restore original selection, active end included (EM_SETSEL)
        self.send_editor_message(0x00B1, anchor as usize, active as isize);

        // Restore the viewport to the line that was visible before we started.
        // EM_LINESCROLL (0x00B6): lParam = signed delta in lines (positive = down).
        let after_first_visible = self.get_first_visible_line();
        let delta = first_visible - after_first_visible;
        if delta != 0 {
            self.send_editor_message(0x00B6, 0, delta as isize);
        }

        // Re-enable change events and redraw
        *self.suppress_change.borrow_mut() = false;
        self.send_editor_message(0x000B, 1, 0); // WM_SETREDRAW(TRUE)
        self.invalidate_editor();
    }

    /// Returns the zero-based index of the topmost visible line in the editor
    /// (EM_GETFIRSTVISIBLELINE = 0x00CE).
    pub(super) fn get_first_visible_line(&self) -> i32 {
        use nwg::ControlHandle;
        if let ControlHandle::Hwnd(hwnd) = self.editor.handle {
            unsafe {
                let r = windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                    windows::Win32::Foundation::HWND(hwnd as *mut _),
                    0x00CE,
                    None,
                    None,
                );
                return r.0 as i32;
            }
        }
        0
    }

    /// Apply a background color to the current RichTextBox selection via CHARFORMAT2W.
    /// Pass `None` to reset to the control's default background (CFE_AUTOBACKCOLOR).
    pub(super) fn apply_char_format_background(&self, color: Option<u32>) {
        use nwg::ControlHandle;
        if let ControlHandle::Hwnd(hwnd) = self.editor.handle {
            // CHARFORMAT2W is 116 bytes (pack(4)). Key offsets:
            //   cbSize=0, dwMask=4, dwEffects=8, crBackColor=96.
            // Full layout: cbSize(4) dwMask(4) dwEffects(4) yHeight(4) yOffset(4)
            //   crTextColor(4) bCharSet(1) bPitchAndFamily(1) szFaceName(64)
            //   wWeight(2) sSpacing(2) [pad 2] crBackColor(4) ...
            let mut buf = [0u8; 116];

            // cbSize (offset 0)
            buf[0..4].copy_from_slice(&116u32.to_le_bytes());

            // dwMask (offset 4) — CFM_BACKCOLOR = 0x04000000
            buf[4..8].copy_from_slice(&0x04000000u32.to_le_bytes());

            // dwEffects (offset 8) — CFE_AUTOBACKCOLOR = 0x04000000 when resetting
            let effects: u32 = if color.is_none() { 0x04000000 } else { 0 };
            buf[8..12].copy_from_slice(&effects.to_le_bytes());

            // crBackColor (offset 96)
            buf[96..100].copy_from_slice(&color.unwrap_or(0).to_le_bytes());

            unsafe {
                // EM_SETCHARFORMAT = 0x0444, SCF_SELECTION = 0x0001
                windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                    windows::Win32::Foundation::HWND(hwnd as *mut _),
                    0x0444,
                    Some(windows::Win32::Foundation::WPARAM(0x0001)),
                    Some(windows::Win32::Foundation::LPARAM(buf.as_mut_ptr() as isize)),
                );
            }
        }
    }

    /// Invalidate the editor control to trigger a repaint.
    pub(super) fn invalidate_editor(&self) {
        use nwg::ControlHandle;
        if let ControlHandle::Hwnd(hwnd) = self.editor.handle {
            unsafe {
                let _ = windows::Win32::Graphics::Gdi::InvalidateRect(
                    Some(windows::Win32::Foundation::HWND(hwnd as *mut _)),
                    None,
                    true,
                );
            }
        }
    }
}
