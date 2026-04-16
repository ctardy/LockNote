// GoToLineDialog — Ctrl+G go-to-line dialog
//
// Input: line number (pre-filled with current line)
// Validation: integer between 1 and max_lines
// Returns Option<u32>: line number, or None if cancelled

use native_windows_gui as nwg;
use std::cell::RefCell;
use std::rc::Rc;

pub struct GoToLineDialog;

impl GoToLineDialog {
    /// Show the go-to-line dialog.
    ///
    /// `current_line` is pre-filled in the input.
    /// `max_lines` is the upper bound for validation.
    pub fn show(current_line: u32, max_lines: u32) -> Option<u32> {
        let mut window = Default::default();
        nwg::Window::builder()
            .size((320, 150))
            .position((350, 300))
            .title("Go to Line")
            .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
            .build(&mut window)
            .expect("Failed to build GoToLineDialog window");

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
            .text(&format!("Line number (1 - {}):", max_lines))
            .parent(&window)
            .position((20, 15))
            .size((280, 20))
            .font(Some(&font))
            .build(&mut lbl)
            .expect("Failed to build label");

        // Line number input
        let mut txt_line = Default::default();
        nwg::TextInput::builder()
            .text(&current_line.to_string())
            .parent(&window)
            .position((20, 40))
            .size((280, 25))
            .font(Some(&font))
            .focus(true)
            .build(&mut txt_line)
            .expect("Failed to build line number input");

        // Error label
        let mut lbl_error = Default::default();
        nwg::Label::builder()
            .text("")
            .parent(&window)
            .position((20, 70))
            .size((280, 20))
            .font(Some(&font))
            .build(&mut lbl_error)
            .expect("Failed to build error label");

        // OK button
        let mut btn_ok = Default::default();
        nwg::Button::builder()
            .text("OK")
            .parent(&window)
            .position((120, 95))
            .size((80, 30))
            .font(Some(&font))
            .build(&mut btn_ok)
            .expect("Failed to build OK button");

        // Cancel button
        let mut btn_cancel = Default::default();
        nwg::Button::builder()
            .text("Cancel")
            .parent(&window)
            .position((215, 95))
            .size((80, 30))
            .font(Some(&font))
            .build(&mut btn_cancel)
            .expect("Failed to build Cancel button");

        // --- Enter key → OK button ---
        super::bind_enter_to_button(window.handle, btn_ok.handle);

        let result: Rc<RefCell<Option<u32>>> = Rc::new(RefCell::new(None));
        let window_handle = window.handle;
        let result_clone = result.clone();

        let handler = nwg::full_bind_event_handler(&window_handle, move |evt, _evt_data, handle| {
            match evt {
                nwg::Event::OnButtonClick => {
                    if handle == btn_ok.handle {
                        let text = txt_line.text();
                        match text.trim().parse::<u32>() {
                            Ok(n) if n >= 1 && n <= max_lines => {
                                *result_clone.borrow_mut() = Some(n);
                                nwg::stop_thread_dispatch();
                            }
                            _ => {
                                lbl_error.set_text(
                                    &format!("Enter a number between 1 and {}", max_lines),
                                );
                            }
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

        let out = result.borrow_mut().take();
        out
    }
}

#[cfg(test)]
mod tests {
    // GoToLineDialog is UI-only; validation logic is inline.
    // Structural test to confirm the module compiles.
    #[test]
    fn module_compiles() {
        // If this compiles, the module structure is valid.
        let _ = std::mem::size_of::<super::GoToLineDialog>();
    }

    #[test]
    fn goto_line_dialog_is_zero_sized() {
        assert_eq!(std::mem::size_of::<super::GoToLineDialog>(), 0);
    }
}
