// PreferencesDialog — Public version preferences UI
//
// Simplified preferences dialog (no i18n, no auto-save, no global-hotkey).
//
// Section "Appearance":
//   - Theme: ComboBox (Dark, Light)
//   - Font family: ComboBox (Consolas, Courier New, Lucida Console, Cascadia Mono, Segoe UI)
//   - Font size: ComboBox (8..24)
//
// Section "Behavior":
//   - On close: ComboBox (Ask, Always save, Never save)
//   - Save shortcut: ComboBox (Ctrl+A .. Ctrl+Z)
//   - Minimize to tray: CheckBox
//
// Buttons: OK / Cancel
// Returns Option<PreferencesChanges>: None if cancelled

use native_windows_gui as nwg;
use std::cell::RefCell;
use std::rc::Rc;

use crate::settings::{CloseAction, Settings, ThemeChoice};

// ---------------------------------------------------------------------------
// PreferencesChanges — what the dialog returns
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct PreferencesChanges {
    pub save_on_close: CloseAction,
    pub theme: ThemeChoice,
    pub minimize_to_tray: bool,
    pub font_family: String,
    pub font_size: f64,
    pub save_key: u32,
    pub save_modifiers: u32,
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const CLOSE_BEHAVIORS: &[&str] = &["Ask", "Always save", "Never save"];
const THEMES: &[&str] = &["Dark", "Light"];
const FONT_FAMILIES: &[&str] = &[
    "Consolas",
    "Courier New",
    "Lucida Console",
    "Cascadia Mono",
    "Segoe UI",
];
const FONT_SIZES: &[&str] = &["8", "9", "10", "11", "12", "14", "16", "18", "20", "24"];
fn format_shortcut(modifiers: u32, vk: u32) -> String {
    let mut parts = Vec::new();
    if modifiers & 0x02 != 0 { parts.push("Ctrl"); }
    if modifiers & 0x04 != 0 { parts.push("Shift"); }
    if modifiers & 0x01 != 0 { parts.push("Alt"); }
    if vk >= 0x41 && vk <= 0x5A {
        parts.push(match vk {
            0x41 => "A", 0x42 => "B", 0x43 => "C", 0x44 => "D", 0x45 => "E",
            0x46 => "F", 0x47 => "G", 0x48 => "H", 0x49 => "I", 0x4A => "J",
            0x4B => "K", 0x4C => "L", 0x4D => "M", 0x4E => "N", 0x4F => "O",
            0x50 => "P", 0x51 => "Q", 0x52 => "R", 0x53 => "S", 0x54 => "T",
            0x55 => "U", 0x56 => "V", 0x57 => "W", 0x58 => "X", 0x59 => "Y",
            0x5A => "Z", _ => "?",
        });
    } else if vk >= 0x70 && vk <= 0x87 {
        let fnum = vk - 0x70 + 1;
        return format!("{}F{}", if parts.is_empty() { String::new() } else { format!("{}+", parts.join("+")) }, fnum);
    }
    parts.join("+")
}

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

// ---------------------------------------------------------------------------
// build_combo_row helper
// ---------------------------------------------------------------------------

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
    const LABEL_X: i32 = 20;
    const CONTROL_X: i32 = 180;
    const CONTROL_W: i32 = 210;

    nwg::Label::builder()
        .text(label_text)
        .parent(window)
        .position((LABEL_X, y + 2))
        .size((150, 20))
        .font(Some(font))
        .build(label_out)
        .expect("Failed to build label");

    nwg::ComboBox::builder()
        .parent(window)
        .position((CONTROL_X, y))
        .size((CONTROL_W, 25))
        .font(Some(font))
        .collection(items)
        .selected_index(Some(selected))
        .build(combo_out)
        .expect("Failed to build combo");
}

// ---------------------------------------------------------------------------
// PreferencesDialog
// ---------------------------------------------------------------------------

pub struct PreferencesDialog;

