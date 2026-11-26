// Auto-lock, lock overlay and re-authentication.
// Part of the EditorForm implementation, split out of the former editor.rs.

#[allow(unused_imports)]
use super::*;
use crate::ui::lock_card::Tone;

impl EditorForm {
    // =====================================================================
    // Re-authenticate after auto-lock
    // =====================================================================

    /// Take the encrypted data of this session (its last save: storage keeps
    /// it, whatever the files say now) and prompt for the password.
    /// On success, restores the editor content and password. Returns true on success.
    pub(super) fn unlock_from_disk(&self) -> bool {
        let raw_payload = match crate::storage::read_session_data(&self.exe_path) {
            Ok(Some(data)) => data,
            Ok(None) => {
                eprintln!("ERROR unlock: the session holds no notes exe={}", self.exe_path.display());
                return false;
            }
            Err(e) => {
                eprintln!("ERROR unlock: {} exe={}", e, self.exe_path.display());
                nwg::modal_info_message(&self.window, "LockNote", &e);
                return false;
            }
        };
        let (_hint, encrypted) = crate::storage::split_hint_and_encrypted(&raw_payload);
        let parent = if let nwg::ControlHandle::Hwnd(h) = self.window.handle { h as isize } else { 0 };
        match crate::ui::dialogs::unlock::show_with_parent(&encrypted, parent) {
            Some((password, decrypted_text)) => {
                let decrypted_text = crate::features::compression::decompress(&decrypted_text);
                let (settings, note_text) = crate::settings::Settings::parse(&decrypted_text);

                // Restore password
                *self.password.borrow_mut() = password;

                // Settings from file first: the payload parse needs them
                // (vault categories, issue 052). The side panel widths of
                // this session stay, saved or not.
                let mut settings = settings;
                crate::ui::side_panel::keep_machine_panel_widths(&self.settings.borrow(), &mut settings);
                *self.settings.borrow_mut() = settings;

                // Restore every block + the session (decryption succeeded,
                // so the state captured at lock time can be consumed).
                let (note_text, cursor_line) =
                    self.load_payload(&note_text, self.locked_session_state.take());

                // Restore editor content
                self.editor.set_text(&note_text);
                self.mark_clean();
                self.update_title();
                self.update_status_bar();
                self.layout_controls();

                self.paint_tab_bar();

                // Caret last, once the layout is final.
                if let Some(line) = cursor_line {
                    self.apply_cursor_line(line);
                }

                // Reset auto-lock timer
                self.autolock.borrow_mut().reset();

                // Keys a failed removal at lock time left in ssh-agent.
                self.unload_agent_keys(true);

                true
            }
            None => false,
        }
    }

    // =====================================================================
    // Auto-lock
    // =====================================================================

    pub(super) fn on_autolock(&self) {
        // Serialize against unlock / autosave / manual save (fix #036).
        // Take the global persistence lock for the whole lock sequence:
        // save-if-dirty + clear editor + zeroize password.
        let _persistence_guard = crate::features::persistence_lock::acquire();

        // Save first. Pass 5: a failed save no longer leads to a wipe of the
        // unsaved changes (data loss): the curtain hides the note, the
        // reason is shown, and once the user closes it the note is shown
        // again, still unsaved, the inactivity delay restarted. Nothing else
        // of the lock is done.
        let modified = self.is_modified.load(Ordering::Relaxed);
        let saved = if modified { self.save_note() } else { Ok(()) };
        if !lock_proceeds(modified, saved.is_ok()) {
            if let Err(f) = saved {
                eprintln!("WARN lock: save before the lock failed, the note stays open (hidden while the reason is shown)");
                self.lock_in_progress.set(true);
                self.show_curtain();
                self.inform(&t("save.error.title"), &lock_save_failed_text(f.message()));
                self.hide_curtain();
                self.lock_in_progress.set(false);
                self.autolock.borrow_mut().reset();
                return;
            }
        }
        // Every dialog result from before this lock is now stale (pass 4).
        self.lock_epoch.set(self.lock_epoch.get() + 1);

        // Take this file's SSH keys out of ssh-agent before anything else.
        // No modal (the lock must not block, issue 049): failures are logged
        // and the keys stay listed for the next unlock.
        self.unload_agent_keys(false);
        // Remember where the user was (tab + caret line) before the editor
        // is cleared. The disk copy may be stale (tab switches are not a
        // modification, and the save above may have failed), so unlock
        // prefers this in-memory state.
        self.locked_session_state.set(Some(self.capture_session_state()));
        self.wipe_plaintext_stores();

        // Mark clean so auto-save won't fire with an empty password
        self.mark_clean();

        // Show lock overlay — unlock popup will appear on focus (WM_ACTIVATE) or click
        *self.is_locked.borrow_mut() = true;
        self.set_menus_for_lock(true);
        // A curtain shown before becomes the real lock screen.
        self.curtain.set(false);
        self.show_lock_overlay();
    }

