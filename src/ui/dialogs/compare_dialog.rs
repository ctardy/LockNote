// CompareDialog — Tab > Compare with Last Save
//
// Shows the unified line diff (`features::line_diff`) between the active
// tab's last saved text and its current text, read-only, in the editor
// font. Nothing comes back: Close, Enter, Esc or the title bar close it.

use native_windows_gui as nwg;

fn t(key: &str) -> String {
    crate::features::i18n::get(key).to_string()
}

pub struct CompareDialog;

impl CompareDialog {
    /// Show `text` (the diff lines joined with "\r\n", as the edit control
    /// wants them). `font_family` / `font_size` are the editor's (nwg size).
    /// `owner` (the main window) is disabled while the dialog is open.
    pub fn show(text: &str, font_family: &str, font_size: u32, owner: Option<nwg::ControlHandle>) {
        let mut window = Default::default();
        nwg::Window::builder()
            .size((760, 540))
            .position((250, 150))
            .title(&t("compare.title"))
            .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
            .build(&mut window)
            .expect("Failed to build CompareDialog window");

        super::apply_dialog_theme(&window);

        let mut ui_font = Default::default();
        nwg::Font::builder()
            .family("Segoe UI")
            .size(16)
            .build(&mut ui_font)
            .expect("Failed to build font");

        let mut diff_font = Default::default();
        nwg::Font::builder()
            .family(font_family)
            .size(font_size)
            .build(&mut diff_font)
            .expect("Failed to build editor font");

        // No wrap: a long line scrolls sideways, so the "- " / "+ " column
        // stays the first one.
        let mut txt_diff = Default::default();
        nwg::TextBox::builder()
            .parent(&window)
            .position((15, 15))
            .size((730, 455))
            .font(Some(&diff_font))
            .readonly(true)
            .flags(
                nwg::TextBoxFlags::VISIBLE
                    | nwg::TextBoxFlags::TAB_STOP
                    | nwg::TextBoxFlags::VSCROLL
                    | nwg::TextBoxFlags::HSCROLL
                    | nwg::TextBoxFlags::AUTOVSCROLL
                    | nwg::TextBoxFlags::AUTOHSCROLL,
            )
            .text(text)
            .build(&mut txt_diff)
            .expect("Failed to build diff box");

        let mut btn_close = Default::default();
        nwg::Button::builder()
            .text(&t("compare.close"))
            .parent(&window)
            .position((645, 485))
            .size((100, 32))
            .font(Some(&ui_font))
            .focus(true)
            .build(&mut btn_close)
            .expect("Failed to build Close button");

        // Enter → Close.
        super::bind_enter_to_button(window.handle, btn_close.handle);

        let window_handle = window.handle;
        let handler = nwg::full_bind_event_handler(&window_handle, move |evt, _evt_data, handle| match evt {
            nwg::Event::OnButtonClick if handle == btn_close.handle => {
                nwg::stop_thread_dispatch();
            }
            nwg::Event::OnWindowClose => {
                nwg::stop_thread_dispatch();
            }
            _ => {}
        });

        let owner_disabled = super::disable_owner(owner, true);
        nwg::dispatch_thread_events();
        drop(owner_disabled);
        nwg::unbind_event_handler(&handler);
        // The diff is note text: empty the control before it is destroyed.
        txt_diff.set_text("");
    }
}
