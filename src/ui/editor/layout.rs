// Control layout.
// Part of the EditorForm implementation, split out of the former editor.rs.

#[allow(unused_imports)]
use super::*;

impl EditorForm {
    // =====================================================================
    // Layout
    // =====================================================================

    /// Reposition controls when the window is resized.
    pub(super) fn layout_controls(&self) {
        // Locked or under the curtain (pass 6): only the lock screen is laid
        // out (resize, restore, DPI change); nothing behind it is shown again.
        if layout_mode(self.input_blocked()) == LayoutMode::OverlayOnly {
            self.fit_lock_overlay();
            return;
        }

        let (w, h) = self.window.size();
        let status_h: u32 = STATUS_BAR_HEIGHT as u32;
        self.position_status_bar(w as i32, h as i32);
        // Search bar height (if visible)
        // Find/Replace is now a popup dialog, no vertical band reserved in the layout.
        let search_h: u32 = 0;

        // Tab bar height (only when tabs feature is active)
        let tab_bar_h: u32 = crate::ui::tab_layout::TAB_STRIP_H as u32;

        // Position toolbar buttons (top-right, in the tab bar area)
        {
            // Same slots the tab strip reserves (tab_layout::toolbar_slots).
            {
                use crate::ui::tab_layout::{toolbar_buttons, toolbar_slots, ToolbarBtn};
                let btn_h: i32 = 24;
                let btn_y: i32 = ((tab_bar_h as i32) - btn_h) / 2; // vertically centered in tab bar
                for (btn, slot) in toolbar_slots(w as i32, &toolbar_buttons()) {
                    let ctl: &nwg::Button = match btn {
                        ToolbarBtn::Lock => &self.toolbar_lock,
                        ToolbarBtn::Attach => &self.toolbar_attach,
                        ToolbarBtn::Totp => &self.toolbar_totp,
                        ToolbarBtn::Vault => &self.toolbar_vault,
                        ToolbarBtn::Prompts => &self.toolbar_prompts,
                        #[allow(unreachable_patterns)]
                        _ => continue,
                    };
                    ctl.set_position(slot.x, btn_y);
                    ctl.set_size(slot.w as u32, btn_h as u32);
                }
            }
        }

        // Calculate gutter width based on paragraph count (real lines, not visual)
        let text = self.editor.text();
        let total_paragraphs = if text.is_empty() { 1 } else { text.matches('\n').count() + 1 };
        let show_line_numbers = self.settings.borrow().show_line_numbers;
        // Digits as wide as the gutter draws them (the zoom included, A2);
        // measured only when the gutter is shown.
        let digit_px = if show_line_numbers {
            let (family, size) = {
                let font = self.editor_font_state.borrow();
                (font.family.clone(), font.size_pt)
            };
            gutter_digit_px(&family, size)
        } else {
            0
        };
        let gutter_w = shown_gutter_width(show_line_numbers, total_paragraphs, digit_px) as u32;

        // Side panel widths: the user's width (or the one being dragged),
        // bounded so the editor keeps its minimum (ui/side_panel.rs).
        let vault_w: u32 = if self.vault_panel.is_visible() {
            self.side_panel_width(crate::ui::side_panel::SidePanel::Vault, w, gutter_w)
        } else {
            0
        };

        // TOTP panel width
        let totp_w: u32 = if self.totp_panel.is_visible() {
            self.side_panel_width(crate::ui::side_panel::SidePanel::Totp, w, gutter_w)
        } else {
            0
        };

        // Attachments panel width
        let attach_w: u32 = if self.attach_panel.is_visible() {
            self.side_panel_width(crate::ui::side_panel::SidePanel::Attach, w, gutter_w)
        } else {
            0
        };

        // Prompts panel width
        let prompts_w: u32 = if self.prompts_panel.is_visible() {
            self.side_panel_width(crate::ui::side_panel::SidePanel::Prompts, w, gutter_w)
        } else {
            0
        };

        // Resize grip, between the editor and the open panel
        let grip_w: u32 = if self.open_side_panel().is_some() { crate::ui::side_panel::GRIP_W } else { 0 };

        // Total right-side panel width (grip included)
        let right_panel_w = vault_w + totp_w + attach_w + prompts_w + grip_w;

        // Position vault panel on the right side
        {
            if self.vault_panel.is_visible() {
                let vault_x = crate::ui::side_panel::frame_x(w, vault_w);
                let vault_h = (h as i32) - (status_h as i32) - (tab_bar_h as i32);
                let vault_h = if vault_h < 0 { 0 } else { vault_h as u32 };
                self.vault_frame.set_position(vault_x, tab_bar_h as i32);
                self.vault_frame.set_size(vault_w, vault_h);
                self.vault_frame.set_visible(true);
                // Search on top, the tree fills the frame under it; the
                // reveal field sits under the tree, the buttons at the bottom.
                let tree_y: u32 = 8 + 24 + 8;
                let list_h = (vault_h as i32) - tree_y as i32 - 22 - 4 - 24 - 8 - 26 - 8;
                let list_h = if list_h < 50 { 50 } else { list_h as u32 };
                let inner_w = crate::ui::side_panel::inner_width(vault_w);
                self.vault_search.set_position(8, 8);
                self.vault_search.set_size(inner_w, 24);
                self.vault_tree.set_position(8, tree_y as i32);
                self.vault_tree.set_size(inner_w, list_h);
                self.vault_copied.set_position(8, (tree_y + list_h + 4) as i32);
                self.vault_copied.set_size(inner_w, 18);
                let reveal_y = (tree_y + list_h + 22 + 4) as i32;
                self.vault_reveal.set_position(8, reveal_y);
                self.vault_reveal.set_size(inner_w, 24);
                let btn_y = reveal_y + 24 + 8;
                self.vault_btn_add.set_position(8, btn_y);
                self.vault_btn_edit.set_position(64, btn_y);
                self.vault_btn_delete.set_position(120, btn_y);
                self.vault_btn_copy.set_position(176, btn_y);
                self.vault_btn_reveal.set_position(232, btn_y);
            } else {
                self.vault_frame.set_visible(false);
            }
        }

        // Position TOTP panel (left of vault panel)
        {
            if self.totp_panel.is_visible() {
                let totp_x = crate::ui::side_panel::frame_x(w, totp_w);
                let totp_h = (h as i32) - (status_h as i32) - (tab_bar_h as i32);
                let totp_h = if totp_h < 0 { 0 } else { totp_h as u32 };
                self.totp_frame.set_position(totp_x, tab_bar_h as i32);
                self.totp_frame.set_size(totp_w, totp_h);
                self.totp_frame.set_visible(true);
                // Resize list box to fill the frame
                let list_h = (totp_h as i32) - 100;
                let list_h = if list_h < 50 { 50 } else { list_h as u32 };
                let inner_w = crate::ui::side_panel::inner_width(totp_w);
                self.totp_list.set_size(inner_w, list_h);
                // Position countdown label at top
                self.totp_lbl_countdown.set_position(8, 8);
                self.totp_lbl_countdown.set_size(inner_w, 20);
                // List starts below countdown
                self.totp_list.set_position(8, 32);
                // Position buttons below the list
                let btn_y = (32 + list_h + 8) as i32;
                self.totp_btn_copy.set_position(8, btn_y);
                self.totp_btn_add.set_position(70, btn_y);
                self.totp_btn_remove.set_position(132, btn_y);
                // Status label below buttons
                self.totp_lbl_status.set_position(8, btn_y + 30);
                self.totp_lbl_status.set_size(inner_w, 18);
            } else {
                self.totp_frame.set_visible(false);
            }
        }

        // Position attachments panel (left of vault and totp panels if visible)
        {
            if self.attach_panel.is_visible() {
                let attach_x = crate::ui::side_panel::frame_x(w, attach_w);
                let attach_h = (h as i32) - (status_h as i32) - (tab_bar_h as i32);
                let attach_h = if attach_h < 0 { 0 } else { attach_h as u32 };
                self.attach_frame.set_position(attach_x, tab_bar_h as i32);
                self.attach_frame.set_size(attach_w, attach_h);
                self.attach_frame.set_visible(true);
                // Resize list box to fill the frame
                let list_h = (attach_h as i32) - 80;
                let list_h = if list_h < 50 { 50 } else { list_h as u32 };
                let inner_w = crate::ui::side_panel::inner_width(attach_w);
                self.attach_list.set_size(inner_w, list_h);
                // Position buttons below the list
                let btn_y = (8 + list_h + 8) as i32;
                self.attach_btn_add.set_position(8, btn_y);
                self.attach_btn_open.set_position(70, btn_y);
                self.attach_btn_save.set_position(132, btn_y);
                self.attach_btn_delete.set_position(194, btn_y);
                // Status label below buttons
                self.attach_lbl_status.set_position(8, btn_y + 30);
                self.attach_lbl_status.set_size(inner_w, 18);
            } else {
                self.attach_frame.set_visible(false);
            }
        }

        // Position the prompts panel. Only one panel is open at a time
        // (close_other_panels): it sits against the right edge, the grip on
        // its left.
        {
            if self.prompts_panel.is_visible() {
                let prompts_x = crate::ui::side_panel::frame_x(w, prompts_w);
                let prompts_h = (h as i32) - (status_h as i32) - (tab_bar_h as i32);
                let prompts_h = if prompts_h < 0 { 0 } else { prompts_h as u32 };
                self.prompts_frame.set_position(prompts_x, tab_bar_h as i32);
                self.prompts_frame.set_size(prompts_w, prompts_h);
                self.prompts_frame.set_visible(true);
                // Search on top, then the list and the preview sharing the
                // height (list 55 %), the buttons and the status at the bottom.
                let inner_w = crate::ui::side_panel::inner_width(prompts_w);
                self.prompts_search.set_position(8, 8);
                self.prompts_search.set_size(inner_w, 24);
                let room = (prompts_h as i32) - 40 - 6 - 8 - 26 - 4 - 18 - 8;
                let list_h = (room * 55 / 100).max(50) as u32;
                let preview_h = (room - list_h as i32).max(50) as u32;
                self.prompts_list.set_position(8, 40);
                self.prompts_list.set_size(inner_w, list_h);
                let preview_y = (40 + list_h + 6) as i32;
                self.prompts_preview.set_position(8, preview_y);
                self.prompts_preview.set_size(inner_w, preview_h);
                let btn_y = preview_y + preview_h as i32 + 8;
                let buttons = [
                    &self.prompts_btn_add,
                    &self.prompts_btn_edit,
                    &self.prompts_btn_delete,
                    &self.prompts_btn_copy,
                    &self.prompts_btn_insert,
                ];
                let btn_w = crate::ui::prompts_panel::button_width(inner_w, buttons.len() as u32);
                for (i, btn) in buttons.iter().enumerate() {
                    btn.set_position(8 + i as i32 * (btn_w + crate::ui::prompts_panel::BUTTON_GAP) as i32, btn_y);
                    btn.set_size(btn_w, 26);
                }
                self.prompts_status.set_position(8, btn_y + 30);
                self.prompts_status.set_size(inner_w, 18);
            } else {
                self.prompts_frame.set_visible(false);
            }
        }

        // Position the grip on the left edge of the open panel
        {
            let panels_h = (h as i32) - (status_h as i32) - (tab_bar_h as i32);
            // Only one panel is open: its width is the sum minus the grip.
            let open_w = right_panel_w - grip_w;
            let at = (grip_w > 0).then(|| (crate::ui::side_panel::grip_x(w, open_w), tab_bar_h as i32, panels_h.max(0)));
            self.place_panel_grip(at);
        }

        // Position tab bar
        {
            let tb_hwnd = *self.tab_bar_hwnd.borrow();
            if tb_hwnd != 0 {
                unsafe {
                    let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowPos(
                        windows::Win32::Foundation::HWND(tb_hwnd as *mut _),
                        None,
                        0, 0, w as i32, tab_bar_h as i32,
                        windows::Win32::UI::WindowsAndMessaging::SWP_NOZORDER,
                    );
                }
            }
        }

        let (editor_x, editor_w) = editor_columns(w, right_panel_w, gutter_w);
        let editor_y = (search_h + tab_bar_h) as i32;
        let editor_h = (h as i32) - (status_h as i32) - (search_h as i32) - (tab_bar_h as i32);
        let editor_h = if editor_h < 0 { 0 } else { editor_h as u32 };

        // Position gutter panel (hidden with the line numbers, issue #26)
        let gutter = *self.gutter_hwnd.borrow();
        if gutter != 0 {
            use windows::Win32::UI::WindowsAndMessaging::{SWP_HIDEWINDOW, SWP_NOZORDER, SWP_SHOWWINDOW};
            let visibility = if show_line_numbers { SWP_SHOWWINDOW } else { SWP_HIDEWINDOW };
            unsafe {
                if let Err(e) = windows::Win32::UI::WindowsAndMessaging::SetWindowPos(
                    windows::Win32::Foundation::HWND(gutter as *mut _),
                    None,
                    0, editor_y, gutter_w as i32, editor_h as i32,
                    SWP_NOZORDER | visibility,
                ) {
                    eprintln!("WARN layout: gutter SetWindowPos failed show={} err={:?}", show_line_numbers, e);
                }
            }
        }

        self.editor.set_position(editor_x, editor_y);
        self.editor.set_size(editor_w, editor_h);

        self.paint_gutter();
        self.paint_tab_bar();
    }
}
