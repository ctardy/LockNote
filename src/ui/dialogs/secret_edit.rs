// SecretEditDialog — Create/edit a secret entry
//
// Fields: Name (TextBox), Category (ComboBox), Value (TextBox, masked)
// Show/Hide toggle for value visibility
// Generate...: opens the password generator, whose "Use" fills the value
// Validation: name cannot be empty
// Returns Option<SecretEntry>: the entry, or None if cancelled

use native_windows_gui as nwg;
use nwg::ControlHandle;
use std::cell::RefCell;
use std::rc::Rc;

// ---------------------------------------------------------------------------
// SecretEntry model
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct SecretEntry {
    pub name: String,
    pub category: String,
    pub value: String,
}

/// Index of `category` in `categories` (the pre-selected combo entry);
/// General when absent, so an edited secret never gets an empty category.
fn category_index(categories: &[String], category: Option<&str>) -> usize {
    let find = |c: &str| categories.iter().position(|x| x == c);
    category
        .and_then(find)
        .or_else(|| find(crate::features::vault::categories::GENERAL))
        .unwrap_or(0)
}

/// Stored value (`\n`) → multi-line edit control text (`\r\n`).
fn to_edit_newlines(value: &str) -> String {
    from_edit_newlines(value).replace('\n', "\r\n")
}

/// Multi-line edit control text (`\r\n`) → stored value (`\n`).
fn from_edit_newlines(text: &str) -> String {
    text.replace("\r\n", "\n")
}

/// Stored value units (`\n`) → multi-line edit control units (`\r\n`),
/// without decoding them: a lone surrogate stays where it is.
fn to_edit_newline_units(units: &[u16]) -> zeroize::Zeroizing<Vec<u16>> {
    const CR: u16 = 0x0D;
    const LF: u16 = 0x0A;
    let lfs = units.iter().filter(|&&u| u == LF).count();
    let mut out = zeroize::Zeroizing::new(Vec::with_capacity(units.len() + lfs));
    for (i, &u) in units.iter().enumerate() {
        if u == LF && (i == 0 || units[i - 1] != CR) {
            out.push(CR);
        }
        out.push(u);
    }
    out
}

/// The value from the view that is active, read strictly. Hidden: the
/// field is kept as it is on a refusal (it may hold a stored secret the
/// user cannot type again; Show reveals the bad character selected);
/// shown: the bad character is selected. `None`: refused, the user was
/// told in `lbl_error`, nothing is to be copied, switched or saved.
fn read_value(
    visible: bool,
    txt_value: &nwg::TextInput,
    txt_multi: &nwg::TextBox,
    lbl_error: &nwg::Label,
) -> Option<zeroize::Zeroizing<String>> {
    use crate::ui::field_text::{read_kept, read_visible};
    if visible {
        let text = read_visible(txt_multi.handle, "vault.value", |m| lbl_error.set_text(m))?;
        Some(zeroize::Zeroizing::new(from_edit_newlines(&text)))
    } else {
        read_kept(txt_value.handle, "vault.value", |m| lbl_error.set_text(m))
    }
}

