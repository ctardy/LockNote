// Vault panel operations.
// Part of the EditorForm implementation, split out of the former editor.rs.

#[allow(unused_imports)]
use super::*;

impl EditorForm {
    // =====================================================================
    // Vault operations
    // =====================================================================

    /// Toggle the vault panel visibility.
    /// Force repaint of all toolbar toggle buttons (to update active state colors).
    pub(super) fn repaint_toolbar_buttons(&self) {
        let repaint = |btn: &nwg::Button| {
            if let nwg::ControlHandle::Hwnd(hwnd) = btn.handle {
                unsafe {
                    let hwnd_w = windows::Win32::Foundation::HWND(hwnd as *mut _);
                    let _ = windows::Win32::Graphics::Gdi::InvalidateRect(Some(hwnd_w), None, true);
                }
            }
        };
        repaint(&self.toolbar_prompts);
        repaint(&self.toolbar_vault);
        repaint(&self.toolbar_totp);
        repaint(&self.toolbar_attach);
        repaint(&self.toolbar_lock);
    }

    /// Close all tool panels except the one identified by `keep`.
    pub(super) fn close_other_panels(&self, keep: &str) {
        if keep != "vault" && self.vault_panel.is_visible() {
            self.vault_panel.set_visible(false);
            self.menu_tools_vault.set_checked(false);
            self.vault_timer.stop();
            self.vault_panel.hide_reveal();
        }
        if keep != "totp" && self.totp_panel.is_visible() {
            self.totp_panel.set_visible(false);
            self.menu_tools_totp.set_checked(false);
            self.totp_timer.stop();
        }
        if keep != "attachments" && self.attach_panel.is_visible() {
            self.attach_panel.set_visible(false);
            self.menu_tools_attachments.set_checked(false);
        }
        if keep != "prompts" && self.prompts_panel.is_visible() {
            self.prompts_panel.set_visible(false);
            self.menu_tools_prompts.set_checked(false);
        }
    }

    pub(super) fn on_toggle_vault(&self) {
        self.vault_panel.toggle_visible();
        let visible = self.vault_panel.is_visible();
        self.menu_tools_vault.set_checked(visible);
        if visible {
            self.close_other_panels("vault");
            self.refresh_vault_tree(None);
            self.vault_timer.start();
        } else {
            self.vault_timer.stop();
            self.hide_vault_reveal();
        }
        self.layout_controls();
        self.repaint_toolbar_buttons();
    }