    /// A store could not be wiped now: retried on the next highlight tick
    /// while locked or exiting.
    pub(super) fn defer_wipe(&self, what: &str) {
        eprintln!("WARN wipe: {} borrowed further up, wipe deferred to the next tick", what);
        self.wipe_pending.set(true);
    }

    /// The deferred wipe of the next tick: only while the note is locked or
    /// the session ended (pass 4: after an unlock it would wipe the restored
    /// notes).
    pub(super) fn retry_pending_wipe(&self) {
        if !wipe_retry_due(self.wipe_pending.get(), *self.is_locked.borrow(), self.exiting.get()) {
            if self.wipe_pending.get() {
                self.wipe_pending.set(false);
            }
            return;
        }
        self.wipe_pending.set(false);
        eprintln!("INFO wipe: deferred wipe retried");
        self.wipe_plaintext_stores();
    }

    /// The editor text, the password and every other plaintext store: used
    /// by the lock and by the end of the Windows session.
    pub(super) fn wipe_plaintext_stores(&self) {
        // Clear sensitive data from editor
        self.editor.set_text("");
        self.wipe_password();

        // Wipe every other in-memory plaintext store so a memory dump taken
        // after the overlay appears does not still contain notes
        // or attachment bytes. The UI structures stay (names, colors, sha256)
        // so restoration after unlock works — only the secret payloads are
        // zeroed. Restoration re-reads from disk via unlock_from_disk().
        // Never a panic on a store still borrowed further up the stack
        // (pass 4: the release build aborts): what cannot be wiped now is
        // retried on the next tick (`wipe_pending`).
        match self.tab_bar.try_borrow() {
            Ok(tb) => tb.wipe_contents(),
            Err(_) => self.defer_wipe("tabs"),
        }
        match self.attachments.try_borrow_mut() {
            Ok(mut atts) => {
                for a in atts.iter_mut() {
                    a.wipe();
                }
                atts.clear();
            }
            Err(_) => self.defer_wipe("attachments"),
        }
        // TOTP secrets too (pass 4): the unlock reads them back with the
        // settings from the file.
        self.wipe_totp_secrets();
        // Vault: hide any revealed value, drop the secrets (SecretEntry is
        // ZeroizeOnDrop) and empty the tree. Unlock reloads from disk.
        {
            self.hide_vault_reveal();
            self.vault_panel.set_secrets(Vec::new());
            self.clear_vault_search();
            self.suppress_tree_events.set(true);
            self.vault_tree.clear();
            self.suppress_tree_events.set(false);
        }
        // Prompts: same as the vault (issue 058).
        self.clear_prompts_for_lock();
    }

    /// Manual lock (Ctrl+L or toolbar button). Reuses the auto-lock sequence;
    /// the inline overlay is now visible with the password field focused, so
    /// there is no separate popup to dispatch.
    pub(super) fn on_manual_lock(&self) {
        if *self.is_locked.borrow() {
            return;
        }
        // Under a dialog the toolbar stays clickable: as the auto-lock, the
        // open dialogs are cancelled first (owner decision on #30).
        self.request_lock(true);
    }

    /// Grey out every menu command while locked (pass 3): only File > Quit
    /// and Help > About stay; the menu handler ignores the others anyway.
    pub(super) fn set_menus_for_lock(&self, locked: bool) {
        let enabled = !locked;
        // Whole menus.
        self.menu_edit.set_enabled(enabled);
        self.menu_view.set_enabled(enabled);
        self.menu_tab.set_enabled(enabled);
        self.menu_tools.set_enabled(enabled);
        self.menu_file_settings.set_enabled(enabled);
        // File and Help items, Quit and About excepted.
        self.menu_file_save.set_enabled(enabled);
        self.menu_file_print.set_enabled(enabled);
        self.menu_file_export.set_enabled(enabled);
        self.menu_file_backup_export.set_enabled(enabled);
        self.menu_file_backup_import.set_enabled(enabled);
        self.menu_file_import_exe.set_enabled(enabled);
        self.menu_file_backups.set_enabled(enabled);
        self.menu_help_updates.set_enabled(enabled);
        if let Some(h) = self.window.handle.hwnd() {
            unsafe {
                if let Err(e) = windows::Win32::UI::WindowsAndMessaging::DrawMenuBar(windows::Win32::Foundation::HWND(h as *mut _)) {
                    eprintln!("WARN menu: DrawMenuBar failed err={:?}", e);
                }
            }
        }
    }

