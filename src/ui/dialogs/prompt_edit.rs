// PromptEditDialog — Create/edit a stored prompt (issue 058)
//
// Fields: Name (TextInput), Category (TextInput, with a list of the
// categories already used next to it: choosing one fills the field), Text
// (multi-line TextBox: Enter adds a line, Tab adds a tab, Ctrl+Tab leaves the
// field).
// Validation (`features::prompts::check_new_entry`): name, category and text must
// not be empty; the category cannot hold `:`; explicit message, no default
// filled in silently.
// Returns Option<PromptEntry>: the entry, or None if cancelled.

use native_windows_gui as nwg;
use std::cell::RefCell;
use std::rc::Rc;

use crate::features::prompts::{PromptEntry, PromptError, MAX_PROMPT_TEXT};
use crate::ui::prompts_panel::{from_crlf, to_crlf};

fn t(key: &str) -> String {
    crate::features::i18n::get(key).to_string()
}

/// A character as it can be shown in a message (line breaks and NUL are
/// invisible otherwise).
fn shown_char(c: char) -> String {
    match c {
        '\n' | '\r' => "\\n".to_string(),
        '\0' => "\\0".to_string(),
        c => format!("\"{}\"", c),
    }
}

/// Message of a refused entry, translated by `t(key)`.
fn error_text(e: &PromptError, t: &dyn Fn(&str) -> String) -> String {
    match e {
        PromptError::EmptyName => t("prompts.err.name"),
        PromptError::EmptyCategory => t("prompts.err.category"),
        PromptError::EmptyText => t("prompts.err.text"),
        PromptError::TextTooLong => t("prompts.err.toolong")
            .replace("{max}", &(MAX_PROMPT_TEXT / (1024 * 1024)).to_string()),
        PromptError::NameChar(c) => {
            t("prompts.err.name.char").replace("{char}", &shown_char(*c))
        }
        PromptError::CategoryChar(c) => {
            t("prompts.err.category.char").replace("{char}", &shown_char(*c))
        }
    }
}

/// The entry the dialog returns for what was typed: name and category
/// trimmed, text with `\n` line ends; or why it is refused. `original` is
/// the stored text of the prompt being edited: when the text was not touched
/// (same once line ends are normalized) it is kept byte for byte, so Edit
/// then OK never rewrites the line ends of a text that came with `\r\n`
/// (an import).
fn entry_from_fields(name: &str, category: &str, edit_text: &str, original: Option<&str>) -> Result<PromptEntry, PromptError> {
    let typed = from_crlf(edit_text);
    let text = match original {
        Some(o) if from_crlf(o) == typed => o.to_string(),
        _ => typed,
    };
    let (name, category) = (name.trim(), category.trim());
    crate::features::prompts::check_new_entry(name, &text, category)?;
    Ok(PromptEntry::new(name, &text, category))
}

pub struct PromptEditDialog;

