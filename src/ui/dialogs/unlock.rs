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

        let mut window = Default::default();
        nwg::Window::builder()
            .size((380, 200))
            .position((300, 250))
            .title("Unlock")
            .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
            .build(&mut window)
            .expect("Failed to build UnlockDialog window");

        let mut font = Default::default();
        nwg::Font::builder()
            .family("Segoe UI")
            .size(16)
            .build(&mut font)
            .expect("Failed to build font");

        // Password label
        let mut lbl_password = Default::default();
        nwg::Label::builder()
            .text("Password:")
            .parent(&window)
            .position((20, 15))
            .size((340, 20))
            .font(Some(&font))
            .build(&mut lbl_password)
            .expect("Failed to build label");

        // Password input
        let mut txt_password = Default::default();
        nwg::TextInput::builder()
            .parent(&window)
            .position((20, 40))
            .size((340, 25))
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
            .position((20, 75))
            .size((340, 20))
            .font(Some(&font))
            .build(&mut lbl_status)
            .expect("Failed to build status label");

        // Unlock button
        let mut btn_unlock = Default::default();
        nwg::Button::builder()
            .text("Unlock")
            .parent(&window)
            .position((170, 120))
            .size((85, 30))
            .font(Some(&font))
            .build(&mut btn_unlock)
            .expect("Failed to build Unlock button");

        // Cancel button
        let mut btn_cancel = Default::default();
        nwg::Button::builder()
            .text("Cancel")
            .parent(&window)
            .position((270, 120))
            .size((85, 30))
            .font(Some(&font))
            .build(&mut btn_cancel)
            .expect("Failed to build Cancel button");

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
}