    /// Show the lock overlay, covering the entire window with a themed curtain.
    /// The overlay contains an inline password field; entering the correct
    /// password unlocks the editor in place — no modal popup.
    pub(super) fn show_lock_overlay(&self) {
        // Reset inline unlock state: clear any stale text, cancel backoff,
        // re-enable input. Attempts counter is persisted across a cancel
        // sequence but we only start fresh when the overlay is (re)shown.
        self.lock_input.set_text("");
        self.lock_input.set_enabled(true);
        self.lock_btn.set_enabled(true);
        self.lock_backoff_timer.stop();
        self.lock_backoff_remaining.set(0);
        *self.lock_attempts.borrow_mut() = 0;

        // Show the interactive controls
        self.lock_input.set_visible(true);
        self.lock_eye.set_visible(true);
        self.lock_btn.set_visible(true);
        self.lock_status.set_visible(true);
        self.lock_status_msg.clear();
        // The typed text is never left visible from a previous session.
        crate::ui::lock_card::set_eye_names(
            &t("unlock.show"),
            &t("unlock.hide"),
        );
        crate::ui::lock_card::hide_password(&lock_overlay_hwnds());
        self.lock_status_msg.set_caps_text(&t("unlock.capslock"));

        // Refresh i18n-sensitive texts each show so a language change while
        // the app is running takes effect the next time the overlay appears.
        // (Colour handlers are installed once in `bind_events()` — see
        // `install_lock_overlay_theme`.)
        self.lock_label.set_text(&t("lock.overlay.title"));
        self.lock_subtitle.set_text(&t("unlock.desc"));
        self.lock_btn.set_text(&t("unlock.btn"));

        // Force a full repaint so the WM_ERASEBKGND handler re-fills with
        // the current theme colours (handles theme switches while locked).
        if let nwg::ControlHandle::Hwnd(h) = self.lock_frame.handle {
            unsafe {
                let _ = windows::Win32::Graphics::Gdi::InvalidateRect(
                    Some(windows::Win32::Foundation::HWND(h as *mut _)),
                    None,
                    true,
                );
            }
        }

        // Cover the full client area AND raise to top of Z-order. Then
        // hide every sibling (editor, toolbar, status bar, tab bar, etc.)
        // so their repaints can't leak through our backdrop. We remember
        // which ones were actually visible so hide_lock_overlay can
        // restore only those on unlock (some panels may have been hidden
        // by the user before the lock).
        if let (nwg::ControlHandle::Hwnd(main_hwnd), nwg::ControlHandle::Hwnd(lock_hwnd)) =
            (self.window.handle, self.lock_frame.handle)
        {
            unsafe {
                use windows::Win32::Foundation::{HWND, RECT};
                use windows::Win32::UI::WindowsAndMessaging::{
                    GetClientRect, GetParent, GetWindow, GetWindowLongPtrW, HWND_TOP,
                    SetWindowPos, ShowWindow, GWL_STYLE, GW_CHILD, GW_HWNDNEXT,
                    SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE, SW_SHOW,
                };
                const WS_VISIBLE: u32 = 0x10000000;

                let main = HWND(main_hwnd as *mut _);
                let lock = HWND(lock_hwnd as *mut _);

                let mut rc: RECT = std::mem::zeroed();
                let _ = GetClientRect(main, &mut rc);
                let w = rc.right - rc.left;
                let h = rc.bottom - rc.top;

                self.lock_frame.set_position(0, 0);
                self.lock_frame.set_size(w as u32, h as u32);
                self.layout_lock_overlay();

                ShowWindow(lock, SW_SHOW);
                SetWindowPos(
                    lock,
                    Some(HWND_TOP),
                    0, 0, w, h,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );

                // Walk direct children of the main window and hide every
                // visible sibling of lock_frame. CRITICAL: GW_CHILD returns
                // the topmost CHILD of `main`, and GW_HWNDNEXT walks through
                // that child's siblings (= other children of `main`). We
                // also verify GetParent(cur) == main for safety — a stray
                // top-level window must never be hidden.
                // Appended, not cleared: under a curtain the siblings it hid
                // are already listed (pass 5); hide_lock_overlay drains it.
                let mut hidden = self.lock_hidden_siblings.borrow_mut();
                let mut cur = GetWindow(main, GW_CHILD).unwrap_or(HWND(std::ptr::null_mut()));
                while !cur.0.is_null() {
                    let next = GetWindow(cur, GW_HWNDNEXT).unwrap_or(HWND(std::ptr::null_mut()));
                    let parent = GetParent(cur).unwrap_or(HWND(std::ptr::null_mut()));
                    if cur.0 != lock.0 && parent.0 == main.0 {
                        let style = GetWindowLongPtrW(cur, GWL_STYLE) as u32;
                        if style & WS_VISIBLE != 0 {
                            ShowWindow(cur, SW_HIDE);
                            hidden.push(cur.0 as isize);
                        }
                    }
                    cur = next;
                }

                let _ = windows::Win32::Graphics::Gdi::InvalidateRect(Some(lock), None, true);
            }
        }

        // Focus the password input so the user can type immediately. If the
        // window is in the background (wake-from-sleep), focus lands when
        // Windows activates us; WM_ACTIVATE also re-focuses via the handler.
        self.focus_lock_input();
    }

