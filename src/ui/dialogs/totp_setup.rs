// TotpSetupDialog — TOTP setup with secret generation and verification
//
// Generates random 20-byte secret if none provided. `show_add_key` (Tools >
// TOTP > Add Key) also lets the user paste the secret given by a site.
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

/// What the dialog is open for.
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    /// 2FA of the unlock: LockNote's own secret, verified with the user's
    /// authenticator app before Confirm.
    Generate,
    /// An entry of the TOTP panel (issue #23): the secret field is editable.
    AddKey,
}

/// The message for a refused pasted secret.
pub(crate) fn secret_input_error_text(e: totp::SecretInputError) -> String {
    use totp::SecretInputError::*;
    match e {
        Empty => t("totp.secret.err.empty"),
        InvalidBase32 => t("totp.secret.err.base32"),
        InvalidLength => t("totp.secret.err.length"),
        TooShort => t("totp.secret.err.short"),
        NotTotpUri => t("totp.secret.err.not_totp"),
        UriWithoutSecret => t("totp.secret.err.no_secret"),
        UnsupportedParameters => t("totp.secret.err.unsupported"),
    }
}

impl TotpSetupDialog {
    /// Show the TOTP setup dialog.
    ///
    /// `name` is the label for the entry.
    /// `existing_secret` pre-fills if Some, otherwise generates a new one.
    /// Returns Some(entry) on confirmed + verified, None on Cancel.
    pub fn show(name: &str, existing_secret: Option<&str>) -> Option<TotpEntry> {
        Self::show_mode(name, existing_secret, Mode::Generate).0
    }

    /// Add Key of the TOTP panel: the field holds a new secret, which the user
    /// can replace with the one a site gives (Base32 or `otpauth://totp/`
    /// link, `totp::parse_secret_input`). Confirm stores what the field holds
    /// once it is valid, without the verification step: the site, not an
    /// authenticator app, is waiting for LockNote's code. A refused secret is
    /// explained and never stored.
    ///
    /// `copied` receives the secret last copied with the Copy button (OK or
    /// Cancel), so the caller arms the clipboard guard (pass 3).
    pub fn show_add_key(name: &str, copied: &mut Option<String>) -> Option<TotpEntry> {
        let (entry, last_copied) = Self::show_mode(name, None, Mode::AddKey);
        *copied = last_copied;
        entry
    }

