// TotpManagerDialog — TOTP Authenticator viewer/manager
//
// Shows a list of stored TOTP entries with live codes and countdown.
// Supports: view codes, copy to clipboard, add new entry, remove entry.
// Returns updated Vec<TotpEntry> (or None if cancelled).

use native_windows_gui as nwg;
use std::cell::RefCell;
use std::rc::Rc;

use crate::features::totp::{self, TotpEntry};

fn t(key: &str) -> String {
    crate::features::i18n::get(key).to_string()
}

/// Result from the TOTP manager dialog.
pub enum TotpManagerResult {
    /// User closed with changes to the entry list.
    Changed(Vec<TotpEntry>),
    /// User closed without changes.
    Unchanged,
}

pub struct TotpManagerDialog;

impl TotpManagerDialog {
    /// Show the TOTP manager dialog.
    ///
    /// `entries` is the current list of TOTP entries.
    /// Returns `TotpManagerResult` indicating if entries were modified.
    pub fn show(entries: &[TotpEntry]) -> TotpManagerResult {
        // Clone entries for mutation
        let entries_state: Rc<RefCell<Vec<TotpEntry>>> =
            Rc::new(RefCell::new(entries.to_vec()));
        let modified: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));

        // --- Center on screen ---
        #[link(name = "user32")]
        extern "system" {
            fn GetSystemMetrics(index: i32) -> i32;
        }
        let win_w: i32 = 500;
        let win_h: i32 = 440;
        let screen_w = unsafe { GetSystemMetrics(0) };
        let screen_h = unsafe { GetSystemMetrics(1) };
        let pos_x = (screen_w - win_w) / 2;
        let pos_y = (screen_h - win_h) / 2;

        let mut window = Default::default();
        nwg::Window::builder()
            .size((win_w, win_h))
            .position((pos_x, pos_y))
            .title(&t("totp.manager.title"))
            .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
            .build(&mut window)
            .expect("Failed to build TotpManagerDialog window");

        super::apply_dialog_theme(&window);

        // --- Fonts ---
        let mut font = Default::default();
        nwg::Font::builder()
            .family("Segoe UI")
            .size(16)
            .build(&mut font)
            .expect("Failed to build font");

        let mut font_title = Default::default();
        nwg::Font::builder()
            .family("Segoe UI")
            .size(22)
            .weight(700)
            .build(&mut font_title)
            .expect("Failed to build title font");

        let mut font_desc = Default::default();
        nwg::Font::builder()
            .family("Segoe UI")
            .size(15)
            .build(&mut font_desc)
            .expect("Failed to build description font");

        let content_w: i32 = 450;
        let margin: i32 = 25;

        // --- Title ---
        let mut _lbl_title = Default::default();
        nwg::Label::builder()
            .text(&t("totp.manager.title"))
            .parent(&window)
            .position((margin, 15))
            .size((content_w, 28))
            .font(Some(&font_title))
            .build(&mut _lbl_title)
            .expect("Failed to build title label");

        // --- Description ---
        let mut _lbl_desc = Default::default();
        nwg::Label::builder()
            .text(&t("totp.manager.desc"))
            .parent(&window)
            .position((margin, 48))
            .size((content_w, 20))
            .font(Some(&font_desc))
            .build(&mut _lbl_desc)
            .expect("Failed to build description label");

        // --- Separator ---
        let mut _separator = Default::default();
        nwg::Frame::builder()
            .parent(&window)
            .position((margin, 76))
            .size((content_w, 1))
            .build(&mut _separator)
            .expect("Failed to build separator");

        // --- Countdown label ---
        let mut lbl_countdown = Default::default();
        nwg::Label::builder()
            .text("")
            .parent(&window)
            .position((margin, 85))
            .size((content_w, 20))
            .font(Some(&font_desc))
            .build(&mut lbl_countdown)
            .expect("Failed to build countdown label");

        // --- List box for entries ---
        let mut list_entries = Default::default();
        nwg::ListBox::builder()
            .parent(&window)
            .position((margin, 110))
            .size((content_w, 220))
            .font(Some(&font))
            .build(&mut list_entries)
            .expect("Failed to build entries list");

        // Populate the list
        fn refresh_list(list: &nwg::ListBox<String>, entries: &[TotpEntry]) {
            list.clear();
            for entry in entries {
                let code = totp::generate_code_now(&entry.secret)
                    .unwrap_or_else(|| "------".to_string());
                list.push(format!("{}  —  {}", entry.name, code));
            }
        }

        refresh_list(&list_entries, &entries_state.borrow());

        // --- Buttons row ---
        let btn_y: i32 = 340;

        let mut btn_copy = Default::default();
        nwg::Button::builder()
            .text(&t("totp.manager.copy"))
            .parent(&window)
            .position((margin, btn_y))
            .size((100, 32))
            .font(Some(&font))
            .build(&mut btn_copy)
            .expect("Failed to build Copy button");

        let mut btn_add = Default::default();
        nwg::Button::builder()
            .text(&t("totp.add"))
            .parent(&window)
            .position((margin + 110, btn_y))
            .size((80, 32))
            .font(Some(&font))
            .build(&mut btn_add)
            .expect("Failed to build Add button");

        let mut btn_remove = Default::default();
        nwg::Button::builder()
            .text(&t("totp.remove"))
            .parent(&window)
            .position((margin + 200, btn_y))
            .size((80, 32))
            .font(Some(&font))
            .build(&mut btn_remove)
            .expect("Failed to build Remove button");

        let mut btn_close = Default::default();
        nwg::Button::builder()
            .text(&t("totp.manager.close"))
            .parent(&window)
            .position((margin + content_w - 90, btn_y))
            .size((90, 32))
            .font(Some(&font))
            .build(&mut btn_close)
            .expect("Failed to build Close button");

        // --- Status label ---
        let mut lbl_status = Default::default();
        nwg::Label::builder()
            .text("")
            .parent(&window)
            .position((margin, btn_y + 40))
            .size((content_w, 20))
            .font(Some(&font_desc))
            .build(&mut lbl_status)
            .expect("Failed to build status label");

        // --- Timer for refreshing codes ---
        let mut timer = Default::default();
        nwg::AnimationTimer::builder()
            .parent(&window)
            .interval(std::time::Duration::from_secs(1))
            .active(false)
            .build(&mut timer)
            .expect("Failed to build timer");
        timer.start();

        let window_handle = window.handle;
        let entries_for_handler = entries_state.clone();
        let modified_for_handler = modified.clone();
        let entries_for_result = entries_state.clone();
        let modified_for_result = modified.clone();

        let handler = nwg::full_bind_event_handler(&window_handle, move |evt, _evt_data, handle| {
            match evt {
                nwg::Event::OnTimerTick => {
                    if handle == timer.handle {
                        // Refresh codes and countdown
                        let remaining = totp::remaining_seconds();
                        lbl_countdown.set_text(&t("totp.nextcode").replace("{n}", &remaining.to_string()));

                        // Refresh the list items with new codes
                        let sel = list_entries.selection();
                        refresh_list(&list_entries, &entries_for_handler.borrow());
                        if let Some(idx) = sel {
                            list_entries.set_selection(Some(idx));
                        }
                    }
                }
                nwg::Event::OnButtonClick => {
                    if handle == btn_copy.handle {
                        if let Some(idx) = list_entries.selection() {
                            let entries = entries_for_handler.borrow();
                            if let Some(entry) = entries.get(idx) {
                                if let Some(code) = totp::generate_code_now(&entry.secret) {
                                    let copied = t("totp.manager.copied").replace("{name}", &entry.name);
                                    lbl_status.set_text(&crate::ui::copy_feedback(&set_clipboard_text(&code), &copied));
                                }
                            }
                        } else {
                            lbl_status.set_text(&t("totp.manager.select"));
                        }
                    } else if handle == btn_add.handle {
                        // Ask for a name via a simple input dialog
                        if let Some(name) = simple_input_dialog(&window_handle, &t("totp.add.title"), &t("totp.add.prompt")) {
                            if !name.is_empty() {
                                if let Some(entry) = super::totp_setup::TotpSetupDialog::show_add_key(&name, &mut None) {
                                    entries_for_handler.borrow_mut().push(entry);
                                    *modified_for_handler.borrow_mut() = true;
                                    refresh_list(&list_entries, &entries_for_handler.borrow());
                                    lbl_status.set_text(&t("totp.manager.added"));
                                }
                            }
                        }
                    } else if handle == btn_remove.handle {
                        if let Some(idx) = list_entries.selection() {
                            let name = {
                                let entries = entries_for_handler.borrow();
                                entries.get(idx).map(|e| e.name.clone())
                            };
                            if let Some(name) = name {
                                let confirm = nwg::modal_message(
                                    &window_handle,
                                    &nwg::MessageParams {
                                        title: &t("totp.remove.title"),
                                        content: &t("totp.remove.msg").replace("{name}", &name),
                                        buttons: nwg::MessageButtons::YesNo,
                                        icons: nwg::MessageIcons::Warning,
                                    },
                                );
                                if confirm == nwg::MessageChoice::Yes {
                                    entries_for_handler.borrow_mut().remove(idx);
                                    *modified_for_handler.borrow_mut() = true;
                                    refresh_list(&list_entries, &entries_for_handler.borrow());
                                    lbl_status.set_text(&t("totp.manager.removed"));
                                }
                            }
                        } else {
                            lbl_status.set_text(&t("totp.manager.select"));
                        }
                    } else if handle == btn_close.handle {
                        timer.stop();
                        nwg::stop_thread_dispatch();
                    }
                }
                nwg::Event::OnWindowClose => {
                    timer.stop();
                    nwg::stop_thread_dispatch();
                }
                _ => {}
            }
        });

        nwg::dispatch_thread_events();
        nwg::unbind_event_handler(&handler);

        let was_modified = *modified_for_result.borrow();
        if was_modified {
            TotpManagerResult::Changed(entries_for_result.borrow().clone())
        } else {
            TotpManagerResult::Unchanged
        }
    }
}

/// Set text to the Windows clipboard.
fn set_clipboard_text(text: &str) -> Result<(), String> {
    use clipboard_win::{formats, set_clipboard};
    set_clipboard(formats::Unicode, text).map_err(|e| e.to_string())
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
                    *result_clone.borrow_mut() = Some(txt.text());
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

#[cfg(test)]
mod tests {
    #[test]
    fn module_compiles() {
        let _ = std::mem::size_of::<super::TotpManagerDialog>();
    }
}
