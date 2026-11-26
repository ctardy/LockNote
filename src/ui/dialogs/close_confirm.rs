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

#[cfg(test)]
mod tests {
    use super::*;

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
