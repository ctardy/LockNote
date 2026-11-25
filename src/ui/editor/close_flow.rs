// Window state and close flow.
// Part of the EditorForm implementation, split out of the former editor.rs.

#[allow(unused_imports)]
use super::*;

impl EditorForm {
    // =====================================================================
    // Close flow
    // =====================================================================

    pub(super) fn save_window_state(&self) {
        use nwg::ControlHandle;
        if let ControlHandle::Hwnd(hwnd) = self.window.handle {
            let hwnd = windows::Win32::Foundation::HWND(hwnd as *mut _);
            let mut wp = windows::Win32::UI::WindowsAndMessaging::WINDOWPLACEMENT::default();
            wp.length = std::mem::size_of::<windows::Win32::UI::WindowsAndMessaging::WINDOWPLACEMENT>() as u32;
            let ok = unsafe {
                windows::Win32::UI::WindowsAndMessaging::GetWindowPlacement(hwnd, &mut wp)
            };
            match ok {
                Ok(()) => {
                    let rc = wp.rcNormalPosition;
                    let g = crate::ui::window_geometry::geometry_from_placement(
                        wp.showCmd,
                        wp.flags.0,
                        crate::ui::window_geometry::Rect {
                            left: rc.left,
                            top: rc.top,
                            right: rc.right,
                            bottom: rc.bottom,
                        },
                    );
                    let mut settings = self.settings.borrow_mut();
                    settings.window_width = g.w;
                    settings.window_height = g.h;
                    settings.window_x = g.x;
                    settings.window_y = g.y;
                    settings.window_maximized = g.maximized;
                }
                Err(e) => eprintln!("window: GetWindowPlacement failed err={:?}", e),
            }
        }
    }

    /// Handle close request. Returns `true` if the app should close,
    /// `false` if the user cancelled.
    pub(super) fn on_close(&self) -> bool {
        // A modal dialog runs its own message loop: closing now would wipe
        // the password and the text, then stop that loop instead of the
        // application's, leaving an empty editor that cannot save.
        if self.modal_dialog_open.get() {
            eprintln!("INFO close: refused while a modal dialog is open");
            return false;
        }

        // If locked, data was already saved by on_autolock — just exit.
        // Password is zeroized, so saving now would corrupt the file.
        if *self.is_locked.borrow() {
            // Retry what the lock could not remove; the list cannot be saved
            // (password wiped) and is retried at the next unlock.
            self.unload_agent_keys(false);
            return true;
        }

        // Capture window position/size before any save so it is included
        self.save_window_state();

        let modified = self.is_modified.load(Ordering::Relaxed);
        let save_on_close = self.settings.borrow().save_on_close;

        let mut user_declined_save = false;

        if modified {
            match save_on_close {
                crate::settings::CloseAction::Always => {
                    let _ = self.on_save();
                }
                crate::settings::CloseAction::Ask => {
                    // Show close confirmation dialog
                    let params = nwg::MessageParams {
                        title: "Unsaved Changes",
                        content: "You have unsaved changes. Save before closing?",
                        buttons: nwg::MessageButtons::YesNoCancel,
                        icons: nwg::MessageIcons::Warning,
                    };
                    match nwg::message(&params) {
                        nwg::MessageChoice::Yes => {
                            let _ = self.on_save();
                        }
                        nwg::MessageChoice::No => {
                            user_declined_save = true;
                        }
                        _ => {
                            return false; // Cancel — don't close
                        }
                    }
                }
                crate::settings::CloseAction::Never => {
                    user_declined_save = true;
                }
            }
        }

        // Past the Cancel branch (closing for sure) and before the final save,
        // so the saved list matches the agent.
        self.unload_agent_keys(true);

        // Always do a final save to persist settings (window state, etc.)
        // unless the user explicitly declined saving.
        if !user_declined_save {
            let _ = self.on_save();
        }

        // Clear sensitive data from memory
        {
            use zeroize::Zeroize;
            let mut pw = self.password.borrow_mut();
            pw.zeroize();
        }

        // Clear editor text
        self.editor.set_text("");

        // The swap helper that moves the .tmp over the .exe is not launched
        // here: `ui::run` does it once, after the message loop, for every
        // way out of the application (storage::promote_pending_on_exit).

        true
    }
}
