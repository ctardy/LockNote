// CreatePasswordDialog — Password creation with strength meter and confirmation
//
// Returns Option<Zeroizing<String>>: the chosen password wrapped so its buffer
// is wiped when the caller drops it, or None if cancelled. Clears TextBox
// contents on drop for security.

use native_windows_gui as nwg;
use std::cell::RefCell;
use std::rc::Rc;
use zeroize::Zeroizing;

use crate::theme;

fn t(key: &str) -> String {
    crate::features::i18n::get(key).to_string()
}

// ---------------------------------------------------------------------------
// Password strength scoring (0-7)
// ---------------------------------------------------------------------------

pub(crate) fn password_score(pw: &str) -> u32 {
    let mut score: u32 = 0;
    let len = pw.len();
    if len >= 8 {
        score += 1;
    }
    if len >= 12 {
        score += 1;
    }
    if len >= 16 {
        score += 1;
    }
    if pw.chars().any(|c| c.is_ascii_lowercase()) {
        score += 1;
    }
    if pw.chars().any(|c| c.is_ascii_uppercase()) {
        score += 1;
    }
    if pw.chars().any(|c| c.is_ascii_digit()) {
        score += 1;
    }
    if pw.chars().any(|c| !c.is_ascii_alphanumeric()) {
        score += 1;
    }
    score
}

pub(crate) fn strength_label(score: u32) -> String {
    match score {
        0..=1 => t("createpwd.weak"),
        2..=3 => t("createpwd.fair"),
        4..=5 => t("createpwd.strong"),
        _ => t("createpwd.verystrong"),
    }
}

fn strength_color(score: u32) -> [u8; 3] {
    match score {
        0..=1 => [220, 50, 50],   // red
        2..=3 => [230, 160, 30],  // orange
        _ => [50, 180, 50],       // green
    }
}

fn strength_percent(score: u32) -> u32 {
    match score {
        0..=1 => 25,
        2..=3 => 50,
        4..=5 => 75,
        _ => 100,
    }
}

// ---------------------------------------------------------------------------
// Helper: convert theme Color to nwg [r,g,b]
// ---------------------------------------------------------------------------

#[allow(dead_code)]
fn to_nwg_color(c: theme::Color) -> [u8; 3] {
    [c.r, c.g, c.b]
}

// ---------------------------------------------------------------------------
// CreatePasswordDialog
// ---------------------------------------------------------------------------

pub struct CreatePasswordDialog;

