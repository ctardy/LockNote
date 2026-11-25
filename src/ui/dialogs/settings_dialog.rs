// SettingsDialog — Settings UI
//
// Layout (top to bottom):
//   1. Dropdowns with labels (On close, Theme, Language, Font, etc.)
//   2. Password hint text field
//   3. Checkboxes (Minimize to tray, TOTP 2FA)
//   4. OK / Cancel buttons
//
// Returns Option<SettingsChanges>: None if cancelled

use native_windows_gui as nwg;
use std::cell::RefCell;
use std::rc::Rc;

use crate::settings::{CloseAction, Settings, ThemeChoice};

fn t(key: &str) -> String {
    crate::features::i18n::get(key).to_string()
}

// ---------------------------------------------------------------------------
// SettingsChanges — what the dialog returns
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct SettingsChanges {
    pub save_on_close: CloseAction,
    pub theme: ThemeChoice,
    pub min_password_length: u32,
    pub minimize_to_tray: bool,
    pub font_family: String,
    pub font_size: f64,
    pub save_key: u32,
    pub save_modifiers: u32,
    pub totp_2fa: bool,
    pub totp_2fa_secret: String,
    pub language: crate::settings::Language,
    pub auto_lock: u32,
    pub auto_save: u32,
    pub clipboard_clear: u32,
    pub hotkey_modifiers: u32,
    pub hotkey_key: u32,
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

fn close_behaviors() -> Vec<String> {
    vec![
        t("settings.ask"),
        t("settings.always"),
        t("settings.never"),
    ]
}
fn themes() -> Vec<String> {
    vec![t("settings.dark"), t("settings.light")]
}
const MIN_PW_LENGTHS: &[&str] = &["1", "4", "6", "8", "12", "16"];
const FONT_FAMILIES: &[&str] = &[
    "Consolas",
    "Courier New",
    "Lucida Console",
    "Cascadia Mono",
    "Segoe UI",
];
const FONT_SIZES: &[&str] = &["8", "9", "10", "11", "12", "14", "16", "18", "20", "24"];
/// Format a shortcut from modifiers bitmask (0x01=Alt, 0x02=Ctrl, 0x04=Shift) and VK code.
fn format_shortcut(modifiers: u32, vk: u32) -> String {
    let mut parts = Vec::new();
    if modifiers & 0x02 != 0 { parts.push("Ctrl"); }
    if modifiers & 0x04 != 0 { parts.push("Shift"); }
    if modifiers & 0x01 != 0 { parts.push("Alt"); }
    if vk >= 0x41 && vk <= 0x5A {
        let ch = (vk as u8) as char;
        parts.push(match ch {
            'A'..='Z' => match vk {
                0x41 => "A", 0x42 => "B", 0x43 => "C", 0x44 => "D", 0x45 => "E",
                0x46 => "F", 0x47 => "G", 0x48 => "H", 0x49 => "I", 0x4A => "J",
                0x4B => "K", 0x4C => "L", 0x4D => "M", 0x4E => "N", 0x4F => "O",
                0x50 => "P", 0x51 => "Q", 0x52 => "R", 0x53 => "S", 0x54 => "T",
                0x55 => "U", 0x56 => "V", 0x57 => "W", 0x58 => "X", 0x59 => "Y",
                0x5A => "Z", _ => "?",
            },
            _ => "?",
        });
    } else if vk >= 0x70 && vk <= 0x87 {
        let fnum = vk - 0x70 + 1;
        return format!("{}F{}", if parts.is_empty() { String::new() } else { format!("{}+", parts.join("+")) }, fnum);
    }
    parts.join("+")
}

const LANGUAGES: &[&str] = &["English", "Français", "Español", "Deutsch"];

const AUTO_LOCK_OPTIONS: &[&str] = &["Off", "1 min", "5 min", "10 min", "15 min", "30 min", "60 min"];
const AUTO_LOCK_VALUES: &[u32] = &[0, 1, 5, 10, 15, 30, 60];

const AUTO_SAVE_OPTIONS: &[&str] = &["Off", "10 sec", "30 sec", "60 sec", "120 sec", "300 sec"];
const AUTO_SAVE_VALUES: &[u32] = &[0, 10, 30, 60, 120, 300];

const CLIPBOARD_OPTIONS: &[&str] = &["Off", "10 sec", "30 sec", "60 sec", "120 sec"];
const CLIPBOARD_VALUES: &[u32] = &[0, 10, 30, 60, 120];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn close_action_index(a: CloseAction) -> usize {
    match a {
        CloseAction::Ask => 0,
        CloseAction::Always => 1,
        CloseAction::Never => 2,
    }
}

fn index_to_close_action(i: usize) -> CloseAction {
    match i {
        1 => CloseAction::Always,
        2 => CloseAction::Never,
        _ => CloseAction::Ask,
    }
}

fn theme_index(t: ThemeChoice) -> usize {
    match t {
        ThemeChoice::Dark => 0,
        ThemeChoice::Light => 1,
    }
}

fn index_to_theme(i: usize) -> ThemeChoice {
    match i {
        1 => ThemeChoice::Light,
        _ => ThemeChoice::Dark,
    }
}

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

fn font_family_index(family: &str) -> usize {
    FONT_FAMILIES
        .iter()
        .position(|f| f.eq_ignore_ascii_case(family))
        .unwrap_or(0)
}

fn font_size_index(size: f64) -> usize {
    let s = format!("{}", size as u32);
    FONT_SIZES.iter().position(|f| *f == s).unwrap_or(4) // default 12
}

fn language_index(lang: crate::settings::Language) -> usize {
    match lang {
        crate::settings::Language::En => 0,
        crate::settings::Language::Fr => 1,
        crate::settings::Language::Es => 2,
        crate::settings::Language::De => 3,
    }
}

fn index_to_language(i: usize) -> crate::settings::Language {
    match i {
        1 => crate::settings::Language::Fr,
        2 => crate::settings::Language::Es,
        3 => crate::settings::Language::De,
        _ => crate::settings::Language::En,
    }
}

fn auto_lock_index(val: u32) -> usize {
    AUTO_LOCK_VALUES.iter().position(|&v| v == val).unwrap_or(4) // default 15 min
}

fn auto_save_index(val: u32) -> usize {
    AUTO_SAVE_VALUES.iter().position(|&v| v == val).unwrap_or(2) // default 30 sec
}

fn clipboard_index(val: u32) -> usize {
    CLIPBOARD_VALUES.iter().position(|&v| v == val).unwrap_or(2) // default 30 sec
}

fn format_hotkey(modifiers: u32, key: u32) -> String {
    let mut parts = Vec::new();
    if modifiers & 0x02 != 0 { parts.push("Ctrl"); }
    if modifiers & 0x04 != 0 { parts.push("Shift"); }
    if modifiers & 0x01 != 0 { parts.push("Alt"); }
    if key >= 0x41 && key <= 0x5A {
        parts.push(match key {
            0x41 => "A", 0x42 => "B", 0x43 => "C", 0x44 => "D", 0x45 => "E",
            0x46 => "F", 0x47 => "G", 0x48 => "H", 0x49 => "I", 0x4A => "J",
            0x4B => "K", 0x4C => "L", 0x4D => "M", 0x4E => "N", 0x4F => "O",
            0x50 => "P", 0x51 => "Q", 0x52 => "R", 0x53 => "S", 0x54 => "T",
            0x55 => "U", 0x56 => "V", 0x57 => "W", 0x58 => "X", 0x59 => "Y",
            0x5A => "Z", _ => "?",
        });
    }
    parts.join("+")
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
// SettingsDialog
// ---------------------------------------------------------------------------

pub struct SettingsDialog;

impl SettingsDialog {
    /// Show the settings dialog. `current` is the current settings to pre-fill.
    /// Returns Some(changes) on OK, None on Cancel.
    pub fn show(current: &Settings) -> Option<SettingsChanges> {
        // Window height from the rows:
        // Appearance: theme, font family, font size, language
        // Security: pw len, hint, auto-lock, clipboard clear
        // Behavior: on close, save key, auto-save, global hotkey
        let row_count: i32 = 12;
        let checkbox_count: i32 = 2; // minimize_to_tray, TOTP 2FA
        let section_count: i32 = 3; // Appearance, Security, Behavior
        let win_height = 15 + section_count * SECTION_H + row_count * ROW_H + checkbox_count * 35 + 15 + 40 + 10;

        let mut window = Default::default();
        nwg::Window::builder()
            .size((420, win_height))
            .position((280, 180))
            .title(&t("settings.title"))
            .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
            .build(&mut window)
            .expect("Failed to build SettingsDialog window");

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
        // Appearance
        // =================================================================

        let mut lbl_section_appearance = Default::default();
        nwg::Label::builder()
            .text(&t("settings.section.appearance"))
            .parent(&window)
            .position((LABEL_X, y))
            .size((370, 20))
            .font(Some(&font_bold))
            .build(&mut lbl_section_appearance)
            .expect("section label");
        y += SECTION_H;

        // --- Theme ---
        let (mut lbl_theme, mut cmb_theme) = (Default::default(), Default::default());
        build_combo_row(
            &window, &font, y, &t("settings.theme"),
            themes(),
            theme_index(current.theme),
            &mut lbl_theme, &mut cmb_theme,
        );
        y += ROW_H;

        // --- Language ---
        let (mut lbl_lang, mut cmb_lang) = (Default::default(), Default::default());
        {
            build_combo_row(
                &window, &font, y, &t("settings.language"),
                LANGUAGES.iter().map(|s| s.to_string()).collect(),
                language_index(current.language),
                &mut lbl_lang, &mut cmb_lang,
            );
            y += ROW_H;
        }

        // --- Font family ---
        let (mut lbl_font, mut cmb_font) = (Default::default(), Default::default());
        build_combo_row(
            &window, &font, y, &t("settings.font"),
            FONT_FAMILIES.iter().map(|s| s.to_string()).collect(),
            font_family_index(&current.font_family),
            &mut lbl_font, &mut cmb_font,
        );
        y += ROW_H;

        // --- Font size ---
        let (mut lbl_size, mut cmb_size) = (Default::default(), Default::default());
        build_combo_row(
            &window, &font, y, &t("settings.fontsize"),
            FONT_SIZES.iter().map(|s| s.to_string()).collect(),
            font_size_index(current.font_size),
            &mut lbl_size, &mut cmb_size,
        );
        y += ROW_H;

        // =================================================================
        // Security
        // =================================================================

        let mut lbl_section_security = Default::default();
        nwg::Label::builder()
            .text(&t("settings.section.security"))
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
        // Behavior
        // =================================================================

        let mut lbl_section_behavior = Default::default();
        nwg::Label::builder()
            .text(&t("settings.section.behavior"))
            .parent(&window)
            .position((LABEL_X, y))
            .size((370, 20))
            .font(Some(&font_bold))
            .build(&mut lbl_section_behavior)
            .expect("section label");
        y += SECTION_H;

        // --- On close ---
        let (mut lbl_close, mut cmb_close) = (Default::default(), Default::default());
        build_combo_row(
            &window, &font, y, &t("settings.onclose"),
            close_behaviors(),
            close_action_index(current.save_on_close),
            &mut lbl_close, &mut cmb_close,
        );
        y += ROW_H;

        // --- Save shortcut (hotkey capture field) ---
        let mut lbl_savekey = Default::default();
        nwg::Label::builder()
            .text(&t("settings.savekey"))
            .parent(&window)
            .position((LABEL_X, y + 2))
            .size((150, 20))
            .font(Some(&font))
            .build(&mut lbl_savekey)
            .expect("label");

        let mut txt_savekey = Default::default();
        nwg::TextInput::builder()
            .parent(&window)
            .position((CONTROL_X, y))
            .size((CONTROL_W, 25))
            .font(Some(&font))
            .text(&format_shortcut(current.save_modifiers, current.save_key))
            .readonly(true)
            .build(&mut txt_savekey)
            .expect("hotkey input");

        let captured_save_mod: Rc<RefCell<u32>> = Rc::new(RefCell::new(current.save_modifiers));
        let captured_save_key: Rc<RefCell<u32>> = Rc::new(RefCell::new(current.save_key));

        let sk_mod = captured_save_mod.clone();
        let sk_key = captured_save_key.clone();
        let _sk_handler = nwg::bind_raw_event_handler(
            &txt_savekey.handle,
            0x20000,
            move |_hwnd, msg, w_param, _l_param| {
                if msg == 0x0100 { // WM_KEYDOWN
                    let vk = w_param as u32;
                    if vk == 0x10 || vk == 0x11 || vk == 0x12 { return None; }
                    let ctrl = unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(0x11) } < 0;
                    let shift = unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(0x10) } < 0;
                    let alt = unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(0x12) } < 0;
                    if !ctrl && !shift && !alt { return Some(0); }
                    let valid = (vk >= 0x41 && vk <= 0x5A) || (vk >= 0x70 && vk <= 0x87);
                    if !valid { return Some(0); }
                    let modifiers = (if ctrl { 0x02u32 } else { 0 })
                        | (if shift { 0x04 } else { 0 })
                        | (if alt { 0x01 } else { 0 });
                    *sk_mod.borrow_mut() = modifiers;
                    *sk_key.borrow_mut() = vk;
                    let text = format_shortcut(modifiers, vk);
                    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
                    unsafe {
                        windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                            windows::Win32::Foundation::HWND(_hwnd as *mut _),
                            0x000C, // WM_SETTEXT
                            None,
                            Some(windows::Win32::Foundation::LPARAM(wide.as_ptr() as isize)),
                        );
                    }
                    return Some(0);
                }
                if msg == 0x0102 { return Some(0); } // WM_CHAR
                None
            },
        );
        y += ROW_H;

        // --- Auto-save ---
        let (mut lbl_autosave, mut cmb_autosave) = (Default::default(), Default::default());
        {
            build_combo_row(
                &window, &font, y, &t("settings.autosave"),
                AUTO_SAVE_OPTIONS.iter().map(|s| s.to_string()).collect(),
                auto_save_index(current.auto_save),
                &mut lbl_autosave, &mut cmb_autosave,
            );
            y += ROW_H;
        }

        // --- Minimize to tray ---
        let mut chk_tray = Default::default();
        nwg::CheckBox::builder()
            .text(&t("settings.mintray"))
            .parent(&window)
            .position((LABEL_X, y))
            .size((370, 25))
            .font(Some(&font))
            .check_state(if current.minimize_to_tray {
                nwg::CheckBoxState::Checked
            } else {
                nwg::CheckBoxState::Unchecked
            })
            .build(&mut chk_tray)
            .expect("tray checkbox");
        super::disable_control_theme(chk_tray.handle);
        y += 35;

        // --- Global hotkey (read-only display) ---
        let (mut lbl_hotkey, mut lbl_hotkey_val) = (Default::default(), Default::default());
        {
            nwg::Label::builder()
                .text(&t("settings.hotkey"))
                .parent(&window)
                .position((LABEL_X, y + 2))
                .size((150, 20))
                .font(Some(&font))
                .build(&mut lbl_hotkey)
                .expect("hotkey label");

            let hotkey_text = format_hotkey(current.hotkey_modifiers, current.hotkey_key);
            nwg::Label::builder()
                .text(&hotkey_text)
                .parent(&window)
                .position((CONTROL_X, y + 2))
                .size((CONTROL_W, 20))
                .font(Some(&font))
                .build(&mut lbl_hotkey_val)
                .expect("hotkey value label");
            y += ROW_H;
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

        // Capture current TOTP state for the closure
        let current_totp_secret = current.totp_2fa_secret.clone();

        // Capture hotkey values (read-only display, not editable yet)
        let captured_hotkey_modifiers = current.hotkey_modifiers;
        let captured_hotkey_key = current.hotkey_key;

        let result: Rc<RefCell<Option<SettingsChanges>>> = Rc::new(RefCell::new(None));
        let window_handle = window.handle;
        let result_clone = result.clone();

        let handler = nwg::full_bind_event_handler(&window_handle, move |evt, _evt_data, handle| {
            match evt {
                nwg::Event::OnButtonClick => {
                    if handle == btn_ok.handle {
                        let close_idx = cmb_close.selection().unwrap_or(0);
                        let theme_idx = cmb_theme.selection().unwrap_or(0);
                        let pw_idx = cmb_pw.selection().unwrap_or(1);
                        let tray = chk_tray.check_state() == nwg::CheckBoxState::Checked;
                        let font_idx = cmb_font.selection().unwrap_or(0);
                        let size_idx = cmb_size.selection().unwrap_or(4);

                        let font_family =
                            FONT_FAMILIES.get(font_idx).unwrap_or(&"Consolas").to_string();
                        let font_size: f64 = FONT_SIZES
                            .get(size_idx)
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(12.0);

                        // Handle TOTP 2FA toggle
                        let (totp_enabled, totp_secret) = {
                            let checked = chk_totp.check_state() == nwg::CheckBoxState::Checked;
                            if checked && current_totp_secret.is_empty() {
                                // Enabling TOTP: launch setup dialog
                                if let Some(entry) = super::totp_setup::TotpSetupDialog::show("LockNote 2FA", None) {
                                    (true, entry.secret.clone())
                                } else {
                                    // User cancelled setup — don't enable
                                    (false, String::new())
                                }
                            } else if checked {
                                // Already configured, keep secret
                                (true, current_totp_secret.clone())
                            } else if !current_totp_secret.is_empty() {
                                // Disabling TOTP: require current code to confirm
                                if super::totp_verify::TotpVerifyDialog::show(&current_totp_secret) {
                                    (false, String::new())
                                } else {
                                    // Verification failed — keep TOTP enabled
                                    (true, current_totp_secret.clone())
                                }
                            } else {
                                // TOTP was never configured, nothing to disable
                                (false, String::new())
                            }
                        };

                        *result_clone.borrow_mut() = Some(SettingsChanges {
                            save_on_close: index_to_close_action(close_idx),
                            theme: index_to_theme(theme_idx),
                            min_password_length: index_to_pw_length(pw_idx),
                            minimize_to_tray: tray,
                            font_family,
                            font_size,
                            save_key: *captured_save_key.borrow(),
                            save_modifiers: *captured_save_mod.borrow(),
                            totp_2fa: totp_enabled,
                            totp_2fa_secret: totp_secret,
                            language: index_to_language(cmb_lang.selection().unwrap_or(0)),
                            auto_lock: AUTO_LOCK_VALUES[cmb_autolock.selection().unwrap_or(4)],
                            auto_save: AUTO_SAVE_VALUES[cmb_autosave.selection().unwrap_or(2)],
                            clipboard_clear: CLIPBOARD_VALUES[cmb_clipboard.selection().unwrap_or(2)],
                            hotkey_modifiers: captured_hotkey_modifiers,
                            hotkey_key: captured_hotkey_key,
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
        drop(lbl_section_appearance);
        drop(lbl_section_security);
        drop(lbl_section_behavior);
        drop(lbl_close);
        drop(lbl_theme);
        drop(lbl_font);
        drop(lbl_size);
        drop(lbl_savekey);
        drop(txt_savekey);
        drop(lbl_pw);
        drop(lbl_lang);
        drop(lbl_autolock);
        drop(lbl_autosave);
        drop(lbl_clipboard);
        { drop(lbl_hotkey); drop(lbl_hotkey_val); }

        let out = result.borrow_mut().take();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_action_roundtrip() {
        for action in &[CloseAction::Ask, CloseAction::Always, CloseAction::Never] {
            let idx = close_action_index(*action);
            assert_eq!(index_to_close_action(idx), *action);
        }
    }

    #[test]
    fn theme_roundtrip() {
        for theme in &[ThemeChoice::Dark, ThemeChoice::Light] {
            let idx = theme_index(*theme);
            assert_eq!(index_to_theme(idx), *theme);
        }
    }

    #[test]
    fn pw_length_roundtrip() {
        for len in &[1u32, 4, 6, 8, 12, 16] {
            let idx = pw_length_index(*len);
            assert_eq!(index_to_pw_length(idx), *len);
        }
    }

    #[test]
    fn font_family_index_lookup() {
        assert_eq!(font_family_index("Consolas"), 0);
        assert_eq!(font_family_index("Courier New"), 1);
        assert_eq!(font_family_index("Segoe UI"), 4);
        assert_eq!(font_family_index("Unknown"), 0); // fallback
    }

    #[test]
    fn font_size_index_lookup() {
        assert_eq!(font_size_index(8.0), 0);
        assert_eq!(font_size_index(12.0), 4);
        assert_eq!(font_size_index(24.0), 9);
        assert_eq!(font_size_index(99.0), 4); // fallback
    }

    #[test]
    fn close_action_all_values_roundtrip() {
        let actions = vec![CloseAction::Ask, CloseAction::Always, CloseAction::Never];
        for action in actions {
            let idx = close_action_index(action);
            let recovered = index_to_close_action(idx);
            assert_eq!(action, recovered, "Failed roundtrip for {:?}", action);
        }
    }

    #[test]
    fn theme_all_values_roundtrip() {
        let themes = vec![ThemeChoice::Dark, ThemeChoice::Light];
        for theme in themes {
            let idx = theme_index(theme);
            let recovered = index_to_theme(idx);
            assert_eq!(theme, recovered, "Failed roundtrip for {:?}", theme);
        }
    }

    #[test]
    fn all_min_pw_lengths_roundtrip() {
        let lengths = vec![1u32, 4, 6, 8, 12, 16];
        for len in lengths {
            let idx = pw_length_index(len);
            let recovered = index_to_pw_length(idx);
            assert_eq!(len, recovered, "Failed roundtrip for length {}", len);
        }
    }

    #[test]
    fn all_font_families_roundtrip() {
        for family in FONT_FAMILIES {
            let idx = font_family_index(family);
            let recovered = FONT_FAMILIES.get(idx).unwrap();
            assert_eq!(*family, *recovered, "Failed roundtrip for family {}", family);
        }
    }

    #[test]
    fn all_font_sizes_roundtrip() {
        for size_str in FONT_SIZES {
            let size_f64: f64 = size_str.parse().unwrap();
            let idx = font_size_index(size_f64);
            let recovered = FONT_SIZES.get(idx).unwrap();
            assert_eq!(*size_str, *recovered, "Failed roundtrip for size {}", size_str);
        }
    }

    #[test]
    fn font_family_case_insensitive_lookup() {
        assert_eq!(font_family_index("consolas"), 0);
        assert_eq!(font_family_index("CONSOLAS"), 0);
        assert_eq!(font_family_index("Consolas"), 0);
        assert_eq!(font_family_index("courier new"), 1);
        assert_eq!(font_family_index("COURIER NEW"), 1);
    }

    #[test]
    fn password_length_boundary_values() {
        assert_eq!(index_to_pw_length(0), 1);   // minimum
        assert_eq!(index_to_pw_length(5), 16);  // maximum
        assert_eq!(index_to_pw_length(6), 4);   // out of range defaults
        assert_eq!(index_to_pw_length(999), 4); // out of range defaults
    }

    #[test]
    fn font_size_boundary_values() {
        let min_size = 8.0;
        let max_size = 24.0;
        assert_eq!(font_size_index(min_size), 0);
        assert_eq!(font_size_index(max_size), 9);
        assert_eq!(font_size_index(8.5), 0);  // truncates to 8 → index 0
        assert_eq!(font_size_index(11.9), 3); // truncates to 11 → index 3
        assert_eq!(font_size_index(99.0), 4);
    }

    #[test]
    fn close_action_index_bounds() {
        assert!(close_action_index(CloseAction::Ask) < 3);
        assert!(close_action_index(CloseAction::Always) < 3);
        assert!(close_action_index(CloseAction::Never) < 3);
    }

    #[test]
    fn theme_index_bounds() {
        assert!(theme_index(ThemeChoice::Dark) < 2);
        assert!(theme_index(ThemeChoice::Light) < 2);
    }
}
