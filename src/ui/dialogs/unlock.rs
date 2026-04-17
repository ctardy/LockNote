// UnlockDialog — Password prompt with attempt limiting
//
// Max 5 attempts. On each failure displays "Wrong password (N/5)".
// Returns Option<(String, String)>: (password, decrypted_text), or None if cancelled/max reached.
// Clears TextBox on drop for security.

use native_windows_gui as nwg;
use std::cell::RefCell;
use std::rc::Rc;
use zeroize::Zeroizing;

const MAX_ATTEMPTS: u32 = 5;

/// Exponential backoff schedule for failed password attempts.
/// Index = attempts counter after the failure (1-based): 1->0s, 2->1s, 3->2s, 4->4s, 5->8s.
fn backoff_seconds(att: u32) -> u32 {
    match att {
        0 | 1 => 0,
        2 => 1,
        3 => 2,
        4 => 4,
        _ => 8,
    }
}

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
        let win_h: i32 = 310;
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
            .position((200, 230))
            .size((90, 32))
            .font(Some(&font))
            .build(&mut btn_unlock)
            .expect("Failed to build Unlock button");

        // Cancel button
        let mut btn_cancel = Default::default();
        nwg::Button::builder()
            .text("Cancel")
            .parent(&window)
            .position((305, 230))
            .size((90, 32))
            .font(Some(&font))
            .build(&mut btn_cancel)
            .expect("Failed to build Cancel button");

        // --- Enter key → Unlock button ---
        super::bind_enter_to_button(window.handle, btn_unlock.handle);

        // --- Backoff countdown timer (ticks every 1s while rate-limited) ---
        let mut backoff_timer = nwg::AnimationTimer::default();
        nwg::AnimationTimer::builder()
            .parent(&window)
            .interval(std::time::Duration::from_secs(1))
            .active(false)
            .build(&mut backoff_timer)
            .expect("Failed to build backoff timer");

        // Remaining seconds in the current backoff window. 0 = no backoff active.
        let backoff_remaining: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));

        // Result: Some((password, decrypted_text))
        let result: Rc<RefCell<Option<(String, String)>>> = Rc::new(RefCell::new(None));
        let attempts: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));

        let window_handle = window.handle;
        let txt_password_handle = txt_password.handle;
        let result_clone = result.clone();
        let attempts_clone = attempts.clone();
        let backoff_remaining_clone = backoff_remaining.clone();

        let handler = nwg::full_bind_event_handler(&window_handle, move |evt, _evt_data, handle| {
            match evt {
                nwg::Event::OnTimerTick => {
                    if handle == backoff_timer.handle {
                        let mut remaining = backoff_remaining_clone.borrow_mut();
                        if *remaining > 0 {
                            *remaining -= 1;
                        }
                        if *remaining == 0 {
                            backoff_timer.stop();
                            btn_unlock.set_enabled(true);
                            txt_password.set_enabled(true);
                            lbl_status.set_text("");
                            // Restore focus to the password field
                            if let nwg::ControlHandle::Hwnd(h) = txt_password.handle {
                                unsafe {
                                    let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(
                                        Some(windows::Win32::Foundation::HWND(h as *mut _)),
                                    );
                                }
                            }
                        } else {
                            lbl_status.set_text(&format!("Retry in {}s…", *remaining));
                        }
                    }
                }
                nwg::Event::OnButtonClick => {
                    if handle == btn_unlock.handle {
                        // Wrap the password in Zeroizing so it is wiped from the stack
                        // whether decryption succeeds or fails. On success we clone
                        // the inner String out to the caller; the local wrapper still
                        // zeroes its buffer on drop.
                        let pw: Zeroizing<String> = Zeroizing::new(txt_password.text());
                        lbl_status.set_text("Decrypting...");

                        match crate::crypto::decrypt(&data, pw.as_str()) {
                            Ok(plaintext) => {
                                *result_clone.borrow_mut() = Some(((*pw).clone(), plaintext));
                                txt_password.set_text("");
                                nwg::stop_thread_dispatch();
                            }
                            Err(err) => {
                                // Classify: HmacMismatch or Decryption -> generic
                                // "wrong password" (anti-oracle). Anything else is a
                                // real crypto error and we surface it so the user
                                // can report the issue.
                                let is_wrong_password = matches!(
                                    &err,
                                    crate::crypto::CryptoError::HmacMismatch
                                        | crate::crypto::CryptoError::Decryption(_)
                                );
                                let mut att = attempts_clone.borrow_mut();
                                *att += 1;
                                txt_password.set_text("");

                                if !is_wrong_password {
                                    // Structural crypto error (bad params, KDF failure, ...)
                                    lbl_status.set_text(&format!("Crypto error: {}", err));
                                    // Do not count this as a password attempt toward max-lock:
                                    // it's not a guess, it's a malformed payload.
                                    *att -= 1;
                                } else if *att >= MAX_ATTEMPTS {
                                    lbl_status.set_text("Maximum attempts reached.");
                                    nwg::modal_info_message(
                                        &window_handle,
                                        "Locked Out",
                                        "Maximum password attempts reached. The application will close.",
                                    );
                                    nwg::stop_thread_dispatch();
                                } else {
                                    let delay = backoff_seconds(*att);
                                    if delay == 0 {
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
                                    } else {
                                        // Start exponential backoff: disable controls and
                                        // run the 1s tick timer until the delay elapses.
                                        btn_unlock.set_enabled(false);
                                        txt_password.set_enabled(false);
                                        *backoff_remaining_clone.borrow_mut() = delay;
                                        lbl_status.set_text(&format!(
                                            "Wrong password ({}/{}) — Retry in {}s…",
                                            att, MAX_ATTEMPTS, delay,
                                        ));
                                        backoff_timer.start();
                                    }
                                }
                            }
                        }
                    } else if handle == btn_cancel.handle {
                        backoff_timer.stop();
                        txt_password.set_text("");
                        nwg::stop_thread_dispatch();
                    }
                }
                nwg::Event::OnWindowClose => {
                    backoff_timer.stop();
                    txt_password.set_text("");
                    nwg::stop_thread_dispatch();
                }
                _ => {}
            }
        });

        // Force focus on password input before entering the message loop
        if let nwg::ControlHandle::Hwnd(h) = txt_password_handle {
            unsafe {
                let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(
                    Some(windows::Win32::Foundation::HWND(h as *mut _)),
                );
            }
        }

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

    #[test]
    fn backoff_schedule() {
        assert_eq!(backoff_seconds(0), 0);
        assert_eq!(backoff_seconds(1), 0);
        assert_eq!(backoff_seconds(2), 1);
        assert_eq!(backoff_seconds(3), 2);
        assert_eq!(backoff_seconds(4), 4);
        assert_eq!(backoff_seconds(5), 8);
        assert_eq!(backoff_seconds(10), 8);
    }
}
