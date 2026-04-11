// CreatePasswordDialog — Password creation with strength meter and confirmation
//
// Returns Option<String>: the chosen password, or None if cancelled.
// Clears TextBox contents on drop for security.

use native_windows_gui as nwg;
use std::cell::RefCell;
use std::rc::Rc;

use crate::theme;

// ---------------------------------------------------------------------------
// Password strength scoring (0-7)
// ---------------------------------------------------------------------------

fn password_score(pw: &str) -> u32 {
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

fn strength_label(score: u32) -> &'static str {
    match score {
        0..=1 => "Weak",
        2..=3 => "Fair",
        4..=5 => "Strong",
        _ => "Very strong",
    }
}

#[allow(dead_code)]
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
    pub fn show(min_length: u32) -> Option<String> {
        let _pal = theme::current();

        // --- Center on screen ---
        #[link(name = "user32")]
        extern "system" {
            fn GetSystemMetrics(index: i32) -> i32;
        }
        let win_w: i32 = 440;
        let win_h: i32 = 400;
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
            .text("Welcome to LockNote")
            .parent(&window)
            .position((margin, 18))
            .size((content_w, 30))
            .font(Some(&font_title))
            .build(&mut lbl_title)
            .expect("Failed to build title label");

        // --- Description ---
        let mut lbl_desc = Default::default();
        nwg::Label::builder()
            .text("Your notes are encrypted and stored inside this \
                   executable.\nChoose a password to protect them.")
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

        // --- Password label ---
        let mut lbl_password = Default::default();
        nwg::Label::builder()
            .text("Password:")
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
            .text("Confirm password:")
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
            .position((margin, 280))
            .size((content_w, 20))
            .font(Some(&font))
            .build(&mut lbl_error)
            .expect("Failed to build error label");

        // --- Create button ---
        let mut btn_ok = Default::default();
        nwg::Button::builder()
            .text("Create")
            .parent(&window)
            .position((220, 320))
            .size((90, 32))
            .font(Some(&font))
            .build(&mut btn_ok)
            .expect("Failed to build Create button");

        // --- Cancel button ---
        let mut btn_cancel = Default::default();
        nwg::Button::builder()
            .text("Cancel")
            .parent(&window)
            .position((325, 320))
            .size((90, 32))
            .font(Some(&font))
            .build(&mut btn_cancel)
            .expect("Failed to build Cancel button");

        // --- Result ---
        let result: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));

        // --- Event handler ---
        let window_handle = window.handle;
        let result_clone = result.clone();

        let handler = nwg::full_bind_event_handler(&window_handle, move |evt, _evt_data, handle| {
            match evt {
                nwg::Event::OnButtonClick => {
                    if handle == btn_ok.handle {
                        let pw = txt_password.text();
                        let confirm = txt_confirm.text();

                        if (pw.len() as u32) < min_length {
                            lbl_error.set_text(
                                &format!("Password must be at least {} characters", min_length),
                            );
                            return;
                        }
                        if pw != confirm {
                            lbl_error.set_text("Passwords do not match");
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
                        let pw = txt_password.text();
                        let score = password_score(&pw);
                        let pct = strength_percent(score);
                        let label = strength_label(score);
                        lbl_strength.set_text(label);

                        // Resize the strength bar
                        let bar_w = (390 * pct / 100) as i32;
                        strength_bar.set_position(25, 196);
                        strength_bar.set_size(bar_w as u32, 10);
                    }
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

// ---------------------------------------------------------------------------
// Free function (convenience API expected by callers)
// ---------------------------------------------------------------------------

/// Show the create-password dialog and return the chosen password.
/// This is a convenience wrapper around `CreatePasswordDialog::show()`.
pub fn show(min_length: u32) -> Option<String> {
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
        assert_eq!(strength_label(0), "Weak");
        assert_eq!(strength_label(1), "Weak");
        assert_eq!(strength_label(2), "Fair");
        assert_eq!(strength_label(3), "Fair");
        assert_eq!(strength_label(4), "Strong");
        assert_eq!(strength_label(5), "Strong");
        assert_eq!(strength_label(6), "Very strong");
        assert_eq!(strength_label(7), "Very strong");
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
