// Window event wiring (`EditorForm::bind_events`).
// Part of the EditorForm implementation, split out of the former editor.rs.

#[allow(unused_imports)]
use super::*;

impl EditorForm {
    /// Bind all event handlers to the form controls.
    pub(super) fn bind_events(form: std::rc::Rc<EditorForm>) {
        // Install the lock-overlay theme handlers ONCE per form. They paint:
        //   - lock_frame          (WM_ERASEBKGND → pal.background fill)
        //   - lock_card           (WM_ERASEBKGND → pal.surface fill,
        //                          WM_CTLCOLORSTATIC → text on card bg)
        //   - lock_input_border   (WM_ERASEBKGND → pal.input_border fill,
        //                          WM_CTLCOLOREDIT → text + input bg)
        //
        // Doing this once here (rather than on each show_lock_overlay call)
        // avoids NWG's bind_raw_event_handler returning an error on duplicate
        // handler IDs, and lets the closures read `crate::theme::current()`
        // lazily so a theme switch takes effect on the next Invalidate.
        install_lock_overlay_theme(&form);

        Self::bind_status_bar_paint(form.clone());

        let f = form.clone();
        let handler = nwg::full_bind_event_handler(
            &form.window.handle,
            move |evt, _evt_data, handle| {
                use nwg::Event;
                match evt {
                    Event::OnWindowClose => {
                        // WM_CLOSE is handled by the raw handler — this is a fallback
                        if f.on_close() {
                            nwg::stop_thread_dispatch();
                        }
                    }
                    Event::OnMenuItemSelected => {
                        // Issue #30: while locked, no menu command may open a
                        // dialog that changes the (wiped) note; quitting,
                        // About and the tray stay available.
                        if f.input_blocked()
                            && handle != f.menu_file_quit.handle
                            && handle != f.menu_help_about.handle
                            && handle != f.tray_menu_restore.handle
                            && handle != f.tray_menu_quit.handle
                        {
                            eprintln!("INFO menu: command ignored while locked");
                            return;
                        }
                        // File menu
                        if handle == f.menu_file_save.handle {
                            let _ = f.on_save();
                        }
                        if handle == f.menu_file_settings_preferences.handle {
                            f.on_preferences();
                        } else if handle == f.menu_file_settings_security.handle {
                            f.on_security();
                        } else if handle == f.menu_file_settings_tempfiles.handle {
                            if f.on_temp_files() {
                                nwg::stop_thread_dispatch();
                            }
                        } else if handle == f.menu_file_quit.handle {
                            if f.on_close() {
                                nwg::stop_thread_dispatch();
                            }
                        }
                        // View menu
                        else if handle == f.menu_view_always_on_top.handle {
                            f.on_toggle_always_on_top();
                        } else if handle == f.menu_view_word_wrap.handle {
                            f.on_toggle_word_wrap();
                        } else if handle == f.menu_view_line_numbers.handle {
                            f.on_toggle_line_numbers();
                        }
                        // Edit menu
                        else if handle == f.menu_edit_cut.handle {
                            f.on_cut();
                        } else if handle == f.menu_edit_copy.handle {
                            f.on_copy();
                        } else if handle == f.menu_edit_paste.handle {
                            f.on_paste();
                        } else if handle == f.menu_edit_paste_plain.handle {
                            f.on_paste_plain();
                        } else if handle == f.menu_edit_find.handle {
                            f.on_show_find();
                        } else if handle == f.menu_edit_replace.handle {
                            f.on_show_replace();
                        } else if handle == f.menu_edit_goto_line.handle {
                            f.on_goto_line();
                        } else if handle == f.menu_edit_duplicate_line.handle {
                            f.on_duplicate_line();
                        } else if handle == f.menu_edit_delete_line.handle {
                            f.on_delete_line();
                        } else if handle == f.menu_edit_select_all.handle {
                            f.on_select_all();
                        } else if handle == f.menu_edit_insert_timestamp.handle {
                            f.on_insert_timestamp();
                        }
                        // Context menu
                        else if handle == f.ctx_cut.handle {
                            f.on_cut();
                        } else if handle == f.ctx_copy.handle {
                            f.on_copy();
                        } else if handle == f.ctx_paste.handle {
                            f.on_paste();
                        } else if handle == f.ctx_paste_plain.handle {
                            f.on_paste_plain();
                        } else if handle == f.ctx_select_all.handle {
                            f.on_select_all();
                        }
                        // Help menu
                        else if handle == f.menu_help_updates.handle {
                            f.on_check_updates();
                        } else if handle == f.menu_help_about.handle {
                            f.on_about();
                        }
                        // Tray menu
                        else if handle == f.tray_menu_restore.handle {
                            f.on_tray_restore();
                        } else if handle == f.tray_menu_quit.handle {
                            if f.on_close() {
                                nwg::stop_thread_dispatch();
                            }
                        }
                        // Tools menu
                        if handle == f.menu_tools_vault.handle {
                            f.on_toggle_vault();
                        }
                        if handle == f.menu_tools_prompts.handle {
                            f.on_toggle_prompts();
                        }
                        if handle == f.menu_tools_totp.handle {
                            f.on_toggle_totp();
                        }
                        if handle == f.menu_tools_attachments.handle {
                            f.on_toggle_attachments();
                        }
                        if handle == f.menu_tools_passgen.handle {
                            f.on_password_generator();
                        }
                        // Steganography
                        if handle == f.menu_tools_stego_hide.handle {
                            f.on_stego_hide();
                        }
                        if handle == f.menu_tools_stego_extract.handle {
                            f.on_stego_extract();
                        }
                        // SSH keys (issue 054)
                        if handle == f.menu_tools_ssh_generate.handle {
                            f.on_ssh_generate();
                        }
                        if handle == f.menu_tools_ssh_agent.handle {
                            f.on_ssh_agent_load();
                        }
                        // Safety backups: history, restore, force one
                        if handle == f.menu_file_backups.handle {
                            f.on_backups();
                        }
                        // Print
                        if handle == f.menu_file_print.handle {
                            f.on_print();
                        }
                        // Export
                        if handle == f.menu_file_export.handle {
                            f.on_export();
                        }
                        // Full backup export / import
                        if handle == f.menu_file_backup_export.handle {
                            f.on_export_backup();
                        }
                        if handle == f.menu_file_backup_import.handle {
                            f.on_import_backup();
                        }
                        if handle == f.menu_file_import_exe.handle {
                            f.on_import_from_exe();
                        }
                        // Tab menu
                        {
                            if handle == f.menu_tab_new.handle {
                                f.on_new_tab();
                            } else if handle == f.menu_tab_close.handle {
                                f.on_close_tab();
                            } else if handle == f.menu_tab_rename.handle {
                                f.on_rename_tab();
                            } else if handle == f.menu_tab_next.handle {
                                f.on_next_tab();
                            } else if handle == f.menu_tab_prev.handle {
                                f.on_prev_tab();
                            }
                        }
                    }
                    Event::OnContextMenu => {
                        if handle == f.tray_icon.handle {
                            let (x, y) = nwg::GlobalCursor::position();
                            f.tray_menu.popup(x, y);
                        }
                    }
                    Event::OnMousePress(nwg::MousePressEvent::MousePressLeftUp) => {
                        if handle == f.tray_icon.handle {
                            f.on_tray_restore();
                        }
                    }
                    Event::OnTextInput => {
                        if handle == f.editor.handle && !*f.suppress_change.borrow() {
                            f.mark_modified();
                            f.update_status_bar();
                            f.paint_gutter();
                            // Post deferred repaint so gutter updates after auto-scroll
                            if let nwg::ControlHandle::Hwnd(hwnd) = f.window.handle {
                                unsafe {
                                    let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                                        Some(windows::Win32::Foundation::HWND(hwnd as *mut _)),
                                        0x8003, // WM_APP+3: deferred gutter repaint
                                        windows::Win32::Foundation::WPARAM(0),
                                        windows::Win32::Foundation::LPARAM(0),
                                    );
                                }
                            }
                            // Reset autolock timer on activity
                            f.autolock.borrow_mut().reset();
                        }
                    }
                    Event::OnMinMaxInfo => {
                        // Set minimum window size
                    }
                    Event::OnResize | Event::OnResizeEnd => {
                        // Minimize to tray: hide window when minimized
                        if f.settings.borrow().minimize_to_tray {
                            if let nwg::ControlHandle::Hwnd(hwnd) = f.window.handle {
                                let hwnd = windows::Win32::Foundation::HWND(hwnd as *mut _);
                                if unsafe { windows::Win32::UI::WindowsAndMessaging::IsIconic(hwnd) }.as_bool() {
                                    f.window.set_visible(false);
                                    return;
                                }
                            }
                        }
                        f.layout_controls();
                    }
                    Event::OnTimerTick => {
                        if handle == f.highlight_timer.handle && f.wipe_pending.get() {
                            f.retry_pending_wipe();
                        }
                        if handle == f.highlight_timer.handle {
                            f.update_occurrence_highlights();
                            f.repaint_gutter_if_caret_moved();
                        }
                        if handle == f.highlight_timer.handle {
                            f.run_pending_requests();
                        }
                        if handle == f.autolock_timer.handle && !*f.is_locked.borrow() {
                            // Curtain at once, dialogs answered No / Cancel,
                            // real lock when possible (lock_flow, #30).
                            let due = !matches!(f.autolock.borrow().lock_decision(), crate::features::autolock::LockDecision::Wait);
                            if due {
                                f.request_lock();
                            }
                        }
                        if handle == f.autosave_timer.handle {
                            use super::modal_guard::TimerTick;
                            // The curtain counts as locked (pass 9): the
                            // lock's own retry is the only save under it.
                            match f.autosave_gate.tick(f.modal_open(), f.input_blocked()) {
                                TimerTick::Skip => {}
                                TimerTick::RestartDelay => f.autosave.borrow_mut().mark_saved(),
                                TimerTick::Run => {
                                    if f.autosave.borrow().should_save() && f.is_modified.load(Ordering::Relaxed) {
                                        f.on_autosave();
                                    }
                                }
                            }
                        }
                        if handle == f.clipboard_timer.handle {
                            if f.clipboard_guard.borrow().should_clear().is_some() {
                                f.on_clipboard_clear();
                            }
                        }
                        if handle == f.lock_backoff_timer.handle {
                            f.on_lock_backoff_tick();
                        }
                    }
                    Event::OnButtonClick => {
                        if handle == f.lock_btn.handle {
                            f.on_unlock_overlay();
                        }
                    }
                    // Keyboard shortcuts are handled in raw event handler below
                    _ => {}
                }
            },
        );

