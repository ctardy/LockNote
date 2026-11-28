// PasswordGeneratorDialog — generate a password
//
// Opened from Tools > Password Generator and from the "Generate..." button of
// the vault's secret dialog (then with a "Use" button that hands the password
// back). The password is masked until "Show"; it is drawn again whenever the
// length, a character class, the symbol list or the look-alike option changes.
// All rules are in `features::passgen`; this file only maps the window to them.
//
// Returns the options last used successfully (the caller saves them), the
// password chosen with "Use", and the last value copied (for the clipboard
// guard).

use crate::features::passgen::{self, Options, PassgenError, MAX_LENGTH, MIN_LENGTH};
use native_windows_gui as nwg;
use std::cell::RefCell;
use std::rc::Rc;
use zeroize::Zeroizing;

fn t(key: &str) -> String {
    crate::features::i18n::get(key).to_string()
}

/// Character shown instead of the password while it is hidden.
const MASK: char = '*';

/// What the dialog hands back when it closes.
pub struct Outcome {
    /// Options of the last successful draw (the initial ones when none was).
    pub options: Options,
    /// Password chosen with "Use" (`for_field` only).
    pub used: Option<Zeroizing<String>>,
    /// Last value copied with "Copy".
    pub copied: Option<String>,
}

/// Message shown under the options when they cannot give a password.
pub(crate) fn error_text(e: &PassgenError) -> String {
    match e {
        PassgenError::Length => t("passgen.err.length")
            .replace("{min}", &MIN_LENGTH.to_string())
            .replace("{max}", &MAX_LENGTH.to_string()),
        PassgenError::NoClass => t("passgen.err.no_class"),
        PassgenError::NotASymbol(c) => {
            t("passgen.err.not_a_symbol").replace("{char}", &c.to_string())
        }
        PassgenError::NoSymbolLeft => {
            t("passgen.err.no_symbol")
        }
    }
}

fn check(on: bool) -> nwg::CheckBoxState {
    if on {
        nwg::CheckBoxState::Checked
    } else {
        nwg::CheckBoxState::Unchecked
    }
}

fn is_checked(c: &nwg::CheckBox) -> bool {
    c.check_state() == nwg::CheckBoxState::Checked
}

/// The controls the event handler works on.
struct Form {
    txt_password: nwg::TextInput,
    btn_show: nwg::Button,
    btn_copy: nwg::Button,
    txt_length: nwg::TextInput,
    chk_upper: nwg::CheckBox,
    chk_lower: nwg::CheckBox,
    chk_digits: nwg::CheckBox,
    chk_symbols: nwg::CheckBox,
    txt_symbols: nwg::TextInput,
    chk_ambiguous: nwg::CheckBox,
    lbl_error: nwg::Label,
}

impl Form {
    /// The options of the form. The length and symbol fields are read
    /// strictly: a half emoji used to read as "Decoding error" and be
    /// reported as a wrong length or symbol.
    fn options(&self) -> Result<Options, String> {
        use crate::ui::field_text::read;
        let length = read(self.txt_length.handle).map_err(|e| e.typing_message())?;
        let symbols = read(self.txt_symbols.handle).map_err(|e| e.typing_message())?;
        Options::from_form(
            &length,
            is_checked(&self.chk_upper),
            is_checked(&self.chk_lower),
            is_checked(&self.chk_digits),
            is_checked(&self.chk_symbols),
            is_checked(&self.chk_ambiguous),
            &symbols,
        )
        .map_err(|e| error_text(&e))
    }

    /// Draw a new password from the window's options. On success `last`
    /// becomes these options; otherwise the field is emptied and the reason
    /// is shown (never a password that does not match what is displayed).
    fn regenerate(&self, last: &RefCell<Options>) {
        match self.options().and_then(|o| passgen::generate(&o).map(|p| (o, p)).map_err(|e| error_text(&e))) {
            Ok((options, password)) => {
                self.txt_password.set_text(&password);
                self.lbl_error.set_text("");
                *last.borrow_mut() = options;
            }
            Err(message) => {
                self.txt_password.set_text("");
                self.lbl_error.set_text(&message);
            }
        }
        self.btn_copy.set_text(&t("vault.btn.copy"));
    }

