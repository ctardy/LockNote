// The name prompt of Tools > TOTP > Add Key (`simple_input_dialog`).
//
// The former TotpManagerDialog was never shown by the application (only the
// snapshot tool opened it; the TOTP panel replaced it): it is gone (pass 3).

use native_windows_gui as nwg;
use std::cell::RefCell;
use std::rc::Rc;

fn t(key: &str) -> String {
    crate::features::i18n::get(key).to_string()
}

/// Simple single-line input dialog. Returns None if cancelled.
pub fn simple_input_dialog(
    parent: &nwg::ControlHandle,
    title: &str,
    prompt: &str,
) -> Option<String> {
    let mut window = Default::default();
    nwg::Window::builder()
        .size((350, 150))
        .position((400, 300))
        .title(title)
        .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
        .parent(Some(parent))
        .build(&mut window)
        .expect("Failed to build input dialog");

    super::apply_dialog_theme(&window);

    let mut font = Default::default();
    nwg::Font::builder()
        .family("Segoe UI")
        .size(16)
        .build(&mut font)
        .expect("Failed to build font");

    let mut _lbl = Default::default();
    nwg::Label::builder()
        .text(prompt)
        .parent(&window)
        .position((15, 15))
        .size((320, 20))
        .font(Some(&font))
        .build(&mut _lbl)
        .expect("Failed to build label");

    let mut txt = Default::default();
    nwg::TextInput::builder()
        .parent(&window)
        .position((15, 40))
        .size((320, 28))
        .font(Some(&font))
        .focus(true)
        .build(&mut txt)
        .expect("Failed to build text input");

    let mut btn_ok = Default::default();
    nwg::Button::builder()
        .text(&t("settings.ok"))
        .parent(&window)
        .position((150, 80))
        .size((80, 30))
        .font(Some(&font))
        .build(&mut btn_ok)
        .expect("Failed to build OK button");

    let mut btn_cancel = Default::default();
    nwg::Button::builder()
        .text(&t("settings.cancel"))
        .parent(&window)
        .position((245, 80))
        .size((80, 30))
        .font(Some(&font))
        .build(&mut btn_cancel)
        .expect("Failed to build Cancel button");

    super::bind_enter_to_button(window.handle, btn_ok.handle);

    let result: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let window_handle = window.handle;
    let result_clone = result.clone();

    let handler = nwg::full_bind_event_handler(&window_handle, move |evt, _evt_data, handle| {
        match evt {
            nwg::Event::OnButtonClick => {
                if handle == btn_ok.handle {
                    // The name is stored: read strictly, the dialog stays
                    // open on a refusal.
                    let Some(name) = crate::ui::field_text::read_visible(txt.handle, "totp.name", |m| {
                        nwg::modal_error_message(window_handle, &t("editor.text.title"), m);
                    }) else {
                        return;
                    };
                    *result_clone.borrow_mut() = Some(name.to_string());
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
