// SearchAllDialog — search in every tab, the vault names and the prompts
// (Edit > Search All Tabs..., Ctrl+Shift+F).
//
// Layout:
//   [Search: ] [query                         ] [☐ Match case]
//   [result list ................................................]
//   [status: "12 matches" / "No matches"]          [Go to] [Close]
//
// The dialog searches the copy it is given (`search_all::Sources`) and
// returns the result the user activated; the caller applies it (switch tab,
// open a panel) after its own checks (lock epoch, input not blocked). The
// list refreshes on Enter in the query, on Match case, and a short delay
// after the last key typed. Enter in the list, a double click or Go to
// activate the selected result. Logs: counts only, never the query or a
// matched text.

use native_windows_gui as nwg;
use std::cell::RefCell;
use std::rc::Rc;

use crate::ui::search_all::{self, Hit, Sources};

fn t(key: &str) -> String {
    crate::features::i18n::get(key).to_string()
}

/// Delay after the last key typed before the list refreshes.
const DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(300);

/// The list lines of `results`: one per hit, then the translated "more
/// results not shown" line when the cap was reached (it activates nothing:
/// rows past the hits are ignored).
pub(crate) fn list_lines(results: &search_all::Results, vault: &str, prompt: &str, more: &str) -> Vec<String> {
    let mut lines: Vec<String> = results.hits.iter().map(|h| search_all::hit_label(h, vault, prompt)).collect();
    if results.more {
        lines.push(more.replace("{max}", &search_all::MAX_RESULTS.to_string()));
    }
    lines
}

/// What Enter or Go to does: activate the selected row when the query field
/// does not have the focus, else search again.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum EnterAction {
    Refresh,
    Activate(usize),
}

pub(crate) fn enter_action(query_focused: bool, selected_row: Option<usize>, hit_count: usize) -> EnterAction {
    match selected_row {
        Some(row) if !query_focused && row < hit_count => EnterAction::Activate(row),
        _ => EnterAction::Refresh,
    }
}

pub struct SearchAllDialog;