    fn set_revealed(&self, revealed: bool) {
        self.txt_password.set_password_char(if revealed { None } else { Some(MASK) });
        self.btn_show.set_text(&if revealed { t("passgen.hide") } else { t("passgen.show") });
        // The edit only redraws its text with the new mask when invalidated.
        if let nwg::ControlHandle::Hwnd(h) = self.txt_password.handle {
            unsafe {
                let _ = windows::Win32::Graphics::Gdi::InvalidateRect(
                    Some(windows::Win32::Foundation::HWND(h as *mut _)),
                    None,
                    true,
                );
            }
        }
    }
}

pub struct PasswordGeneratorDialog;

impl PasswordGeneratorDialog {
    /// Show the generator starting from `initial`. `for_field`: add the "Use"
    /// button (the caller puts the password in its own field). `owner`: the
    /// dialog it was opened from, disabled while the generator is open.
    pub fn show(initial: &Options, for_field: bool, owner: Option<nwg::ControlHandle>) -> Outcome {
        let mut window = Default::default();
        nwg::Window::builder()
            .size((440, 380))
            .position((300, 220))
            .title(&t("passgen.title"))
            .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
            .build(&mut window)
            .expect("Failed to build PasswordGeneratorDialog window");

        super::apply_dialog_theme(&window);

        let mut font = Default::default();
        nwg::Font::builder()
            .family("Segoe UI")
            .size(16)
            .build(&mut font)
            .expect("Failed to build font");

        // --- Password ---
        let mut lbl_password = Default::default();
        nwg::Label::builder()
            .text(&t("passgen.password"))
            .parent(&window)
            .position((20, 15))
            .size((400, 20))
            .font(Some(&font))
            .build(&mut lbl_password)
            .expect("Failed to build password label");

        let mut txt_password = Default::default();
        nwg::TextInput::builder()
            .parent(&window)
            .position((20, 38))
            .size((300, 25))
            .password(Some(MASK))
            .font(Some(&font))
            .build(&mut txt_password)
            .expect("Failed to build password input");

        let mut btn_show = Default::default();
        nwg::Button::builder()
            .text(&t("passgen.show"))
            .parent(&window)
            .position((330, 38))
            .size((90, 25))
            .font(Some(&font))
            .build(&mut btn_show)
            .expect("Failed to build show button");

        let mut btn_generate = Default::default();
        nwg::Button::builder()
            .text(&t("passgen.generate"))
            .parent(&window)
            .position((20, 72))
            .size((140, 26))
            .font(Some(&font))
            .focus(true)
            .build(&mut btn_generate)
            .expect("Failed to build generate button");

        let mut btn_copy = Default::default();
        nwg::Button::builder()
            .text(&t("vault.btn.copy"))
            .parent(&window)
            .position((170, 72))
            .size((110, 26))
            .font(Some(&font))
            .build(&mut btn_copy)
            .expect("Failed to build copy button");

        // --- Length ---
        let mut lbl_length = Default::default();
        nwg::Label::builder()
            .text(
                &t("passgen.length")
                    .replace("{min}", &MIN_LENGTH.to_string())
                    .replace("{max}", &MAX_LENGTH.to_string()),
            )
            .parent(&window)
            .position((20, 117))
            .size((200, 20))
            .font(Some(&font))
            .build(&mut lbl_length)
            .expect("Failed to build length label");

        let mut txt_length = Default::default();
        nwg::TextInput::builder()
            .text(&initial.length.to_string())
            .parent(&window)
            .position((230, 114))
            .size((60, 25))
            .flags(
                nwg::TextInputFlags::VISIBLE
                    | nwg::TextInputFlags::NUMBER
                    | nwg::TextInputFlags::AUTO_SCROLL
                    | nwg::TextInputFlags::TAB_STOP,
            )
            .limit(2)
            .font(Some(&font))
            .build(&mut txt_length)
            .expect("Failed to build length input");

        // --- Character classes ---
        let checkbox = |text: String, x: i32, y: i32, w: i32, on: bool| {
            let mut c = nwg::CheckBox::default();
            nwg::CheckBox::builder()
                .text(&text)
                .parent(&window)
                .position((x, y))
                .size((w, 22))
                .font(Some(&font))
                .check_state(check(on))
                .build(&mut c)
                .expect("Failed to build checkbox");
            c
        };
        let chk_upper = checkbox(t("passgen.upper"), 20, 150, 190, initial.upper);
        let chk_lower = checkbox(t("passgen.lower"), 220, 150, 200, initial.lower);
        let chk_digits = checkbox(t("passgen.digits"), 20, 176, 190, initial.digits);
        let chk_symbols = checkbox(t("passgen.symbols"), 220, 176, 200, initial.symbols);

        let mut lbl_symbols = Default::default();
        nwg::Label::builder()
            .text(&t("passgen.symbol_set"))
            .parent(&window)
            .position((20, 208))
            .size((400, 20))
            .font(Some(&font))
            .build(&mut lbl_symbols)
            .expect("Failed to build symbols label");

        let mut txt_symbols = Default::default();
        nwg::TextInput::builder()
            .text(&initial.symbol_set)
            .parent(&window)
            .position((20, 230))
            .size((400, 25))
            .limit(64)
            .font(Some(&font))
            .build(&mut txt_symbols)
            .expect("Failed to build symbols input");

        let chk_ambiguous = checkbox(
            t("passgen.exclude_ambiguous")
                .replace("{chars}", &passgen::AMBIGUOUS.chars().map(String::from).collect::<Vec<_>>().join(" ")),
            20,
            264,
            400,
            initial.exclude_ambiguous,
        );

        // --- Error label ---
        let mut lbl_error = Default::default();
        nwg::Label::builder()
            .text("")
            .parent(&window)
            .position((20, 294))
            .size((400, 20))
            .font(Some(&font))
            .build(&mut lbl_error)
            .expect("Failed to build error label");
        super::mark_error(window.handle, lbl_error.handle);

        // --- Footer ---
        let mut btn_use: nwg::Button = Default::default();
        if for_field {
            nwg::Button::builder()
                .text(&t("passgen.use"))
                .parent(&window)
                .position((230, 330))
                .size((90, 30))
                .font(Some(&font))
                .build(&mut btn_use)
                .expect("Failed to build Use button");
        }

        let mut btn_close = Default::default();
        nwg::Button::builder()
            .text(&t("passgen.close"))
            .parent(&window)
            .position((330, 330))
            .size((90, 30))
            .font(Some(&font))
            .build(&mut btn_close)
            .expect("Failed to build Close button");

        // Enter: use the password (opened from a field), else copy it.
        super::bind_enter_to_button(window.handle, if for_field { btn_use.handle } else { btn_copy.handle });

        let form = Form {
            txt_password,
            btn_show,
            btn_copy,
            txt_length,
            chk_upper,
            chk_lower,
            chk_digits,
            chk_symbols,
            txt_symbols,
            chk_ambiguous,
            lbl_error,
        };

        let last: Rc<RefCell<Options>> = Rc::new(RefCell::new(initial.clone()));
        let used: Rc<RefCell<Option<Zeroizing<String>>>> = Rc::new(RefCell::new(None));
        let copied: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        let revealed = Rc::new(RefCell::new(false));

        // A password is ready as soon as the window opens.
        form.regenerate(&last);

        let (last_h, used_h, copied_h) = (last.clone(), used.clone(), copied.clone());
        let window_handle = window.handle;
        let handler = nwg::full_bind_event_handler(&window_handle, move |evt, _evt_data, handle| match evt {
            nwg::Event::OnButtonClick => {
                if handle == btn_generate.handle
                    || handle == form.chk_upper.handle
                    || handle == form.chk_lower.handle
                    || handle == form.chk_digits.handle
                    || handle == form.chk_symbols.handle
                    || handle == form.chk_ambiguous.handle
                {
                    form.regenerate(&last_h);
                } else if handle == form.btn_show.handle {
                    let now = !*revealed.borrow();
                    *revealed.borrow_mut() = now;
                    form.set_revealed(now);
                } else if handle == form.btn_copy.handle {
                    let Some(value) = crate::ui::field_text::read_masked(form.txt_password.handle, "passgen.password", |m| {
                        form.lbl_error.set_text(m)
                    }) else {
                        return;
                    };
                    if value.is_empty() {
                        return;
                    }
                    match clipboard_win::set_clipboard(clipboard_win::formats::Unicode, value.as_str()) {
                        Ok(()) => {
                            // Keep it out of Win+V history / cloud clipboard
                            crate::features::clipboard_guard::exclude_current_clipboard_entry_from_history();
                            *copied_h.borrow_mut() = Some(value.to_string());
                            form.lbl_error.set_text("");
                            form.btn_copy.set_text(&t("vault.btn.copied"));
                        }
                        Err(e) => {
                            eprintln!("passgen: copy failed err={:?}", e);
                            form.lbl_error.set_text(&t("vault.copy.failed"));
                        }
                    }
                } else if for_field && handle == btn_use.handle {
                    let Some(value) = crate::ui::field_text::read_masked(form.txt_password.handle, "passgen.password", |m| {
                        form.lbl_error.set_text(m)
                    }) else {
                        return;
                    };
                    if value.is_empty() {
                        return;
                    }
                    *used_h.borrow_mut() = Some(value);
                    form.txt_password.set_text("");
                    nwg::stop_thread_dispatch();
                } else if handle == btn_close.handle {
                    form.txt_password.set_text("");
                    nwg::stop_thread_dispatch();
                }
            }
            nwg::Event::OnTextInput => {
                if handle == form.txt_length.handle || handle == form.txt_symbols.handle {
                    form.regenerate(&last_h);
                }
            }
            nwg::Event::OnWindowClose => {
                form.txt_password.set_text("");
                nwg::stop_thread_dispatch();
            }
            _ => {}
        });

        // Its previous state comes back when the guard drops (pass 6).
        let owner_disabled = super::disable_owner(owner, true);
        nwg::dispatch_thread_events();
        drop(owner_disabled);
        nwg::unbind_event_handler(&handler);

        let options = last.borrow().clone();
        let used = used.borrow_mut().take();
        let copied = copied.borrow_mut().take();
        Outcome { options, used, copied }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_error_has_its_own_message() {
        let texts = [
            error_text(&PassgenError::Length),
            error_text(&PassgenError::NoClass),
            error_text(&PassgenError::NotASymbol('a')),
            error_text(&PassgenError::NoSymbolLeft),
        ];
        for (i, a) in texts.iter().enumerate() {
            assert!(!a.is_empty() && !a.starts_with('['), "message {i} is missing: {a:?}");
            for b in &texts[i + 1..] {
                assert_ne!(a, b);
            }
        }
        // Placeholders are filled in.
        assert!(texts[0].contains(&MIN_LENGTH.to_string()) && texts[0].contains(&MAX_LENGTH.to_string()));
        assert!(texts[2].contains('a'));
        assert!(texts.iter().all(|t| !t.contains('{')), "{texts:?}");
    }

    // -----------------------------------------------------------------
    // Live test: the real dialog, driven through its controls from another
    // thread (never the clipboard: "Copy" is not clicked).
    // -----------------------------------------------------------------

    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumChildWindows, EnumThreadWindows, GetWindowTextLengthW, GetWindowTextW, IsWindowVisible, SendMessageW,
    };