        // Keep the handler alive for the lifetime of the dispatch loop
        std::mem::forget(handler);

        // Install custom WNDPROC to intercept WM_DROPFILES before NWG subclasses
        {
            use nwg::ControlHandle;
            use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
            static ORIG_WNDPROC: AtomicIsize = AtomicIsize::new(0);
            // Réentrance : Windows peut délivrer un WM_DROPFILES pendant qu'un modal est ouvert.
            // On ignore silencieusement les drops additionnels tant que le précédent n'est pas résolu.
            static DROP_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
            static mut EDITOR_FORM: Option<std::rc::Rc<EditorForm>> = None;

            unsafe extern "system" fn drop_wndproc(
                hwnd: windows::Win32::Foundation::HWND,
                msg: u32,
                wparam: windows::Win32::Foundation::WPARAM,
                lparam: windows::Win32::Foundation::LPARAM,
            ) -> windows::Win32::Foundation::LRESULT {
                if msg == 0x0233 {
                    // WM_DROPFILES — handle before anything else
                    #[link(name = "shell32")]
                    extern "system" {
                        fn DragQueryFileW(hDrop: usize, iFile: u32, buf: *mut u16, cch: u32) -> u32;
                        fn DragFinish(hDrop: usize);
                    }
                    let hdrop = wparam.0;
                    // Récupère le premier fichier uniquement (comportement historique).
                    // Taille allouée dynamiquement via la probe DragQueryFileW(path=NULL, cch=0)
                    // qui retourne le nombre de wchars du path sans le NUL ; on alloue len+1.
                    // L'ancien buffer fixe [u16; 260] tronquait silencieusement les chemins
                    // longs (\\?\ ou noms > MAX_PATH) ce qui pouvait rediriger `fs::read`
                    // vers un autre fichier attaquant-contrôlé au même préfixe.
                    let mut first_path: Option<String> = None;
                    let count = DragQueryFileW(hdrop, 0xFFFF_FFFF, std::ptr::null_mut(), 0);
                    if count > 0 {
                        let needed = DragQueryFileW(hdrop, 0, std::ptr::null_mut(), 0);
                        if needed > 0 && needed < 0x10_0000 {
                            // +1 for the terminating NUL (cch counts wchars, buffer must include it)
                            let cap = (needed as usize) + 1;
                            let mut buf = vec![0u16; cap];
                            let len = DragQueryFileW(hdrop, 0, buf.as_mut_ptr(), cap as u32);
                            if len > 0 && (len as usize) < cap {
                                first_path = Some(String::from_utf16_lossy(&buf[..len as usize]));
                            }
                        }
                    }
                    DragFinish(hdrop);

                    // Guard de réentrance : si un dialog est déjà ouvert, on abandonne ce drop.
                    if DROP_IN_PROGRESS.swap(true, Ordering::SeqCst) {
                        return windows::Win32::Foundation::LRESULT(0);
                    }
                    if let (Some(path_str), Some(ref f)) = (first_path, EDITOR_FORM.as_ref()) {
                        handle_dropped_file(f, &path_str);
                    }
                    DROP_IN_PROGRESS.store(false, Ordering::SeqCst);
                    return windows::Win32::Foundation::LRESULT(0);
                }
                // Forward all other messages to original WNDPROC
                let orig = ORIG_WNDPROC.load(Ordering::Relaxed);
                windows::Win32::UI::WindowsAndMessaging::CallWindowProcW(
                    std::mem::transmute(orig),
                    hwnd, msg, wparam, lparam,
                )
            }

            /// Handle a single dropped file: sniff type, prompt the user, execute the chosen action.
            fn handle_dropped_file(f: &EditorForm, path_str: &str) {
                // Refuse drops while the editor is locked. Without this, a
                // programmatic SendMessage(WM_DROPFILES) from a sibling process
                // would inject text/attachments into the next unlocked session.
                // The curtain counts as locked (pass 6).
                if f.input_blocked() {
                    return;
                }

                let path = std::path::Path::new(path_str);
                let file_name = path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| path_str.to_string());

                // Les dossiers ne sont pas supportés : on avertit l'utilisateur plutôt que
                // d'ignorer silencieusement (éviter l'impression de bug).
                if path.is_dir() {
                    f.inform("LockNote",
                        &t("drop.directory"),
                    );
                    return;
                }

                let is_text = is_probably_text_file(path);

                // In builds without the attachments feature, keep legacy behaviour: text insert only.

                {
                    use crate::ui::dialogs::drop_choice::{DropChoiceDialog, DropChoice};
                    let epoch = f.lock_epoch();
                    let choice = DropChoiceDialog::show(&file_name, is_text);
                    if !matches!(choice, DropChoice::Cancel) && !f.still_valid(epoch) {
                        eprintln!("INFO drop: dropped, the note locked meanwhile");
                        return;
                    }
                    match choice {
                        DropChoice::InsertText => {
                            insert_text_from_file(f, path);
                        }
                        DropChoice::Attach => {
                            f.attach_file_from_path(path);
                        }
                        DropChoice::Cancel => {}
                    }
                }
            }

