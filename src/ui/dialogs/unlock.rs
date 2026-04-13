// UnlockDialog — Password prompt with attempt limiting
//
// Max 5 attempts. On each failure displays "Wrong password (N/5)".
// Returns Option<(String, String)>: (password, decrypted_text), or None if cancelled/max reached.
// Clears TextBox on drop for security.

use native_windows_gui as nwg;
use std::cell::RefCell;
use std::rc::Rc;

const MAX_ATTEMPTS: u32 = 5;

pub struct UnlockDialog;

impl UnlockDialog {
    /// Show the unlock dialog.
    ///
    /// `encrypted_data` is the raw encrypted payload.
    /// Uses `crate::crypto::decrypt` to attempt decryption.
    /// Returns Some((password, decrypted_text)) on success, None on cancel/max attempts.
    pub fn show(encrypted_data: &[u8]) -> Option<(String, String)> {
        let data = encrypted_data.to_vec();

        // --- Center on screen ---
        #[link(name = "user32")]
        extern "system" {
            fn GetSystemMetrics(index: i32) -> i32;
        }
        let win_w: i32 = 420;
        let win_h: i32 = 280;
        let screen_w = unsafe { GetSystemMetrics(0) }; // SM_CXSCREEN
        let screen_h = unsafe { GetSystemMetrics(1) }; // SM_CYSCREEN
        let pos_x = (screen_w - win_w) / 2;
        let pos_y = (screen_h - win_h) / 2;

        let mut window = Default::default();
        nwg::Window::builder()
            .size((win_w, win_h))
            .position((pos_x, pos_y))
            .title("LockNote")
            .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
            .build(&mut window)
            .expect("Failed to build UnlockDialog window");

        super::apply_dialog_theme(&window);

        // --- Fonts ---
        let mut font = Default::default();
        nwg::Font::builder()
            .family("Segoe UI")
            .size(16)
            .build(&mut font)
            .expect("Failed to build font");

        let mut font_title = Default::default();
        nwg::Font::builder()
            .family("Segoe UI")
            .size(24)
            .weight(700)
            .build(&mut font_title)
            .expect("Failed to build title font");

        let mut font_desc = Default::default();
        nwg::Font::builder()
            .family("Segoe UI")
            .size(15)
            .build(&mut font_desc)
            .expect("Failed to build description font");

        let content_w: i32 = 370;
        let margin: i32 = 25;

        // --- Welcome title ---
        let mut _lbl_title = Default::default();
        nwg::Label::builder()
            .text("Welcome back")
            .parent(&window)
            .position((margin, 18))
            .size((content_w, 30))
            .font(Some(&font_title))
            .build(&mut _lbl_title)
            .expect("Failed to build title label");

        // --- Description ---
        let mut _lbl_desc = Default::default();
        nwg::Label::builder()
            .text("Enter your password to unlock your notes.")
            .parent(&window)
            .position((margin, 55))
            .size((content_w, 22))
            .font(Some(&font_desc))
            .build(&mut _lbl_desc)
            .expect("Failed to build description label");

        // --- Separator ---
        let mut _separator = Default::default();
        nwg::Frame::builder()
            .parent(&window)
            .position((margin, 85))
            .size((content_w, 1))
            .build(&mut _separator)
            .expect("Failed to build separator");

        // Password label
        let mut _lbl_password = Default::default();
        nwg::Label::builder()
            .text("Password:")
            .parent(&window)
            .position((margin, 100))
            .size((content_w, 20))
            .font(Some(&font))
            .build(&mut _lbl_password)
            .expect("Failed to build label");

        // Password input
        let mut txt_password = Default::default();
        nwg::TextInput::builder()
            .parent(&window)
            .position((margin, 125))
            .size((content_w, 28))
            .password(Some('*'))
            .font(Some(&font))
            .focus(true)
            .build(&mut txt_password)
            .expect("Failed to build password input");

        // Attempts / status label
        let mut lbl_status = Default::default();
        nwg::Label::builder()
            .text("")
            .parent(&window)
            .position((margin, 160))
            .size((content_w, 20))
            .font(Some(&font))
            .build(&mut lbl_status)
            .expect("Failed to build status label");

        // Unlock button
        let mut btn_unlock = Default::default();
        nwg::Button::builder()
            .text("Unlock")
            .parent(&window)
            .position((200, 200))
            .size((90, 32))
            .font(Some(&font))
            .build(&mut btn_unlock)
            .expect("Failed to build Unlock button");

        // Cancel button
        let mut btn_cancel = Default::default();
        nwg::Button::builder()
            .text("Cancel")
            .parent(&window)
            .position((305, 200))
            .size((90, 32))
            .font(Some(&font))
            .build(&mut btn_cancel)
            .expect("Failed to build Cancel button");

        // --- Enter key → Unlock button ---
        {
            let btn_handle = btn_unlock.handle;
            let enter_handler = nwg::bind_raw_event_handler(
                &txt_password.handle,
                0x20001,
                move |_hwnd, msg, wparam, _lparam| {
                    let is_enter = (msg == 0x0100 && wparam == 0x0D)
                                || (msg == 0x0102 && wparam == 0x0D);
                    if is_enter {
                        if let nwg::ControlHandle::Hwnd(btn_hwnd) = btn_handle {
                            unsafe {
                                let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                                    Some(windows::Win32::Foundation::HWND(btn_hwnd as *mut _)),
                                    0x00F5, // BM_CLICK
                                    windows::Win32::Foundation::WPARAM(0),
                                    windows::Win32::Foundation::LPARAM(0),
                                );
                            }
                        }
                        return Some(0);
                    }
                    None
                },
            );
            std::mem::forget(enter_handler);
        }

