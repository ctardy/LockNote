// SettingsDialog — Public version settings UI
//
// - On close behavior: ComboBox (Ask/Always/Never)
// - Theme: ComboBox (Dark/Light)
// - Min password length: ComboBox (1, 4, 6, 8, 12, 16)
// - Minimize to tray: Checkbox
// - Font family: ComboBox (Consolas, Courier New, Lucida Console, Cascadia Mono, Segoe UI)
// - Font size: ComboBox (8, 9, 10, 11, 12, 14, 16, 18, 20, 24)
// - OK / Cancel buttons
// Returns Option<SettingsChanges>: None if cancelled

use native_windows_gui as nwg;
use std::cell::RefCell;
use std::rc::Rc;

use crate::settings::{CloseAction, Settings, ThemeChoice};

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
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const CLOSE_BEHAVIORS: &[&str] = &["Ask", "Always save", "Never save"];
const THEMES: &[&str] = &["Dark", "Light"];
const MIN_PW_LENGTHS: &[&str] = &["1", "4", "6", "8", "12", "16"];
const FONT_FAMILIES: &[&str] = &[
    "Consolas",
    "Courier New",
    "Lucida Console",
    "Cascadia Mono",
    "Segoe UI",
];
const FONT_SIZES: &[&str] = &["8", "9", "10", "11", "12", "14", "16", "18", "20", "24"];
const SAVE_KEYS: &[&str] = &[
    "A","B","C","D","E","F","G","H","I","J","K","L","M",
    "N","O","P","Q","R","S","T","U","V","W","X","Y","Z",
];

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

fn save_key_index(vk: u32) -> usize {
    if vk >= 0x41 && vk <= 0x5A { (vk - 0x41) as usize } else { 18 } // default S
}

fn index_to_save_key(i: usize) -> u32 {
    0x41 + i as u32
}

// ---------------------------------------------------------------------------
// SettingsDialog
// ---------------------------------------------------------------------------

pub struct SettingsDialog;