            /// Read a text file and insert its contents at the caret using EM_REPLACESEL.
            fn insert_text_from_file(f: &EditorForm, path: &std::path::Path) {
                // Cap "text file" drops to a sensible ceiling. The sniffer
                // only reads 8 KiB before deciding "text", so a 5 GB
                // pseudo-UTF-8 file would still be accepted and then
                // `read_to_string` would allocate its entire size.
                const MAX_TEXT_DROP_SIZE: u64 = 32 * 1024 * 1024; // 32 MiB
                if let Ok(md) = std::fs::metadata(path) {
                    if md.len() > MAX_TEXT_DROP_SIZE {
                        f.inform("LockNote",
                            &format!(
                                "{}\n\n{}",
                                path.display(),
                                t("drop.invalid")
                            ),
                        );
                        return;
                    }
                }

                match std::fs::read_to_string(path) {
                    Ok(content) => {
                        let wide: Vec<u16> = content.encode_utf16().chain(std::iter::once(0)).collect();
                        f.send_editor_message(0x00C2, 1, wide.as_ptr() as isize);
                        f.mark_modified();
                        f.update_status_bar();
                    }
                    Err(_) => {
                        f.inform("LockNote",
                            &format!(
                                "{}\n\n{}",
                                path.display(),
                                t("drop.invalid")
                            ),
                        );
                    }
                }
            }


