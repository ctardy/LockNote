// UI Dialogs — Password creation, unlock, settings, about, etc.

pub mod create_password;
pub mod unlock;
pub mod close_confirm;
pub mod goto_line;
pub mod goto_heading;
pub mod compare_dialog;
pub mod find_replace;
pub mod regex_search;
pub mod search_all;
pub mod about;
pub mod settings_dialog;
pub mod preferences_dialog;
pub mod security_dialog;
pub mod temp_files_dialog;

pub mod drop_choice;

pub mod rename_tab;

pub mod secret_edit;

pub mod prompt_edit;

pub mod ssh_keygen;

pub mod totp_setup;

pub mod totp_verify;

pub mod totp_manager;

pub mod backups_dialog;

pub mod export_encoding;

pub mod confirm_password;

pub mod backup_password;

pub mod password_generator;

pub mod command_palette;

use native_windows_gui as nwg;

pub mod metrics;
pub mod style;

#[cfg(test)]
mod snapshot;

pub use style::{mark_error, set_frame_color};

/// Combo items for durations in seconds: 0 is "Off", then "{n} sec" in the
/// current language (the items were English literals, issue #25 pass 2).
pub(crate) fn seconds_options(values: &[u32]) -> Vec<String> {
    values
        .iter()
        .map(|&v| if v == 0 { crate::features::i18n::get("option.off").to_string() } else { crate::features::i18n::get("option.seconds").replace("{n}", &v.to_string()) })
        .collect()
}

/// Combo items for durations in minutes, as `seconds_options`.
pub(crate) fn minutes_options(values: &[u32]) -> Vec<String> {
    values
        .iter()
        .map(|&v| if v == 0 { crate::features::i18n::get("option.off").to_string() } else { crate::features::i18n::get("option.minutes").replace("{n}", &v.to_string()) })
        .collect()
}

#[cfg(test)]
mod open_dialog_tests {
    use native_windows_gui as nwg;

    /// Pass 3: the count the timers and the lock rely on follows a real
    /// dialog: one while it exists, none once it is destroyed (no leaked
    /// registration).
    #[test]
    fn a_themed_dialog_is_counted_while_it_exists() {
        let _ = nwg::init();
        let before = super::style::open_dialog_count();
        {
            let mut window = nwg::Window::default();
            nwg::Window::builder()
                .size((200, 100))
                .position((-3000, -3000))
                .title("count test")
                .flags(nwg::WindowFlags::WINDOW)
                .build(&mut window)
                .expect("test window");
            super::apply_dialog_theme(&window);
            assert_eq!(super::style::open_dialog_count(), before + 1);
            assert!(super::style::registered_dialogs().len() == before + 1);
        }
        assert_eq!(super::style::open_dialog_count(), before);
    }
}

#[cfg(test)]
mod duration_option_tests {
    #[test]
    fn duration_options_are_translated() {
        use crate::features::i18n::{set_language, Language};
        let _g = crate::features::i18n::test_support::lang_guard();
        set_language(Language::French);
        let s = super::seconds_options(&[0, 10]);
        let m = super::minutes_options(&[0, 5]);
        set_language(Language::English);
        let en = super::seconds_options(&[0, 30]);
        assert_eq!(s, ["D\u{e9}sactiv\u{e9}", "10 s"]);
        assert_eq!(m, ["D\u{e9}sactiv\u{e9}", "5 min"]);
        assert_eq!(en, ["Off", "30 sec"]);
    }
}

/// Calls `set_enabled(false)` now and, when dropped (also on an early
/// return or a panic), puts back the state it had: `set_enabled` returns
/// whether the window was enabled before the call (pass 5: an owner already
/// disabled by another dialog was enabled under it).
pub(crate) struct DisabledWhile<F: FnMut(bool) -> bool> {
    set_enabled: F,
    was_enabled: bool,
}

impl<F: FnMut(bool) -> bool> DisabledWhile<F> {
    pub(crate) fn new(mut set_enabled: F) -> Self {
        let was_enabled = set_enabled(false);
        DisabledWhile { set_enabled, was_enabled }
    }
}

impl<F: FnMut(bool) -> bool> Drop for DisabledWhile<F> {
    fn drop(&mut self) {
        if self.was_enabled {
            (self.set_enabled)(true);
        }
    }
}