impl PromptEditDialog {
    /// Show the dialog. `existing` pre-fills the fields; `categories` are the
    /// choices of the category list (the default first, see
    /// `prompts_panel::category_choices`), the first one pre-fills a new
    /// prompt. Returns Some(entry) on OK, None on Cancel.
    pub fn show(existing: Option<&PromptEntry>, categories: &[String]) -> Option<PromptEntry> {
        let mut window = Default::default();
        nwg::Window::builder()
            .size((540, 480))
            .position((260, 160))
            .title(&if existing.is_some() { t("prompts.edit") } else { t("prompts.new") })
            .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
            .build(&mut window)
            .expect("Failed to build PromptEditDialog window");

        super::apply_dialog_theme(&window);

        let mut font = Default::default();
        nwg::Font::builder().family("Segoe UI").size(16).build(&mut font).expect("Failed to build font");

        // --- Name ---
        let mut lbl_name = Default::default();
        nwg::Label::builder()
            .text(&t("prompts.name"))
            .parent(&window)
            .position((20, 15))
            .size((500, 20))
            .font(Some(&font))
            .build(&mut lbl_name)
            .expect("Failed to build name label");

        let mut txt_name = Default::default();
        nwg::TextInput::builder()
            .text(existing.map(|e| e.name.as_str()).unwrap_or(""))
            .parent(&window)
            .position((20, 38))
            .size((500, 25))
            .font(Some(&font))
            .focus(true)
            .build(&mut txt_name)
            .expect("Failed to build name input");

        // --- Category: free text, or one of the categories in use ---
        let mut lbl_category = Default::default();
        nwg::Label::builder()
            .text(&t("prompts.category"))
            .parent(&window)
            .position((20, 75))
            .size((500, 20))
            .font(Some(&font))
            .build(&mut lbl_category)
            .expect("Failed to build category label");

        let initial_category = match existing {
            Some(e) => e.category.clone(),
            None => categories.first().cloned().unwrap_or_default(),
        };
        let mut txt_category = Default::default();
        nwg::TextInput::builder()
            .text(&initial_category)
            .parent(&window)
            .position((20, 98))
            .size((300, 25))
            .font(Some(&font))
            .build(&mut txt_category)
            .expect("Failed to build category input");

        let mut cmb_category = Default::default();
        nwg::ComboBox::builder()
            .parent(&window)
            .position((330, 98))
            .size((190, 25))
            .font(Some(&font))
            .collection(categories.to_vec())
            .selected_index(categories.iter().position(|c| *c == initial_category))
            .build(&mut cmb_category)
            .expect("Failed to build category list");

        // --- Text ---
        let mut lbl_text = Default::default();
        nwg::Label::builder()
            .text(&t("prompts.text"))
            .parent(&window)
            .position((20, 135))
            .size((500, 20))
            .font(Some(&font))
            .build(&mut lbl_text)
            .expect("Failed to build text label");

        let mut txt_text = Default::default();
        nwg::TextBox::builder()
            .text(&existing.map(|e| to_crlf(&e.text)).unwrap_or_default())
            .parent(&window)
            .position((20, 158))
            .size((500, 220))
            .font(Some(&font))
            .limit(MAX_PROMPT_TEXT)
            .flags(nwg::TextBoxFlags::VISIBLE | nwg::TextBoxFlags::VSCROLL | nwg::TextBoxFlags::AUTOVSCROLL | nwg::TextBoxFlags::TAB_STOP)
            .build(&mut txt_text)
            .expect("Failed to build text box");

        // Tab inserts a tab in the text (a prompt is often indented code):
        // claim VK_TAB from IsDialogMessage. Ctrl+Tab keeps moving the focus;
        // every other key gets the default answer (Enter stays a new line,
        // ES_WANTRETURN).
        let tab_handler = nwg::bind_raw_event_handler(&txt_text.handle, 0x30010, move |_hwnd, msg, _wparam, lparam| {
            const WM_GETDLGCODE: u32 = 0x0087;
            const WM_KEYDOWN: u32 = 0x0100;
            const WM_CHAR: u32 = 0x0102;
            const VK_TAB: usize = 0x09;
            const DLGC_WANTTAB: isize = 0x0002;
            const DLGC_WANTCHARS: isize = 0x0080;
            if msg != WM_GETDLGCODE || lparam == 0 {
                return None;
            }
            let m = unsafe { &*(lparam as *const windows::Win32::UI::WindowsAndMessaging::MSG) };
            let ctrl = unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState(0x11) } < 0;
            if (m.message == WM_KEYDOWN || m.message == WM_CHAR) && m.wParam.0 == VK_TAB && !ctrl {
                return Some(DLGC_WANTTAB | DLGC_WANTCHARS);
            }
            None
        });
        if let Err(e) = &tab_handler {
            eprintln!("WARN prompts: Tab key handler not bound, Tab moves the focus err={:?}", e);
        }

        // --- Error label ---
        let mut lbl_error = Default::default();
        nwg::Label::builder()
            .text("")
            .parent(&window)
            .position((20, 385))
            .size((500, 20))
            .font(Some(&font))
            .build(&mut lbl_error)
            .expect("Failed to build error label");
        super::mark_error(window.handle, lbl_error.handle);

        let mut btn_ok = Default::default();
        nwg::Button::builder()
            .text(&t("settings.ok"))
            .parent(&window)
            .position((330, 415))
            .size((85, 30))
            .font(Some(&font))
            .build(&mut btn_ok)
            .expect("Failed to build OK button");

        let mut btn_cancel = Default::default();
        nwg::Button::builder()
            .text(&t("settings.cancel"))
            .parent(&window)
            .position((430, 415))
            .size((85, 30))
            .font(Some(&font))
            .build(&mut btn_cancel)
            .expect("Failed to build Cancel button");

        // Enter in the one-line fields = OK (the text box keeps its Enter).
        super::bind_enter_to_button(window.handle, btn_ok.handle);