impl CreatePasswordDialog {
    /// Show the dialog and return the chosen password, or None if cancelled.
    pub fn show(min_length: u32) -> Option<Zeroizing<String>> {
        let _pal = theme::current();

        // --- Center on screen ---
        #[link(name = "user32")]
        extern "system" {
            fn GetSystemMetrics(index: i32) -> i32;
        }
        let win_w: i32 = 440;
        let win_h: i32 = 390;
        let screen_w = unsafe { GetSystemMetrics(0) }; // SM_CXSCREEN
        let screen_h = unsafe { GetSystemMetrics(1) }; // SM_CYSCREEN
        let pos_x = (screen_w - win_w) / 2;
        let pos_y = (screen_h - win_h) / 2;

        // --- Build window ---
        let mut window = Default::default();
        nwg::Window::builder()
            .size((win_w, win_h))
            .position((pos_x, pos_y))
            .title("LockNote")
            .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
            .build(&mut window)
            .expect("Failed to build CreatePasswordDialog window");

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

        let content_w: i32 = 390;
        let margin: i32 = 25;

        // --- Welcome title ---
        let mut lbl_title = Default::default();
        nwg::Label::builder()
            .text(&t("createpwd.title"))
            .parent(&window)
            .position((margin, 18))
            .size((content_w, 30))
            .font(Some(&font_title))
            .build(&mut lbl_title)
            .expect("Failed to build title label");

        // --- Description ---
        let mut lbl_desc = Default::default();
        nwg::Label::builder()
            .text(&t("createpwd.desc"))
            .parent(&window)
            .position((margin, 55))
            .size((content_w, 40))
            .font(Some(&font_desc))
            .build(&mut lbl_desc)
            .expect("Failed to build description label");

        // --- Separator line (thin frame) ---
        let mut separator = Default::default();
        nwg::Frame::builder()
            .parent(&window)
            .position((margin, 102))
            .size((content_w, 1))
            .build(&mut separator)
            .expect("Failed to build separator");

        // --- Password label (with minimum length indication) ---
        let mut lbl_password = Default::default();
        let pw_label = t("createpwd.minlen")
            .replace("{}", &min_length.to_string());
        nwg::Label::builder()
            .text(&pw_label)
            .parent(&window)
            .position((margin, 115))
            .size((content_w, 20))
            .font(Some(&font))
            .build(&mut lbl_password)
            .expect("Failed to build label");

        // --- Password input ---
        let mut txt_password = Default::default();
        nwg::TextInput::builder()
            .parent(&window)
            .position((margin, 140))
            .size((content_w, 28))
            .password(Some('*'))
            .font(Some(&font))
            .focus(true)
            .build(&mut txt_password)
            .expect("Failed to build password input");

        // --- Strength label ---
        let mut lbl_strength = Default::default();
        nwg::Label::builder()
            .text("")
            .parent(&window)
            .position((margin, 174))
            .size((content_w, 20))
            .font(Some(&font_desc))
            .build(&mut lbl_strength)
            .expect("Failed to build strength label");

        // --- Strength bar (use a Frame as a colored bar) ---
        let mut strength_bar_bg = Default::default();
        nwg::Frame::builder()
            .parent(&window)
            .position((margin, 196))
            .size((content_w, 10))
            .build(&mut strength_bar_bg)
            .expect("Failed to build strength bar bg");

        // Track of the bar: the palette border color. Painted per control (all
        // nwg frames share one window class, so a class brush would recolor
        // the separators and the bar itself).
        super::set_frame_color(strength_bar_bg.handle, crate::theme::current().border);

        let mut strength_bar = Default::default();
        nwg::Frame::builder()
            .parent(&window)
            .position((margin, 196))
            .size((0, 10))
            .build(&mut strength_bar)
            .expect("Failed to build strength bar");

        // --- Confirm label ---
        let mut lbl_confirm = Default::default();
        nwg::Label::builder()
            .text(&t("createpwd.confirm"))
            .parent(&window)
            .position((margin, 218))
            .size((content_w, 20))
            .font(Some(&font))
            .build(&mut lbl_confirm)
            .expect("Failed to build confirm label");

        // --- Confirm input ---
        let mut txt_confirm = Default::default();
        nwg::TextInput::builder()
            .parent(&window)
            .position((margin, 243))
            .size((content_w, 28))
            .password(Some('*'))
            .font(Some(&font))
            .build(&mut txt_confirm)
            .expect("Failed to build confirm input");

        // --- Validation / error label ---
        let mut lbl_error = Default::default();
        nwg::Label::builder()
            .text("")
            .parent(&window)
            .position((margin, 283))
            .size((content_w, 20))
            .font(Some(&font))
            .build(&mut lbl_error)
            .expect("Failed to build error label");
        super::mark_error(window.handle, lbl_error.handle);

        // --- Create button ---
        let mut btn_ok = Default::default();
        nwg::Button::builder()
            .text(&t("createpwd.create"))
            .parent(&window)
            .position((220, 320))
            .size((90, 32))
            .font(Some(&font))
            .build(&mut btn_ok)
            .expect("Failed to build Create button");

        // --- Cancel button ---
        let mut btn_cancel = Default::default();
        nwg::Button::builder()
            .text(&t("createpwd.cancel"))
            .parent(&window)
            .position((325, 320))
            .size((90, 32))
            .font(Some(&font))
            .build(&mut btn_cancel)
            .expect("Failed to build Cancel button");

        // --- Enter key → Create button ---
        super::bind_enter_to_button(window.handle, btn_ok.handle);

        // --- Result: password ---
        let result: Rc<RefCell<Option<Zeroizing<String>>>> = Rc::new(RefCell::new(None));

        // --- Event handler ---
        let focus_field_handle = txt_password.handle;
        let window_handle = window.handle;
        let txt_password_handle = txt_password.handle;
        let result_clone = result.clone();

        let handler = nwg::full_bind_event_handler(&window_handle, move |evt, _evt_data, handle| {
            match evt {
                nwg::Event::OnButtonClick => {
                    if handle == btn_ok.handle {
                        // Wrap both candidates in Zeroizing: the confirm field is wiped
                        // on drop no matter which branch we take; the final password is
                        // moved out (not cloned) into the result, preserving the
                        // zeroize-on-drop guarantee all the way to the caller.
                        let pw: Zeroizing<String> = Zeroizing::new(txt_password.text());
                        let confirm: Zeroizing<String> = Zeroizing::new(txt_confirm.text());

                        if (pw.len() as u32) < min_length {
                            lbl_error.set_text(
                                &t("createpwd.tooshort"),
                            );
                            if let nwg::ControlHandle::Hwnd(h) = txt_password.handle {
                                unsafe { let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(Some(windows::Win32::Foundation::HWND(h as *mut _))); }
                            }
                            return;
                        }
                        if *pw != *confirm {
                            lbl_error.set_text(&t("createpwd.mismatch"));
                            txt_confirm.set_text("");
                            if let nwg::ControlHandle::Hwnd(h) = txt_confirm.handle {
                                unsafe { let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(Some(windows::Win32::Foundation::HWND(h as *mut _))); }
                            }
                            return;
                        }

                        *result_clone.borrow_mut() = Some(pw);

                        // Clear text boxes for security
                        txt_password.set_text("");
                        txt_confirm.set_text("");

                        nwg::stop_thread_dispatch();
                    } else if handle == btn_cancel.handle {
                        txt_password.set_text("");
                        txt_confirm.set_text("");
                        nwg::stop_thread_dispatch();
                    }
                }
                nwg::Event::OnWindowClose => {
                    txt_password.set_text("");
                    txt_confirm.set_text("");
                    nwg::stop_thread_dispatch();
                }
                nwg::Event::OnTextInput => {
                    if handle == txt_password.handle {
                        let pw: Zeroizing<String> = Zeroizing::new(txt_password.text());
                        let score = password_score(&pw);
                        let pct = strength_percent(score);
                        let label = strength_label(score);
                        lbl_strength.set_text(&label);

                        // Resize and color the strength bar on top of its track
                        // (the track's real geometry: the dialog is DPI-scaled).
                        let (bar_x, bar_y) = strength_bar_bg.position();
                        let (track_w, track_h) = strength_bar_bg.size();
                        strength_bar.set_position(bar_x, bar_y);
                        strength_bar.set_size(track_w * pct / 100, track_h);

                        let [r, g, b] = strength_color(score);
                        super::set_frame_color(strength_bar.handle, crate::theme::Color::new(r, g, b));
                    }
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

        // Caret in the field as soon as the window has the focus.
        let focus_guard = crate::ui::lock_card::keep_focus_on(&window_handle, &focus_field_handle)
            .expect("Failed to bind the focus handler");

        nwg::dispatch_thread_events();
        nwg::unbind_event_handler(&handler);
        nwg::unbind_raw_event_handler(&focus_guard).ok();

        let val = result.borrow_mut().take();
        val
    }
}

// ---------------------------------------------------------------------------
// Free function (convenience API expected by callers)
// ---------------------------------------------------------------------------

/// Show the create-password dialog and return the password.
/// This is a convenience wrapper around `CreatePasswordDialog::show()`.
pub fn show(min_length: u32) -> Option<Zeroizing<String>> {
    CreatePasswordDialog::show(min_length)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_score_empty() {
        assert_eq!(password_score(""), 0);
    }

    #[test]
    fn test_score_short_lowercase() {
        assert_eq!(password_score("abc"), 1); // lowercase only
    }

    #[test]
    fn test_score_8_mixed() {
        // 8+ chars (+1), lowercase (+1), uppercase (+1), digit (+1) = 4
        assert_eq!(password_score("Abcdefg1"), 4);
    }

    #[test]
    fn test_score_max() {
        // 16+ chars (+3), lower (+1), upper (+1), digit (+1), symbol (+1) = 7
        assert_eq!(password_score("Abcdefghijklm1!A"), 7);
    }

    #[test]
    fn test_strength_labels() {
        let _g = crate::features::i18n::test_support::lang_guard();
        crate::features::i18n::set_language(crate::features::i18n::Language::English);
        assert_eq!(strength_label(0), "Weak".to_string());
        assert_eq!(strength_label(1), "Weak".to_string());
        assert_eq!(strength_label(2), "Fair".to_string());
        assert_eq!(strength_label(3), "Fair".to_string());
        assert_eq!(strength_label(4), "Strong".to_string());
        assert_eq!(strength_label(5), "Strong".to_string());
        assert_eq!(strength_label(6), "Very strong".to_string());
        assert_eq!(strength_label(7), "Very strong".to_string());
    }

    #[test]
    fn test_strength_percent() {
        assert_eq!(strength_percent(0), 25);
        assert_eq!(strength_percent(1), 25);
        assert_eq!(strength_percent(3), 50);
        assert_eq!(strength_percent(5), 75);
        assert_eq!(strength_percent(7), 100);
    }
}