    /// Colour the vault tree with the current palette. WM_CTLCOLOR* does not
    /// reach a TreeView, so it is coloured directly, at build and on every
    /// theme switch.
    pub(super) fn apply_vault_tree_colors(&self) {
        let pal = crate::theme::current();
        self.vault_tree.set_text_color(pal.text_primary.r, pal.text_primary.g, pal.text_primary.b);
        if let nwg::ControlHandle::Hwnd(h) = self.vault_tree.handle {
            use windows::Win32::UI::Controls::TVM_SETBKCOLOR;
            unsafe {
                windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                    windows::Win32::Foundation::HWND(h as *mut _),
                    TVM_SETBKCOLOR,
                    Some(windows::Win32::Foundation::WPARAM(0)),
                    Some(windows::Win32::Foundation::LPARAM(pal.background.to_colorref() as isize)),
                );
            }
        }
    }

    /// Apply the current theme to the vault panel frame (bound once).
    pub(super) fn theme_vault_panel(&self) {
        self.apply_vault_tree_colors();

        // The palette is read on every message (and the brush comes from the
        // colour-keyed cache), so a theme switch needs no re-binding.
        let handler = nwg::bind_raw_event_handler(
            &self.vault_frame.handle,
            0x40002,
            move |_hwnd, msg, wparam, _lparam| {
                let pal = crate::theme::current();
                let bg_colorref = pal.background.to_colorref();
                let text_colorref = pal.text_primary.to_colorref();
                let bg_brush = windows::Win32::Graphics::Gdi::HBRUSH(
                    crate::ui::lock_card::cached_brush(pal.background) as *mut _,
                );
                match msg {
                    // WM_CTLCOLORSTATIC | WM_CTLCOLORLISTBOX | WM_CTLCOLOREDIT (search field)
                    0x0138 | 0x0134 | 0x0133 => {
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

    /// Rebuild the vault tree from the panel state (issue 048). Categories
    /// are always listed (even empty) and expanded; leaves carry the storage
    /// index in their lParam and show the name only. `select` re-selects a
    /// secret by storage index — never through an HTREEITEM kept from a
    /// previous build (F28). Any revealed value is hidden first.
    pub(super) fn refresh_vault_tree(&self, select: Option<usize>) {
        use crate::ui::vault_panel as vp;
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        use windows::Win32::UI::Controls::{TVGN_CARET, TVM_SELECTITEM};
        use windows::Win32::UI::WindowsAndMessaging::SendMessageW;

        self.hide_vault_reveal();
        // Collect everything before touching the TreeView: its
        // notifications are synchronous, no borrow may be held (F30).
        let secrets = self.vault_panel.get_secrets();
        let categories = self.vault_categories();
        // A filter only: the display text (a half emoji is U+FFFD).
        let groups = match crate::ui::field_text::read_display(self.vault_search.handle) {
            Ok(query) => vp::tree_groups(&secrets, &categories, vp::compare_names_user_locale)
                .and_then(|g| vp::filter_groups(g, &secrets, &query, &vp::contains_name_user_locale))
                .map_err(|e| format!("{:?}", e)),
            Err(e) => Err(format!("search field unreadable {:?}", e)),
        };

        let Some(hwnd) = (match self.vault_tree.handle {
            nwg::ControlHandle::Hwnd(h) => Some(windows::Win32::Foundation::HWND(h as *mut _)),
            _ => None,
        }) else {
            eprintln!("vault: tree has no HWND");
            return;
        };

        self.suppress_tree_events.set(true);
        unsafe { SendMessageW(hwnd, 0x000B, Some(WPARAM(0)), Some(LPARAM(0))) }; // WM_SETREDRAW(FALSE)
        self.vault_tree.clear();
        match groups {
            Ok(groups) => {
                let mut to_select: Option<isize> = None;
                for group in groups {
                    let node = self.vault_tree.insert_item_with_param(
                        &group.name,
                        None,
                        nwg::TreeInsert::Last,
                        vp::encode_category_param(group.category_index),
                    );
                    for i in group.secrets {
                        let leaf = self.vault_tree.insert_item_with_param(
                            &vp::leaf_label(&secrets[i]),
                            Some(&node),
                            nwg::TreeInsert::Last,
                            vp::encode_leaf_param(i),
                        );
                        if select == Some(i) {
                            to_select = Some(leaf.handle as isize);
                        }
                    }
                    self.vault_tree.set_expand_state(&node, nwg::ExpandState::Expand);
                }
                if let Some(item) = to_select {
                    unsafe {
                        SendMessageW(hwnd, TVM_SELECTITEM, Some(WPARAM(TVGN_CARET as usize)), Some(LPARAM(item)))
                    };
                }
            }
            Err(e) => {
                // Never an empty tree without explanation (F33).
                eprintln!("vault: tree build failed err={}", e);
                self.vault_tree.insert_item_with_param(
                    &t("vault.tree.error"),
                    None,
                    nwg::TreeInsert::Last,
                    0,
                );
            }
        }
        unsafe { SendMessageW(hwnd, 0x000B, Some(WPARAM(1)), Some(LPARAM(0))) }; // WM_SETREDRAW(TRUE)
        self.vault_tree.invalidate();
        self.suppress_tree_events.set(false);
        self.update_vault_buttons();
    }

    /// Rebuild the tree and select the secret `index` just added or edited.
    /// When the search would hide it, the search is cleared first (without
    /// the rebuild its change triggers): a saved secret never vanishes.
    pub(super) fn show_vault_secret(&self, index: usize) {
        use crate::ui::vault_panel as vp;
        if let Some(entry) = self.vault_panel.get_secret(index) {
            // The rebuild below meets the same failures and shows them.
            match crate::ui::field_text::read_display(self.vault_search.handle) {
                Ok(query) => match vp::search_hides(&entry.name, &query, &vp::contains_name_user_locale) {
                    Ok(true) => self.clear_vault_search(),
                    Ok(false) => {}
                    Err(e) => eprintln!("vault: search check failed err={:?}", e),
                },
                Err(e) => eprintln!("vault: search field unreadable err={:?}", e),
            }
        }
        self.refresh_vault_tree(Some(index));
    }

    /// Empty the search field without the rebuild its change would trigger
    /// (the caller rebuilds, or empties the tree).
    pub(super) fn clear_vault_search(&self) {
        self.suppress_tree_events.set(true);
        self.vault_search.set_text("");
        self.suppress_tree_events.set(false);
    }

    /// What is selected in the vault tree (TVGN_CARET, single selection).
    pub(super) fn vault_tree_selection(&self) -> Result<crate::ui::vault_panel::TreeSel, crate::ui::vault_panel::VaultTreeError> {
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        use windows::Win32::UI::Controls::{TVGN_CARET, TVM_GETNEXTITEM};
        use windows::Win32::UI::WindowsAndMessaging::SendMessageW;

        let nwg::ControlHandle::Hwnd(h) = self.vault_tree.handle else {
            return Err(crate::ui::vault_panel::VaultTreeError::MissingParam);
        };
        let hwnd = windows::Win32::Foundation::HWND(h as *mut _);
        let item = unsafe {
            SendMessageW(hwnd, TVM_GETNEXTITEM, Some(WPARAM(TVGN_CARET as usize)), Some(LPARAM(0)))
        }
        .0;
        self.vault_tree_item(item)
    }

    /// What a given tree item is (0 = no item).
    pub(super) fn vault_tree_item(&self, item: isize) -> Result<crate::ui::vault_panel::TreeSel, crate::ui::vault_panel::VaultTreeError> {
        use crate::ui::vault_panel::{decode_tree_param, TreeSel};
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        use windows::Win32::UI::Controls::{HTREEITEM, TVIF_HANDLE, TVIF_PARAM, TVITEMW, TVM_GETITEMW};
        use windows::Win32::UI::WindowsAndMessaging::SendMessageW;

        if item == 0 {
            return Ok(TreeSel::Nothing);
        }
        let nwg::ControlHandle::Hwnd(h) = self.vault_tree.handle else {
            return Err(crate::ui::vault_panel::VaultTreeError::MissingParam);
        };
        let hwnd = windows::Win32::Foundation::HWND(h as *mut _);
        let mut tv = TVITEMW { mask: TVIF_HANDLE | TVIF_PARAM, hItem: HTREEITEM(item), ..Default::default() };
        let ok = unsafe {
            SendMessageW(hwnd, TVM_GETITEMW, Some(WPARAM(0)), Some(LPARAM(&mut tv as *mut TVITEMW as isize)))
        }
        .0;
        decode_tree_param(
            (ok != 0).then_some(tv.lParam.0),
            self.vault_panel.count(),
            self.vault_categories().len(),
        )
    }

    /// Storage index of the selected secret, `None` for a category, no
    /// selection or an unreadable selection (logged, action refused).
    pub(super) fn selected_vault_secret(&self) -> Option<usize> {
        match self.vault_tree_selection() {
            Ok(sel) => crate::ui::vault_panel::secret_index(&sel),
            Err(e) => {
                eprintln!("vault: selection unreadable err={:?}", e);
                None
            }
        }
    }

    /// Enable the buttons that make sense for the current selection (A2).
    pub(super) fn update_vault_buttons(&self) {
        let sel = self.vault_tree_selection().unwrap_or_else(|e| {
            eprintln!("vault: selection unreadable err={:?}", e);
            crate::ui::vault_panel::TreeSel::Nothing
        });
        let b = crate::ui::vault_panel::button_states(&sel);
        self.vault_btn_add.set_enabled(b.add);
        self.vault_btn_edit.set_enabled(b.edit);
        self.vault_btn_delete.set_enabled(b.delete);
        self.vault_btn_copy.set_enabled(b.copy);
        self.vault_btn_reveal.set_enabled(b.reveal);
    }

    /// Vault categories in use (user list or the 7 defaults, issue 052).
    pub(super) fn vault_categories(&self) -> Vec<String> {
        crate::features::vault::categories::effective(&self.settings.borrow().vault_categories)
    }

    /// Clear and hide the revealed value.
    pub(super) fn hide_vault_reveal(&self) {
        self.vault_panel.hide_reveal();
        self.vault_reveal.set_text("");
        self.vault_reveal.set_visible(false);
    }

    /// Right click in the vault tree (issue 052): New category everywhere;
    /// Rename / Delete on a category node (greyed for General).
    pub(super) fn on_vault_tree_context_menu(&self) {
        use crate::ui::vault_panel::TreeSel;
        use windows::Win32::Foundation::{LPARAM, POINT, WPARAM};
        use windows::Win32::Graphics::Gdi::ScreenToClient;
        use windows::Win32::UI::Controls::{TVGN_CARET, TVHITTESTINFO, TVM_HITTEST, TVM_SELECTITEM};
        use windows::Win32::UI::WindowsAndMessaging::*;

        let nwg::ControlHandle::Hwnd(h) = self.vault_tree.handle else { return };
        let tree = windows::Win32::Foundation::HWND(h as *mut _);

        // Select the item under the cursor so the menu acts on what the
        // user right-clicked.
        let mut screen = POINT::default();
        if unsafe { GetCursorPos(&mut screen) }.is_err() {
            eprintln!("vault: GetCursorPos failed");
            return;
        }
        let mut hit = TVHITTESTINFO { pt: screen, ..Default::default() };
        let _ = unsafe { ScreenToClient(tree, &mut hit.pt) };
        let item = unsafe {
            SendMessageW(tree, TVM_HITTEST, Some(WPARAM(0)), Some(LPARAM(&mut hit as *mut TVHITTESTINFO as isize)))
        }
        .0;
        if item != 0 {
            unsafe { SendMessageW(tree, TVM_SELECTITEM, Some(WPARAM(TVGN_CARET as usize)), Some(LPARAM(item))) };
        }

        let categories = self.vault_categories();
        let target: Option<String> = match self.vault_tree_selection() {
            Ok(TreeSel::Category(k)) => categories.get(k).cloned(),
            Ok(_) => None,
            Err(e) => {
                eprintln!("vault: selection unreadable err={:?}", e);
                None
            }
        };

        const CMD_NEW: usize = 1;
        const CMD_RENAME: usize = 2;
        const CMD_DELETE: usize = 3;
        let wide = |s: String| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
        let new_label = wide(t("vault.cat.menu.new"));
        let rename_label = wide(t("vault.cat.menu.rename"));
        let delete_label = wide(t("vault.cat.menu.delete"));

        let owner = match self.window.handle {
            nwg::ControlHandle::Hwnd(w) => windows::Win32::Foundation::HWND(w as *mut _),
            _ => return,
        };
        let cmd = unsafe {
            let Ok(menu) = CreatePopupMenu() else {
                eprintln!("vault: CreatePopupMenu failed");
                return;
            };
            let _ = AppendMenuW(menu, MF_STRING, CMD_NEW, windows::core::PCWSTR(new_label.as_ptr()));
            if let Some(name) = &target {
                let flags = if name == crate::features::vault::categories::GENERAL { MF_STRING | MF_GRAYED } else { MF_STRING };
                let _ = AppendMenuW(menu, MF_SEPARATOR, 0, windows::core::PCWSTR::null());
                let _ = AppendMenuW(menu, flags, CMD_RENAME, windows::core::PCWSTR(rename_label.as_ptr()));
                let _ = AppendMenuW(menu, flags, CMD_DELETE, windows::core::PCWSTR(delete_label.as_ptr()));
            }
            let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON, screen.x, screen.y, None, owner, None);
            let _ = DestroyMenu(menu);
            cmd.0 as usize
        };

        // The auto-lock timer runs inside TrackPopupMenu (issue 049).
        if *self.is_locked.borrow() {
            return;
        }
        match (cmd, target) {
            (CMD_NEW, _) => self.on_vault_category_new(),
            (CMD_RENAME, Some(name)) => self.on_vault_category_rename(&name),
            (CMD_DELETE, Some(name)) => self.on_vault_category_delete(&name),
            _ => {}
        }
    }

    /// Store a changed category list, refresh the tree, mark for saving. The
    /// search is cleared: a new or renamed category holds no matching secret
    /// and would not show.
    pub(super) fn apply_vault_categories(&self, list: Vec<String>) {
        self.settings.borrow_mut().vault_categories = list;
        self.clear_vault_search();
        self.refresh_vault_tree(None);
        self.mark_modified();
    }

    pub(super) fn show_vault_category_error(&self, e: &crate::features::vault::categories::CategoryError) {
        eprintln!("vault: category refused err={:?}", e);
        self.inform(&t("vault.cat.error.title"),
            &crate::ui::vault_panel::category_error_message(e),
        );
    }

    pub(super) fn on_vault_category_new(&self) {
        let Some(name) = crate::ui::dialogs::rename_tab::RenameTabDialog::show_with_labels(
            "",
            &t("vault.cat.new.title"),
            &t("vault.cat.name"),
        ) else {
            return;
        };
        if *self.is_locked.borrow() {
            return;
        }
        let mut list = self.vault_categories();
        match crate::features::vault::categories::add(&mut list, &name, &crate::ui::vault_panel::same_category_name) {
            Ok(_) => self.apply_vault_categories(list),
            Err(e) => self.show_vault_category_error(&e),
        }
    }

    pub(super) fn on_vault_category_rename(&self, old: &str) {
        let epoch = self.lock_epoch();
        let Some(new) = crate::ui::dialogs::rename_tab::RenameTabDialog::show_with_labels(
            old,
            &t("vault.cat.rename.title"),
            &t("vault.cat.name"),
        ) else {
            return;
        };
        if !self.still_valid(epoch) {
            return; // locked or curtain while the dialog was open (#30)
        }
        let mut list = self.vault_categories();
        let mut secrets = self.vault_panel.get_secrets();
        match crate::features::vault::categories::rename(&mut list, &mut secrets, old, &new, &crate::ui::vault_panel::same_category_name) {
            Ok(_) => {
                self.vault_panel.set_secrets(secrets);
                self.apply_vault_categories(list);
            }
            Err(e) => self.show_vault_category_error(&e),
        }
    }

    pub(super) fn on_vault_category_delete(&self, name: &str) {
        let count = self.vault_panel.get_secrets().iter().filter(|s| s.category == name).count();
        let content = t("vault.cat.delete.confirm")
            .replace("{name}", name)
            .replace("{count}", &count.to_string());
        let title = t("vault.cat.delete.title");
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
            return;
        }
        let mut list = self.vault_categories();
        let mut secrets = self.vault_panel.get_secrets();
        match crate::features::vault::categories::delete(&mut list, &mut secrets, name) {
            Ok(_) => {
                self.vault_panel.set_secrets(secrets);
                self.apply_vault_categories(list);
            }
            Err(e) => self.show_vault_category_error(&e),
        }
    }

    /// Tree item under a point in tree client coordinates (0 = none).
    pub(super) fn vault_tree_hit(&self, pt: windows::Win32::Foundation::POINT) -> isize {
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        use windows::Win32::UI::Controls::{TVHITTESTINFO, TVM_HITTEST};
        let nwg::ControlHandle::Hwnd(h) = self.vault_tree.handle else { return 0 };
        let mut hit = TVHITTESTINFO { pt, ..Default::default() };
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                windows::Win32::Foundation::HWND(h as *mut _),
                TVM_HITTEST,
                Some(WPARAM(0)),
                Some(LPARAM(&mut hit as *mut TVHITTESTINFO as isize)),
            )
        }
        .0
    }

    /// Drop-target highlight (TVGN_DROPHILITE); 0 clears it.
    pub(super) fn vault_drag_highlight(&self, item: isize) {
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        use windows::Win32::UI::Controls::{TVGN_DROPHILITE, TVM_SELECTITEM};
        let nwg::ControlHandle::Hwnd(h) = self.vault_tree.handle else { return };
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                windows::Win32::Foundation::HWND(h as *mut _),
                TVM_SELECTITEM,
                Some(WPARAM(TVGN_DROPHILITE as usize)),
                Some(LPARAM(item)),
            )
        };
    }

    /// Start dragging a secret (categories are not draggable).
    pub(super) fn on_vault_drag_start(&self, item: isize) {
        if *self.is_locked.borrow() {
            return;
        }
        match self.vault_tree_item(item) {
            Ok(crate::ui::vault_panel::TreeSel::Leaf(i)) => {
                self.vault_drag.set(Some(i));
                if let nwg::ControlHandle::Hwnd(h) = self.vault_tree.handle {
                    unsafe {
                        windows::Win32::UI::Input::KeyboardAndMouse::SetCapture(
                            windows::Win32::Foundation::HWND(h as *mut _),
                        )
                    };
                }
            }
            Ok(_) => {}
            Err(e) => eprintln!("vault: drag start unreadable err={:?}", e),
        }
    }

    /// End (or cancel) a drag: clear state and highlight, release capture.
    /// Returns the dragged secret, if a drag was active.
    pub(super) fn vault_drag_end(&self) -> Option<usize> {
        // Clear first: ReleaseCapture sends WM_CAPTURECHANGED synchronously.
        let dragged = self.vault_drag.take()?;
        self.vault_drag_highlight(0);
        let _ = unsafe { windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture() };
        Some(dragged)
    }

    /// Drop the dragged secret on `item`: move it to that category (or to
    /// the category of the secret under the cursor).
    pub(super) fn on_vault_drop(&self, item: isize) {
        let Some(index) = self.vault_drag_end() else { return };
        if *self.is_locked.borrow() {
            return;
        }
        let target = match self.vault_tree_item(item) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("vault: drop target unreadable err={:?}", e);
                return;
            }
        };
        let categories = self.vault_categories();
        let mut secrets = self.vault_panel.get_secrets();
        let Some(k) = crate::ui::vault_panel::drop_category(&target, &secrets, &categories) else { return };
        match crate::ui::vault_panel::move_secret(&mut secrets, index, &categories[k]) {
            Ok(true) => {
                self.vault_panel.set_secrets(secrets);
                self.refresh_vault_tree(Some(index));
                self.mark_modified();
            }
            Ok(false) => {}
            Err(e) => eprintln!("vault: drop refused err={:?}", e),
        }
    }

    /// Start the clipboard auto-clear timer for a value copied from the
    /// secret dialog (the dialog itself has no access to the guard).
    /// `copied` is a `String`, or a `Zeroizing<String>` when it holds a
    /// secret (the 2FA setup, issue #38): it is dropped here, wiped then.
    pub(super) fn arm_clipboard_guard<S: AsRef<str>>(&self, copied: Option<S>) {
        if let Some(text) = copied {
            self.clipboard_guard.borrow_mut().on_copy(text.as_ref());
        }
    }

    // =====================================================================
    // Password generator
    // =====================================================================

    /// Tools > Password Generator.
    pub(super) fn on_password_generator(&self) {
        let start = self.settings.borrow().passgen.clone();
        let outcome = crate::ui::dialogs::password_generator::PasswordGeneratorDialog::show(&start, false, None);
        self.arm_clipboard_guard(outcome.copied);
        self.store_passgen_options(outcome.options);
    }

    /// Keep the generator options a dialog ended with: they are part of the
    /// settings, so the note becomes modified when they changed.
    pub(super) fn store_passgen_options(&self, options: crate::features::passgen::Options) {
        // Locked while the dialog was open (issue 049): the settings are
        // read again from the file at unlock, nothing to keep here.
        if *self.is_locked.borrow() {
            return;
        }
        if self.settings.borrow().passgen == options {
            return;
        }
        self.settings.borrow_mut().passgen = options;
        self.mark_modified();
    }
}