    /// Set keyboard focus on the inline unlock password input.
    pub(super) fn focus_lock_input(&self) {
        if let nwg::ControlHandle::Hwnd(h) = self.lock_input.handle {
            unsafe {
                let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(
                    Some(windows::Win32::Foundation::HWND(h as *mut _)),
                );
            }
        }
    }

    /// Hide the note without locking it (pass 5): the lock screen without
    /// its unlock field, with a "locking" text. Used while dialogs that do
    /// not close delay the lock, and while the reason of a failed save at
    /// lock is shown.
    pub(super) fn show_curtain(&self) {
        if self.curtain.replace(true) {
            return;
        }
        eprintln!("INFO lock: curtain shown");
        self.show_lock_overlay();
        // Not locked: nothing to unlock, no field to type into.
        self.lock_input.set_visible(false);
        self.lock_eye.set_visible(false);
        self.lock_btn.set_visible(false);
        self.lock_input.set_enabled(false);
        self.lock_btn.set_enabled(false);
        self.lock_subtitle.set_text(&t("lock.curtain"));
    }

    /// Lift the curtain (the note was not locked).
    pub(super) fn hide_curtain(&self) {
        if !self.curtain.replace(false) {
            return;
        }
        eprintln!("INFO lock: curtain lifted");
        self.hide_lock_overlay();
    }

    /// Hide the lock overlay.
    pub(super) fn hide_lock_overlay(&self) {
        if let nwg::ControlHandle::Hwnd(hwnd) = self.lock_frame.handle {
            unsafe {
                windows::Win32::UI::WindowsAndMessaging::ShowWindow(
                    windows::Win32::Foundation::HWND(hwnd as *mut _),
                    windows::Win32::UI::WindowsAndMessaging::SW_HIDE,
                );
            }
        }
        // Re-show every sibling that was hidden by show_lock_overlay. Any
        // sibling that was already hidden (closed panel, etc.) stays hidden
        // because we only recorded those that had WS_VISIBLE set.
        let mut hidden = self.lock_hidden_siblings.borrow_mut();
        for raw in hidden.drain(..) {
            unsafe {
                windows::Win32::UI::WindowsAndMessaging::ShowWindow(
                    windows::Win32::Foundation::HWND(raw as *mut _),
                    windows::Win32::UI::WindowsAndMessaging::SW_SHOW,
                );
            }
        }
    }

