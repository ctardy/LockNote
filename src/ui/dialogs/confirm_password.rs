// ConfirmPasswordDialog — Ask the user to re-enter their password before a sensitive action.
//
// Returns true if the entered password matches, false if cancelled.

use native_windows_gui as nwg;
use std::cell::RefCell;
use std::rc::Rc;
use zeroize::Zeroizing;

fn t(key: &str) -> String {
    crate::features::i18n::get(key).to_string()
}

/// Compare two byte slices in time that depends only on the longer input's
/// length. Returns `true` iff they are equal. Used to avoid leaking the
/// common prefix length of the typed password vs the real one through
/// timing (a local attacker with repeated attempts could otherwise recover
/// the password character-by-character).
pub(crate) fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    // Over the longer length, with no early return on a length mismatch
    // (pass 7): the time does not tell how long the password is either.
    let n = a.len().max(b.len());
    let mut diff: u8 = (a.len() != b.len()) as u8;
    for i in 0..n {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        diff |= x ^ y;
    }
    diff == 0
}

pub struct ConfirmPasswordDialog;

impl ConfirmPasswordDialog {
    /// Show the dialog. Returns `true` if the user entered the correct password.
    /// `prompt`: the translated question, e.g. `t("confirmpwd.export")` (a
    /// whole sentence per action: word order differs between languages).
    pub fn show(current_password: &str, prompt: &str) -> bool {
        // --- Center on screen ---
        #[link(name = "user32")]
        extern "system" {
            fn GetSystemMetrics(index: i32) -> i32;
        }
        let win_w: i32 = 380;
        let win_h: i32 = 200;
        let screen_w = unsafe { GetSystemMetrics(0) };
        let screen_h = unsafe { GetSystemMetrics(1) };
        let pos_x = (screen_w - win_w) / 2;
        let pos_y = (screen_h - win_h) / 2;

        let mut window = Default::default();
        nwg::Window::builder()
            .size((win_w, win_h))
            .position((pos_x, pos_y))
            .title(&t("confirmpwd.title"))
            .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
            .build(&mut window)
            .expect("Failed to build ConfirmPasswordDialog window");

        super::apply_dialog_theme(&window);

        let mut font = Default::default();
        nwg::Font::builder()
            .family("Segoe UI")
            .size(16)
            .build(&mut font)
            .expect("Failed to build font");

        let margin: i32 = 20;
        let content_w: i32 = win_w - margin * 2;

        // Label
        let mut lbl = Default::default();
        nwg::Label::builder()
            .text(prompt)
            .parent(&window)
            .position((margin, 15))
            .size((content_w, 22))
            .font(Some(&font))
            .build(&mut lbl)
            .expect("Failed to build label");

        // Password input
        let mut txt_password = Default::default();
        nwg::TextInput::builder()
            .parent(&window)
            .position((margin, 45))
            .size((content_w, 28))
            .password(Some('*'))
            .font(Some(&font))
            .focus(true)
            .build(&mut txt_password)
            .expect("Failed to build password input");

        // Error label
        let mut lbl_error = Default::default();
        nwg::Label::builder()
            .text("")
            .parent(&window)
            .position((margin, 80))
            .size((content_w, 22))
            .font(Some(&font))
            .build(&mut lbl_error)
            .expect("Failed to build error label");
        super::mark_error(window.handle, lbl_error.handle);

        // OK button
        let mut btn_ok = Default::default();
        nwg::Button::builder()
            .text(&t("settings.ok"))
            .parent(&window)
            .position((win_w - margin - 185, 120))
            .size((85, 32))
            .font(Some(&font))
            .build(&mut btn_ok)
            .expect("Failed to build OK button");

        // Cancel button
        let mut btn_cancel = Default::default();
        nwg::Button::builder()
            .text(&t("settings.cancel"))
            .parent(&window)
            .position((win_w - margin - 90, 120))
            .size((90, 32))
            .font(Some(&font))
            .build(&mut btn_cancel)
            .expect("Failed to build Cancel button");

        // Enter key → OK button
        super::bind_enter_to_button(window.handle, btn_ok.handle);

        // Own a copy of the reference password in a buffer that wipes on
        // drop — otherwise a crash dump taken while this dialog is up would
        // contain the password in plain String memory.
        let expected: Zeroizing<String> = Zeroizing::new(current_password.to_string());
        let result: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));
        let result_clone = result.clone();
        let focus_field_handle = txt_password.handle;
        let window_handle = window.handle;

        let handler = nwg::full_bind_event_handler(&window_handle, move |evt, _evt_data, handle| {
            match evt {
                nwg::Event::OnButtonClick => {
                    if handle == btn_ok.handle {
                        let pw: Zeroizing<String> = Zeroizing::new(txt_password.text());
                        if constant_time_eq(pw.as_bytes(), expected.as_bytes()) {
                            *result_clone.borrow_mut() = true;
                            txt_password.set_text("");
                            nwg::stop_thread_dispatch();
                        } else {
                            lbl_error.set_text(&t("unlock.wrong"));
                            txt_password.set_text("");
                            if let nwg::ControlHandle::Hwnd(h) = txt_password.handle {
                                unsafe {
                                    let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(
                                        Some(windows::Win32::Foundation::HWND(h as *mut _)),
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

        // Caret in the field as soon as the window has the focus.
        let focus_guard = crate::ui::lock_card::keep_focus_on(&window_handle, &focus_field_handle)
            .expect("Failed to bind the focus handler");

        nwg::dispatch_thread_events();
        nwg::unbind_event_handler(&handler);
        nwg::unbind_raw_event_handler(&focus_guard).ok();

        let confirmed = *result.borrow();
        confirmed
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn module_compiles() {
        let _ = std::mem::size_of::<super::ConfirmPasswordDialog>();
    }
}