impl SearchAllDialog {
    /// Show the dialog on `sources`, the query field filled with `initial`.
    /// `owner` is disabled while it is open, so the copy stays the note's
    /// state. `None`: closed (or cancelled by a lock) without a choice.
    pub fn show(sources: Sources, initial: &str, owner: Option<nwg::ControlHandle>) -> Option<Hit> {
        let _owner_disabled = super::disable_owner(owner, true);

        let mut window = Default::default();
        nwg::Window::builder()
            .size((680, 460))
            .position((300, 200))
            .title(&t("searchall.title"))
            .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
            .build(&mut window)
            .expect("Failed to build SearchAllDialog window");

        super::apply_dialog_theme(&window);

        let mut font = Default::default();
        nwg::Font::builder()
            .family("Segoe UI")
            .size(16)
            .build(&mut font)
            .expect("Failed to build font");

        let mut lbl_query = Default::default();
        nwg::Label::builder()
            .text(&t("searchall.query"))
            .parent(&window)
            .position((15, 18))
            .size((90, 20))
            .font(Some(&font))
            .build(&mut lbl_query)
            .expect("lbl_query");

        let mut txt_query = Default::default();
        nwg::TextInput::builder()
            .text(initial)
            .parent(&window)
            .position((110, 15))
            .size((370, 26))
            .font(Some(&font))
            .focus(true)
            .build(&mut txt_query)
            .expect("txt_query");

        let mut chk_case = Default::default();
        nwg::CheckBox::builder()
            .text(&t("search.case"))
            .parent(&window)
            .position((495, 17))
            .size((170, 22))
            .font(Some(&font))
            .build(&mut chk_case)
            .expect("chk_case");
        super::disable_control_theme(chk_case.handle);

        let mut list: nwg::ListBox<String> = Default::default();
        nwg::ListBox::builder()
            .parent(&window)
            .position((15, 52))
            .size((650, 330))
            .font(Some(&font))
            .build(&mut list)
            .expect("result list");

        let mut lbl_status = Default::default();
        nwg::Label::builder()
            .text("")
            .parent(&window)
            .position((15, 400))
            .size((430, 22))
            .font(Some(&font))
            .build(&mut lbl_status)
            .expect("lbl_status");

        let mut btn_go = Default::default();
        nwg::Button::builder()
            .text(&t("searchall.go"))
            .parent(&window)
            .position((470, 395))
            .size((95, 30))
            .font(Some(&font))
            .build(&mut btn_go)
            .expect("btn_go");

        let mut btn_close = Default::default();
        nwg::Button::builder()
            .text(&t("search.close"))
            .parent(&window)
            .position((570, 395))
            .size((95, 30))
            .font(Some(&font))
            .build(&mut btn_close)
            .expect("btn_close");

        let mut timer = nwg::AnimationTimer::default();
        nwg::AnimationTimer::builder()
            .parent(&window)
            .interval(DEBOUNCE)
            .active(false)
            .build(&mut timer)
            .expect("debounce timer");

        // Enter = Go to: it activates or searches again (`enter_action`).
        super::bind_enter_to_button(window.handle, btn_go.handle);

        let window_handle = window.handle;
        let query_h = txt_query.handle;
        let list_h = list.handle;
        let case_h = chk_case.handle;
        let go_h = btn_go.handle;
        let close_h = btn_close.handle;
        let timer_h = timer.handle;

        let hits: Rc<RefCell<Vec<Hit>>> = Rc::new(RefCell::new(Vec::new()));
        let chosen: Rc<RefCell<Option<Hit>>> = Rc::new(RefCell::new(None));

        // Search the copy again and rebuild the list.
        let hits_r = hits.clone();
        let refresh = move |txt_query: &nwg::TextInput, chk_case: &nwg::CheckBox, list: &nwg::ListBox<String>, lbl_status: &nwg::Label| {
            // A search only: the display text (a half emoji is U+FFFD),
            // never stored.
            let query = match crate::ui::field_text::read_display(txt_query.handle) {
                Ok(query) => query,
                Err(e) => {
                    eprintln!("WARN search_all: query unreadable, results unchanged err={:?}", e);
                    lbl_status.set_text(&e.typing_message());
                    return;
                }
            };
            let match_case = chk_case.check_state() == nwg::CheckBoxState::Checked;
            let results = search_all::search(&sources, &query, match_case, search_all::MAX_RESULTS);
            let lines = list_lines(&results, &t("searchall.vault"), &t("searchall.prompt"), &t("searchall.more"));
            list.set_collection(lines);
            let status = if query.is_empty() {
                String::new()
            } else if results.hits.is_empty() {
                t("search.count.none")
            } else {
                t("search.count.total").replace("{total}", &results.hits.len().to_string())
            };
            lbl_status.set_text(&status);
            eprintln!("INFO search_all: results count={} more={}", results.hits.len(), results.more);
            *hits_r.borrow_mut() = results.hits;
        };

        if !initial.is_empty() {
            refresh(&txt_query, &chk_case, &list, &lbl_status);
        }

        let hits_h = hits.clone();
        let chosen_h = chosen.clone();
        let activate = move |row: usize| {
            let hit = hits_h.borrow().get(row).cloned();
            if let Some(hit) = hit {
                *chosen_h.borrow_mut() = Some(hit);
                nwg::stop_thread_dispatch();
            }
        };

        let hits_e = hits.clone();
        let handler = nwg::full_bind_event_handler(&window_handle, move |evt, _data, handle| {
            match evt {
                nwg::Event::OnTextInput if handle == query_h => {
                    // Restart the delay at every key.
                    timer.stop();
                    timer.start();
                }
                nwg::Event::OnTimerTick if handle == timer_h => {
                    timer.stop();
                    refresh(&txt_query, &chk_case, &list, &lbl_status);
                }
                nwg::Event::OnListBoxDoubleClick if handle == list_h => {
                    if let Some(row) = list.selection() {
                        activate(row);
                    }
                }
                nwg::Event::OnButtonClick => {
                    if handle == case_h {
                        timer.stop();
                        refresh(&txt_query, &chk_case, &list, &lbl_status);
                    } else if handle == go_h {
                        let focused = unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetFocus() };
                        let query_focused = query_h.hwnd().is_some_and(|h| focused.0 as isize == h as isize);
                        let hit_count = hits_e.borrow().len();
                        match enter_action(query_focused, list.selection(), hit_count) {
                            EnterAction::Activate(row) => activate(row),
                            EnterAction::Refresh => {
                                timer.stop();
                                refresh(&txt_query, &chk_case, &list, &lbl_status);
                            }
                        }
                    } else if handle == close_h {
                        nwg::stop_thread_dispatch();
                    }
                }
                nwg::Event::OnWindowClose => {
                    // Also the lock's harmless answer (WM_CLOSE): no choice.
                    nwg::stop_thread_dispatch();
                }
                _ => {}
            }
        });

