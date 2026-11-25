// TotpVerifyDialog — TOTP code verification during unlock
//
// Shows a 6-digit code input with up to MAX_TOTP_ATTEMPTS tries.
// Returns true if verified, false if cancelled or max attempts reached.

use native_windows_gui as nwg;
use std::cell::RefCell;
use std::rc::Rc;

use crate::features::totp;

fn t(key: &str) -> String {
    crate::features::i18n::get(key).to_string()
}

const MAX_TOTP_ATTEMPTS: u32 = 3;

/// Exponential backoff schedule for failed TOTP attempts.
/// Attempts 1 -> 0s, 2 -> 2s, 3 -> 4s (but MAX=3 so 3 closes the dialog).
fn backoff_seconds(att: u32) -> u32 {
    match att {
        0 | 1 => 0,
        2 => 2,
        _ => 4,
    }
}

pub struct TotpVerifyDialog;

impl TotpVerifyDialog {
    /// Show the TOTP verification dialog.
    ///
    /// `secret` is the Base32-encoded TOTP secret.
    /// Returns true if the user entered a valid code, false if cancelled
    /// or all attempts exhausted.
    pub fn show(secret: &str) -> bool {
        // --- Center on screen ---
        #[link(name = "user32")]
        extern "system" {
            fn GetSystemMetrics(index: i32) -> i32;
        }
        let win_w: i32 = 420;
        let win_h: i32 = 250;
        let screen_w = unsafe { GetSystemMetrics(0) };
        let screen_h = unsafe { GetSystemMetrics(1) };
        let pos_x = (screen_w - win_w) / 2;
        let pos_y = (screen_h - win_h) / 2;

        let mut window = Default::default();
        nwg::Window::builder()
            .size((win_w, win_h))
            .position((pos_x, pos_y))
            .title(&t("2fa.title"))
            .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
            .build(&mut window)
            .expect("Failed to build TotpVerifyDialog window");

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
            .size(22)
            .weight(700)
            .build(&mut font_title)
            .expect("Failed to build title font");

        let mut font_desc = Default::default();
        nwg::Font::builder()
            .family("Segoe UI")
            .size(15)
            .build(&mut font_desc)
            .expect("Failed to build description font");

        let mut font_code = Default::default();
        nwg::Font::builder()
            .family("Consolas")
            .size(22)
            .build(&mut font_code)
            .expect("Failed to build code font");

        let content_w: i32 = 370;
        let margin: i32 = 25;

        // --- Title ---
        let mut _lbl_title = Default::default();
        nwg::Label::builder()
            .text(&t("2fa.header"))
            .parent(&window)
            .position((margin, 15))
            .size((content_w, 28))
            .font(Some(&font_title))
            .build(&mut _lbl_title)
            .expect("Failed to build title label");

        // --- Description ---
        let mut _lbl_desc = Default::default();
        nwg::Label::builder()
            .text(&t("2fa.prompt"))
            .parent(&window)
            .position((margin, 48))
            .size((content_w, 22))
            .font(Some(&font_desc))
            .build(&mut _lbl_desc)
            .expect("Failed to build description label");

        // --- Separator ---
        let mut _separator = Default::default();
        nwg::Frame::builder()
            .parent(&window)
            .position((margin, 78))
            .size((content_w, 1))
            .build(&mut _separator)
            .expect("Failed to build separator");

        // --- Code input ---
        let mut txt_code = Default::default();
        nwg::TextInput::builder()
            .parent(&window)
            .position((margin, 95))
            .size((160, 32))
            .font(Some(&font_code))
            .limit(6)
            .focus(true)
            .build(&mut txt_code)
            .expect("Failed to build code input");

        // --- Status label ---
        let mut lbl_status = Default::default();
        nwg::Label::builder()
            .text("")
            .parent(&window)
            .position((margin, 135))
            .size((content_w, 20))
            .font(Some(&font))
            .build(&mut lbl_status)
            .expect("Failed to build status label");

        // --- Verify button ---
        let mut btn_verify = Default::default();
        nwg::Button::builder()
            .text(&t("2fa.verify"))
            .parent(&window)
            .position((200, 175))
            .size((90, 32))
            .font(Some(&font))
            .build(&mut btn_verify)
            .expect("Failed to build Verify button");

        // --- Cancel button ---
        let mut btn_cancel = Default::default();
        nwg::Button::builder()
            .text(&t("2fa.cancel"))
            .parent(&window)
            .position((305, 175))
            .size((90, 32))
            .font(Some(&font))
            .build(&mut btn_cancel)
            .expect("Failed to build Cancel button");

        // --- Enter key -> Verify button ---
        super::bind_enter_to_button(window.handle, btn_verify.handle);

        // --- Backoff countdown timer ---
        let mut backoff_timer = nwg::AnimationTimer::default();
        nwg::AnimationTimer::builder()
            .parent(&window)
            .interval(std::time::Duration::from_secs(1))
            .active(false)
            .build(&mut backoff_timer)
            .expect("Failed to build backoff timer");
        let backoff_remaining: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));

        let result: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));
        let attempts: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));
        let focus_field_handle = txt_code.handle;
        let window_handle = window.handle;
        let result_clone = result.clone();
        let attempts_clone = attempts.clone();
        let backoff_remaining_clone = backoff_remaining.clone();
        // Zeroizing so the closure-captured String is wiped when the
        // handler is dropped (end of dialog) — not just when the
        // allocator happens to recycle the buffer.
        let secret_owned: zeroize::Zeroizing<String> = zeroize::Zeroizing::new(secret.to_string());

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
                            btn_verify.set_enabled(true);
                            txt_code.set_enabled(true);
                            lbl_status.set_text("");
                            if let nwg::ControlHandle::Hwnd(h) = txt_code.handle {
                                unsafe {
                                    let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(
                                        Some(windows::Win32::Foundation::HWND(h as *mut _)),
                                    );
                                }
                            }
                        } else {
                            lbl_status.set_text(
                                &t("2fa.retryin")
                                    .replace("{seconds}", &remaining.to_string()),
                            );
                        }
                    }
                }
                nwg::Event::OnButtonClick => {
                    if handle == btn_verify.handle {
                        let code = txt_code.text();
                        let code_trimmed = code.trim();

                        if code_trimmed.len() != 6 {
                            lbl_status.set_text(&t("2fa.enter6"));
                            return;
                        }

                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .expect("system clock before Unix epoch")
                            .as_secs();

                        if totp::verify(&secret_owned, code_trimmed, now) {
                            *result_clone.borrow_mut() = true;
                            nwg::stop_thread_dispatch();
                        } else {
                            let mut att = attempts_clone.borrow_mut();
                            *att += 1;
                            txt_code.set_text("");

                            if *att >= MAX_TOTP_ATTEMPTS {
                                let msg = t("2fa.maxattempts");
                                lbl_status.set_text(&msg);
                                nwg::modal_info_message(
                                    &window_handle,
                                    &msg,
                                    &msg,
                                );
                                nwg::stop_thread_dispatch();
                            } else {
                                let remaining = MAX_TOTP_ATTEMPTS - *att;
                                let delay = backoff_seconds(*att);
                                if delay == 0 {
                                    lbl_status.set_text(&format!(
                                        "Invalid code. {} attempt{} remaining.",
                                        remaining,
                                        if remaining == 1 { "" } else { "s" }
                                    ));
                                } else {
                                    btn_verify.set_enabled(false);
                                    txt_code.set_enabled(false);
                                    *backoff_remaining_clone.borrow_mut() = delay;
                                    let retry_msg = t("2fa.retryin")
                                        .replace("{seconds}", &delay.to_string());
                                    lbl_status.set_text(&format!(
                                        "Invalid code. {} attempt{} left — {}",
                                        remaining,
                                        if remaining == 1 { "" } else { "s" },
                                        retry_msg,
                                    ));
                                    backoff_timer.start();
                                }
                            }
                        }
                    } else if handle == btn_cancel.handle {
                        backoff_timer.stop();
                        nwg::stop_thread_dispatch();
                    }
                }
                nwg::Event::OnWindowClose => {
                    backoff_timer.stop();
                    nwg::stop_thread_dispatch();
                }
                _ => {}
            }
        });

        // Caret in the field as soon as the window has the focus.
        let focus_guard = crate::ui::lock_card::keep_focus_on(&window_handle, &focus_field_handle)
            .expect("Failed to bind the focus handler");

        nwg::dispatch_thread_events();
        nwg::unbind_event_handler(&handler);
        nwg::unbind_raw_event_handler(&focus_guard).ok();

        let out = *result.borrow();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_compiles() {
        let _ = std::mem::size_of::<TotpVerifyDialog>();
    }

    #[test]
    fn max_totp_attempts_constant() {
        assert_eq!(MAX_TOTP_ATTEMPTS, 3);
    }
}
