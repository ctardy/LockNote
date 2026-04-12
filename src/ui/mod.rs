// UI module — Main window, editor, dialogs, search bar
//
// Entry point: `ui::run(exe_path, data)` called from main.rs.
// Handles the startup flow: create-password or unlock, then launches EditorForm.

use std::path::PathBuf;

pub mod editor;
pub mod search_bar;
pub mod dialogs;
#[cfg(feature = "tabs")]
pub mod tab_bar;

/// Main UI entry point. Initializes NWG, handles password flow, launches editor.
pub fn run(exe_path: PathBuf, data: Option<Vec<u8>>) {
    // Initialize NWG
    nwg::init().expect("Failed to init Native Windows GUI");

    // Set global default font (Segoe UI 9pt)
    let mut default_font = nwg::Font::default();
    nwg::Font::builder()
        .family("Segoe UI")
        .size(17) // 9pt in NWG logical units
        .build(&mut default_font)
        .expect("Failed to build default font");
    nwg::Font::set_global_default(Some(default_font));

    match data {
        None => {
            // No existing data — first run, create a new password
            let password = match dialogs::create_password::show(4) {
                Some(pw) => pw,
                None => return, // User cancelled
            };
            let settings = crate::settings::Settings::default_public();
            editor::EditorForm::run(exe_path, password, String::new(), settings);
        }
        Some(encrypted) => {
            // Existing data — prompt to unlock
            match dialogs::unlock::show(&encrypted) {
                Some((password, decrypted_text)) => {
                    let (settings, note_text) = crate::settings::Settings::parse(&decrypted_text);

                    // Apply theme from settings
                    let theme_mode = match settings.theme {
                        crate::settings::ThemeChoice::Dark => crate::theme::ThemeMode::Dark,
                        crate::settings::ThemeChoice::Light => crate::theme::ThemeMode::Light,
                    };
                    crate::theme::set_mode(theme_mode);

                    editor::EditorForm::run(exe_path, password, note_text, settings);
                }
                None => {
                    // Unlock failed or cancelled
                    return;
                }
            }
        }
    }
}

// Re-export nwg for convenience within the ui module
use native_windows_gui as nwg;