/// Show/Hide: carry the value over to the other view and switch to it.
/// Hide reads the shown value strictly: a refusal selects the bad
/// character and leaves the view, both fields and the button unchanged.
/// Show moves the hidden value's units as they are: when they hold an
/// invalid character, it is selected in the shown view so the user can
/// delete it, and nothing of the value is lost.
fn toggle_view(
    visible: &mut bool,
    txt_value: &nwg::TextInput,
    txt_multi: &nwg::TextBox,
    btn_toggle: &nwg::Button,
    lbl_error: &nwg::Label,
) {
    use crate::ui::field_text::{decode_exact, read_raw, select_invalid, write_units, FieldTextError};
    if *visible {
        let Some(value) = read_value(true, txt_value, txt_multi, lbl_error) else {
            return;
        };
        txt_value.set_text(&value);
        txt_multi.set_text("");
    } else {
        let tell = |e: &FieldTextError| {
            eprintln!("WARN vault: value not shown reason={}", e.kind());
            lbl_error.set_text(&e.hidden_message());
        };
        let raw = match read_raw(txt_value.handle) {
            Ok(raw) => raw,
            Err(e) => return tell(&e),
        };
        let invalid = match decode_exact(&raw) {
            Ok(_) => false,
            Err(FieldTextError::InvalidUtf16 { .. }) => true,
            Err(e) => return tell(&e),
        };
        if let Err(e) = write_units(txt_multi.handle, &to_edit_newline_units(&raw)) {
            return tell(&e);
        }
        txt_value.set_text("");
        if !invalid {
            lbl_error.set_text("");
        }
    }
    *visible = !*visible;
    txt_value.set_visible(!*visible);
    txt_multi.set_visible(*visible);
    btn_toggle.set_text(&t(if *visible { "vault.hide" } else { "vault.show" }));
    if *visible {
        // After the switch, so the selection is in the field now shown.
        select_invalid(txt_multi.handle, |m| lbl_error.set_text(m));
    }
}

// ---------------------------------------------------------------------------
// SecretEditDialog
// ---------------------------------------------------------------------------

fn t(key: &str) -> String {
    crate::features::i18n::get(key).to_string()
}

pub struct SecretEditDialog;

