// UI Dialogs — Password creation, unlock, settings, about, etc.

pub mod create_password;
pub mod unlock;
pub mod close_confirm;
pub mod goto_line;
pub mod find_replace;
pub mod about;
pub mod settings_dialog;
pub mod preferences_dialog;
pub mod security_dialog;
pub mod temp_files_dialog;

pub mod drop_choice;

pub mod rename_tab;

pub mod secret_edit;

pub mod prompt_edit;

pub mod ssh_keygen;

pub mod totp_setup;

pub mod totp_verify;

pub mod totp_manager;

pub mod backups_dialog;

pub mod export_encoding;

pub mod confirm_password;

pub mod backup_password;

pub mod password_generator;

use native_windows_gui as nwg;

pub mod metrics;
pub mod style;

#[cfg(test)]
mod snapshot;

pub use style::{mark_error, set_frame_color};

/// Give a dialog the shared look. Call right after the window is built, before
/// its controls: the styling pass runs from the first message of the dialog's
/// loop, when every control exists (see `style.rs`).
///
/// Dark title bar and window background, then, per control: palette colors for
/// labels / edit boxes / lists (`WM_CTLCOLOR*`), DPI scaling, the font
/// hierarchy, reflow of text that does not fit, uniform footer buttons, dark
/// native controls (Dark theme), Esc = close, centering on the main window.
pub fn apply_dialog_theme(window: &nwg::Window) {
    apply_window_chrome(window);
    style::register(window);

    // Install raw event handler for the shared look
    let raw = nwg::bind_raw_event_handler(
        &window.handle,
        0x30000, // unique handler ID
        move |hwnd, msg, wparam, lparam| style::handle_message(hwnd as isize, msg, wparam as usize, lparam),
    );
    // Keep the handler alive for the dialog lifetime
    std::mem::forget(raw);
}

/// Title bar and background brush only, for windows that lay out and paint
/// their own content (the unlock card): no DPI rescale, no restyled controls.
pub fn apply_window_chrome(window: &nwg::Window) {
    use nwg::ControlHandle;
    use crate::theme;

    let pal = theme::current();

    // Dark/light title bar (no-op look change in the Light theme)
    crate::ui::dark_chrome::apply_title_bar(&window.handle);

    // Set window background brush
    if let ControlHandle::Hwnd(hwnd) = window.handle {
        let hwnd_w = windows::Win32::Foundation::HWND(hwnd as *mut _);
        // Shared, colour-keyed brush: nothing to leak when the theme changes.
        let brush = windows::Win32::Graphics::Gdi::HBRUSH(
            crate::ui::lock_card::cached_brush(pal.background) as *mut _,
        );
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::SetClassLongPtrW(
                hwnd_w,
                windows::Win32::UI::WindowsAndMessaging::GCL_HBRBACKGROUND,
                brush.0 as isize,
            );
            // Force repaint
            let _ = windows::Win32::Graphics::Gdi::InvalidateRect(Some(hwnd_w), None, true);
        }
    }
}

/// Disable Windows visual themes on a control so WM_CTLCOLOR* messages are respected.
/// Required for checkboxes in dark mode — visual themes override text/background colors.
pub fn disable_control_theme(handle: nwg::ControlHandle) {
    if let nwg::ControlHandle::Hwnd(hwnd) = handle {
        let hwnd_w = windows::Win32::Foundation::HWND(hwnd as *mut _);
        let empty = [0u16; 1];
        unsafe {
            let _ = windows::Win32::UI::Controls::SetWindowTheme(
                hwnd_w,
                windows::core::PCWSTR(empty.as_ptr()),
                windows::core::PCWSTR(empty.as_ptr()),
            );
        }
    }
}

/// Bind Enter key to trigger a button click.
///
/// nwg's dispatch loop uses `IsDialogMessage` which intercepts VK_RETURN
/// and sends WM_COMMAND with IDOK (1) to the parent window.
/// This function installs a handler on the **window** to catch that command.
///
/// The button also becomes the dialog's primary button (accent fill).
pub fn bind_enter_to_button(window_handle: nwg::ControlHandle, button_handle: nwg::ControlHandle) {
    style::set_primary(window_handle, button_handle);
    let raw = nwg::bind_raw_event_handler(
        &window_handle,
        0x20000,
        move |_hwnd, msg, wparam, _lparam| {
            // WM_COMMAND = 0x0111, IDOK = 1 (sent by IsDialogMessage when Enter is pressed)
            if msg == 0x0111 && (wparam & 0xFFFF) == 1 {
                if let nwg::ControlHandle::Hwnd(btn_hwnd) = button_handle {
                    unsafe {
                        windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                            windows::Win32::Foundation::HWND(btn_hwnd as *mut _),
                            0x00F5, // BM_CLICK
                            Some(windows::Win32::Foundation::WPARAM(0)),
                            Some(windows::Win32::Foundation::LPARAM(0)),
                        );
                    }
                }
                return Some(0);
            }
            None
        },
    );
    std::mem::forget(raw);
}