    /// Where each control sits in `wait_for_dialog`'s result: the edits, then
    /// the buttons and check boxes, then the labels, each group in reading
    /// order (top to bottom, left to right), as laid out in `show`.
    const TXT_PASSWORD: usize = 0;
    const TXT_LENGTH: usize = 1;
    const TXT_SYMBOLS: usize = 2;
    const BTN_GENERATE: usize = 3 + 1;
    const CHK_LOWER: usize = 3 + 4;
    const CHK_AMBIGUOUS: usize = 3 + 7;
    const BTN_USE: usize = 3 + 8;
    const LBL_ERROR: usize = 3 + 10 + 3;
    const CHILD_COUNT: usize = 17;

    unsafe extern "system" fn collect(h: HWND, lp: LPARAM) -> windows::core::BOOL {
        (*(lp.0 as *mut Vec<isize>)).push(h.0 as isize);
        true.into()
    }

    /// The dialog of thread `tid` and its children, once it is fully built.
    fn wait_for_dialog(tid: u32) -> Vec<isize> {
        for _ in 0..200 {
            std::thread::sleep(std::time::Duration::from_millis(50));
            let mut tops: Vec<isize> = Vec::new();
            unsafe {
                let _ = EnumThreadWindows(tid, Some(collect), LPARAM(&mut tops as *mut _ as isize));
            }
            let dialog = tops.into_iter().find(|h| unsafe {
                let h = HWND(*h as *mut _);
                IsWindowVisible(h).as_bool() && GetWindowTextLengthW(h) > 0
            });
            let Some(dialog) = dialog else { continue };
            let mut children: Vec<isize> = Vec::new();
            unsafe {
                let _ = EnumChildWindows(Some(HWND(dialog as *mut _)), Some(collect), LPARAM(&mut children as *mut _ as isize));
            }
            if children.len() == CHILD_COUNT {
                // Let the first messages (the shared styling pass) go by.
                std::thread::sleep(std::time::Duration::from_millis(300));
                return in_reading_order(children);
            }
        }
        panic!("the generator window did not appear");
    }