impl PreferencesDialog {
    /// Show the preferences dialog. `current` is the current settings to pre-fill.
    /// Returns Some(changes) on OK, None on Cancel.
    pub fn show(current: &Settings) -> Option<PreferencesChanges> {
        // Layout constants
        const LABEL_X: i32 = 20;
        const CONTROL_W: i32 = 210;
        const ROW_H: i32 = 45;
        const SECTION_H: i32 = 32;

        // 2 sections + 3 combo rows (Appearance) + 2 combo rows (Behavior) + 1 checkbox + buttons
        let win_height = 15 + 2 * SECTION_H + 5 * ROW_H + 35 + 15 + 40 + 10;

        let mut window = Default::default();
        nwg::Window::builder()
            .size((420, win_height))
            .position((280, 180))
            .title("Preferences")
            .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
            .build(&mut window)
            .expect("Failed to build PreferencesDialog window");

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
        // Appearance
        // =================================================================

        let mut lbl_section_appearance = Default::default();
        nwg::Label::builder()
            .text("Appearance")
            .parent(&window)
            .position((LABEL_X, y))
            .size((370, 20))
            .font(Some(&font_bold))
            .build(&mut lbl_section_appearance)
            .expect("section label");
        y += SECTION_H;

        // --- Theme ---
        let mut lbl_theme = Default::default();
        let mut cmb_theme: nwg::ComboBox<String> = Default::default();
        build_combo_row(
            &window, &font, y, "Theme:",
            THEMES.iter().map(|s| s.to_string()).collect(),
            theme_index(current.theme),
            &mut lbl_theme, &mut cmb_theme,
        );
        y += ROW_H;

        // --- Font family ---
        let mut lbl_font = Default::default();
        let mut cmb_font: nwg::ComboBox<String> = Default::default();
        build_combo_row(
            &window, &font, y, "Font family:",
            FONT_FAMILIES.iter().map(|s| s.to_string()).collect(),
            font_family_index(&current.font_family),
            &mut lbl_font, &mut cmb_font,
        );
        y += ROW_H;

        // --- Font size ---
        let mut lbl_size = Default::default();
        let mut cmb_size: nwg::ComboBox<String> = Default::default();
        build_combo_row(
            &window, &font, y, "Font size:",
            FONT_SIZES.iter().map(|s| s.to_string()).collect(),
            font_size_index(current.font_size),
            &mut lbl_size, &mut cmb_size,
        );
        y += ROW_H;

        // =================================================================
        // Behavior
        // =================================================================

        let mut lbl_section_behavior = Default::default();
        nwg::Label::builder()
            .text("Behavior")
            .parent(&window)
            .position((LABEL_X, y))
            .size((370, 20))
            .font(Some(&font_bold))
            .build(&mut lbl_section_behavior)
            .expect("section label");
        y += SECTION_H;

        // --- On close behavior ---
        let mut lbl_close = Default::default();
        let mut cmb_close: nwg::ComboBox<String> = Default::default();
        build_combo_row(
            &window, &font, y, "On close:",
            CLOSE_BEHAVIORS.iter().map(|s| s.to_string()).collect(),
            close_action_index(current.save_on_close),
            &mut lbl_close, &mut cmb_close,
        );
        y += ROW_H;

        // --- Save shortcut (hotkey capture field) ---
        const CONTROL_X: i32 = 180;
        let mut lbl_savekey = Default::default();
        nwg::Label::builder()
            .text("Save shortcut:")
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

        // --- Minimize to tray ---
        let mut chk_tray = Default::default();
        nwg::CheckBox::builder()
            .text("Minimize to tray")
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
            .expect("Failed to build tray checkbox");
        super::disable_control_theme(chk_tray.handle);

        y += 35 + 15;

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

        let result: Rc<RefCell<Option<PreferencesChanges>>> = Rc::new(RefCell::new(None));
        let window_handle = window.handle;
        let result_clone = result.clone();

        let handler = nwg::full_bind_event_handler(&window_handle, move |evt, _evt_data, handle| {
            match evt {
                nwg::Event::OnButtonClick => {
                    if handle == btn_ok.handle {
                        let close_idx = cmb_close.selection().unwrap_or(0);
                        let theme_idx = cmb_theme.selection().unwrap_or(0);
                        let tray = chk_tray.check_state() == nwg::CheckBoxState::Checked;
                        let font_idx = cmb_font.selection().unwrap_or(0);
                        let size_idx = cmb_size.selection().unwrap_or(4);

                        let font_family =
                            FONT_FAMILIES.get(font_idx).unwrap_or(&"Consolas").to_string();
                        let font_size: f64 = FONT_SIZES
                            .get(size_idx)
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(12.0);

                        *result_clone.borrow_mut() = Some(PreferencesChanges {
                            save_on_close: index_to_close_action(close_idx),
                            theme: index_to_theme(theme_idx),
                            minimize_to_tray: tray,
                            font_family,
                            font_size,
                            save_key: *captured_save_key.borrow(),
                            save_modifiers: *captured_save_mod.borrow(),
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
        drop(lbl_section_behavior);
        drop(lbl_theme);
        drop(lbl_font);
        drop(lbl_size);
        drop(lbl_close);
        drop(lbl_savekey);
        drop(txt_savekey);

        let out = result.borrow_mut().take();
        out
    }
}
