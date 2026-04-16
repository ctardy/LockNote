// UI Dialogs — Password creation, unlock, settings, about, etc.

pub mod create_password;
pub mod unlock;
pub mod close_confirm;
pub mod goto_line;
pub mod about;
pub mod settings_dialog;
pub mod preferences_dialog;
pub mod security_dialog;

use native_windows_gui as nwg;

/// Apply theme colors to a dialog window and its child controls.
pub fn apply_dialog_theme(window: &nwg::Window) {
    use nwg::ControlHandle;
    use crate::theme;

    let pal = theme::current();

    if let ControlHandle::Hwnd(hwnd) = window.handle {
        let hwnd_w = windows::Win32::Foundation::HWND(hwnd as *mut _);
        let brush = unsafe {
            windows::Win32::Graphics::Gdi::CreateSolidBrush(
                windows::Win32::Foundation::COLORREF(pal.background.to_colorref()),
            )
        };
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::SetClassLongPtrW(
                hwnd_w,
                windows::Win32::UI::WindowsAndMessaging::GCL_HBRBACKGROUND,
                brush.0 as isize,
            );
            let _ = windows::Win32::Graphics::Gdi::InvalidateRect(Some(hwnd_w), None, true);
        }
    }

    let bg_colorref = pal.background.to_colorref();
    let text_colorref = pal.text_primary.to_colorref();
    let input_bg_colorref = pal.input_background.to_colorref();

    let bg_brush = unsafe {
        windows::Win32::Graphics::Gdi::CreateSolidBrush(
            windows::Win32::Foundation::COLORREF(bg_colorref),
        )
    };
    let input_brush = unsafe {
        windows::Win32::Graphics::Gdi::CreateSolidBrush(
            windows::Win32::Foundation::COLORREF(input_bg_colorref),
        )
    };

    let raw = nwg::bind_raw_event_handler(
        &window.handle,
        0x30000,
        move |_hwnd, msg, _wparam, _lparam| {
            const WM_CTLCOLORSTATIC: u32 = 0x0138;
            const WM_CTLCOLOREDIT: u32 = 0x0133;
            const WM_CTLCOLORBTN: u32 = 0x0135;

            match msg {
                WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
                    let hdc = _wparam as *mut std::ffi::c_void;
                    let hdc = windows::Win32::Graphics::Gdi::HDC(hdc);
                    unsafe {
                        windows::Win32::Graphics::Gdi::SetTextColor(
                            hdc,
                            windows::Win32::Foundation::COLORREF(text_colorref),
                        );
                        windows::Win32::Graphics::Gdi::SetBkColor(
                            hdc,
                            windows::Win32::Foundation::COLORREF(bg_colorref),
                        );
                        windows::Win32::Graphics::Gdi::SetBkMode(
                            hdc,
                            windows::Win32::Graphics::Gdi::TRANSPARENT,
                        );
                    }
                    Some(bg_brush.0 as isize)
                }
                WM_CTLCOLOREDIT => {
                    let hdc = _wparam as *mut std::ffi::c_void;
                    let hdc = windows::Win32::Graphics::Gdi::HDC(hdc);
                    unsafe {
                        windows::Win32::Graphics::Gdi::SetTextColor(
                            hdc,
                            windows::Win32::Foundation::COLORREF(text_colorref),
                        );
                        windows::Win32::Graphics::Gdi::SetBkColor(
                            hdc,
                            windows::Win32::Foundation::COLORREF(input_bg_colorref),
                        );
                    }
                    Some(input_brush.0 as isize)
                }
                _ => None,
            }
        },
    );
    std::mem::forget(raw);
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
pub fn bind_enter_to_button(window_handle: nwg::ControlHandle, button_handle: nwg::ControlHandle) {
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
