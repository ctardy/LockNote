// Auto-save, clipboard guard and global hotkey.
// Part of the EditorForm implementation, split out of the former editor.rs.

#[allow(unused_imports)]
use super::*;

impl EditorForm {
    // =====================================================================
    // Auto-save
    // =====================================================================

    pub(super) fn on_autosave(&self) {
        // Never auto-save when locked (password cleared by auto-lock)
        if self.password.borrow().is_empty() {
            return;
        }

        // Fix #036: try (non-blocking) to take the persistence mutex. If
        // another persistence op (unlock / autolock / manual save) is in
        // flight, skip this tick and retry at the next autosave interval.
        let Some(_persistence_guard) = crate::features::autosave::AutoSave::acquire_lock() else {
            return;
        };

        let _ = self.on_save();
        self.autosave.borrow_mut().mark_saved();
    }

    // =====================================================================
    // Clipboard guard
    // =====================================================================

    pub(super) fn on_clipboard_clear(&self) {
        // Only clear if the clipboard still holds our snapshot. The guard
        // header docs this behaviour — without the check, a user who
        // copied another value (a URL, a code from a different app) in
        // the meantime would silently lose it when the timer fires.
        let snapshot_matches_clipboard = {
            let guard = self.clipboard_guard.borrow();
            match (guard.should_clear(), clipboard_win::get_clipboard_string().ok()) {
                (Some(snap), Some(current)) => snap == current,
                _ => false,
            }
        };

        if snapshot_matches_clipboard {
            let cleared = unsafe {
                use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard};
                match OpenClipboard(None) {
                    Err(e) => Err(format!("OpenClipboard {:?}", e)),
                    Ok(()) => {
                        let emptied = EmptyClipboard().map_err(|e| format!("EmptyClipboard {:?}", e));
                        if let Err(e) = CloseClipboard() {
                            eprintln!("WARN clipboard: auto-clear, CloseClipboard failed err={:?}", e);
                        }
                        emptied
                    }
                }
            };
            if let Err(e) = cleared {
                // The secret is still there: keep the guard armed, the next
                // tick tries again (another app may hold the clipboard).
                eprintln!("WARN clipboard: auto-clear failed, retried next tick err={}", e);
                return;
            }
            eprintln!("INFO clipboard: auto-cleared");
        }
        // Reset the guard so it does not fire again, also when the user
        // had replaced the clipboard content in the meantime.
        self.clipboard_guard.borrow_mut().mark_cleared();
    }

    // =====================================================================
    // Global hotkey
    // =====================================================================

    pub(super) fn on_global_hotkey(&self) {
        if self.window.visible() {
            self.window.set_visible(false);
        } else {
            // If locked by auto-lock, show with overlay
            if *self.is_locked.borrow() {
                self.window.set_visible(true);
                unsafe {
                    let raw_hwnd = self.window.handle.hwnd().unwrap();
                    let hwnd = windows::Win32::Foundation::HWND(raw_hwnd as *mut _);
                    windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(hwnd);
                }
                self.lock_input.set_focus();
                return;
            }

            // If password was cleared, re-authenticate
            if self.password.borrow().is_empty() {
                if !self.unlock_from_disk() {
                    return;
                }
            }
            self.window.set_visible(true);
            // Bring to foreground
            unsafe {
                let raw_hwnd = self.window.handle.hwnd().unwrap();
                let hwnd = windows::Win32::Foundation::HWND(raw_hwnd as *mut _);
                windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(hwnd);
            }
        }
    }
}