    fn show_mode(name: &str, existing_secret: Option<&str>, mode: Mode) -> (Option<TotpEntry>, Option<String>) {
        let add = mode == Mode::AddKey;
        // Add Key starts EMPTY (pass 2): a pre-filled random secret took a
        // paste without select-all as a longer valid secret, and a forgotten
        // paste stored a random secret under the site's name. Generate
        // fills it on request.
        let secret = match (mode, existing_secret) {
            (Mode::AddKey, _) => String::new(),
            (Mode::Generate, Some(s)) => s.to_string(),
            (Mode::Generate, None) => totp::generate_secret(),
        };

        let uri = if add { String::new() } else { totp::build_uri(name, &secret, Some("LockNote")) };

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
            .text(&t("totp.setup.for").replace("{name}", name))
            .parent(&window)
            .position((20, 15))
            .size((460, 20))
            .font(Some(&font))
            .build(&mut lbl_name)
            .expect("Failed to build name label");

        // --- Secret label ---
        let mut lbl_secret_title = Default::default();
        nwg::Label::builder()
            .text(&if add { t("totp.add.secret") } else { t("2fa.setup.secret") })
            .parent(&window)
            .position((20, 50))
            .size((460, 20))
            .font(Some(&font))
            .build(&mut lbl_secret_title)
            .expect("Failed to build secret title");

        let hint = t("totp.add.secret_hint");
        let mut txt_secret = Default::default();
        nwg::TextInput::builder()
            .text(&secret)
            .parent(&window)
            .position((20, 73))
            // Add Key: room for the Generate button.
            .size((if add { 275 } else { 370 }, 25))
            .font(Some(&font_mono))
            .readonly(!add)
            .placeholder_text(if add { Some(&hint) } else { None })
            .build(&mut txt_secret)
            .expect("Failed to build secret text");

        // Add Key: fills the field with a new random secret.
        let mut btn_generate = Default::default();
        nwg::Button::builder()
            .text(&t("totp.add.generate"))
            .parent(&window)
            .position((300, 73))
            .size((90, 25))
            .font(Some(&font))
            .build(&mut btn_generate)
            .expect("Failed to build generate button");
        btn_generate.set_visible(add);

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
            // Add Key: valid secrets are checked at Confirm, no verification.
            .enabled(add)
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
        let copied: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        let copied_clone = copied.clone();
        let verified_clone = verified.clone();
        let secret_for_verify = secret.clone();
        let secret_for_confirm = secret.clone();
        let name_owned = name.to_string();

        let handler = nwg::full_bind_event_handler(&window_handle, move |evt, _evt_data, handle| {
            match evt {
                // Add Key: the link shown follows the secret typed, empty
                // while it is not valid.
                nwg::Event::OnTextInput if add && handle == txt_secret.handle => {
                    match totp::parse_secret_input(&txt_secret.text()) {
                        Ok(s) => txt_uri.set_text(&totp::build_uri(&name_owned, &s, Some("LockNote"))),
                        Err(_) => txt_uri.set_text(""),
                    }
                    lbl_status.set_text("");
                }
                nwg::Event::OnButtonClick => {
                    if add && handle == btn_generate.handle {
                        // OnTextInput then updates the link.
                        txt_secret.set_text(&totp::generate_secret());
                        eprintln!("INFO totp: add key, secret generated");
                    } else if handle == btn_copy.handle {
                        // Copy secret to clipboard
                        let secret_text = txt_secret.text();
                        let result = set_clipboard_text(&secret_text);
                        if result.is_ok() {
                            *copied_clone.borrow_mut() = Some(secret_text.clone());
                        }
                        lbl_status.set_text(&crate::ui::copy_feedback(&result, &t("totp.secret.copied")));
                    } else if handle == btn_verify.handle {
                        let code = txt_code.text();
                        let code_trimmed = code.trim();

                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .expect("system clock before Unix epoch")
                            .as_secs();

                        // Add Key: the code is checked against the secret
                        // the field holds now.
                        let checked = if add {
                            match totp::parse_secret_input(&txt_secret.text()) {
                                Ok(s) => s,
                                Err(e) => {
                                    lbl_status.set_text(&t("totp.add.invalid"));
                                    eprintln!("INFO totp: verify refused, secret err={:?}", e);
                                    return;
                                }
                            }
                        } else {
                            secret_for_verify.clone()
                        };
                        if totp::verify(&checked, code_trimmed, now) {
                            *verified_clone.borrow_mut() = true;
                            btn_confirm.set_enabled(true);
                            lbl_status.set_text(&t("2fa.setup.verified"));
                        } else {
                            lbl_status.set_text(&t("2fa.setup.invalid"));
                        }
                    } else if handle == btn_confirm.handle && add {
                        match totp::parse_secret_input(&txt_secret.text()) {
                            Ok(s) => {
                                eprintln!("INFO totp: key added name_len={}", name_owned.len());
                                *result_clone.borrow_mut() = Some(TotpEntry {
                                    name: name_owned.clone(),
                                    secret: s,
                                });
                                nwg::stop_thread_dispatch();
                            }
                            Err(e) => {
                                eprintln!("WARN totp: pasted secret refused err={:?}", e);
                                nwg::modal_info_message(
                                    window_handle,
                                    &t("totp.add.title"),
                                    &secret_input_error_text(e),
                                );
                            }
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
        let last_copied = copied.borrow_mut().take();
        (val, last_copied)
    }
}

/// Set text to the Windows clipboard using clipboard-win crate.
fn set_clipboard_text(text: &str) -> Result<(), String> {
    use clipboard_win::{formats, set_clipboard};
    set_clipboard(formats::Unicode, text).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn module_compiles() {
        let _ = std::mem::size_of::<super::TotpSetupDialog>();
    }

    /// Issue #23: every refusal has its own translated explanation.
    #[test]
    fn every_refused_secret_has_a_message() {
        use crate::features::totp::SecretInputError::*;
        let _g = crate::features::i18n::test_support::lang_guard();
        crate::features::i18n::set_language(crate::features::i18n::Language::French);
        let texts: Vec<String> = [Empty, InvalidBase32, InvalidLength, TooShort, NotTotpUri, UriWithoutSecret, UnsupportedParameters]
            .into_iter()
            .map(super::secret_input_error_text)
            .collect();
        crate::features::i18n::set_language(crate::features::i18n::Language::English);
        for (i, a) in texts.iter().enumerate() {
            assert!(!a.starts_with('['), "missing key: {}", a);
            assert!(texts.iter().skip(i + 1).all(|b| b != a), "same text twice: {}", a);
        }
        assert!(texts[1].contains("Base32"), "{}", texts[1]);
    }
}
