// GoToHeadingDialog — Ctrl+Shift+O go-to-heading dialog (Edit menu)
//
// Lists the headings of the active tab (`features::outline`), indented by
// level, with a filter field above the list. Enter, OK or a double-click
// returns the chosen heading; the caller moves the caret.
// Returns Option<usize>: index into `headings`, or None if cancelled.

use native_windows_gui as nwg;
use std::cell::RefCell;
use std::rc::Rc;

use crate::features::outline::{self, Heading};

fn t(key: &str) -> String {
    crate::features::i18n::get(key).to_string()
}

/// The hint under the list: no heading at all, none left by the filter, or
/// nothing to say.
fn hint_text(total: usize, shown: usize) -> String {
    if total == 0 {
        t("heading.none")
    } else if shown == 0 {
        t("heading.no_match")
    } else {
        String::new()
    }
}

pub struct GoToHeadingDialog;

impl GoToHeadingDialog {
    /// Show the headings. `owner` (the main window) is disabled while the
    /// dialog is open, so the tab and its lines cannot change under it.
    pub fn show(headings: &[Heading], owner: Option<nwg::ControlHandle>) -> Option<usize> {
        let mut window = Default::default();
        nwg::Window::builder()
            .size((460, 440))
            .position((300, 200))
            .title(&t("heading.title"))
            .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
            .build(&mut window)
            .expect("Failed to build GoToHeadingDialog window");

        super::apply_dialog_theme(&window);

        let mut font = Default::default();
        nwg::Font::builder()
            .family("Segoe UI")
            .size(16)
            .build(&mut font)
            .expect("Failed to build font");

        let mut lbl_filter = Default::default();
        nwg::Label::builder()
            .text(&t("heading.filter"))
            .parent(&window)
            .position((20, 15))
            .size((420, 20))
            .font(Some(&font))
            .build(&mut lbl_filter)
            .expect("Failed to build filter label");

        let mut txt_filter = Default::default();
        nwg::TextInput::builder()
            .parent(&window)
            .position((20, 40))
            .size((420, 25))
            .font(Some(&font))
            .focus(true)
            .build(&mut txt_filter)
            .expect("Failed to build filter input");

        let mut list_box: nwg::ListBox<String> = Default::default();
        nwg::ListBox::builder()
            .parent(&window)
            .position((20, 75))
            .size((420, 270))
            .font(Some(&font))
            .build(&mut list_box)
            .expect("Failed to build heading list");

        let mut lbl_hint = Default::default();
        nwg::Label::builder()
            .text("")
            .parent(&window)
            .position((20, 350))
            .size((420, 20))
            .font(Some(&font))
            .build(&mut lbl_hint)
            .expect("Failed to build hint label");

        let mut btn_ok = Default::default();
        nwg::Button::builder()
            .text(&t("goto.ok"))
            .parent(&window)
            .position((260, 380))
            .size((80, 30))
            .font(Some(&font))
            .build(&mut btn_ok)
            .expect("Failed to build OK button");

        let mut btn_cancel = Default::default();
        nwg::Button::builder()
            .text(&t("goto.cancel"))
            .parent(&window)
            .position((355, 380))
            .size((80, 30))
            .font(Some(&font))
            .build(&mut btn_cancel)
            .expect("Failed to build Cancel button");

        // Enter in the filter or the list → OK (the selected heading).
        super::bind_enter_to_button(window.handle, btn_ok.handle);

        let all: Vec<Heading> = headings.to_vec();
        // Row of the list → index into `all`.
        let shown: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));
        // Fill the list for a filter; the first row is selected so that
        // Enter right after typing goes to the first match.
        let refill = {
            let shown = shown.clone();
            let all = all.clone();
            move |list_box: &nwg::ListBox<String>, lbl_hint: &nwg::Label, query: &str| {
                let rows = outline::filter(&all, query);
                list_box.set_collection(rows.iter().map(|&i| outline::list_label(&all[i])).collect());
                list_box.set_selection(if rows.is_empty() { None } else { Some(0) });
                lbl_hint.set_text(&hint_text(all.len(), rows.len()));
                *shown.borrow_mut() = rows;
            }
        };
        refill(&list_box, &lbl_hint, "");

        let result: Rc<RefCell<Option<usize>>> = Rc::new(RefCell::new(None));
        let window_handle = window.handle;
        let result_h = result.clone();
        let shown_h = shown.clone();

        let handler = nwg::full_bind_event_handler(&window_handle, move |evt, _evt_data, handle| {
            let choose = |list_box: &nwg::ListBox<String>| {
                let picked = list_box.selection().and_then(|row| shown_h.borrow().get(row).copied());
                if let Some(index) = picked {
                    *result_h.borrow_mut() = Some(index);
                    nwg::stop_thread_dispatch();
                }
            };
            match evt {
                nwg::Event::OnTextInput if handle == txt_filter.handle => {
                    refill(&list_box, &lbl_hint, &txt_filter.text());
                }
                nwg::Event::OnListBoxDoubleClick if handle == list_box.handle => {
                    choose(&list_box);
                }
                nwg::Event::OnButtonClick => {
                    if handle == btn_ok.handle {
                        choose(&list_box);
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

        let owner_disabled = super::disable_owner(owner, true);
        nwg::dispatch_thread_events();
        drop(owner_disabled);
        nwg::unbind_event_handler(&handler);

        let out = result.borrow_mut().take();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hint_says_why_the_list_is_empty() {
        let _g = crate::features::i18n::test_support::lang_guard();
        assert_eq!(hint_text(0, 0), t("heading.none"));
        assert_eq!(hint_text(3, 0), t("heading.no_match"));
        assert_eq!(hint_text(3, 1), "");
        assert_ne!(t("heading.none"), t("heading.no_match"));
    }
}
