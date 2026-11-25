// TotpSetupDialog — TOTP setup with secret generation and verification
//
// Generates random 20-byte secret if none provided.
// Displays: Base32 secret, otpauth URI, test code input.
// Copy button for Base32 secret.
// Verify before confirm: user must enter a valid code before Confirm enables.
// Returns Option<TotpEntry>: the entry, or None if cancelled.

use native_windows_gui as nwg;
use std::cell::RefCell;
use std::rc::Rc;

// Re-use the TOTP module
use crate::features::totp::{self, TotpEntry};

fn t(key: &str) -> String {
    crate::features::i18n::get(key).to_string()
}

pub struct TotpSetupDialog;

impl TotpSetupDialog {
    /// Show the TOTP setup dialog.
    ///
    /// `name` is the label for the entry.
    /// `existing_secret` pre-fills if Some, otherwise generates a new one.
    /// Returns Some(entry) on confirmed + verified, None on Cancel.
    pub fn show(name: &str, existing_secret: Option<&str>) -> Option<TotpEntry> {
        let secret = match existing_secret {
            Some(s) => s.to_string(),
            None => totp::generate_secret(),
        };

        let uri = totp::build_uri(name, &secret, Some("LockNote"));

        let mut window = Default::default();
        nwg::Window::builder()
            .size((500, 380))
            .position((250, 180))
            .title(&t("2fa.setup.title"))
            .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
            .build(&mut window)
            .expect("Failed to build TotpSetupDialog window");

        super::apply_dialog_theme(&window);

        let mut font = Default::default();
        nwg::Font::builder()
            .family("Segoe UI")
            .size(16)
            .build(&mut font)
            .expect("Failed to build font");

        let mut font_mono = Default::default();
        nwg::Font::builder()
            .family("Consolas")
            .size(14)
            .build(&mut font_mono)
            .expect("Failed to build mono font");

        // --- Name label ---
        let mut lbl_name = Default::default();
        nwg::Label::builder()
            .text(&format!("Setting up TOTP for: {}", name))
            .parent(&window)
            .position((20, 15))
            .size((460, 20))
            .font(Some(&font))
            .build(&mut lbl_name)
            .expect("Failed to build name label");

        // --- Secret label ---
        let mut lbl_secret_title = Default::default();
        nwg::Label::builder()
            .text(&t("2fa.setup.secret"))
            .parent(&window)
            .position((20, 50))
            .size((460, 20))
            .font(Some(&font))
            .build(&mut lbl_secret_title)
            .expect("Failed to build secret title");

        let mut txt_secret = Default::default();
        nwg::TextInput::builder()
            .text(&secret)
            .parent(&window)
            .position((20, 73))
            .size((370, 25))
            .font(Some(&font_mono))
            .readonly(true)
            .build(&mut txt_secret)
            .expect("Failed to build secret text");

        // Copy button
        let mut btn_copy = Default::default();
        nwg::Button::builder()
            .text(&t("2fa.setup.copy"))
            .parent(&window)
            .position((400, 73))
            .size((75, 25))
            .font(Some(&font))
            .build(&mut btn_copy)
            .expect("Failed to build copy button");

        // --- URI label ---
        let mut lbl_uri_title = Default::default();
        nwg::Label::builder()
            .text(&t("2fa.setup.uri"))
            .parent(&window)
            .position((20, 115))
            .size((460, 20))
            .font(Some(&font))
            .build(&mut lbl_uri_title)
            .expect("Failed to build URI title");

        let mut txt_uri = Default::default();
        nwg::TextInput::builder()
            .text(&uri)
            .parent(&window)
            .position((20, 138))
            .size((455, 25))
            .font(Some(&font_mono))
            .readonly(true)
            .build(&mut txt_uri)
            .expect("Failed to build URI text");

        // --- Separator ---
        let mut sep = Default::default();
        nwg::Frame::builder()
            .parent(&window)
            .position((20, 180))
            .size((455, 1))
            .build(&mut sep)
            .expect("Failed to build separator");

        // --- Verification section ---
        let mut lbl_verify = Default::default();
        nwg::Label::builder()
            .text(&t("2fa.setup.verify"))
            .parent(&window)
            .position((20, 195))
            .size((460, 20))
            .font(Some(&font))
            .build(&mut lbl_verify)
            .expect("Failed to build verify label");

        let mut txt_code = Default::default();
        nwg::TextInput::builder()
            .text("")
            .parent(&window)
            .position((20, 220))
            .size((150, 25))
            .font(Some(&font_mono))
            .limit(6)
            .build(&mut txt_code)
            .expect("Failed to build code input");

        // Verify button
        let mut btn_verify = Default::default();
        nwg::Button::builder()
            .text(&t("2fa.verify"))
            .parent(&window)
            .position((180, 220))
            .size((80, 25))
            .font(Some(&font))
            .build(&mut btn_verify)
            .expect("Failed to build verify button");

        // Status label
        let mut lbl_status = Default::default();
        nwg::Label::builder()
            .text("")
            .parent(&window)
            .position((270, 223))
            .size((200, 20))
            .font(Some(&font))
            .build(&mut lbl_status)
            .expect("Failed to build status label");

        // --- Confirm button (disabled until verified) ---
        let mut btn_confirm = Default::default();
        nwg::Button::builder()
            .text(&t("2fa.setup.confirm"))
            .parent(&window)
            .position((290, 310))
            .size((85, 30))
            .font(Some(&font))
            .enabled(false)
            .build(&mut btn_confirm)
            .expect("Failed to build confirm button");

        // --- Cancel button ---
        let mut btn_cancel = Default::default();
        nwg::Button::builder()
            .text(&t("2fa.cancel"))
            .parent(&window)
            .position((390, 310))
            .size((85, 30))
            .font(Some(&font))
            .build(&mut btn_cancel)
            .expect("Failed to build cancel button");

        let result: Rc<RefCell<Option<TotpEntry>>> = Rc::new(RefCell::new(None));
        let verified: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));
        let window_handle = window.handle;
        let result_clone = result.clone();
        let verified_clone = verified.clone();
        let secret_for_verify = secret.clone();
        let secret_for_confirm = secret.clone();
        let name_owned = name.to_string();

        let handler = nwg::full_bind_event_handler(&window_handle, move |evt, _evt_data, handle| {
            match evt {
                nwg::Event::OnButtonClick => {
                    if handle == btn_copy.handle {
                        // Copy secret to clipboard
                        let secret_text = txt_secret.text();
                        let _ = set_clipboard_text(&secret_text);
                    } else if handle == btn_verify.handle {
                        let code = txt_code.text();
                        let code_trimmed = code.trim();

                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .expect("system clock before Unix epoch")
                            .as_secs();

                        if totp::verify(&secret_for_verify, code_trimmed, now) {
                            *verified_clone.borrow_mut() = true;
                            btn_confirm.set_enabled(true);
                            lbl_status.set_text(&t("2fa.setup.verified"));
                        } else {
                            lbl_status.set_text(&t("2fa.setup.invalid"));
                        }
                    } else if handle == btn_confirm.handle {
                        if *verified.borrow() {
                            *result_clone.borrow_mut() = Some(TotpEntry {
                                name: name_owned.clone(),
                                secret: secret_for_confirm.clone(),
                            });
                            nwg::stop_thread_dispatch();
                        }
                    } else if handle == btn_cancel.handle {
                        nwg::stop_thread_dispatch();
                    }
                }
                nwg::Event::OnWindowClose => {
                    nwg::stop_thread_dispatch();
                }
                _ => {}
            }
        });

        nwg::dispatch_thread_events();
        nwg::unbind_event_handler(&handler);

        let val = result.borrow_mut().take();
        val
    }
}

/// Set text to the Windows clipboard using clipboard-win crate.
fn set_clipboard_text(text: &str) -> Result<(), String> {
    use clipboard_win::{formats, set_clipboard};
    set_clipboard(formats::Unicode, text).map_err(|e| format!("Clipboard error: {}", e))
}

#[cfg(test)]
mod tests {
    #[test]
    fn module_compiles() {
        let _ = std::mem::size_of::<super::TotpSetupDialog>();
    }
}