impl SecretEditDialog {
    /// Show the dialog. `existing` pre-fills fields if Some; `categories`
    /// is the user's category list (issue 052).
    /// Returns Some(entry) on OK, None on Cancel. `copied` receives the last
    /// value copied with the dialog's Copy button (OK or Cancel), so the
    /// caller can arm the clipboard guard. `passgen` holds the password
    /// generator options: "Generate..." starts from them and leaves there the
    /// ones last used (OK or Cancel), for the caller to save.
    pub fn show(
        existing: Option<&SecretEntry>,
        categories: &[String],
        copied: &mut Option<String>,
        passgen: &mut crate::features::passgen::Options,
    ) -> Option<SecretEntry> {
        let mut window = Default::default();
        nwg::Window::builder()
            .size((420, 380))
            .position((280, 200))
            .title(&if existing.is_some() {
                t("vault.edit")
            } else {
                t("vault.new")
            })
            .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
            .build(&mut window)
            .expect("Failed to build SecretEditDialog window");

        super::apply_dialog_theme(&window);

        let mut font = Default::default();
        nwg::Font::builder()
            .family("Segoe UI")
            .size(16)
            .build(&mut font)
            .expect("Failed to build font");

        // --- Name ---
        let mut lbl_name = Default::default();
        nwg::Label::builder()
            .text(&t("vault.name"))
            .parent(&window)
            .position((20, 15))
            .size((380, 20))
            .font(Some(&font))
            .build(&mut lbl_name)
            .expect("Failed to build name label");

        let mut txt_name = Default::default();
        nwg::TextInput::builder()
            .text(existing.map(|e| e.name.as_str()).unwrap_or(""))
            .parent(&window)
            .position((20, 38))
            .size((380, 25))
            .font(Some(&font))
            .focus(true)
            .build(&mut txt_name)
            .expect("Failed to build name input");

        // --- Category ---
        let mut lbl_category = Default::default();
        nwg::Label::builder()
            .text(&t("vault.category"))
            .parent(&window)
            .position((20, 75))
            .size((380, 20))
            .font(Some(&font))
            .build(&mut lbl_category)
            .expect("Failed to build category label");

        let cat_index = category_index(categories, existing.map(|e| e.category.as_str()));

        let mut cmb_category = Default::default();
        nwg::ComboBox::builder()
            .parent(&window)
            .position((20, 98))
            .size((380, 25))
            .font(Some(&font))
            .collection(categories.to_vec())
            .selected_index(Some(cat_index))
            .build(&mut cmb_category)
            .expect("Failed to build category combo");

        // --- Value ---
        let mut lbl_value = Default::default();
        nwg::Label::builder()
            .text(&t("vault.value"))
            .parent(&window)
            .position((20, 135))
            .size((380, 20))
            .font(Some(&font))
            .build(&mut lbl_value)
            .expect("Failed to build value label");

        let mut txt_value = Default::default();
        nwg::TextInput::builder()
            .text(existing.map(|e| e.value.as_str()).unwrap_or(""))
            .parent(&window)
            .position((20, 158))
            .size((290, 25))
            .password(Some('*'))
            .font(Some(&font))
            .build(&mut txt_value)
            .expect("Failed to build value input");

        // Show/Hide toggle button
        let mut btn_toggle = Default::default();
        nwg::Button::builder()
            .text(&t("vault.show"))
            .parent(&window)
            .position((320, 158))
            .size((80, 25))
            .font(Some(&font))
            .build(&mut btn_toggle)
            .expect("Failed to build toggle button");

        // Multi-line view of the value, shown instead of the masked field
        // while "Show" is active: a secret can span several lines.
        let mut txt_multi = Default::default();
        nwg::TextBox::builder()
            .parent(&window)
            .position((20, 158))
            .size((290, 100))
            .font(Some(&font))
            .flags(
                nwg::TextBoxFlags::VSCROLL
                    | nwg::TextBoxFlags::AUTOVSCROLL
                    | nwg::TextBoxFlags::TAB_STOP,
            )
            .build(&mut txt_multi)
            .expect("Failed to build value text box");

        // Copy the value (whichever view is active) to the clipboard
        let mut btn_copy = Default::default();
        nwg::Button::builder()
            .text(&t("vault.btn.copy"))
            .parent(&window)
            .position((320, 190))
            .size((80, 25))
            .font(Some(&font))
            .build(&mut btn_copy)
            .expect("Failed to build copy button");

        // Open the password generator; "Use" there fills the value
        let mut btn_generate = Default::default();
        nwg::Button::builder()
            .text(&t("vault.generate"))
            .parent(&window)
            .position((320, 222))
            .size((80, 25))
            .font(Some(&font))
            .build(&mut btn_generate)
            .expect("Failed to build generate button");

        // --- Error label ---
        let mut lbl_error = Default::default();
        nwg::Label::builder()
            .text("")
            .parent(&window)
            .position((20, 268))
            .size((380, 20))
            .font(Some(&font))
            .build(&mut lbl_error)
            .expect("Failed to build error label");
        super::mark_error(window.handle, lbl_error.handle);

        // --- OK button ---
        let mut btn_ok = Default::default();
        nwg::Button::builder()
            .text(&t("settings.ok"))
            .parent(&window)
            .position((210, 310))
            .size((85, 30))
            .font(Some(&font))
            .build(&mut btn_ok)
            .expect("Failed to build OK button");

        // --- Cancel button ---
        let mut btn_cancel = Default::default();
        nwg::Button::builder()
            .text(&t("settings.cancel"))
            .parent(&window)
            .position((310, 310))
            .size((85, 30))
            .font(Some(&font))
            .build(&mut btn_cancel)
            .expect("Failed to build Cancel button");

        // --- Enter key → OK button ---
        super::bind_enter_to_button(window.handle, btn_ok.handle);

        let result: Rc<RefCell<Option<SecretEntry>>> = Rc::new(RefCell::new(None));
        let is_visible: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));
        let window_handle = window.handle;
        let result_clone = result.clone();
        let categories_owned = categories.to_vec();
        let is_visible_clone = is_visible.clone();
        let copied_value: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        let copied_clone = copied_value.clone();
        let passgen_options = Rc::new(RefCell::new(passgen.clone()));
        let passgen_clone = passgen_options.clone();

        let handler = nwg::full_bind_event_handler(&window_handle, move |evt, _evt_data, handle| {
            match evt {
                nwg::Event::OnButtonClick => {
                    if handle == btn_toggle.handle {
                        // A refusal never erases the value (the lossy value
                        // used to replace the secret, then the emptied one).
                        let mut vis = is_visible_clone.borrow_mut();
                        toggle_view(&mut *vis, &txt_value, &txt_multi, &btn_toggle, &lbl_error);
                    } else if handle == btn_copy.handle {
                        let showing = *is_visible_clone.borrow();
                        let Some(value) = read_value(showing, &txt_value, &txt_multi, &lbl_error) else {
                            return;
                        };
                        match clipboard_win::set_clipboard(clipboard_win::formats::Unicode, value.as_str()) {
                            Ok(()) => {
                                // Keep it out of Win+V history / cloud clipboard
                                crate::features::clipboard_guard::exclude_current_clipboard_entry_from_history();
                                *copied_clone.borrow_mut() = Some(value.to_string());
                                lbl_error.set_text("");
                                btn_copy.set_text(&t("vault.btn.copied"));
                            }
                            Err(e) => {
                                eprintln!("vault: dialog copy failed err={:?}", e);
                                lbl_error.set_text(&t("vault.copy.failed"));
                            }
                        }
                    } else if handle == btn_generate.handle {
                        // This dialog is disabled while the generator is open.
                        let start = passgen_clone.borrow().clone();
                        let outcome = super::password_generator::PasswordGeneratorDialog::show(
                            &start,
                            true,
                            Some(window_handle),
                        );
                        *passgen_clone.borrow_mut() = outcome.options;
                        if outcome.copied.is_some() {
                            *copied_clone.borrow_mut() = outcome.copied;
                        }
                        if let Some(password) = outcome.used {
                            // Into whichever view of the value is active.
                            if *is_visible_clone.borrow() {
                                txt_multi.set_text(&password);
                            } else {
                                txt_value.set_text(&password);
                            }
                        }
                    } else if handle == btn_ok.handle {
                        // The name is the lookup key, the value the secret:
                        // both read strictly, nothing saved on a refusal.
                        let Some(name) =
                            crate::ui::field_text::read_visible(txt_name.handle, "vault.name", |m| lbl_error.set_text(m))
                        else {
                            return;
                        };
                        let trimmed_name = name.trim();
                        if trimmed_name.is_empty() {
                            lbl_error.set_text(&t("vault.name.empty"));
                            return;
                        }

                        let Some(category) = cmb_category.selection().and_then(|i| categories_owned.get(i)).cloned() else {
                            lbl_error.set_text(&t("vault.category.none"));
                            return;
                        };
                        let showing = *is_visible_clone.borrow();
                        let Some(value) = read_value(showing, &txt_value, &txt_multi, &lbl_error) else {
                            return;
                        };

                        *result_clone.borrow_mut() = Some(SecretEntry {
                            name: trimmed_name.to_string(),
                            category,
                            value: value.to_string(),
                        });

                        // Security: clear value fields
                        txt_value.set_text("");
                        txt_multi.set_text("");
                        nwg::stop_thread_dispatch();
                    } else if handle == btn_cancel.handle {
                        txt_value.set_text("");
                        txt_multi.set_text("");
                        nwg::stop_thread_dispatch();
                    }
                }
                nwg::Event::OnWindowClose => {
                    txt_value.set_text("");
                    txt_multi.set_text("");
                    nwg::stop_thread_dispatch();
                }
                _ => {}
            }
        });

        nwg::dispatch_thread_events();
        nwg::unbind_event_handler(&handler);

        *copied = copied_value.borrow_mut().take();
        *passgen = passgen_options.borrow().clone();
        let val = result.borrow_mut().take();
        val
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preselects_existing_category_or_general() {
        let cats: Vec<String> = ["Perso", "General", "Work"].iter().map(|s| s.to_string()).collect();
        assert_eq!(category_index(&cats, Some("Work")), 2);
        assert_eq!(category_index(&cats, Some("Gone")), 1);
        assert_eq!(category_index(&cats, None), 1);
    }

    #[test]
    fn multiline_value_roundtrips_through_the_edit_control() {
        let v = "user\npass\n\nlast";
        assert_eq!(to_edit_newlines(v), "user\r\npass\r\n\r\nlast");
        assert_eq!(from_edit_newlines(&to_edit_newlines(v)), v);
        // Already CRLF stays CRLF (no \r\r\n), a lone value is untouched.
        assert_eq!(to_edit_newlines("a\r\nb"), "a\r\nb");
        assert_eq!(from_edit_newlines("single"), "single");
    }

    #[test]
    fn secret_entry_equality() {
        let a = SecretEntry {
            name: "Test".into(),
            category: "General".into(),
            value: "secret123".into(),
        };
        let b = a.clone();
        assert_eq!(a, b);
    }

    // ── live EDIT controls (no RichEdit: no live_rich_edit guard needed) ──

    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_SETTEXT};

    struct Fields {
        _window: nwg::Window,
        value: nwg::TextInput,
        multi: nwg::TextBox,
        error: nwg::Label,
    }

    fn fields(existing: &str) -> Fields {
        let _ = nwg::init();
        let mut window = nwg::Window::default();
        nwg::Window::builder()
            .size((300, 200))
            .position((-32000, -32000))
            .flags(nwg::WindowFlags::WINDOW)
            .build(&mut window)
            .expect("window");
        let mut value = Default::default();
        nwg::TextInput::builder().text(existing).parent(&window).password(Some('*')).build(&mut value).expect("value");
        let mut multi = Default::default();
        nwg::TextBox::builder().parent(&window).build(&mut multi).expect("multi");
        let mut error = Default::default();
        nwg::Label::builder().text("").parent(&window).build(&mut error).expect("label");
        Fields { _window: window, value, multi, error }
    }

    fn hwnd(handle: ControlHandle) -> windows::Win32::Foundation::HWND {
        crate::ui::editor_text::hwnd_of(handle)
    }

    fn units_of(handle: ControlHandle) -> Vec<u16> {
        crate::ui::editor_text::read_units(hwnd(handle)).expect("readable").to_vec()
    }

    fn set_units(handle: ControlHandle, u: &[u16]) {
        let mut z = u.to_vec();
        z.push(0);
        unsafe {
            SendMessageW(hwnd(handle), WM_SETTEXT, None, Some(LPARAM(z.as_ptr() as isize)));
        }
    }

    /// The stored secret "Xk9#" with half an emoji pasted after it.
    fn stored_plus_half_emoji() -> Vec<u16> {
        let mut u: Vec<u16> = "Xk9#".encode_utf16().collect();
        u.push(0xD83D);
        u
    }

    #[test]
    fn a_refused_hidden_value_keeps_the_stored_secret_and_never_reads_as_empty() {
        // The secret may exist nowhere else: a refusal must not erase it,
        // and OK pressed again must not read an emptied field as the value.
        let f = fields("Xk9#");
        let bad = stored_plus_half_emoji();
        set_units(f.value.handle, &bad);
        for attempt in 1..=2 {
            assert!(read_value(false, &f.value, &f.multi, &f.error).is_none(), "attempt {attempt}: refused");
            assert_eq!(units_of(f.value.handle), bad, "attempt {attempt}: the stored secret was erased");
            assert!(!f.error.text().is_empty(), "attempt {attempt}: the user was told");
        }
    }

    fn button(f: &Fields) -> nwg::Button {
        let mut b = Default::default();
        nwg::Button::builder().text("Show").parent(&f._window).build(&mut b).expect("button");
        b
    }

    fn delete_selection(handle: ControlHandle) {
        const EM_REPLACESEL: u32 = 0x00C2;
        let empty = [0u16];
        unsafe {
            SendMessageW(hwnd(handle), EM_REPLACESEL, Some(WPARAM(1)), Some(LPARAM(empty.as_ptr() as isize)));
        }
    }

    #[test]
    fn show_reveals_a_refused_value_with_the_bad_character_selected_then_ok_saves_the_secret() {
        let _lang = crate::features::i18n::test_support::lang_guard();
        crate::features::i18n::set_language(crate::features::i18n::Language::English);
        let f = fields("Xk9#");
        let toggle = button(&f);
        let bad = stored_plus_half_emoji();
        set_units(f.value.handle, &bad);
        let mut visible = false;
        // OK refused, the field kept (as above); Show then reveals it.
        assert!(read_value(visible, &f.value, &f.multi, &f.error).is_none());
        toggle_view(&mut visible, &f.value, &f.multi, &toggle, &f.error);
        assert!(visible, "Show switched the view");
        assert_eq!(units_of(f.multi.handle), bad, "the value moved as it is, nothing lost");
        assert_eq!(units_of(f.value.handle), Vec::<u16>::new(), "no second copy left in the hidden field");
        assert_eq!(f.multi.selection(), 4..5, "the bad character is selected");
        assert_eq!(f.error.text(), crate::ui::field_text::FieldTextError::InvalidUtf16 { unit: 0, count: 0 }.visible_message());
        // OK on the shown view is still refused, and keeps the text.
        assert!(read_value(visible, &f.value, &f.multi, &f.error).is_none());
        assert_eq!(units_of(f.multi.handle), bad);
        // Hide is refused too: view, fields and button unchanged.
        toggle_view(&mut visible, &f.value, &f.multi, &toggle, &f.error);
        assert!(visible);
        assert_eq!(toggle.text(), "Hide");
        assert_eq!(units_of(f.multi.handle), bad);
        // Delete the selected character: OK reads the stored secret back.
        delete_selection(f.multi.handle);
        assert_eq!(read_value(visible, &f.value, &f.multi, &f.error).expect("valid").as_str(), "Xk9#");
        toggle_view(&mut visible, &f.value, &f.multi, &toggle, &f.error);
        assert!(!visible);
        assert_eq!(read_value(visible, &f.value, &f.multi, &f.error).expect("valid").as_str(), "Xk9#");
    }

    #[test]
    fn the_show_hide_button_keeps_the_user_language_after_each_toggle() {
        use crate::features::i18n::{set_language, Language};
        let _lang = crate::features::i18n::test_support::lang_guard();
        for lang in [Language::English, Language::French, Language::Spanish, Language::German] {
            set_language(lang);
            let f = fields("Xk9#");
            let toggle = button(&f);
            let mut visible = false;
            toggle_view(&mut visible, &f.value, &f.multi, &toggle, &f.error);
            assert!(visible);
            assert_eq!(toggle.text(), t("vault.hide"), "{lang:?}: Hide");
            assert!(!toggle.text().starts_with('['), "{lang:?}: vault.hide is missing");
            toggle_view(&mut visible, &f.value, &f.multi, &toggle, &f.error);
            assert!(!visible);
            assert_eq!(toggle.text(), t("vault.show"), "{lang:?}: Show");
        }
        set_language(Language::English);
    }

    #[test]
    fn show_keeps_line_breaks_of_a_refused_value_and_selects_its_edit_position() {
        let f = fields("");
        let toggle = button(&f);
        let mut u: Vec<u16> = "a\nb".encode_utf16().collect();
        u.push(0xDC00);
        set_units(f.value.handle, &u);
        assert_eq!(units_of(f.value.handle), u, "a single-line EDIT keeps a line feed");
        let mut visible = false;
        toggle_view(&mut visible, &f.value, &f.multi, &toggle, &f.error);
        let mut expected: Vec<u16> = "a\r\nb".encode_utf16().collect();
        expected.push(0xDC00);
        assert_eq!(units_of(f.multi.handle), expected);
        assert_eq!(f.multi.selection(), 4..5);
    }

    #[test]
    fn line_feeds_become_crlf_without_decoding_the_units() {
        let u = |s: &str| s.encode_utf16().collect::<Vec<u16>>();
        assert_eq!(to_edit_newline_units(&u("a\nb")).as_slice(), u("a\r\nb").as_slice());
        assert_eq!(to_edit_newline_units(&u("a\r\nb\n")).as_slice(), u("a\r\nb\r\n").as_slice());
        assert_eq!(to_edit_newline_units(&[0x0A, 0xD800]).as_slice(), &[0x0D, 0x0A, 0xD800]);
        assert_eq!(to_edit_newline_units(&u("\u{1F600}")).as_slice(), u("\u{1F600}").as_slice());
    }
}
