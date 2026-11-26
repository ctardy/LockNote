// CloseConfirmDialog — "Unsaved changes" confirmation
//
// Three buttons: Save, Don't save, Cancel
// Checkbox: "Remember my choice"
// Returns CloseConfirmResult { action, remember }

use native_windows_gui as nwg;
use std::cell::RefCell;
use std::rc::Rc;

fn t(key: &str) -> String {
    crate::features::i18n::get(key).to_string()
}

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CloseAction {
    Save,
    DontSave,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CloseConfirmResult {
    pub action: CloseAction,
    pub remember: bool,
}

// ---------------------------------------------------------------------------
// Owner disabled while a close dialog is shown (pass 4: Ctrl+S, Ctrl+Q and
// typing reached the main window under the prompt)
// ---------------------------------------------------------------------------

/// Calls `set_enabled(false)` now and, when dropped (also on an early
/// return or a panic), puts back the state it had: `set_enabled` returns
/// whether the window was enabled before the call (pass 5: an owner already
/// disabled by another dialog was enabled under it).
pub struct DisabledWhile<F: FnMut(bool) -> bool> {
    set_enabled: F,
    was_enabled: bool,
}

impl<F: FnMut(bool) -> bool> DisabledWhile<F> {
    pub fn new(mut set_enabled: F) -> Self {
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

/// Disable `owner` until the guard is dropped.
fn disable_owner(owner: Option<nwg::ControlHandle>) -> DisabledWhile<impl FnMut(bool) -> bool> {
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
        !was_disabled.as_bool()
    })
}

// ---------------------------------------------------------------------------
// Dialog
// ---------------------------------------------------------------------------

pub struct CloseConfirmDialog;

impl CloseConfirmDialog {
    /// Show the confirmation dialog. Returns the user's choice. `owner`: the
    /// main window, so the dialog stays above it even when it is always on
    /// top (an owned window is never behind its owner).
    pub fn show(owner: Option<nwg::ControlHandle>) -> CloseConfirmResult {
        let mut window = Default::default();
        nwg::Window::builder()
            .size((420, 170))
            .position((300, 300))
            .title(&t("close.title"))
            .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
            .parent(owner)
            .build(&mut window)
            .expect("Failed to build CloseConfirmDialog window");
        // Declared after the window: dropped first, so the owner is enabled
        // again before the dialog goes and gets the focus back.
        let _owner_disabled = disable_owner(owner);

        super::apply_dialog_theme(&window);

        let mut font = Default::default();
        nwg::Font::builder()
            .family("Segoe UI")
            .size(16)
            .build(&mut font)
            .expect("Failed to build font");

        // Message
        let mut lbl_msg = Default::default();
        nwg::Label::builder()
            .text(&t("close.msg"))
            .parent(&window)
            .position((20, 15))
            .size((380, 25))
            .font(Some(&font))
            .build(&mut lbl_msg)
            .expect("Failed to build message label");

        // Remember checkbox
        let mut chk_remember = Default::default();
        nwg::CheckBox::builder()
            .text(&t("close.remember"))
            .parent(&window)
            .position((20, 55))
            .size((250, 25))
            .font(Some(&font))
            .build(&mut chk_remember)
            .expect("Failed to build remember checkbox");

        // Save button
        let mut btn_save = Default::default();
        nwg::Button::builder()
            .text(&t("close.save"))
            .parent(&window)
            .position((80, 100))
            .size((90, 30))
            .font(Some(&font))
            .build(&mut btn_save)
            .expect("Failed to build Save button");

        super::bind_enter_to_button(window.handle, btn_save.handle);

        // Don't save button
        let mut btn_dont = Default::default();
        nwg::Button::builder()
            .text(&t("close.dontsave"))
            .parent(&window)
            .position((185, 100))
            .size((90, 30))
            .font(Some(&font))
            .build(&mut btn_dont)
            .expect("Failed to build Don't Save button");

        // Cancel button
        let mut btn_cancel = Default::default();
        nwg::Button::builder()
            .text(&t("close.cancel"))
            .parent(&window)
            .position((290, 100))
            .size((90, 30))
            .font(Some(&font))
            .build(&mut btn_cancel)
            .expect("Failed to build Cancel button");

        let result: Rc<RefCell<CloseConfirmResult>> = Rc::new(RefCell::new(CloseConfirmResult {
            action: CloseAction::Cancel,
            remember: false,
        }));

        let window_handle = window.handle;
        let result_clone = result.clone();

        let handler = nwg::full_bind_event_handler(&window_handle, move |evt, _evt_data, handle| {
            match evt {
                nwg::Event::OnButtonClick => {
                    let remember = chk_remember.check_state() == nwg::CheckBoxState::Checked;

                    if handle == btn_save.handle {
                        *result_clone.borrow_mut() = CloseConfirmResult {
                            action: CloseAction::Save,
                            remember,
                        };
                        nwg::stop_thread_dispatch();
                    } else if handle == btn_dont.handle {
                        *result_clone.borrow_mut() = CloseConfirmResult {
                            action: CloseAction::DontSave,
                            remember,
                        };
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

        let r = { *result.borrow() };
        r
    }
}

// ---------------------------------------------------------------------------
// Save failed at close (pass 3: a Yes/No box had Windows' button labels and
// an ambiguous question; ES even said the opposite)
// ---------------------------------------------------------------------------

/// What the user did in the "could not save" dialog.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SaveFailedAnswer {
    KeepOpen,
    CloseWithoutSaving,
    /// Title bar close, Esc.
    Dismissed,
}

/// Whether the close goes on without saving: only an explicit click on
/// "Close without saving"; Enter, Esc and the title bar keep LockNote open.
pub fn closes_without_saving(answer: SaveFailedAnswer) -> bool {
    answer == SaveFailedAnswer::CloseWithoutSaving
}

pub struct SaveFailedDialog;

impl SaveFailedDialog {
    /// `message`: `close.save_failed` with the reason. Owned by `owner` (the
    /// main window). "Keep LockNote open" is the default (Enter) button.
    pub fn show(owner: Option<nwg::ControlHandle>, message: &str) -> SaveFailedAnswer {
        let mut window = Default::default();
        nwg::Window::builder()
            .size((460, 190))
            .position((300, 300))
            .title(&t("save.error.title"))
            .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
            .parent(owner)
            .build(&mut window)
            .expect("Failed to build SaveFailedDialog window");
        let _owner_disabled = disable_owner(owner);

        super::apply_dialog_theme(&window);

        let mut font = Default::default();
        nwg::Font::builder()
            .family("Segoe UI")
            .size(16)
            .build(&mut font)
            .expect("Failed to build font");

        // Several lines (the reason can be long): the shared pass wraps it.
        let mut lbl_msg = Default::default();
        nwg::Label::builder()
            .text(message)
            .parent(&window)
            .position((20, 15))
            .size((420, 100))
            .font(Some(&font))
            .build(&mut lbl_msg)
            .expect("Failed to build message label");

        let mut btn_keep = Default::default();
        nwg::Button::builder()
            .text(&t("close.keep_open"))
            .parent(&window)
            .position((110, 130))
            .size((160, 30))
            .font(Some(&font))
            .build(&mut btn_keep)
            .expect("Failed to build Keep open button");
        super::bind_enter_to_button(window.handle, btn_keep.handle);

        let mut btn_close = Default::default();
        nwg::Button::builder()
            .text(&t("close.close_unsaved"))
            .parent(&window)
            .position((280, 130))
            .size((160, 30))
            .font(Some(&font))
            .build(&mut btn_close)
            .expect("Failed to build Close without saving button");

        let answer = Rc::new(RefCell::new(SaveFailedAnswer::Dismissed));
        let answer_clone = answer.clone();
        let handler = nwg::full_bind_event_handler(&window.handle, move |evt, _evt_data, handle| {
            match evt {
                nwg::Event::OnButtonClick => {
                    if handle == btn_keep.handle {
                        *answer_clone.borrow_mut() = SaveFailedAnswer::KeepOpen;
                        nwg::stop_thread_dispatch();
                    } else if handle == btn_close.handle {
                        *answer_clone.borrow_mut() = SaveFailedAnswer::CloseWithoutSaving;
                        nwg::stop_thread_dispatch();
                    }
                }
                nwg::Event::OnWindowClose => nwg::stop_thread_dispatch(),
                _ => {}
            }
        });

        nwg::dispatch_thread_events();
        nwg::unbind_event_handler(&handler);
        let a = *answer.borrow();
        a
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_owner_is_enabled_again_on_every_path() {
        let calls = std::cell::RefCell::new(Vec::new());
        {
            let _g = DisabledWhile::new(|e| { calls.borrow_mut().push(e); true });
        }
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _g = DisabledWhile::new(|e| { calls.borrow_mut().push(e); true });
            panic!("dialog failed");
        }));
        assert!(r.is_err());
        assert_eq!(*calls.borrow(), [false, true, false, true]);
    }

    /// Pass 5: an owner that was already disabled (under another dialog)
    /// stays disabled; its previous state is restored, not "enabled".
    #[test]
    fn the_owner_gets_its_previous_state_back() {
        let enabled = std::cell::Cell::new(false);
        {
            let _g = DisabledWhile::new(|e: bool| enabled.replace(e));
            assert!(!enabled.get());
        }
        assert!(!enabled.get(), "was disabled before: stays disabled");
        enabled.set(true);
        {
            let _g = DisabledWhile::new(|e: bool| enabled.replace(e));
            assert!(!enabled.get());
        }
        assert!(enabled.get());
    }

    #[test]
    fn only_the_explicit_button_closes_without_saving() {
        assert!(closes_without_saving(SaveFailedAnswer::CloseWithoutSaving));
        assert!(!closes_without_saving(SaveFailedAnswer::KeepOpen));
        assert!(!closes_without_saving(SaveFailedAnswer::Dismissed));
    }

    /// The buttons carry LockNote's own words, distinct, in every language,
    /// and the message no longer asks a Yes/No question.
    #[test]
    fn the_save_failed_texts_name_the_two_actions() {
        use crate::features::i18n::{get, set_language, Language};
        let _g = crate::features::i18n::test_support::lang_guard();
        for lang in [Language::English, Language::French, Language::Spanish, Language::German] {
            set_language(lang);
            let keep = get("close.keep_open").to_string();
            let close = get("close.close_unsaved").to_string();
            let msg = get("close.save_failed").to_string();
            assert!(!keep.starts_with('[') && !close.starts_with('['), "{:?}", lang);
            assert_ne!(keep, close);
            assert!(msg.contains("{error}") && !msg.contains('?'), "{:?}: {}", lang, msg);
        }
    }

    #[test]
    fn default_result_is_cancel() {
        let r = CloseConfirmResult {
            action: CloseAction::Cancel,
            remember: false,
        };
        assert_eq!(r.action, CloseAction::Cancel);
        assert!(!r.remember);
    }

    #[test]
    fn close_action_variants() {
        assert_ne!(CloseAction::Save, CloseAction::DontSave);
        assert_ne!(CloseAction::DontSave, CloseAction::Cancel);
    }
}
