// SecurityDialog — Public version security UI
//
// - Change Password button (opens create_password dialog)
// - Min password length: ComboBox (1, 4, 6, 8, 12, 16)
// - OK / Cancel buttons
// Returns Option<SecurityChanges>: None if cancelled

use native_windows_gui as nwg;
use std::cell::RefCell;
use std::rc::Rc;

use crate::settings::Settings;

// ---------------------------------------------------------------------------
// SecurityChanges — what the dialog returns
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct SecurityChanges {
    pub min_password_length: u32,
    pub new_password: Option<String>,
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const MIN_PW_LENGTHS: &[&str] = &["1", "4", "6", "8", "12", "16"];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn pw_length_index(len: u32) -> usize {
    match len {
        1 => 0,
        4 => 1,
        6 => 2,
        8 => 3,
        12 => 4,
        16 => 5,
        _ => 1, // default to 4
    }
}

fn index_to_pw_length(i: usize) -> u32 {
    match i {
        0 => 1,
        1 => 4,
        2 => 6,
        3 => 8,
        4 => 12,
        5 => 16,
        _ => 4,
    }
}

fn build_combo_row(
    window: &nwg::Window,
    font: &nwg::Font,
    label_text: &str,
    items: &[&str],
    selected: usize,
    y: i32,
    label_x: i32,
    control_x: i32,
    control_w: i32,
) -> (nwg::Label, nwg::ComboBox<String>) {
    let mut label = Default::default();
    nwg::Label::builder()
        .text(label_text)
        .parent(window)
        .position((label_x, y + 2))
        .size((150, 20))
        .font(Some(font))
        .build(&mut label)
        .expect("Failed to build label");

    let mut combo = Default::default();
    nwg::ComboBox::builder()
        .parent(window)
        .position((control_x, y))
        .size((control_w, 25))
        .font(Some(font))
        .collection(items.iter().map(|s| s.to_string()).collect())
        .selected_index(Some(selected))
        .build(&mut combo)
        .expect("Failed to build combo");

    (label, combo)
}

// ---------------------------------------------------------------------------
// SecurityDialog
// ---------------------------------------------------------------------------

pub fn show(current: &Settings) -> Option<SecurityChanges> {
    // Layout constants
    const LABEL_X: i32 = 20;
    const CONTROL_X: i32 = 180;
    const CONTROL_W: i32 = 210;
    const ROW_H: i32 = 45;
    const SECTION_H: i32 = 32;

    // Change Password button + status + section header + 1 combo row + buttons
    let win_height = 15 + 30 + 15 + SECTION_H + ROW_H + 15 + 40 + 10;

    let mut window = Default::default();
    nwg::Window::builder()
        .size((420, win_height))
        .position((280, 180))
        .title("Security")
        .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
        .build(&mut window)
        .expect("Failed to build SecurityDialog window");

    super::apply_dialog_theme(&window);

    let mut font = Default::default();
    nwg::Font::builder()
        .family("Segoe UI")
        .size(16)
        .build(&mut font)
        .expect("Failed to build font");

    let mut font_bold = Default::default();
    nwg::Font::builder()
        .family("Segoe UI")
        .size(16)
        .weight(700)
        .build(&mut font_bold)
        .expect("Failed to build bold font");

    let mut y: i32 = 15;

    // =================================================================
    // Change Password button
    // =================================================================

    let mut btn_change_pw = Default::default();
    nwg::Button::builder()
        .text("Change Password")
        .parent(&window)
        .position((LABEL_X, y))
        .size((370, 30))
        .font(Some(&font_bold))
        .build(&mut btn_change_pw)
        .expect("Failed to build Change Password button");

    let mut lbl_pw_status = Default::default();
    nwg::Label::builder()
        .text("")
        .parent(&window)
        .position((LABEL_X, y + 34))
        .size((370, 20))
        .font(Some(&font))
        .build(&mut lbl_pw_status)
        .expect("Failed to build password status label");

    y += 30 + 15;

    // =================================================================
    // Security section
    // =================================================================

    let mut lbl_section_security = Default::default();
    nwg::Label::builder()
        .text("Security")
        .parent(&window)
        .position((LABEL_X, y))
        .size((370, 20))
        .font(Some(&font_bold))
        .build(&mut lbl_section_security)
        .expect("section label");
    y += SECTION_H;

    // --- Min password length ---
    let (lbl_pw, cmb_pw) = build_combo_row(
        &window,
        &font,
        "Min password length:",
        MIN_PW_LENGTHS,
        pw_length_index(current.min_password_length),
        y,
        LABEL_X,
        CONTROL_X,
        CONTROL_W,
    );

    y += ROW_H + 15;

    // =================================================================
    // Buttons
    // =================================================================

    let mut btn_ok = Default::default();
    nwg::Button::builder()
        .text("OK")
        .parent(&window)
        .position((210, y))
        .size((85, 30))
        .font(Some(&font))
        .build(&mut btn_ok)
        .expect("Failed to build OK button");

    let mut btn_cancel = Default::default();
    nwg::Button::builder()
        .text("Cancel")
        .parent(&window)
        .position((310, y))
        .size((85, 30))
        .font(Some(&font))
        .build(&mut btn_cancel)
        .expect("Failed to build Cancel button");

    // Bind Enter key on window to trigger OK button
    {
        let ok_handle = btn_ok.handle;
        let raw = nwg::bind_raw_event_handler(
            &window.handle,
            0x20001,
            move |_hwnd, msg, wparam, _lparam| {
                if msg == 0x0100 && wparam == 0x0D {
                    if let nwg::ControlHandle::Hwnd(btn_hwnd) = ok_handle {
                        unsafe {
                            windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                                windows::Win32::Foundation::HWND(btn_hwnd as *mut _),
                                0x00F5,
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

    // Shared state for new password
    let new_password: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let result: Rc<RefCell<Option<SecurityChanges>>> = Rc::new(RefCell::new(None));

    let window_handle = window.handle;
    let result_clone = result.clone();
    let new_password_clone = new_password.clone();

    let handler = nwg::full_bind_event_handler(&window_handle, move |evt, _evt_data, handle| {
        match evt {
            nwg::Event::OnButtonClick => {
                if handle == btn_change_pw.handle {
                    let pw_idx = cmb_pw.selection().unwrap_or(1);
                    let min_pw_length = index_to_pw_length(pw_idx);
                    if let Some(pw) = super::create_password::show(min_pw_length) {
                        *new_password_clone.borrow_mut() = Some(pw);
                        lbl_pw_status.set_text("Password changed \u{2713}");
                    }
                } else if handle == btn_ok.handle {
                    let pw_idx = cmb_pw.selection().unwrap_or(1);

                    *result_clone.borrow_mut() = Some(SecurityChanges {
                        min_password_length: index_to_pw_length(pw_idx),
                        new_password: new_password_clone.borrow().clone(),
                    });
                    nwg::stop_thread_dispatch();
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

    // Keep labels alive until dialog closes (prevent Drop destroying HWNDs)
    drop(lbl_section_security);
    drop(lbl_pw);

    let out = result.borrow_mut().take();
    out
}
