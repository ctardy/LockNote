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
                        let mut vis = is_visible_clone.borrow_mut();
                        *vis = !*vis;
                        // Carry the value over to the view that becomes active
                        if *vis {
                            txt_multi.set_text(&to_edit_newlines(&txt_value.text()));
                            txt_value.set_text("");
                        } else {
                            txt_value.set_text(&from_edit_newlines(&txt_multi.text()));
                            txt_multi.set_text("");
                        }
                        txt_value.set_visible(!*vis);
                        txt_multi.set_visible(*vis);
                        btn_toggle.set_text(if *vis { "Hide" } else { "Show" });
                    } else if handle == btn_copy.handle {
                        let value = if *is_visible_clone.borrow() {
                            from_edit_newlines(&txt_multi.text())
                        } else {
                            txt_value.text()
                        };
                        match clipboard_win::set_clipboard(clipboard_win::formats::Unicode, &value) {
                            Ok(()) => {
                                // Keep it out of Win+V history / cloud clipboard
                                crate::features::clipboard_guard::exclude_current_clipboard_entry_from_history();
                                *copied_clone.borrow_mut() = Some(value);
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
                        let name = txt_name.text();
                        let trimmed_name = name.trim();
                        if trimmed_name.is_empty() {
                            lbl_error.set_text(&t("vault.name.empty"));
                            return;
                        }

                        let Some(category) = cmb_category.selection().and_then(|i| categories_owned.get(i)).cloned() else {
                            lbl_error.set_text(&t("vault.category.none"));
                            return;
                        };
                        let value = if *is_visible_clone.borrow() {
                            from_edit_newlines(&txt_multi.text())
                        } else {
                            txt_value.text()
                        };

                        *result_clone.borrow_mut() = Some(SecretEntry {
                            name: trimmed_name.to_string(),
                            category,
                            value,
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

}