        nwg::dispatch_thread_events();
        // Drops the handler and with it the copy of the note (wiped).
        nwg::unbind_event_handler(&handler);

        let hit = chosen.borrow_mut().take();
        hit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn more_line_follows_the_hits() {
        let hit = Hit::Prompt { index: 0, category: "c".into(), name: "P".into() };
        let r = search_all::Results { hits: vec![hit.clone()], more: true };
        let lines = list_lines(&r, "V {category} {name}", "Prompt: {name}", "More not shown ({max})");
        assert_eq!(lines, ["Prompt: P", "More not shown (1000)"]);
        let r = search_all::Results { hits: vec![hit], more: false };
        assert_eq!(list_lines(&r, "", "Prompt: {name}", "x").len(), 1);
    }

    #[test]
    fn enter_searches_in_the_query_and_activates_in_the_list() {
        assert_eq!(enter_action(true, Some(0), 3), EnterAction::Refresh);
        assert_eq!(enter_action(false, Some(2), 3), EnterAction::Activate(2));
        assert_eq!(enter_action(false, None, 3), EnterAction::Refresh);
        // The "more results not shown" row activates nothing.
        assert_eq!(enter_action(false, Some(3), 3), EnterAction::Refresh);
    }

    /// Edit > Search All Tabs has an access key no other Edit item uses, in
    /// every language.
    #[test]
    fn menu_access_key_is_free_in_the_edit_menu() {
        use crate::features::i18n::{get, set_language, Language};
        let key_of = |label: &str| -> Option<char> {
            let i = label.find('&')?;
            label[i + 1..].chars().next().and_then(|c| c.to_lowercase().next())
        };
        let edit = [
            "menu.edit.cut", "menu.edit.copy", "menu.edit.paste", "menu.edit.pasteplain", "menu.edit.find",
            "menu.edit.replace", "menu.edit.goto", "menu.edit.dupline", "menu.edit.delline",
            "menu.edit.selectall", "menu.edit.timestamp",
        ];
        let _g = crate::features::i18n::test_support::lang_guard();
        for lang in [Language::English, Language::French, Language::Spanish, Language::German] {
            set_language(lang);
            let label = get("menu.edit.searchall").to_string();
            assert!(label.ends_with("+F"), "{lang:?}: {label}");
            let mine = key_of(&label).unwrap_or_else(|| panic!("no access key {lang:?}"));
            for item in edit {
                assert_ne!(key_of(get(item)), Some(mine), "{item} of {lang:?}");
            }
        }
        set_language(Language::English);
    }

    /// The vault values are never handed to the dialog: its sources have
    /// no value field, and the dialog never reads a secret.
    #[test]
    fn no_secret_value_reaches_the_dialog() {
        let src = crate::read_source("src/ui/dialogs/search_all.rs");
        let code = &src[..src.find("#[cfg(test)]").expect("test module")];
        for needle in [".value", "get_secrets", "copy_secret_value"] {
            assert!(!code.contains(needle), "{needle}");
        }
    }
}
