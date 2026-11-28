// RenameTabDialog — Tab rename dialog
//
// Input: new tab name (pre-filled with current name)
// Validation: non-empty
// Returns Option<String>: new name, or None if cancelled

use native_windows_gui as nwg;
use std::cell::RefCell;
use std::rc::Rc;

fn t(key: &str) -> String {
    crate::features::i18n::get(key).to_string()
}

pub struct RenameTabDialog;

impl RenameTabDialog {
    /// Show the rename dialog. `current_name` is pre-filled.
    /// Returns Some(new_name) on OK, None on Cancel.
    pub fn show(current_name: &str) -> Option<String> {
        let mut window = Default::default();
        nwg::Window::builder()
            .size((350, 150))
            .position((350, 300))
            .title(&t("rename.title"))
            .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
            .build(&mut window)
            .expect("Failed to build RenameTabDialog window");

        super::apply_dialog_theme(&window);

        let mut font = Default::default();
        nwg::Font::builder()
            .family("Segoe UI")
            .size(16)
            .build(&mut font)
            .expect("Failed to build font");

        // Label
        let mut lbl = Default::default();
        nwg::Label::builder()
            .text(&t("rename.label"))
            .parent(&window)
            .position((20, 15))
            .size((310, 20))
            .font(Some(&font))
            .build(&mut lbl)
            .expect("Failed to build label");

        // Name input
        let mut txt_name = Default::default();
        nwg::TextInput::builder()
            .text(current_name)
            .parent(&window)
            .position((20, 40))
            .size((310, 25))
            .font(Some(&font))
            .focus(true)
            .build(&mut txt_name)
            .expect("Failed to build name input");

        // Error label
        let mut lbl_error = Default::default();
        nwg::Label::builder()
            .text("")
            .parent(&window)
            .position((20, 70))
            .size((310, 20))
            .font(Some(&font))
            .build(&mut lbl_error)
            .expect("Failed to build error label");
        super::mark_error(window.handle, lbl_error.handle);

        // OK button
        let mut btn_ok = Default::default();
        nwg::Button::builder()
            .text(&t("rename.ok"))
            .parent(&window)
            .position((145, 95))
            .size((85, 30))
            .font(Some(&font))
            .build(&mut btn_ok)
            .expect("Failed to build OK button");

        // Cancel button
        let mut btn_cancel = Default::default();
        nwg::Button::builder()
            .text(&t("rename.cancel"))
            .parent(&window)
            .position((245, 95))
            .size((85, 30))
            .font(Some(&font))
            .build(&mut btn_cancel)
            .expect("Failed to build Cancel button");

        // --- Enter key → OK button ---
        super::bind_enter_to_button(window.handle, btn_ok.handle);

        let result: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        let window_handle = window.handle;
        let result_clone = result.clone();

        let handler = nwg::full_bind_event_handler(&window_handle, move |evt, _evt_data, handle| {
            match evt {
                nwg::Event::OnButtonClick => {
                    if handle == btn_ok.handle {
                        let Some(name) =
                            crate::ui::field_text::read_visible(txt_name.handle, "rename.name", |m| lbl_error.set_text(m))
                        else {
                            return;
                        };
                        let trimmed = name.trim();
                        if trimmed.is_empty() {
                            lbl_error.set_text(&t("rename.empty"));
                        } else {
                            *result_clone.borrow_mut() = Some(trimmed.to_string());
                            nwg::stop_thread_dispatch();
                        }
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

        let val = result.borrow_mut().take();
        val
    }

    /// Generic input dialog with custom title and label.
    /// Returns Some(text) on OK, None on Cancel.
    pub fn show_with_labels(current_value: &str, title: &str, label: &str) -> Option<String> {
        let mut window = Default::default();
        nwg::Window::builder()
            .size((350, 150))
            .position((350, 300))
            .title(title)
            .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
            .build(&mut window)
            .expect("Failed to build input dialog window");

        super::apply_dialog_theme(&window);

        let mut font = Default::default();
        nwg::Font::builder()
            .family("Segoe UI")
            .size(16)
            .build(&mut font)
            .expect("Failed to build font");

        let mut lbl = Default::default();
        nwg::Label::builder()
            .text(label)
            .parent(&window)
            .position((20, 15))
            .size((310, 20))
            .font(Some(&font))
            .build(&mut lbl)
            .expect("Failed to build label");

        let mut txt_name = Default::default();
        nwg::TextInput::builder()
            .text(current_value)
            .parent(&window)
            .position((20, 40))
            .size((310, 25))
            .font(Some(&font))
            .focus(true)
            .build(&mut txt_name)
            .expect("Failed to build text input");

        let mut btn_ok = Default::default();
        nwg::Button::builder()
            .text(&t("rename.ok"))
            .parent(&window)
            .position((145, 95))
            .size((85, 30))
            .font(Some(&font))
            .build(&mut btn_ok)
            .expect("Failed to build OK button");

        let mut btn_cancel = Default::default();
        nwg::Button::builder()
            .text(&t("rename.cancel"))
            .parent(&window)
            .position((245, 95))
            .size((85, 30))
            .font(Some(&font))
            .build(&mut btn_cancel)
            .expect("Failed to build Cancel button");

        super::bind_enter_to_button(window.handle, btn_ok.handle);

        let result: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        let window_handle = window.handle;
        let dialog_title = title.to_string();
        let result_clone = result.clone();

        let handler = nwg::full_bind_event_handler(&window_handle, move |evt, _evt_data, handle| {
            match evt {
                nwg::Event::OnButtonClick => {
                    if handle == btn_ok.handle {
                        let Some(name) = crate::ui::field_text::read_visible(txt_name.handle, "rename.name", |m| {
                            nwg::modal_error_message(window_handle, &dialog_title, m);
                        }) else {
                            return;
                        };
                        *result_clone.borrow_mut() = Some(name.trim().to_string());
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

        let val = result.borrow_mut().take();
        val
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn module_compiles() {
        let _ = std::mem::size_of::<super::RenameTabDialog>();
    }
}
