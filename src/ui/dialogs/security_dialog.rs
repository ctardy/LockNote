// SecurityDialog — Security settings UI
//
// Layout (top to bottom):
//   1. Change Password button (full width, prominent)
//   2. Security section: password length, PBKDF2 iterations, auto-lock, clipboard, TOTP
//   3. OK / Cancel buttons
//
// Returns Option<SecurityChanges>: None if cancelled

use native_windows_gui as nwg;
use std::cell::RefCell;
use std::rc::Rc;

use crate::settings::Settings;

fn t(key: &str) -> String {
    crate::features::i18n::get(key).to_string()
}

// ---------------------------------------------------------------------------
// SecurityChanges — what the dialog returns
// ---------------------------------------------------------------------------

// Debug is intentionally NOT derived: a panic!/expect!/println!/format!("{:?}", …)
// would otherwise dump `new_password` and `totp_2fa_secret` into crash logs or
// stdout. The wrappers below guarantee the buffers are wiped on drop.
#[derive(Clone, PartialEq)]
pub struct SecurityChanges {
    pub min_password_length: u32,
    pub new_password: Option<zeroize::Zeroizing<String>>,
    pub totp_2fa: bool,
    pub totp_2fa_secret: String,
    pub auto_lock: u32,
    pub clipboard_clear: u32,
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const MIN_PW_LENGTHS: &[&str] = &["1", "4", "6", "8", "12", "16"];

const AUTO_LOCK_OPTIONS: &[&str] = &["Off", "1 min", "5 min", "10 min", "15 min", "30 min", "60 min"];
const AUTO_LOCK_VALUES: &[u32] = &[0, 1, 5, 10, 15, 30, 60];

const CLIPBOARD_OPTIONS: &[&str] = &["Off", "10 sec", "30 sec", "60 sec", "120 sec"];
const CLIPBOARD_VALUES: &[u32] = &[0, 10, 30, 60, 120];

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

fn auto_lock_index(val: u32) -> usize {
    AUTO_LOCK_VALUES.iter().position(|&v| v == val).unwrap_or(4) // default 15 min
}

fn clipboard_index(val: u32) -> usize {
    CLIPBOARD_VALUES.iter().position(|&v| v == val).unwrap_or(2) // default 30 sec
}

// ---------------------------------------------------------------------------
// Layout helpers
// ---------------------------------------------------------------------------

const LABEL_X: i32 = 20;
const CONTROL_X: i32 = 180;
const CONTROL_W: i32 = 210;
const ROW_H: i32 = 45;
const SECTION_H: i32 = 32;

/// Build a label + combobox row and return the combobox.
fn build_combo_row(
    window: &nwg::Window,
    font: &nwg::Font,
    y: i32,
    label_text: &str,
    items: Vec<String>,
    selected: usize,
    label_out: &mut nwg::Label,
    combo_out: &mut nwg::ComboBox<String>,
) {
    nwg::Label::builder()
        .text(label_text)
        .parent(window)
        .position((LABEL_X, y + 2))
        .size((150, 20))
        .font(Some(font))
        .build(label_out)
        .expect("label");

    nwg::ComboBox::builder()
        .parent(window)
        .position((CONTROL_X, y))
        .size((CONTROL_W, 25))
        .font(Some(font))
        .collection(items)
        .selected_index(Some(selected))
        .build(combo_out)
        .expect("combo");
}

// ---------------------------------------------------------------------------
// SecurityDialog
// ---------------------------------------------------------------------------

pub struct SecurityDialog;

impl SecurityDialog {
    /// Show the security dialog. `current` is the current settings to pre-fill.
    /// Returns Some(changes) on OK, None on Cancel.
    pub fn show(current: &Settings) -> Option<SecurityChanges> {
        // Window height from the rows:
        // Change Password button area: 50
        // Security section header: SECTION_H
        // Rows: pw len, auto-lock, clipboard clear
        let row_count: i32 = 3;
        let checkbox_count: i32 = 1; // TOTP 2FA
        let section_count: i32 = 1; // Security
        let win_height = 15 + 50 + section_count * SECTION_H + row_count * ROW_H + checkbox_count * 35 + 15 + 40 + 10;

        let mut window = Default::default();
        nwg::Window::builder()
            .size((420, win_height))
            .position((280, 180))
            .title(&t("security.title"))
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

        let mut y: i32 = 15;

        // Bold font for section headers
        let mut font_bold = Default::default();
        nwg::Font::builder()
            .family("Segoe UI")
            .size(16)
            .weight(700)
            .build(&mut font_bold)
            .expect("bold font");

        // =================================================================
        // Change Password button (prominent, full width)
        // =================================================================

        let new_password: Rc<RefCell<Option<zeroize::Zeroizing<String>>>> = Rc::new(RefCell::new(None));

        let mut btn_change_pw = Default::default();
        nwg::Button::builder()
            .text(&t("security.changepw"))
            .parent(&window)
            .position((LABEL_X, y))
            .size((270, 30))
            .font(Some(&font_bold))
            .build(&mut btn_change_pw)
            .expect("change password button");

        let mut lbl_pw_status = Default::default();
        nwg::Label::builder()
            .text("")
            .parent(&window)
            .position((300, y + 5))
            .size((100, 20))
            .font(Some(&font))
            .build(&mut lbl_pw_status)
            .expect("password status label");

        y += 50;

        // =================================================================
        // Security
        // =================================================================

        let mut lbl_section_security = Default::default();
        nwg::Label::builder()
            .text(&t("security.section"))
            .parent(&window)
            .position((LABEL_X, y))
            .size((370, 20))
            .font(Some(&font_bold))
            .build(&mut lbl_section_security)
            .expect("section label");
        y += SECTION_H;

        // --- Min password length ---
        let (mut lbl_pw, mut cmb_pw) = (Default::default(), Default::default());
        build_combo_row(
            &window, &font, y, &t("settings.minpwdlen"),
            MIN_PW_LENGTHS.iter().map(|s| s.to_string()).collect(),
            pw_length_index(current.min_password_length),
            &mut lbl_pw, &mut cmb_pw,
        );
        y += ROW_H;

        // --- Auto-lock ---
        let (mut lbl_autolock, mut cmb_autolock) = (Default::default(), Default::default());
        {
            build_combo_row(
                &window, &font, y, &t("settings.autolock"),
                AUTO_LOCK_OPTIONS.iter().map(|s| s.to_string()).collect(),
                auto_lock_index(current.auto_lock),
                &mut lbl_autolock, &mut cmb_autolock,
            );
            y += ROW_H;
        }

        // --- Clipboard clear ---
        let (mut lbl_clipboard, mut cmb_clipboard) = (Default::default(), Default::default());
        {
            build_combo_row(
                &window, &font, y, &t("settings.clipboard"),
                CLIPBOARD_OPTIONS.iter().map(|s| s.to_string()).collect(),
                clipboard_index(current.clipboard_clear),
                &mut lbl_clipboard, &mut cmb_clipboard,
            );
            y += ROW_H;
        }

        // --- TOTP 2FA ---
        let mut chk_totp = Default::default();
        {
            nwg::CheckBox::builder()
                .text(&t("settings.2fa.enable"))
                .parent(&window)
                .position((LABEL_X, y))
                .size((370, 25))
                .font(Some(&font))
                .check_state(if current.totp_2fa {
                    nwg::CheckBoxState::Checked
                } else {
                    nwg::CheckBoxState::Unchecked
                })
                .build(&mut chk_totp)
                .expect("TOTP checkbox");
            super::disable_control_theme(chk_totp.handle);
            y += 35;
        }

        // =================================================================
        // Buttons
        // =================================================================

        y += 10;

        let mut btn_ok = Default::default();
        nwg::Button::builder()
            .text(&t("settings.ok"))
            .parent(&window)
            .position((210, y))
            .size((85, 30))
            .font(Some(&font))
            .build(&mut btn_ok)
            .expect("OK button");

        super::bind_enter_to_button(window.handle, btn_ok.handle);

        let mut btn_cancel = Default::default();
        nwg::Button::builder()
            .text(&t("settings.cancel"))
            .parent(&window)
            .position((310, y))
            .size((85, 30))
            .font(Some(&font))
            .build(&mut btn_cancel)
            .expect("Cancel button");

        // Bind Enter key on window to trigger OK button
        {
            let ok_handle = btn_ok.handle;
            let raw = nwg::bind_raw_event_handler(
                &window.handle,
                0x20001,
                move |_hwnd, msg, wparam, _lparam| {
                    if msg == 0x0100 && wparam == 0x0D { // WM_KEYDOWN + VK_RETURN
                        if let nwg::ControlHandle::Hwnd(btn_hwnd) = ok_handle {
                            unsafe {
                                windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                                    windows::Win32::Foundation::HWND(btn_hwnd as *mut _),
                                    0x00F5, // BM_CLICK
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

        // Capture current TOTP state for the closure (mutable via Rc<RefCell>)
        let totp_state: Rc<RefCell<(bool, String)>> = Rc::new(RefCell::new(
            (current.totp_2fa, current.totp_2fa_secret.clone())
        ));

        let result: Rc<RefCell<Option<SecurityChanges>>> = Rc::new(RefCell::new(None));
        let window_handle = window.handle;
        let result_clone = result.clone();
        let new_password_clone = new_password.clone();
        let totp_state_clone = totp_state.clone();

        let handler = nwg::full_bind_event_handler(&window_handle, move |evt, _evt_data, handle| {
            match evt {
                nwg::Event::OnButtonClick => {
                    if handle == btn_change_pw.handle {
                        let pw_idx = cmb_pw.selection().unwrap_or(1);
                        let min_pw_length = index_to_pw_length(pw_idx);
                        if let Some(password) = super::create_password::show(min_pw_length) {
                            *new_password_clone.borrow_mut() = Some(password);
                            lbl_pw_status.set_text(&t("2fa.pwchanged"));
                        }
                    }
                    // TOTP checkbox: react immediately on click
                    if handle == chk_totp.handle {
                        let checked = chk_totp.check_state() == nwg::CheckBoxState::Checked;
                        let (was_enabled, ref secret) = *totp_state_clone.borrow();
                        let secret = secret.clone();
                        if checked && secret.is_empty() {
                            // Enabling TOTP: launch setup dialog immediately
                            if let Some(entry) = super::totp_setup::TotpSetupDialog::show("LockNote 2FA", None) {
                                *totp_state_clone.borrow_mut() = (true, entry.secret.clone());
                            } else {
                                // User cancelled setup — uncheck
                                chk_totp.set_check_state(nwg::CheckBoxState::Unchecked);
                            }
                        } else if checked {
                            // Already configured, just re-enable
                            totp_state_clone.borrow_mut().0 = true;
                        } else if !secret.is_empty() {
                            // Disabling TOTP: require current code to confirm
                            if super::totp_verify::TotpVerifyDialog::show(&secret) {
                                *totp_state_clone.borrow_mut() = (false, String::new());
                            } else {
                                // Verification failed — keep checked
                                chk_totp.set_check_state(nwg::CheckBoxState::Checked);
                            }
                        }
                    }
                    if handle == btn_ok.handle {
                        let pw_idx = cmb_pw.selection().unwrap_or(1);

                        let (totp_enabled, totp_secret) = totp_state_clone.borrow().clone();

                        *result_clone.borrow_mut() = Some(SecurityChanges {
                            min_password_length: index_to_pw_length(pw_idx),
                            new_password: new_password_clone.borrow().clone(),
                            totp_2fa: totp_enabled,
                            totp_2fa_secret: totp_secret,
                            auto_lock: AUTO_LOCK_VALUES[cmb_autolock.selection().unwrap_or(4)],
                            clipboard_clear: CLIPBOARD_VALUES[cmb_clipboard.selection().unwrap_or(2)],
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
        drop(lbl_autolock);
        drop(lbl_clipboard);

        let out = result.borrow_mut().take();
        out
    }
}