    /// Position the lock overlay controls: a card centred in the client area,
    /// laid out by `lock_card::CardLayout` for the window's current DPI.
    /// The card, the padlock and the field box are painted by the frame's
    /// window procedure at the same coordinates.
    pub(super) fn layout_lock_overlay(&self) {
        use crate::ui::lock_card::{self, CardControls, CardLayout, CardMode};
        use windows::Win32::Foundation::RECT;
        use windows::Win32::UI::WindowsAndMessaging::GetClientRect;

        let main = lock_card::hwnd_of(&self.window.handle);
        let mut rc: RECT = unsafe { std::mem::zeroed() };
        if unsafe { GetClientRect(windows::Win32::Foundation::HWND(main as *mut _), &mut rc) }.is_err() {
            // Fail fast, loudly: laying the card out on a guessed size would
            // leave the password field somewhere unexpected.
            eprintln!("ERROR lock: GetClientRect failed on the main window hwnd=0x{main:x}; lock screen not laid out");
            return;
        }
        let (w, h) = (rc.right - rc.left, rc.bottom - rc.top);
        self.lock_frame.set_position(0, 0);
        self.lock_frame.set_size(w as u32, h as u32);

        let dpi = lock_card::window_dpi(main);
        let controls = CardControls {
            title: &self.lock_label,
            subtitle: &self.lock_subtitle,
            input: &self.lock_input,
            eye: &self.lock_eye,
            status: &self.lock_status,
            primary: &self.lock_btn,
            secondary: None,
        };
        // Fonts follow the DPI (the window may have moved to another monitor).
        {
            let mut cache = self.lock_fonts.borrow_mut();
            if cache.as_ref().map(|(d, _)| *d) != Some(dpi) {
                let fonts = lock_card::CardFonts::build(dpi).expect("Failed to build the lock screen fonts");
                controls.apply_fonts(&fonts);
                *cache = Some((dpi, fonts));
            }
        }
        controls.apply_layout(&CardLayout::compute(CardMode::Overlay, w, h, dpi));
    }

    /// Handle the inline unlock submission.
    ///
    /// Reads the password from the overlay input, attempts decrypt, and on
    /// success restores the editor in place — no modal popup. On failure,
    /// increments the attempts counter, shows the error in the status label,
    /// and applies exponential backoff (same schedule as the previous popup).
    pub(super) fn on_unlock_overlay(&self) {
        // Ignore clicks while backoff is active (defense in depth; the button
        // is also disabled during backoff via set_enabled(false)).
        if self.lock_backoff_remaining.get() > 0 {
            return;
        }

        // Serialize with autosave / manual save so a concurrent persistence
        // op can't race the unlock (same contract as the popup path, #036).
        let _persistence_guard = crate::features::persistence_lock::acquire();

        // Wrap the typed password in Zeroizing so its buffer is wiped when
        // we drop it, whether decrypt succeeds or fails.
        let pw: Zeroizing<String> = Zeroizing::new(self.lock_input.text());
        if pw.is_empty() {
            self.lock_status_msg.set(Tone::Error, &t("unlock.empty"));
            return;
        }

        // The session's last save, kept by storage (never what another
        // writer may have put in the files since).
        let raw_payload = match crate::storage::read_session_data(&self.exe_path) {
            Ok(Some(data)) => data,
            Ok(None) => {
                self.lock_status_msg.set(
                    Tone::Error,
                    &t("unlock.cryptoerror")
                        .replace("{msg}", "cannot read payload"),
                );
                return;
            }
            Err(e) => {
                eprintln!("ERROR unlock: {} exe={}", e, self.exe_path.display());
                self.lock_status_msg.set(
                    Tone::Error,
                    &t("unlock.cryptoerror").replace("{msg}", &e),
                );
                return;
            }
        };
        let (_hint, encrypted) = crate::storage::split_hint_and_encrypted(&raw_payload);

        self.lock_status_msg.set(Tone::Neutral, &t("unlock.decrypting"));

        match crate::crypto::decrypt(&encrypted, pw.as_str()) {
            Ok(plaintext) => {
                self.restore_after_unlock(pw, plaintext);
            }
            Err(err) => {
                // Same anti-oracle classification as UnlockDialog: only
                // HmacMismatch / Decryption count as a wrong-password guess;
                // other crypto errors are surfaced verbatim and not counted.
                let is_wrong_password = matches!(
                    &err,
                    crate::crypto::CryptoError::HmacMismatch
                        | crate::crypto::CryptoError::Decryption(_)
                );
                self.lock_input.set_text("");

                if !is_wrong_password {
                    self.lock_status_msg.set(
                        Tone::Error,
                        &t("unlock.cryptoerror")
                            .replace("{msg}", &err.to_string()),
                    );
                    self.focus_lock_input();
                    return;
                }

                const MAX_ATTEMPTS: u32 = 5;
                let attempts_now = {
                    let mut att = self.lock_attempts.borrow_mut();
                    *att += 1;
                    *att
                };

                if attempts_now >= MAX_ATTEMPTS {
                    nwg::modal_info_message(
                        &self.window,
                        &t("unlock.locked.title"),
                        &t("unlock.maxattempts"),
                    );
                    nwg::stop_thread_dispatch();
                    return;
                }

                let delay = unlock_backoff_seconds(attempts_now);
                if delay == 0 {
                    self.lock_status_msg.set(
                        Tone::Error,
                        &format!(
                            "{} ({}/{})",
                            t("unlock.wrong"),
                            attempts_now,
                            MAX_ATTEMPTS,
                        ),
                    );
                    self.focus_lock_input();
                } else {
                    self.lock_btn.set_enabled(false);
                    self.lock_input.set_enabled(false);
                    self.lock_backoff_remaining.set(delay);
                    let retry_msg = t("unlock.retryin")
                        .replace("{seconds}", &delay.to_string());
                    self.lock_status_msg.set(
                        Tone::Error,
                        &format!(
                            "{} ({}/{}) \u{2014} {}",
                            t("unlock.wrong"),
                            attempts_now,
                            MAX_ATTEMPTS,
                            retry_msg,
                        ),
                    );
                    self.lock_backoff_timer.start();
                }
            }
        }
    }