    /// Edits, then buttons, then labels; each group top to bottom, left to right.
    fn in_reading_order(children: Vec<isize>) -> Vec<isize> {
        use windows::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetWindowRect};
        let mut keyed: Vec<(usize, i32, i32, isize)> = children
            .into_iter()
            .map(|h| {
                let hwnd = HWND(h as *mut _);
                let mut name = [0u16; 32];
                let n = unsafe { GetClassNameW(hwnd, &mut name) } as usize;
                let group = match String::from_utf16_lossy(&name[..n]).to_ascii_lowercase().as_str() {
                    "edit" => 0,
                    "button" => 1,
                    "static" => 2,
                    other => panic!("unexpected control class {other:?}"),
                };
                let mut r = windows::Win32::Foundation::RECT::default();
                unsafe { GetWindowRect(hwnd, &mut r).expect("GetWindowRect") };
                (group, r.top, r.left, h)
            })
            .collect();
        keyed.sort();
        keyed.into_iter().map(|k| k.3).collect()
    }

    fn text_of(h: isize) -> String {
        let mut buf = vec![0u16; 256];
        let n = unsafe { GetWindowTextW(HWND(h as *mut _), &mut buf) } as usize;
        String::from_utf16_lossy(&buf[..n])
    }

    fn set_text(h: isize, text: &str) {
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        // WM_SETTEXT; the edit then notifies its parent (EN_CHANGE).
        unsafe { SendMessageW(HWND(h as *mut _), 0x000C, Some(WPARAM(0)), Some(LPARAM(wide.as_ptr() as isize))) };
    }

    fn click(h: isize) {
        // BM_CLICK
        unsafe { SendMessageW(HWND(h as *mut _), 0x00F5, Some(WPARAM(0)), Some(LPARAM(0))) };
    }

    #[test]
    fn live_dialog_generates_follows_the_options_and_hands_the_password_back() {
        // The dialog's texts and the expected ones must be in one language
        // for the whole test (pass 3: another test switched it midway).
        let _g = crate::features::i18n::test_support::lang_guard();
        let _ = nwg::init();
        let tid = unsafe { windows::Win32::System::Threading::GetCurrentThreadId() };
        let driver = std::thread::spawn(move || {
            let c = wait_for_dialog(tid);
            let mut seen = Vec::new();

            // A password is there as soon as the window opens.
            seen.push(text_of(c[TXT_PASSWORD]));
            // "Generate" draws another one.
            click(c[BTN_GENERATE]);
            seen.push(text_of(c[TXT_PASSWORD]));

            // Impossible options: no password is left in the field, and why.
            set_text(c[TXT_LENGTH], "3");
            seen.push(text_of(c[TXT_PASSWORD]));
            seen.push(text_of(c[LBL_ERROR]));

            // New length, no lowercase, own symbols, no look-alikes.
            set_text(c[TXT_LENGTH], "24");
            click(c[CHK_LOWER]);
            set_text(c[TXT_SYMBOLS], "-_|");
            click(c[CHK_AMBIGUOUS]);
            seen.push(text_of(c[TXT_PASSWORD]));
            seen.push(text_of(c[LBL_ERROR]));

            click(c[BTN_USE]);
            seen
        });

        let outcome = PasswordGeneratorDialog::show(&Options::default(), true, None);
        let seen = driver.join().expect("driver");

        let (first, second) = (&seen[0], &seen[1]);
        assert_eq!(first.chars().count(), 16, "a 16-character password at opening");
        assert_ne!(first, second, "Generate draws a new password");
        assert_eq!(seen[2], "", "no password while the options are impossible");
        assert_eq!(seen[3], error_text(&PassgenError::Length));

        let last = &seen[4];
        assert_eq!(seen[5], "", "the error is gone once the options are valid again");
        assert_eq!(last.chars().count(), 24);
        assert!(!last.chars().any(|c| c.is_ascii_lowercase()), "lowercase was unticked: {last:?}");
        assert!(last.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-' || c == '_'), "{last:?}");
        assert!(!last.chars().any(|c| passgen::AMBIGUOUS.contains(c)), "{last:?}");

        // "Use" hands back the password that was displayed, and the options.
        assert_eq!(outcome.used.as_deref().map(String::as_str), Some(last.as_str()));
        assert_eq!(outcome.copied, None);
        assert_eq!(
            outcome.options,
            Options {
                length: 24,
                upper: true,
                lower: false,
                digits: true,
                symbols: true,
                exclude_ambiguous: true,
                symbol_set: "-_|".into(),
            }
        );
    }
}