            if let ControlHandle::Hwnd(win_hwnd) = form.window.handle {
                let h = windows::Win32::Foundation::HWND(win_hwnd as *mut _);
                unsafe {
                    EDITOR_FORM = Some(form.clone());
                    let orig = windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(
                        h,
                        windows::Win32::UI::WindowsAndMessaging::GWLP_WNDPROC,
                        drop_wndproc as isize,
                    );
                    ORIG_WNDPROC.store(orig, Ordering::Relaxed);
                }
            }
        }

        // Bind keyboard accelerators via a raw event handler for Ctrl+key combos
        let f2 = form.clone();
        let raw_handler = nwg::bind_raw_event_handler(
            &form.window.handle,
            0x10000,
            move |_hwnd, msg, w_param, l_param| {
                // Dark title/menu bar painting (None in the Light theme)
                if let Some(r) = crate::ui::dark_chrome::handle_message(_hwnd as isize, msg, w_param as usize, l_param as isize) {
                    return Some(r);
                }

                // WM_CLOSE = 0x0010 — intercept to allow cancel
                if msg == 0x0010 {
                    if f2.on_close() {
                        // User confirmed or no changes — proceed with close
                        nwg::stop_thread_dispatch();
                    }
                    // Always consume WM_CLOSE to prevent NWG from destroying the window
                    return Some(0);
                }

                // WM_CONTEXTMENU = 0x007B — show right-click context menu on the editor
                if msg == 0x007B {
                    if let nwg::ControlHandle::Hwnd(editor_hwnd) = f2.editor.handle {
                        if _hwnd as usize == editor_hwnd as usize || w_param == editor_hwnd as usize {
                            let x = (l_param & 0xFFFF) as i16 as i32;
                            let y = ((l_param >> 16) & 0xFFFF) as i16 as i32;
                            if let nwg::ControlHandle::Hwnd(win_hwnd) = f2.window.handle {
                                if let nwg::ControlHandle::PopMenu(_parent, hmenu) = f2.context_menu.handle {
                                    use windows::Win32::UI::WindowsAndMessaging::{TrackPopupMenu, TPM_LEFTALIGN, TPM_TOPALIGN, TPM_RIGHTBUTTON};
                                    unsafe {
                                        let _ = TrackPopupMenu(
                                            windows::Win32::UI::WindowsAndMessaging::HMENU(hmenu as *mut _),
                                            TPM_LEFTALIGN | TPM_TOPALIGN | TPM_RIGHTBUTTON,
                                            x,
                                            y,
                                            None,
                                            windows::Win32::Foundation::HWND(win_hwnd as *mut _),
                                            None,
                                        );
                                    }
                                }
                            }
                            return Some(0);
                        }
                    }
                }

                // WM_QUERYENDSESSION = 0x0011 / WM_ENDSESSION = 0x0016:
                // logoff or shutdown (issue #29), never a prompt.
                if msg == 0x0011 {
                    return Some(if f2.on_query_end_session() { 1 } else { 0 });
                }
                if msg == 0x0016 {
                    f2.on_end_session(w_param != 0);
                    return Some(0);
                }

                // WM_HOTKEY = 0x0312 (global hotkey)
                if msg == crate::features::hotkey::WM_HOTKEY {
                    f2.on_global_hotkey();
                    return Some(0);
                }

                // WM_ACTIVATE = 0x0006 — when the window is reactivated while
                // locked, put keyboard focus back on the inline password input
                // so the user can type immediately. Inline overlay: no popup
                // is triggered on activation (the popup on WM_ACTIVATE was the
                // cause of the "popup appears without focus on wake-up" bug).
                if msg == 0x0006 {
                    let activate_state = w_param & 0xFFFF; // WA_ACTIVE=1, WA_CLICKACTIVATE=2
                    if (activate_state == 1 || activate_state == 2) && focus_password_field(*f2.is_locked.borrow(), f2.curtain.get()) {
                        f2.focus_lock_input();
                    }
                }

                // WM_SETFOCUS = 0x0007 — when a top-level window is activated,
                // DefWindowProc gives the keyboard focus to the window itself
                // AFTER the WM_ACTIVATE handler above ran, which takes it away
                // from the password input. Hand it back once the window really
                // has the focus, so the caret is in the field as soon as the
                // window is focused.
                if msg == 0x0007 && focus_password_field(*f2.is_locked.borrow(), f2.curtain.get()) {
                    f2.focus_lock_input();
                }

                // WM_MOUSEMOVE = 0x0200, WM_LBUTTONDOWN = 0x0201. A move with
                // the cursor where it was (sent after a dialog closes) is not
                // activity (pass 6).
                if msg == 0x0201 {
                    f2.autolock.borrow_mut().reset();
                }
                if msg == 0x0200 {
                    let last = f2.last_mouse_pos.replace(Some(l_param));
                    if mouse_really_moved(last, l_param) {
                        f2.autolock.borrow_mut().reset();
                    }
                }

                // WM_KEYDOWN = 0x0100
                if msg == 0x0100 {
                    if f2.handle_shortcut(w_param as u32) {
                        return Some(0);
                    }
                }

                // WM_COMMAND = 0x0111 — handle RichEdit notifications
                // nwg doesn't route EN_CHANGE for "RICHEDIT50W" (only "Edit"),
                // so we intercept it here to update gutter, status bar, and modified state.
                if msg == 0x0111 {
                    let notification = ((w_param >> 16) & 0xFFFF) as u16;
                    match notification {
                        // EN_CHANGE = 0x0300 — text content changed
                        0x0300 => {
                            if !*f2.suppress_change.borrow() {
                                f2.mark_modified();
                                f2.update_status_bar();
                                f2.paint_gutter();
                                // Post deferred repaint for auto-scroll
                                if let nwg::ControlHandle::Hwnd(hwnd) = f2.window.handle {
                                    unsafe {
                                        let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                                            Some(windows::Win32::Foundation::HWND(hwnd as *mut _)),
                                            0x8003,
                                            windows::Win32::Foundation::WPARAM(0),
                                            windows::Win32::Foundation::LPARAM(0),
                                        );
                                    }
                                }
                                f2.autolock.borrow_mut().reset();
                            }
                        }
                        // EN_VSCROLL = 0x0602, EN_HSCROLL = 0x0601 — scroll sync
                        0x0602 | 0x0601 => {
                            f2.paint_gutter();
                        }
                        _ => {}
                    }
                }

                // WM_MOUSEWHEEL = 0x020A — repaint gutter after scroll
                if msg == 0x020A {
                    // Post a repaint; the scroll will happen after this returns
                    let gutter = *f2.gutter_hwnd.borrow();
                    if gutter != 0 {
                        unsafe {
                            let _ = windows::Win32::Graphics::Gdi::InvalidateRect(
                                Some(windows::Win32::Foundation::HWND(gutter as *mut _)),
                                None,
                                true,
                            );
                        }
                    }
                    // Also paint immediately for responsiveness
                    f2.paint_gutter();
                }

                // WM_VSCROLL = 0x0115 — repaint gutter on scroll bar interaction
                if msg == 0x0115 {
                    f2.paint_gutter();
                }

                // WM_APP+3 = 0x8003 — deferred gutter repaint (after auto-scroll)
                if msg == 0x8003 {
                    f2.paint_gutter();
                    return Some(0);
                }

                // WM_APP+5 = 0x8005 — deferred tab bar repaint (posted from
                // tab_bar_wnd_proc on WM_PAINT so system-triggered invalidation
                // redraws instead of showing stale backbuffer pixels).
                if msg == 0x8005 {
                    f2.paint_tab_bar();
                    return Some(0);
                }

                // WM_APP+1 = 0x8001 — tab bar click (x in w_param, y in l_param)
                if msg == 0x8001 {
                    let click_x = w_param as i32;
                    let click_y = l_param as i32;
                    let Some(client_w) = f2.tab_bar_client_width() else { return Some(0) };
                    let Some((tabs, active_idx, layout)) = f2.current_tab_layout(client_w) else {
                        return Some(0);
                    };

                    use crate::ui::tab_layout::TabHit;
                    match crate::ui::tab_layout::hit_test(&layout, click_x) {
                    TabHit::Tab { index: idx, slot } => {
                        // Check if close button (×) was clicked
                        if tabs.len() > 1 && crate::ui::tab_layout::close_hit(slot, crate::ui::tab_layout::TAB_STRIP_H, click_x, click_y) {
                            f2.tab_drag.set(None);
                            f2.close_tab_at(idx);
                        } else {
                            // A press on a tab may turn into a drag (WM_APP+6)
                            f2.tab_drag.set(Some((idx, click_x)));
                            f2.tab_drag_over.set(None);
                        }
                        if f2.tab_drag.get().is_some() && idx != active_idx {
                            let content = f2.get_content();
                            let tb = f2.tab_bar.borrow_mut();
                            tb.set_tab_content(active_idx, &content);
                            tb.set_active(idx);
                            let new_content = tb.get_tab_content(idx);
                            drop(tb);
                            f2.editor.set_text(&new_content);
                            *f2.detected_encoding.borrow_mut() = "UTF-8".to_string();
                            f2.update_title();
                            f2.update_status_bar();
                            f2.paint_tab_bar();
                            f2.layout_controls();
                        }
                    }
                    // "…" clicked — popup menu with the hidden tabs
                    TabHit::Overflow => {
                        f2.tab_drag.set(None);
                        f2.show_tab_overflow_menu(&layout.hidden)
                    }
                    TabHit::Plus => {
                        f2.tab_drag.set(None);
                        f2.on_new_tab();
                        f2.paint_tab_bar();
                    }
                    TabHit::Nothing => f2.tab_drag.set(None),
                    }
                    return Some(0);
                }

                // WM_APP+7 = 0x8007 — mouse moved while a tab is held
                // (x in w_param): update the insertion mark.
                if msg == 0x8007 {
                    let Some((from, start_x)) = f2.tab_drag.get() else { return Some(0) };
                    let Some(client_w) = f2.tab_bar_client_width() else { return Some(0) };
                    let Some((_, _, layout)) = f2.current_tab_layout(client_w) else {
                        return Some(0);
                    };
                    let over = crate::ui::tab_layout::drag_target(&layout, start_x, w_param as i32, from);
                    if over != f2.tab_drag_over.get() {
                        f2.tab_drag_over.set(over);
                        f2.paint_tab_bar();
                    }
                    return Some(0);
                }

                // WM_APP+6 = 0x8006 — left button released on the tab bar
                // (x in w_param): drop the pressed tab where it was released.
                if msg == 0x8006 {
                    let Some((from, start_x)) = f2.tab_drag.take() else { return Some(0) };
                    if f2.tab_drag_over.take().is_some() {
                        f2.paint_tab_bar(); // erase the mark (also when the drop is refused)
                    }
                    let Some(client_w) = f2.tab_bar_client_width() else { return Some(0) };
                    let Some((_, _, layout)) = f2.current_tab_layout(client_w) else {
                        return Some(0);
                    };
                    if let Some(to) = crate::ui::tab_layout::drag_target(&layout, start_x, w_param as i32, from) {
                        f2.on_tab_drop(from, to);
                    }
                    return Some(0);
                }

                // WM_APP+2 = 0x8002 — tab bar right-click (x in w_param, y in l_param)
                if msg == 0x8002 {
                    let click_x = w_param as i32;
                    let Some(client_w) = f2.tab_bar_client_width() else { return Some(0) };
                    let Some((_, _, layout)) = f2.current_tab_layout(client_w) else {
                        return Some(0);
                    };

                    // Hit-test on the drawn tabs only (a right click on "…"
                    // or "+" must not open the menu of a hidden tab).
                    if let crate::ui::tab_layout::TabHit::Tab { index: idx, .. } =
                        crate::ui::tab_layout::hit_test(&layout, click_x)
                    {
                        *f2.right_clicked_tab.borrow_mut() = Some(idx);
                        f2.show_tab_context_menu(idx);
                    }
                    return Some(0);
                }

                // WM_NOTIFY = 0x004E — handle EN_LINK for URL click-to-open
                if msg == 0x004E {
                    let nmhdr = unsafe { std::ptr::read_unaligned(l_param as *const NmHdr) };
                    // EN_MSGFILTER = 0x0700 — intercept double-click for word trim
                    if nmhdr.code == 0x0700 {
                        let mf = unsafe { std::ptr::read_unaligned(l_param as *const MsgFilter) };
                        let mf_msg = mf.msg;
                        // WM_LBUTTONDBLCLK = 0x0203: arm; the widening runs on
                        // the WM_LBUTTONUP = 0x0202 that ends the click.
                        if mf_msg == 0x0203 {
                            *f2.dblclick_armed.borrow_mut() = true;
                        }
                        if mf_msg == 0x0202 && std::mem::take(&mut *f2.dblclick_armed.borrow_mut()) {
                            if let nwg::ControlHandle::Hwnd(win_hwnd) = f2.window.handle {
                                unsafe {
                                    let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                                        Some(windows::Win32::Foundation::HWND(win_hwnd as *mut _)),
                                        0x8004, // WM_APP+4
                                        windows::Win32::Foundation::WPARAM(0),
                                        windows::Win32::Foundation::LPARAM(0),
                                    );
                                }
                            }
                        }
                    }
                    // EN_LINK = 0x070B
                    if nmhdr.code == 0x070B {
                        let enlink = unsafe { std::ptr::read_unaligned(l_param as *const EnLink) };
                        let (en_msg, en_min, en_max) = (enlink.msg, enlink.chrg_min, enlink.chrg_max);
                        // React on WM_LBUTTONUP = 0x0202
                        if en_msg == 0x0202 {
                            let text = f2.get_content();
                            let resolved = link_text_range(&text, en_min, en_max);
                            if let Err(ref err) = resolved {
                                eprintln!("en_link: invalid range err={:?}", err);
                            }
                            if let Ok(url) = resolved {
                                // ShellExecuteW instead of `cmd /c start` —
                                // avoids cmd.exe re-parsing a URL that could
                                // contain `&`/`|`/`"` as shell meta-chars.
                                if url.starts_with("http://") || url.starts_with("https://") {
                                    unsafe {
                                        let wide_op: Vec<u16> = "open\0".encode_utf16().collect();
                                        let wide_url: Vec<u16> = url.encode_utf16().chain(std::iter::once(0)).collect();
                                        let _ = windows::Win32::UI::Shell::ShellExecuteW(
                                            None,
                                            windows::core::PCWSTR(wide_op.as_ptr()),
                                            windows::core::PCWSTR(wide_url.as_ptr()),
                                            windows::core::PCWSTR::null(),
                                            windows::core::PCWSTR::null(),
                                            windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }

                // WM_APP+4 = 0x8004 — deferred double-click word expansion
                if msg == 0x8004 {
                    f2.on_dblclick_expand_selection();
                    return Some(0);
                }

                None
            },
        );
        std::mem::forget(raw_handler);

        // WM_KEYDOWN from the RichEdit child is never routed to the main
        // window's subclass, so bind a second handler directly on the editor
        // control and reuse handle_shortcut(). Without this, Ctrl+F/H/G/Z/Y
        // and friends never fire when the cursor is in the editor.
        //
        // This handler also (a) tells IsDialogMessage — which nwg's dispatch
        // loop uses — that the editor wants Tab itself, so pressing Tab
        // inserts \t instead of moving focus; and (b) implements bullet-list
        // continuation on plain Enter when the current line starts with "• ";
        // (c) block indent / outdent on Tab and Shift+Tab (indent.rs).
        let f_ed = form.clone();
        // Set by the Enter handler when bullet logic consumed a VK_RETURN, so
        // the trailing WM_CHAR(0x0D) gets swallowed and RichEdit doesn't add
        // an extra newline on top of the one we inserted.
        use std::sync::atomic::{AtomicBool, Ordering as SuppressOrd};
        static BULLET_SUPPRESS_ENTER_CHAR: AtomicBool = AtomicBool::new(false);
        // Same for the WM_CHAR(0x09) after a VK_TAB consumed by indent.rs.
        static TAB_SUPPRESS_CHAR: AtomicBool = AtomicBool::new(false);
        // Same for the WM_CHAR(0x0A) after a Ctrl+Enter (checkbox toggle).
        static CTRL_ENTER_SUPPRESS_CHAR: AtomicBool = AtomicBool::new(false);
        let editor_key_handler = nwg::bind_raw_event_handler(
            &form.editor.handle,
            0x10001,
            move |_hwnd, msg, w_param, _l_param| {
                match msg {
                    // WM_GETDLGCODE — claim every key so IsDialogMessage
                    // stops eating Tab (focus nav) and Enter (default-button
                    // routing). Flags: DLGC_WANTARROWS | DLGC_WANTTAB |
                    // DLGC_WANTALLKEYS | DLGC_HASSETSEL | DLGC_WANTCHARS = 0x8F.
                    0x0087 => return Some(0x8F),
                    // WM_KEYDOWN
                    0x0100 => {
                        // Locked or curtain: the hidden editor must not
                        // receive keys.
                        if f_ed.input_blocked() {
                            return Some(0);
                        }
                        let vk = w_param as u32;
                        let ctrl = unsafe {
                            windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(0x11)
                        } < 0;
                        let shift = unsafe {
                            windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(0x10)
                        } < 0;
                        // Plain Enter: try bullet-list continuation first.
                        if vk == 0x0D && !ctrl && !shift && f_ed.handle_enter_bullet() {
                            BULLET_SUPPRESS_ENTER_CHAR.store(true, SuppressOrd::SeqCst);
                            return Some(0);
                        }
                        // Tab / Shift+Tab: block indent, outdent. Ctrl+Tab
                        // stays the tab switch of handle_shortcut.
                        if vk == 0x09 && !ctrl && f_ed.handle_tab_indent(shift) {
                            TAB_SUPPRESS_CHAR.store(true, SuppressOrd::SeqCst);
                            return Some(0);
                        }
                        if f_ed.handle_shortcut(vk) {
                            if vk == 0x0D && ctrl {
                                CTRL_ENTER_SUPPRESS_CHAR.store(true, SuppressOrd::SeqCst);
                            }
                            return Some(0);
                        }
                    }
                    // WM_CHAR — swallow the 0x0D that TranslateMessage queues
                    // right after a bullet-handled VK_RETURN.
                    0x0102 => {
                        if f_ed.input_blocked() {
                            return Some(0);
                        }
                        if w_param == 0x0D
                            && BULLET_SUPPRESS_ENTER_CHAR.swap(false, SuppressOrd::SeqCst)
                        {
                            return Some(0);
                        }
                        if w_param == 0x09
                            && TAB_SUPPRESS_CHAR.swap(false, SuppressOrd::SeqCst)
                        {
                            return Some(0);
                        }
                        if w_param == 0x0A
                            && CTRL_ENTER_SUPPRESS_CHAR.swap(false, SuppressOrd::SeqCst)
                        {
                            return Some(0);
                        }
                    }
                    _ => {}
                }
                None
            },
        );
        std::mem::forget(editor_key_handler);

        // Vault panel event handlers
        {
            let f3 = form.clone();
            let vault_handler = nwg::full_bind_event_handler(
                &form.vault_frame.handle,
                move |evt, _evt_data, handle| {
                    match evt {
                        nwg::Event::OnButtonClick => {
                            if handle == f3.vault_btn_add.handle {
                                f3.on_vault_add();
                            } else if handle == f3.vault_btn_edit.handle {
                                f3.on_vault_edit();
                            } else if handle == f3.vault_btn_delete.handle {
                                f3.on_vault_delete();
                            } else if handle == f3.vault_btn_copy.handle {
                                f3.on_vault_copy();
                            } else if handle == f3.vault_btn_reveal.handle {
                                f3.on_vault_reveal();
                            }
                        }
                        // Selecting a secret updates the buttons and copies
                        // it (decision t reversed by the user, 1.2.16);
                        // double-click does nothing (A4).
                        nwg::Event::OnTreeItemSelectionChanged => {
                            if handle == f3.vault_tree.handle && !f3.suppress_tree_events.get() {
                                f3.update_vault_buttons();
                                f3.on_vault_copy();
                            }
                        }
                        nwg::Event::OnTreeViewRightClick => {
                            if handle == f3.vault_tree.handle {
                                f3.on_vault_tree_context_menu();
                            }
                        }
                        // Typing in the search field filters the tree, keeping
                        // the selected secret selected when it still matches
                        // (re-selected without copying it again).
                        nwg::Event::OnTextInput if handle == f3.vault_search.handle && !f3.suppress_tree_events.get() => {
                            f3.refresh_vault_tree(f3.selected_vault_secret());
                        }
                        _ => {}
                    }
                },
            );
            std::mem::forget(vault_handler);

            // Drag a secret onto another category (issue 053). Start on
            // TVN_BEGINDRAGW (sent to the frame), then track the mouse on the
            // tree while it holds the capture.
            let fd = form.clone();
            let drag_start = nwg::bind_raw_event_handler(&form.vault_frame.handle, 0x40010, move |_h, msg, _w, l| {
                if msg == 0x004E {
                    use windows::Win32::UI::Controls::{NMTREEVIEWW, TVN_BEGINDRAGW};
                    let nm = unsafe { &*(l as *const NMTREEVIEWW) };
                    if nm.hdr.code == TVN_BEGINDRAGW {
                        fd.on_vault_drag_start(nm.itemNew.hItem.0);
                    }
                }
                None
            });
            let fm = form.clone();
            let drag_track = nwg::bind_raw_event_handler(&form.vault_tree.handle, 0x40011, move |_h, msg, w, l| {
                fm.vault_drag.get()?;
                let pt = || windows::Win32::Foundation::POINT {
                    x: (l & 0xFFFF) as u16 as i16 as i32,
                    y: ((l >> 16) & 0xFFFF) as u16 as i16 as i32,
                };
                match msg {
                    0x0200 => {
                        // WM_MOUSEMOVE: highlight the item under the cursor
                        fm.vault_drag_highlight(fm.vault_tree_hit(pt()));
                        Some(0)
                    }
                    0x0202 => {
                        // WM_LBUTTONUP: drop
                        let target = fm.vault_tree_hit(pt());
                        fm.on_vault_drop(target);
                        Some(0)
                    }
                    0x0100 if w == 0x1B => {
                        // Escape: cancel
                        fm.vault_drag_end();
                        Some(0)
                    }
                    0x0215 => {
                        // WM_CAPTURECHANGED: capture lost (another window,
                        // modal, lock) — cancel
                        fm.vault_drag_end();
                        None
                    }
                    _ => None,
                }
            });
            std::mem::forget(drag_start);
            std::mem::forget(drag_track);

            let f4 = form.clone();
            let timer_handler = nwg::full_bind_event_handler(
                &form.window.handle,
                move |evt, _evt_data, handle| {
                    if let nwg::Event::OnTimerTick = evt {
                        if handle == f4.vault_timer.handle {
                            f4.on_vault_timer_tick();
                        }
                    }
                },
            );
            std::mem::forget(timer_handler);

            // Theme and hide vault frame initially
            form.theme_vault_panel();
            form.vault_frame.set_visible(false);
        }

        // Attachments panel event handlers
        {
            let f_att = form.clone();
            let attach_handler = nwg::full_bind_event_handler(
                &form.attach_frame.handle,
                move |evt, _evt_data, handle| {
                    if let nwg::Event::OnButtonClick = evt {
                        if handle == f_att.attach_btn_add.handle {
                            f_att.on_attach_add();
                        } else if handle == f_att.attach_btn_open.handle {
                            f_att.on_attach_open();
                        } else if handle == f_att.attach_btn_save.handle {
                            f_att.on_attach_save();
                        } else if handle == f_att.attach_btn_delete.handle {
                            f_att.on_attach_delete();
                        }
                    }
                },
            );
            std::mem::forget(attach_handler);

            // Double-click on attachment list → open (save as + launch)
            let f_dbl = form.clone();
            let dblclick_handler = nwg::bind_raw_event_handler(
                &form.attach_list.handle,
                0x40003,
                move |_hwnd, msg, _wparam, _lparam| {
                    // WM_LBUTTONDBLCLK = 0x0203
                    if msg == 0x0203 {
                        f_dbl.on_attach_open();
                    }
                    None
                },
            );
            std::mem::forget(dblclick_handler);

            // Theme and hide attach frame initially
            form.theme_attach_panel();
            form.attach_frame.set_visible(false);
        }

        // Prompts panel event handlers (issue 058)
        {
            let f_pr = form.clone();
            let prompts_handler = nwg::full_bind_event_handler(
                &form.prompts_frame.handle,
                move |evt, _evt_data, handle| match evt {
                    nwg::Event::OnButtonClick => {
                        if handle == f_pr.prompts_btn_add.handle {
                            f_pr.on_prompts_add();
                        } else if handle == f_pr.prompts_btn_edit.handle {
                            f_pr.on_prompts_edit();
                        } else if handle == f_pr.prompts_btn_delete.handle {
                            f_pr.on_prompts_delete();
                        } else if handle == f_pr.prompts_btn_copy.handle {
                            f_pr.on_prompts_copy();
                        }
                    }
                    nwg::Event::OnListBoxSelect if handle == f_pr.prompts_list.handle => {
                        f_pr.on_prompts_selection_changed();
                    }
                    // Typing in the search field filters the list, keeping
                    // the selected prompt selected when it still matches.
                    nwg::Event::OnTextInput if handle == f_pr.prompts_search.handle => {
                        f_pr.refresh_prompts_list(f_pr.selected_prompt());
                    }
                    _ => {}
                },
            );
            std::mem::forget(prompts_handler);

            // A click on a row copies it, also when the row was already
            // selected (LBN_SELCHANGE does not fire then). The list box sets
            // the selection on the button press; keyboard moves only update
            // the preview.
            let f_click = form.clone();
            let click_handler = nwg::bind_raw_event_handler(
                &form.prompts_list.handle,
                0x40080,
                move |hwnd, msg, _wparam, lparam| {
                    // WM_LBUTTONUP = 0x0202, LB_ITEMFROMPOINT = 0x01A9
                    if msg == 0x0202 {
                        let hit = unsafe {
                            windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                                windows::Win32::Foundation::HWND(hwnd as *mut _),
                                0x01A9,
                                None,
                                Some(windows::Win32::Foundation::LPARAM(lparam)),
                            )
                        };
                        let rows = f_click.prompts_list.len();
                        if let Some(row) = crate::ui::prompts_panel::clicked_row(hit.0 as u32, rows) {
                            if f_click.prompts_list.selection() == Some(row) {
                                f_click.on_prompts_copy();
                            }
                        }
                    }
                    None
                },
            );
            match click_handler {
                Ok(h) => std::mem::forget(h),
                Err(e) => panic!("prompts list click handler: {e:?}"),
            }

            // Theme and hide the prompts frame initially
            form.theme_prompts_panel();
            form.prompts_frame.set_visible(false);
        }

        // TOTP panel event handlers
        {
            let f_totp = form.clone();
            let totp_handler = nwg::full_bind_event_handler(
                &form.totp_frame.handle,
                move |evt, _evt_data, handle| {
                    if let nwg::Event::OnButtonClick = evt {
                        if handle == f_totp.totp_btn_copy.handle {
                            f_totp.on_totp_copy();
                        } else if handle == f_totp.totp_btn_add.handle {
                            f_totp.on_totp_add();
                        } else if handle == f_totp.totp_btn_remove.handle {
                            f_totp.on_totp_remove();
                        }
                    }
                },
            );
            std::mem::forget(totp_handler);

            let f_tt = form.clone();
            let totp_timer_handler = nwg::full_bind_event_handler(
                &form.window.handle,
                move |evt, _evt_data, handle| {
                    if let nwg::Event::OnTimerTick = evt {
                        if handle == f_tt.totp_timer.handle {
                            f_tt.on_totp_timer_tick();
                        }
                    }
                },
            );
            std::mem::forget(totp_timer_handler);

            // Theme and hide TOTP frame initially
            form.theme_totp_panel();
            form.totp_frame.set_visible(false);
        }

        // Toolbar toggle buttons event handler
        {
            let f_tb = form.clone();
            let toolbar_handler = nwg::full_bind_event_handler(
                &form.window.handle,
                move |evt, _evt_data, handle| {
                    if let nwg::Event::OnButtonClick = evt {
                        if handle == f_tb.toolbar_prompts.handle {
                            f_tb.on_toggle_prompts();
                        }
                        if handle == f_tb.toolbar_vault.handle {
                            f_tb.on_toggle_vault();
                        }
                        if handle == f_tb.toolbar_totp.handle {
                            f_tb.on_toggle_totp();
                        }
                        if handle == f_tb.toolbar_attach.handle {
                            f_tb.on_toggle_attachments();
                        }
                        if handle == f_tb.toolbar_lock.handle {
                            f_tb.on_manual_lock();
                        }
                    }
                },
            );
            std::mem::forget(toolbar_handler);

            // Apply BS_OWNERDRAW to toolbar buttons and set up WM_DRAWITEM handler
            let set_ownerdraw = |btn: &nwg::Button| {
                if let nwg::ControlHandle::Hwnd(hwnd) = btn.handle {
                    let hwnd_w = windows::Win32::Foundation::HWND(hwnd as *mut _);
                    unsafe {
                        let style = windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(
                            hwnd_w,
                            windows::Win32::UI::WindowsAndMessaging::GWL_STYLE,
                        );
                        // BS_OWNERDRAW = 0x0B
                        windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(
                            hwnd_w,
                            windows::Win32::UI::WindowsAndMessaging::GWL_STYLE,
                            (style as u32 | 0x0B) as isize,
                        );
                    }
                }
            };
            set_ownerdraw(&form.toolbar_prompts);
            set_ownerdraw(&form.toolbar_vault);
            set_ownerdraw(&form.toolbar_totp);
            set_ownerdraw(&form.toolbar_attach);
            set_ownerdraw(&form.toolbar_lock);

            // Collect button HWNDs for the draw handler
            let prompts_btn_hwnd = if let nwg::ControlHandle::Hwnd(h) = form.toolbar_prompts.handle { h as isize } else { 0 };
            let vault_btn_hwnd = if let nwg::ControlHandle::Hwnd(h) = form.toolbar_vault.handle { h as isize } else { 0 };
            let totp_btn_hwnd = if let nwg::ControlHandle::Hwnd(h) = form.toolbar_totp.handle { h as isize } else { 0 };
            let attach_btn_hwnd = if let nwg::ControlHandle::Hwnd(h) = form.toolbar_attach.handle { h as isize } else { 0 };
            let lock_btn_hwnd = if let nwg::ControlHandle::Hwnd(h) = form.toolbar_lock.handle { h as isize } else { 0 };

            let f_draw = form.clone();
            let draw_handler = nwg::bind_raw_event_handler(
                &form.window.handle,
                0x40005,
                move |_hwnd, msg, _wparam, lparam| {
                    // WM_DRAWITEM = 0x002B
                    if msg != 0x002B { return None; }

                    let dis = lparam as *const windows::Win32::UI::Controls::DRAWITEMSTRUCT;
                    if dis.is_null() { return None; }
                    let dis = unsafe { &*dis };

                    let item_hwnd = dis.hwndItem.0 as isize;
                    let is_toolbar_btn =
                        item_hwnd == prompts_btn_hwnd ||
                        item_hwnd == vault_btn_hwnd ||
                        item_hwnd == totp_btn_hwnd ||
                        item_hwnd == attach_btn_hwnd ||
                        item_hwnd == lock_btn_hwnd;
                    if !is_toolbar_btn { return None; }

                    // Determine if this panel is active
                    let is_active = {
                        #[allow(unused_mut)]
                        let mut active = false;
                        if item_hwnd == prompts_btn_hwnd {
                            active = f_draw.prompts_panel.is_visible();
                        }
                        if item_hwnd == vault_btn_hwnd {
                            active = f_draw.vault_panel.is_visible();
                        }
                        if item_hwnd == attach_btn_hwnd {
                            active = f_draw.attach_panel.is_visible();
                        }
                        if item_hwnd == totp_btn_hwnd {
                            active = f_draw.totp_panel.is_visible();
                        }
                        active
                    };

                    let pal = crate::theme::current();
                    let paint = toolbar::button_paint(
                        pal,
                        toolbar::ButtonState::from_item_state(
                            dis.itemState.0,
                            is_active,
                            toolbar::is_hot(item_hwnd),
                            toolbar::is_mouse_focused(item_hwnd),
                        ),
                    );

                    // Get entry count for badge
                    let badge_count: usize = {
                        #[allow(unused_mut)]
                        let mut count = 0usize;
                        if item_hwnd == prompts_btn_hwnd {
                            count = f_draw.prompts_panel.count();
                        }
                        if item_hwnd == vault_btn_hwnd {
                            count = f_draw.vault_panel.count();
                        }
                        if item_hwnd == totp_btn_hwnd {
                            count = f_draw.settings.borrow().totp_entries.len();
                        }
                        if item_hwnd == attach_btn_hwnd {
                            count = f_draw.attachments.borrow().len();
                        }
                        count
                    };

                    unsafe {
                        use windows::Win32::Foundation::{COLORREF, RECT};
                        use windows::Win32::Graphics::Gdi::*;

                        // Strip colour behind the rounded corners, then the
                        // rounded background with its 1 px edge.
                        let strip = CreateSolidBrush(COLORREF(pal.chrome.to_colorref()));
                        FillRect(dis.hDC, &dis.rcItem, strip);
                        let _ = DeleteObject(strip.into());
                        let fill = CreateSolidBrush(COLORREF(paint.background.to_colorref()));
                        let edge = CreatePen(PS_SOLID, 1, COLORREF(paint.border.to_colorref()));
                        let (old_brush, old_pen) = (SelectObject(dis.hDC, fill.into()), SelectObject(dis.hDC, edge.into()));
                        let _ = RoundRect(
                            dis.hDC,
                            dis.rcItem.left, dis.rcItem.top, dis.rcItem.right, dis.rcItem.bottom,
                            toolbar::BUTTON_RADIUS * 2, toolbar::BUTTON_RADIUS * 2,
                        );
                        SelectObject(dis.hDC, old_brush);
                        SelectObject(dis.hDC, old_pen);
                        let _ = DeleteObject(fill.into());
                        let _ = DeleteObject(edge.into());

                        // Keyboard focus ring, inside the edge.
                        if let Some(ring) = paint.focus_ring {
                            let ring_brush = CreateSolidBrush(COLORREF(ring.to_colorref()));
                            let inner = RECT {
                                left: dis.rcItem.left + 2,
                                top: dis.rcItem.top + 2,
                                right: dis.rcItem.right - 2,
                                bottom: dis.rcItem.bottom - 2,
                            };
                            let _ = FrameRect(dis.hDC, &inner, ring_brush);
                            let _ = DeleteObject(ring_brush.into());
                        }

                        SetBkMode(dis.hDC, TRANSPARENT);
                        let text_color = paint.foreground.to_colorref();
                        SetTextColor(dis.hDC, COLORREF(text_color));

                        // Lock button: draw a padlock icon instead of text.
                        // Emoji U+1F512 does not render reliably in owner-draw
                        // buttons (default font has no color emoji fallback).
                        let is_lock_btn = item_hwnd == lock_btn_hwnd;

                        if is_lock_btn {
                            let cx = (dis.rcItem.left + dis.rcItem.right) / 2;
                            let cy = (dis.rcItem.top + dis.rcItem.bottom) / 2;
                            let stroke = COLORREF(text_color);
                            let (body, shackle) = toolbar::lock_glyph(cx, cy);

                            // Body: filled rounded rectangle
                            let body_pen = CreatePen(PS_SOLID, 1, stroke);
                            let body_brush = CreateSolidBrush(stroke);
                            let old_pen = SelectObject(dis.hDC, body_pen.into());
                            let old_brush = SelectObject(dis.hDC, body_brush.into());
                            let _ = RoundRect(dis.hDC, body.0, body.1, body.2, body.3, 2, 2);
                            SelectObject(dis.hDC, old_pen);
                            SelectObject(dis.hDC, old_brush);
                            let _ = DeleteObject(body_pen.into());
                            let _ = DeleteObject(body_brush.into());

                            // Shackle: thick arc (U-shape, open at bottom)
                            let shackle_pen = CreatePen(PS_SOLID, 2, stroke);
                            let null_brush = GetStockObject(NULL_BRUSH);
                            let old_pen = SelectObject(dis.hDC, shackle_pen.into());
                            let old_brush = SelectObject(dis.hDC, null_brush);
                            let _ = Arc(
                                dis.hDC,
                                shackle.0, shackle.1, shackle.2, shackle.3,
                                shackle.2, shackle.3, // start: right-bottom
                                shackle.0, shackle.3, // end:   left-bottom (anti-clockwise through top)
                            );
                            SelectObject(dis.hDC, old_pen);
                            SelectObject(dis.hDC, old_brush);
                            let _ = DeleteObject(shackle_pen.into());
                        } else {
                            // Get button text
                            let mut buf = [0u16; 64];
                            let len = windows::Win32::UI::WindowsAndMessaging::GetWindowTextW(
                                dis.hwndItem,
                                &mut buf,
                            );
                            let mut text_slice = buf[..len as usize].to_vec();
                            let mut rc = dis.rcItem;
                            DrawTextW(
                                dis.hDC,
                                &mut text_slice,
                                &mut rc,
                                DT_SINGLELINE | DT_VCENTER | DT_CENTER | DT_NOPREFIX,
                            );
                        }

                        // Draw badge with entry count (top-right corner)
                        if badge_count > 0 {
                            let badge_text = format!("{}", badge_count);
                            let badge_w = (badge_text.len() as i32) * 7 + 6;
                            let badge_h = 13i32;
                            let badge_x = dis.rcItem.right - badge_w - 2;
                            let badge_y = dis.rcItem.top + 1;

                            let badge_rect = RECT {
                                left: badge_x,
                                top: badge_y,
                                right: badge_x + badge_w,
                                bottom: badge_y + badge_h,
                            };

                            // Draw rounded badge background
                            let badge_bg = paint.badge_background.to_colorref();
                            let badge_fg = paint.badge_foreground.to_colorref();
                            let badge_brush = CreateSolidBrush(COLORREF(badge_bg));
                            let badge_pen = CreatePen(PS_SOLID, 0, COLORREF(badge_bg));
                            let old_brush = SelectObject(dis.hDC, badge_brush.into());
                            let old_pen = SelectObject(dis.hDC, badge_pen.into());
                            let _ = RoundRect(dis.hDC, badge_x, badge_y, badge_x + badge_w, badge_y + badge_h, 8, 8);
                            SelectObject(dis.hDC, old_brush);
                            SelectObject(dis.hDC, old_pen);
                            let _ = DeleteObject(badge_brush.into());
                            let _ = DeleteObject(badge_pen.into());

                            // Draw badge text
                            let badge_font = CreateFontW(
                                -10, 0, 0, 0,
                                FW_BOLD.0 as i32,
                                0, 0, 0,
                                DEFAULT_CHARSET,
                                OUT_DEFAULT_PRECIS,
                                CLIP_DEFAULT_PRECIS,
                                CLEARTYPE_QUALITY,
                                0,
                                windows::core::PCWSTR(std::ptr::null()),
                            );
                            let old_font = SelectObject(dis.hDC, badge_font.into());
                            SetTextColor(dis.hDC, COLORREF(badge_fg));
                            let mut badge_wide: Vec<u16> = badge_text.encode_utf16().collect();
                            let mut badge_rc = badge_rect;
                            DrawTextW(
                                dis.hDC,
                                &mut badge_wide,
                                &mut badge_rc,
                                DT_SINGLELINE | DT_VCENTER | DT_CENTER | DT_NOPREFIX,
                            );
                            SelectObject(dis.hDC, old_font);
                            let _ = DeleteObject(badge_font.into());
                        }
                    }

                    Some(1) // handled
                },
            );
            std::mem::forget(draw_handler);

            // Hover tracking and tooltips for the same buttons (toolbar.rs).
            form.install_toolbar_chrome();
        }

        // Resize grip of the side panels (panel_grip.rs).
        Self::bind_panel_grip(form);
    }
}