    /// Advance the inline unlock backoff by one second.
    ///
    /// Called once per second from the main OnTimerTick handler while a
    /// wrong-password backoff is active. When the counter hits zero the
    /// controls are re-enabled and focus returns to the password input.
    pub(super) fn on_lock_backoff_tick(&self) {
        let remaining = self.lock_backoff_remaining.get();
        if remaining == 0 {
            self.lock_backoff_timer.stop();
            return;
        }
        let next = remaining - 1;
        self.lock_backoff_remaining.set(next);
        if next == 0 {
            self.lock_backoff_timer.stop();
            self.lock_btn.set_enabled(true);
            self.lock_input.set_enabled(true);
            self.lock_status_msg.clear();
            self.focus_lock_input();
        } else {
            self.lock_status_msg.set(
                Tone::Error,
                &t("unlock.retryin")
                    .replace("{seconds}", &next.to_string()),
            );
        }
    }

    /// Parse the decrypted payload and restore editor state in place.
    /// Shared between the inline unlock flow and (future) alternative paths.
    pub(super) fn restore_after_unlock(&self, password: Zeroizing<String>, plaintext: String) {
        // A wipe still pending from the lock must not hit the restored notes.
        self.wipe_pending.set(false);
        *self.password.borrow_mut() = password;

        let plaintext = crate::features::compression::decompress(&plaintext);
        let (mut settings, note_text) = crate::settings::Settings::parse(&plaintext);
        // The side panel widths of this session stay, saved or not.
        crate::ui::side_panel::keep_machine_panel_widths(&self.settings.borrow(), &mut settings);
        // Settings first: the payload parse needs them (vault categories).
        *self.settings.borrow_mut() = settings;

        // Decryption succeeded (we are only called on Ok), so the state
        // captured at lock time can be consumed now.
        let (note_text, cursor_line) =
            self.load_payload(&note_text, self.locked_session_state.take());

        self.editor.set_text(&note_text);
        self.mark_clean();

        // Reset inline unlock state
        self.lock_input.set_text("");
        self.lock_status_msg.clear();
        crate::ui::lock_card::hide_password(&lock_overlay_hwnds());
        *self.lock_attempts.borrow_mut() = 0;
        self.lock_backoff_remaining.set(0);
        self.lock_backoff_timer.stop();

        *self.is_locked.borrow_mut() = false;
        self.set_menus_for_lock(false);
        self.hide_lock_overlay();
        self.update_title();
        self.update_status_bar();
        // The settings were read back from the file: the menu follows them,
        // and the TOTP panel (wiped at lock) shows their entries again.
        self.sync_view_menu();
        if self.totp_panel.is_visible() {
            self.refresh_totp_list();
        }
        self.layout_controls();
        self.paint_tab_bar();

        // Caret last: after the overlay is hidden and the layout is final,
        // otherwise the scroll is wrong and focus goes to the overlay.
        if let Some(line) = cursor_line {
            self.apply_cursor_line(line);
        }

        self.autolock.borrow_mut().reset();

        // Keys a failed removal at lock time left in ssh-agent.
        self.unload_agent_keys(true);
    }
}
