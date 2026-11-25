// BackupPasswordDialog — Ask for the password of a backup file being imported.
//
// Unlike ConfirmPasswordDialog there is no reference password to compare
// with: the caller tries the typed one against the file. Returns `None` when
// the user cancels. An empty field is refused inside the dialog with
// `empty_msg` (never a silent close).

use native_windows_gui as nwg;
use std::cell::RefCell;
use std::rc::Rc;
use zeroize::Zeroizing;

pub struct BackupPasswordDialog;

impl BackupPasswordDialog {
    /// Show the dialog. All texts come from the caller so they are translated.
    pub fn show(title: &str, label: &str, ok: &str, cancel: &str, empty_msg: &str) -> Option<Zeroizing<String>> {
        // --- Center on screen ---
        #[link(name = "user32")]
        extern "system" {
            fn GetSystemMetrics(index: i32) -> i32;
        }
        let win_w: i32 = 380;
        let win_h: i32 = 195;
        let screen_w = unsafe { GetSystemMetrics(0) };
        let screen_h = unsafe { GetSystemMetrics(1) };
        let pos_x = (screen_w - win_w) / 2;
        let pos_y = (screen_h - win_h) / 2;

        let mut window = Default::default();
        nwg::Window::builder()
            .size((win_w, win_h))
            .position((pos_x, pos_y))
            .title(title)
            .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
            .build(&mut window)
            .expect("Failed to build BackupPasswordDialog window");

        super::apply_dialog_theme(&window);

        let mut font = Default::default();
        nwg::Font::builder()
            .family("Segoe UI")
            .size(16)
            .build(&mut font)
            .expect("Failed to build font");

        let margin: i32 = 20;
        let content_w: i32 = win_w - margin * 2;

        let mut lbl = Default::default();
        nwg::Label::builder()
            .text(label)
            .parent(&window)
            .position((margin, 15))
            .size((content_w, 22))
            .font(Some(&font))
            .build(&mut lbl)
            .expect("Failed to build label");

        let mut txt_password = Default::default();
        nwg::TextInput::builder()
            .parent(&window)
            .position((margin, 45))
            .size((content_w, 28))
            .password(Some('*'))
            .font(Some(&font))
            .focus(true)
            .build(&mut txt_password)
            .expect("Failed to build password input");

        let mut lbl_error = Default::default();
        nwg::Label::builder()
            .text("")
            .parent(&window)
            .position((margin, 80))
            .size((content_w, 22))
            .font(Some(&font))
            .build(&mut lbl_error)
            .expect("Failed to build error label");
        super::mark_error(window.handle, lbl_error.handle);

        let mut btn_ok = Default::default();
        nwg::Button::builder()
            .text(ok)
            .parent(&window)
            .position((win_w - margin - 185, 120))
            .size((85, 32))
            .font(Some(&font))
            .build(&mut btn_ok)
            .expect("Failed to build OK button");

        let mut btn_cancel = Default::default();
        nwg::Button::builder()
            .text(cancel)
            .parent(&window)
            .position((win_w - margin - 90, 120))
            .size((90, 32))
            .font(Some(&font))
            .build(&mut btn_cancel)
            .expect("Failed to build Cancel button");

        // Enter key → OK button
        super::bind_enter_to_button(window.handle, btn_ok.handle);

        let result: Rc<RefCell<Option<Zeroizing<String>>>> = Rc::new(RefCell::new(None));
        let result_clone = result.clone();
        let focus_field_handle = txt_password.handle;
        let window_handle = window.handle;
        let empty_msg = empty_msg.to_string();

        let handler = nwg::full_bind_event_handler(&window_handle, move |evt, _evt_data, handle| {
            match evt {
                nwg::Event::OnButtonClick => {
                    if handle == btn_ok.handle {
                        let pw: Zeroizing<String> = Zeroizing::new(txt_password.text());
                        txt_password.set_text("");
                        if pw.is_empty() {
                            // Stay open: closing here would look like a silent failure.
                            lbl_error.set_text(&empty_msg);
                            txt_password.set_focus();
                        } else {
                            *result_clone.borrow_mut() = Some(pw);
                            nwg::stop_thread_dispatch();
                        }
                    } else if handle == btn_cancel.handle {
                        txt_password.set_text("");
                        nwg::stop_thread_dispatch();
                    }
                }
                nwg::Event::OnWindowClose => {
                    txt_password.set_text("");
                    nwg::stop_thread_dispatch();
                }
                _ => {}
            }
        });

        // Caret in the field as soon as the window has the focus.
        let focus_guard = crate::ui::lock_card::keep_focus_on(&window_handle, &focus_field_handle)
            .expect("Failed to bind the focus handler");

        nwg::dispatch_thread_events();
        nwg::unbind_event_handler(&handler);
        nwg::unbind_raw_event_handler(&focus_guard).ok();

        let password = result.borrow_mut().take();
        password
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn module_compiles() {
        let _ = std::mem::size_of::<super::BackupPasswordDialog>();
    }
}