        // Result: Some((password, decrypted_text))
        let result: Rc<RefCell<Option<(String, String)>>> = Rc::new(RefCell::new(None));
        let attempts: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));

        let window_handle = window.handle;
        let result_clone = result.clone();
        let attempts_clone = attempts.clone();

        let handler = nwg::full_bind_event_handler(&window_handle, move |evt, _evt_data, handle| {
            match evt {
                nwg::Event::OnButtonClick => {
                    if handle == btn_unlock.handle {
                        let pw = txt_password.text();
                        lbl_status.set_text("Decrypting...");

                        match crate::crypto::decrypt(&data, &pw) {
                            Some(plaintext) => {
                                *result_clone.borrow_mut() = Some((pw, plaintext));
                                txt_password.set_text("");
                                nwg::stop_thread_dispatch();
                            }
                            None => {
                                let mut att = attempts_clone.borrow_mut();
                                *att += 1;
                                txt_password.set_text("");

                                if *att >= MAX_ATTEMPTS {
                                    lbl_status.set_text("Maximum attempts reached.");
                                    nwg::modal_info_message(
                                        &window_handle,
                                        "Locked Out",
                                        "Maximum password attempts reached. The application will close.",
                                    );
                                    nwg::stop_thread_dispatch();
                                } else {
                                    lbl_status.set_text(
                                        &format!("Wrong password ({}/{})", att, MAX_ATTEMPTS),
                                    );
                                    if let nwg::ControlHandle::Hwnd(pwd_hwnd) = txt_password.handle {
                                        unsafe {
                                            let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(
                                                Some(windows::Win32::Foundation::HWND(pwd_hwnd as *mut _)),
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    } else if handle == btn_cancel.handle {
                        txt_password.set_text("");
                        nwg::stop_thread_dispatch();
                    }
                }
                nwg::Event::OnWindowClose => {
                    txt_password.set_text("");
                    nwg::stop_thread_dispatch();
                }
                _ => {}
            }
        });

        nwg::dispatch_thread_events();
        nwg::unbind_event_handler(&handler);

        let out = result.borrow_mut().take();
        out
    }
}

// ---------------------------------------------------------------------------
// Free function (convenience API expected by callers)
// ---------------------------------------------------------------------------

/// Show the unlock dialog. Convenience wrapper.
pub fn show(encrypted_data: &[u8]) -> Option<(String, String)> {
    UnlockDialog::show(encrypted_data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_attempts_constant() {
        assert_eq!(MAX_ATTEMPTS, 5);
    }

    #[test]
    fn max_attempts_is_reasonable() {
        assert!(MAX_ATTEMPTS >= 3, "MAX_ATTEMPTS should be at least 3");
        assert!(MAX_ATTEMPTS <= 10, "MAX_ATTEMPTS should be at most 10");
    }
}