impl SettingsDialog {
    /// Show the settings dialog. `current` is the current settings to pre-fill.
    /// Returns Some(changes) on OK, None on Cancel.
    pub fn show(current: &Settings) -> Option<SettingsChanges> {
        // Layout constants
        const LABEL_X: i32 = 20;
        const CONTROL_X: i32 = 180;
        const CONTROL_W: i32 = 210;
        const ROW_H: i32 = 45;
        const SECTION_H: i32 = 32;

        // 3 sections + 5 combo rows + 1 checkbox + buttons
        let win_height = 15 + 3 * SECTION_H + 5 * ROW_H + 35 + 15 + 40 + 10;

        let mut window = Default::default();
        nwg::Window::builder()
            .size((420, win_height))
            .position((280, 180))
            .title("Settings")
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
        nwg::Label::builder()
            .text("Theme:")
            .parent(&window)
            .position((LABEL_X, y + 2))
            .size((150, 20))
            .font(Some(&font))
            .build(&mut lbl_theme)
            .expect("Failed to build label");

        let mut cmb_theme = Default::default();
        nwg::ComboBox::builder()
            .parent(&window)
            .position((CONTROL_X, y))
            .size((CONTROL_W, 25))
            .font(Some(&font))
            .collection(THEMES.iter().map(|s| s.to_string()).collect())
            .selected_index(Some(theme_index(current.theme)))
            .build(&mut cmb_theme)
            .expect("Failed to build theme combo");

        y += ROW_H;

        // --- Font family ---
        let mut lbl_font = Default::default();
        nwg::Label::builder()
            .text("Font family:")
            .parent(&window)
            .position((LABEL_X, y + 2))
            .size((150, 20))
            .font(Some(&font))
            .build(&mut lbl_font)
            .expect("Failed to build label");

        let mut cmb_font = Default::default();
        nwg::ComboBox::builder()
            .parent(&window)
            .position((CONTROL_X, y))
            .size((CONTROL_W, 25))
            .font(Some(&font))
            .collection(FONT_FAMILIES.iter().map(|s| s.to_string()).collect())
            .selected_index(Some(font_family_index(&current.font_family)))
            .build(&mut cmb_font)
            .expect("Failed to build font combo");

        y += ROW_H;

        // --- Font size ---
        let mut lbl_size = Default::default();
        nwg::Label::builder()
            .text("Font size:")
            .parent(&window)
            .position((LABEL_X, y + 2))
            .size((150, 20))
            .font(Some(&font))
            .build(&mut lbl_size)
            .expect("Failed to build label");

        let mut cmb_size = Default::default();
        nwg::ComboBox::builder()
            .parent(&window)
            .position((CONTROL_X, y))
            .size((CONTROL_W, 25))
            .font(Some(&font))
            .collection(FONT_SIZES.iter().map(|s| s.to_string()).collect())
            .selected_index(Some(font_size_index(current.font_size)))
            .build(&mut cmb_size)
            .expect("Failed to build size combo");

        y += ROW_H;

        // =================================================================
        // Security
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
        let mut lbl_pw = Default::default();
        nwg::Label::builder()
            .text("Min password length:")
            .parent(&window)
            .position((LABEL_X, y + 2))
            .size((150, 20))
            .font(Some(&font))
            .build(&mut lbl_pw)
            .expect("Failed to build label");

        let mut cmb_pw = Default::default();
        nwg::ComboBox::builder()
            .parent(&window)
            .position((CONTROL_X, y))
            .size((CONTROL_W, 25))
            .font(Some(&font))
            .collection(MIN_PW_LENGTHS.iter().map(|s| s.to_string()).collect())
            .selected_index(Some(pw_length_index(current.min_password_length)))
            .build(&mut cmb_pw)
            .expect("Failed to build pw length combo");

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
        nwg::Label::builder()
            .text("On close:")
            .parent(&window)
            .position((LABEL_X, y + 2))
            .size((150, 20))
            .font(Some(&font))
            .build(&mut lbl_close)
            .expect("Failed to build label");

        let mut cmb_close = Default::default();
        nwg::ComboBox::builder()
            .parent(&window)
            .position((CONTROL_X, y))
            .size((CONTROL_W, 25))
            .font(Some(&font))
            .collection(CLOSE_BEHAVIORS.iter().map(|s| s.to_string()).collect())
            .selected_index(Some(close_action_index(current.save_on_close)))
            .build(&mut cmb_close)
            .expect("Failed to build close combo");

        y += ROW_H;

        // --- Save key ---
        let mut lbl_savekey = Default::default();
        nwg::Label::builder()
            .text("Save shortcut:")
            .parent(&window)
            .position((LABEL_X, y + 2))
            .size((150, 20))
            .font(Some(&font))
            .build(&mut lbl_savekey)
            .expect("Failed to build label");

        let mut cmb_savekey = Default::default();
        nwg::ComboBox::builder()
            .parent(&window)
            .position((CONTROL_X, y))
            .size((CONTROL_W, 25))
            .font(Some(&font))
            .collection(SAVE_KEYS.iter().map(|s| format!("Ctrl+{}", s)).collect())
            .selected_index(Some(save_key_index(current.save_key)))
            .build(&mut cmb_savekey)
            .expect("Failed to build save key combo");

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

                        let savekey_idx = cmb_savekey.selection().unwrap_or(18);

                        *result_clone.borrow_mut() = Some(SettingsChanges {
                            save_on_close: index_to_close_action(close_idx),
                            theme: index_to_theme(theme_idx),
                            min_password_length: index_to_pw_length(pw_idx),
                            minimize_to_tray: tray,
                            font_family,
                            font_size,
                            save_key: index_to_save_key(savekey_idx),
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
        drop(lbl_pw);

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
}