/// Disable `owner` until the guard is dropped, then put back its previous
/// state (pass 6: also used by the temporary files dialog and the password
/// generator, which re-enabled their owner unconditionally). `focus_back`:
/// give the owner the foreground again when it is re-enabled.
pub(crate) fn disable_owner(owner: Option<nwg::ControlHandle>, focus_back: bool) -> DisabledWhile<impl FnMut(bool) -> bool> {
    let hwnd = owner.and_then(|o| o.hwnd());
    DisabledWhile::new(move |enabled| {
        let Some(h) = hwnd else { return false };
        // EnableWindow returns TRUE when the window WAS disabled.
        let was_disabled = unsafe {
            windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow(
                windows::Win32::Foundation::HWND(h as *mut _),
                enabled,
            )
        };
        if enabled && focus_back {
            // The return value only says whether it came to the front.
            let _came_to_front = unsafe {
                windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(windows::Win32::Foundation::HWND(h as *mut _))
            };
        }
        !was_disabled.as_bool()
    })
}

/// Give a dialog the shared look. Call right after the window is built, before
/// its controls: the styling pass runs from the first message of the dialog's
/// loop, when every control exists (see `style.rs`).
///
/// Dark title bar and window background, then, per control: palette colors for
/// labels / edit boxes / lists (`WM_CTLCOLOR*`), DPI scaling, the font
/// hierarchy, reflow of text that does not fit, uniform footer buttons, dark
/// native controls (Dark theme), Esc = close, centering on the main window.
pub fn apply_dialog_theme(window: &nwg::Window) {
    apply_window_chrome(window);
    style::register(window);

    // Install raw event handler for the shared look
    let raw = nwg::bind_raw_event_handler(
        &window.handle,
        0x30000, // unique handler ID
        move |hwnd, msg, wparam, lparam| style::handle_message(hwnd as isize, msg, wparam as usize, lparam),
    );
    // Keep the handler alive for the dialog lifetime
    std::mem::forget(raw);
}

/// Title bar and background brush only, for windows that lay out and paint
/// their own content (the unlock card): no DPI rescale, no restyled controls.
pub fn apply_window_chrome(window: &nwg::Window) {
    use nwg::ControlHandle;
    use crate::theme;

    let pal = theme::current();

    // Dark/light title bar (no-op look change in the Light theme)
    crate::ui::dark_chrome::apply_title_bar(&window.handle);

    // Set window background brush
    if let ControlHandle::Hwnd(hwnd) = window.handle {
        let hwnd_w = windows::Win32::Foundation::HWND(hwnd as *mut _);
        // Shared, colour-keyed brush: nothing to leak when the theme changes.
        let brush = windows::Win32::Graphics::Gdi::HBRUSH(
            crate::ui::lock_card::cached_brush(pal.background) as *mut _,
        );
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::SetClassLongPtrW(
                hwnd_w,
                windows::Win32::UI::WindowsAndMessaging::GCL_HBRBACKGROUND,
                brush.0 as isize,
            );
            // Force repaint
            let _ = windows::Win32::Graphics::Gdi::InvalidateRect(Some(hwnd_w), None, true);
        }
    }
}

/// Disable Windows visual themes on a control so WM_CTLCOLOR* messages are respected.
/// Required for checkboxes in dark mode — visual themes override text/background colors.
pub fn disable_control_theme(handle: nwg::ControlHandle) {
    if let nwg::ControlHandle::Hwnd(hwnd) = handle {
        let hwnd_w = windows::Win32::Foundation::HWND(hwnd as *mut _);
        let empty = [0u16; 1];
        unsafe {
            let _ = windows::Win32::UI::Controls::SetWindowTheme(
                hwnd_w,
                windows::core::PCWSTR(empty.as_ptr()),
                windows::core::PCWSTR(empty.as_ptr()),
            );
        }
    }
}

/// Bind Enter key to trigger a button click.
///
/// nwg's dispatch loop uses `IsDialogMessage` which intercepts VK_RETURN
/// and sends WM_COMMAND with IDOK (1) to the parent window.
/// This function installs a handler on the **window** to catch that command.
///
/// The button also becomes the dialog's primary button (accent fill).
pub fn bind_enter_to_button(window_handle: nwg::ControlHandle, button_handle: nwg::ControlHandle) {
    style::set_primary(window_handle, button_handle);
    let raw = nwg::bind_raw_event_handler(
        &window_handle,
        0x20000,
        move |_hwnd, msg, wparam, _lparam| {
            // WM_COMMAND = 0x0111, IDOK = 1 (sent by IsDialogMessage when Enter is pressed)
            if msg == 0x0111 && (wparam & 0xFFFF) == 1 {
                if let nwg::ControlHandle::Hwnd(btn_hwnd) = button_handle {
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