        let result: Rc<RefCell<Option<PromptEntry>>> = Rc::new(RefCell::new(None));
        let original_text: Option<String> = existing.map(|e| e.text.clone());
        let result_clone = result.clone();
        let categories_owned = categories.to_vec();

        let handler = nwg::full_bind_event_handler(&window.handle, move |evt, _evt_data, handle| match evt {
            nwg::Event::OnComboxBoxSelection => {
                if handle == cmb_category.handle {
                    if let Some(c) = cmb_category.selection().and_then(|i| categories_owned.get(i)) {
                        txt_category.set_text(c);
                    }
                }
            }
            nwg::Event::OnButtonClick => {
                if handle == btn_ok.handle {
                    match entry_from_fields(&txt_name.text(), &txt_category.text(), &txt_text.text(), original_text.as_deref()) {
                        Ok(entry) => {
                            *result_clone.borrow_mut() = Some(entry);
                            txt_text.set_text("");
                            nwg::stop_thread_dispatch();
                        }
                        Err(e) => lbl_error.set_text(&error_text(&e, &t)),
                    }
                } else if handle == btn_cancel.handle {
                    txt_text.set_text("");
                    nwg::stop_thread_dispatch();
                }
            }
            nwg::Event::OnWindowClose => {
                txt_text.set_text("");
                nwg::stop_thread_dispatch();
            }
            _ => {}
        });

        nwg::dispatch_thread_events();
        // The raw handler first: the text box it is bound to lives in the
        // event handler's closure.
        if let Ok(h) = &tab_handler {
            if let Err(e) = nwg::unbind_raw_event_handler(h) {
                eprintln!("WARN prompts: Tab key handler not unbound err={:?}", e);
            }
        }
        nwg::unbind_event_handler(&handler);

        let val = result.borrow_mut().take();
        val
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `t` in English (the language other tests may switch).
    fn english(key: &str) -> String {
        let _g = crate::features::i18n::test_support::lang_guard();
        crate::features::i18n::set_language(crate::features::i18n::Language::English);
        t(key)
    }

    #[test]
    fn fields_are_trimmed_and_the_text_stored_with_lf() {
        let e = entry_from_fields("  Fix  ", " Code ", "line 1\r\n\tline 2\r\n", None).unwrap();
        assert_eq!(e, PromptEntry::new("Fix", "line 1\n\tline 2\n", "Code"));
        // Leading spaces of the text are content, not trimmed.
        assert_eq!(entry_from_fields("n", "c", "  x", None).unwrap().text, "  x");
    }

    // Edit then OK without touching the text: the stored text is kept as it
    // was, CRLF included (it may come from an import). A changed text is
    // stored with `\n`, like a new one.
    #[test]
    fn untouched_text_keeps_its_line_ends() {
        let stored = "a\r\nb\nc";
        let shown = to_crlf(stored);
        assert_eq!(entry_from_fields("n", "c", &shown, Some(stored)).unwrap().text, stored);
        assert_eq!(entry_from_fields("n", "c", &format!("{}!", shown), Some(stored)).unwrap().text, "a\nb\nc!");
    }

    #[test]
    fn every_refusal_has_an_explicit_message() {
        let cases = [
            (entry_from_fields(" ", "c", "t", None), "Name cannot be empty"),
            (entry_from_fields("n", " ", "t", None), "Category cannot be empty"),
            (entry_from_fields("n", "c", "", None), "Text cannot be empty"),
            (entry_from_fields("n", "a:b", "t", None), "The category cannot contain \":\""),
            (entry_from_fields("n", "c", &"x".repeat(MAX_PROMPT_TEXT + 1), None), "Text is too long (max 1 MB)"),
        ];
        for (res, msg) in cases {
            assert_eq!(error_text(&res.unwrap_err(), &english), msg);
        }
        assert_eq!(error_text(&PromptError::NameChar('\n'), &english), "The name cannot contain \\n");
        assert_eq!(error_text(&PromptError::CategoryChar('\0'), &english), "The category cannot contain \\0");
    }

    #[test]
    fn messages_are_translated() {
        let _g = crate::features::i18n::test_support::lang_guard();
        crate::features::i18n::set_language(crate::features::i18n::Language::French);
        let msg = error_text(&PromptError::TextTooLong, &t);
        crate::features::i18n::set_language(crate::features::i18n::Language::English);
        assert_eq!(msg, "Texte trop long (max 1 Mo)");
    }
}
