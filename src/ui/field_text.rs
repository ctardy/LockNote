// Reading the text of an input field (an EDIT control: nwg TextInput or
// TextBox) without nwg's decoding.
//
// nwg's `TextInput::text()` / `TextBox::text()` (nwg 1.0.13, window_helper
// `get_window_text` -> base_helper `from_utf16`) return the literal
// "Decoding error" when the field holds an unpaired surrogate, and "" when
// GetWindowTextW fails. An EDIT control does keep a lone surrogate (measured
// 2026-11-28 on Windows 10.0.26200: WM_SETTEXT, EM_REPLACESEL and WM_CHAR,
// in single-line, ES_PASSWORD, ES_NUMBER and multiline fields; typing an
// emoji as two WM_CHAR passes through that state). So a pasted half emoji
// made a new password, a vault secret or a prompt become "Decoding error",
// and an unlock attempt compare that string. nwg's buffer was not wiped
// either.
//
// Two readers, both into a buffer wiped on drop:
//
// - `read`: for every value that is stored, encrypted with, compared, used
//   as a key or acted on. Invalid UTF-16 is an error; the caller refuses the
//   action, tells the user (`read_masked` / `read_visible` do both) and saves
//   nothing.
// - `read_display`: for what only filters or searches what is shown (search
//   boxes). Each lone surrogate becomes U+FFFD. Its text must never be
//   stored nor compared with a secret.
//
// Every production read of an input in src/ui goes through here: a test
// bans nwg's own text reader on fields.

use super::editor_text::{hwnd_of, lone_surrogates, read_units, EditorTextError};
use native_windows_gui as nwg;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use zeroize::Zeroizing;

/// Why a field's text was not returned. Carries no content and, for a
/// masked field, nothing its message shows (no position, no count).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldTextError {
    /// The field has no window handle.
    NoWindow,
    /// GetWindowTextW copied nothing from a non-empty field, or more units
    /// than announced.
    ReadFailed { expected: usize, copied: usize },
    /// The text holds unpaired surrogates: `unit` is the EDIT position of
    /// the first one (a multiline EDIT counts "\r\n" as two), `count` how
    /// many there are. Neither is ever shown to the user.
    InvalidUtf16 { unit: usize, count: usize },
}

impl From<EditorTextError> for FieldTextError {
    fn from(e: EditorTextError) -> Self {
        match e {
            EditorTextError::NoWindow => FieldTextError::NoWindow,
            EditorTextError::ReadFailed { expected, copied } => FieldTextError::ReadFailed { expected, copied },
            // read_units never returns it; kept total rather than panicking.
            EditorTextError::InvalidUtf16 { unit, count, .. } => FieldTextError::InvalidUtf16 { unit, count },
        }
    }
}

fn t(key: &str) -> String {
    crate::features::i18n::get(key).to_string()
}

impl FieldTextError {
    /// The message for a masked field (password, passphrase, hidden secret):
    /// the field was cleared, type it again. No position nor count.
    pub fn masked_message(&self) -> String {
        match self {
            FieldTextError::InvalidUtf16 { .. } => t("field.text.invalid.masked"),
            FieldTextError::NoWindow | FieldTextError::ReadFailed { .. } => t("field.text.unreadable"),
        }
    }

    /// The message for a visible field: the invalid character is selected,
    /// delete it.
    pub fn visible_message(&self) -> String {
        match self {
            FieldTextError::InvalidUtf16 { .. } => t("field.text.invalid"),
            FieldTextError::NoWindow | FieldTextError::ReadFailed { .. } => t("field.text.unreadable"),
        }
    }

    /// The message for a field read as it is typed (nothing is selected
    /// nor cleared: that would break an emoji typed as two characters).
    pub fn typing_message(&self) -> String {
        match self {
            FieldTextError::InvalidUtf16 { .. } => t("field.text.invalid.typing"),
            FieldTextError::NoWindow | FieldTextError::ReadFailed { .. } => t("field.text.unreadable"),
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            FieldTextError::NoWindow => "no_window",
            FieldTextError::ReadFailed { .. } => "read_failed",
            FieldTextError::InvalidUtf16 { .. } => "invalid_utf16",
        }
    }
}

/// `units` as a string whose buffer is wiped on drop, or where its first
/// unpaired surrogate is. The exact UTF-8 length is computed first so the
/// string is allocated once: no grown-and-freed copy of a secret is left
/// behind (`String::from_utf16` grows its buffer as it pushes).
pub fn decode_exact(units: &[u16]) -> Result<Zeroizing<String>, FieldTextError> {
    let mut len = 0usize;
    for c in char::decode_utf16(units.iter().copied()) {
        match c {
            Ok(c) => len += c.len_utf8(),
            Err(_) => {
                let lone = lone_surrogates(units);
                let unit = lone.first().copied().unwrap_or(units.len());
                return Err(FieldTextError::InvalidUtf16 { unit, count: lone.len() });
            }
        }
    }
    let mut s = Zeroizing::new(String::with_capacity(len));
    // Every unit was checked above: no replacement can happen here.
    s.extend(char::decode_utf16(units.iter().copied()).map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER)));
    Ok(s)
}

fn hwnd(handle: nwg::ControlHandle) -> HWND {
    hwnd_of(handle)
}

/// The field's text, refused when it is not valid UTF-16.
pub fn read(handle: nwg::ControlHandle) -> Result<Zeroizing<String>, FieldTextError> {
    decode_exact(&read_units(hwnd(handle))?)
}

/// The field's text for filtering what is shown: each lone surrogate is
/// U+FFFD. Never stored, never compared with a secret.
pub fn read_display(handle: nwg::ControlHandle) -> Result<Zeroizing<String>, FieldTextError> {
    let units = read_units(hwnd(handle))?;
    let mut s = Zeroizing::new(String::with_capacity(units.len() * 3));
    s.extend(char::decode_utf16(units.iter().copied()).map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER)));
    Ok(s)
}

fn focus(h: HWND) {
    // SAFETY: SetFocus on a window of this thread; a failure only leaves the
    // focus where it was.
    unsafe {
        let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(Some(h));
    }
}

/// Read a masked field strictly. On a refusal: log it (no content), empty
/// the field (the user cannot see the bad character), focus it, pass the
/// message to `tell`, return `None`: the caller stops there and saves,
/// compares or decrypts nothing.
pub fn read_masked(handle: nwg::ControlHandle, field: &str, tell: impl FnOnce(&str)) -> Option<Zeroizing<String>> {
    match read(handle) {
        Ok(s) => Some(s),
        Err(e) => {
            eprintln!("WARN field: masked field refused field={} reason={}", field, e.kind());
            let h = hwnd(handle);
            if !h.is_invalid() {
                set_empty(h);
                focus(h);
            }
            tell(&e.masked_message());
            None
        }
    }
}

/// Read a visible field strictly. On a refusal: log it (no content), keep
/// the text (it may be long), select the first invalid character so Delete
/// removes it, focus the field, pass the message to `tell`, return `None`.
pub fn read_visible(handle: nwg::ControlHandle, field: &str, tell: impl FnOnce(&str)) -> Option<Zeroizing<String>> {
    match read(handle) {
        Ok(s) => Some(s),
        Err(e) => {
            eprintln!("WARN field: visible field refused field={} reason={}", field, e.kind());
            let h = hwnd(handle);
            if let FieldTextError::InvalidUtf16 { unit, .. } = e {
                select_unit(h, unit);
            }
            if !h.is_invalid() {
                focus(h);
            }
            tell(&e.visible_message());
            None
        }
    }
}

fn set_empty(h: HWND) {
    use windows::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_SETTEXT};
    let empty = [0u16];
    // SAFETY: a null-terminated buffer that outlives the synchronous call.
    unsafe {
        SendMessageW(h, WM_SETTEXT, None, Some(LPARAM(empty.as_ptr() as isize)));
    }
}

/// Select one unit of an EDIT control. EM_SETSEL positions of an EDIT are
/// WM_GETTEXT indexes ("\r\n" counts two), unlike a RichEdit's.
fn select_unit(h: HWND, unit: usize) {
    use windows::Win32::UI::WindowsAndMessaging::SendMessageW;
    const EM_SETSEL: u32 = 0x00B1;
    const EM_SCROLLCARET: u32 = 0x00B7;
    // SAFETY: plain messages to a window of this thread.
    unsafe {
        SendMessageW(h, EM_SETSEL, Some(WPARAM(unit)), Some(LPARAM(unit as isize + 1)));
        SendMessageW(h, EM_SCROLLCARET, None, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_CHAR, WM_SETTEXT};

    fn units(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    #[test]
    fn valid_text_decodes_into_an_exact_buffer() {
        for s in ["", "abc", "\u{e9}\u{20ac}\u{1F600}", "a\r\nb\u{1F600}"] {
            let d = decode_exact(&units(s)).expect("valid");
            assert_eq!(d.as_str(), s);
            assert_eq!(d.capacity(), s.len(), "{s:?}: the buffer was grown, a copy was freed unwiped");
        }
    }

    #[test]
    fn a_lone_surrogate_is_refused_never_turned_into_a_value() {
        assert_eq!(decode_exact(&[0x61, 0xD83D, 0x62]), Err(FieldTextError::InvalidUtf16 { unit: 1, count: 1 }));
        assert_eq!(decode_exact(&[0xDE00, 0x61, 0xD83D]), Err(FieldTextError::InvalidUtf16 { unit: 0, count: 2 }));
        // Two different invalid inputs no longer read as the same value.
        assert_ne!(decode_exact(&[0xD800]), decode_exact(&[0x7A, 0xDFFF, 0x7A]));
    }

    #[test]
    fn messages_are_translated_and_never_hold_a_position_or_a_count() {
        let _lang = crate::features::i18n::test_support::lang_guard();
        for lang in [
            crate::features::i18n::Language::English,
            crate::features::i18n::Language::French,
            crate::features::i18n::Language::Spanish,
            crate::features::i18n::Language::German,
        ] {
            crate::features::i18n::set_language(lang);
            let e = FieldTextError::InvalidUtf16 { unit: 7, count: 3 };
            let r = FieldTextError::ReadFailed { expected: 4, copied: 0 };
            for m in [e.masked_message(), e.visible_message(), e.typing_message(), r.masked_message(), r.visible_message()] {
                assert!(!m.starts_with("field."), "{lang:?}: untranslated {m}");
                assert!(!m.chars().any(|c| c.is_ascii_digit()), "{lang:?}: {m}");
                assert!(!m.contains('{'), "{lang:?}: {m}");
            }
            assert_ne!(e.masked_message(), e.visible_message());
            assert_ne!(e.typing_message(), e.visible_message());
        }
    }

    // ── live EDIT controls (no RichEdit: no live_rich_edit guard needed) ──

    fn window() -> nwg::Window {
        let _ = nwg::init();
        let mut window = nwg::Window::default();
        nwg::Window::builder()
            .size((300, 200))
            .position((-32000, -32000))
            .flags(nwg::WindowFlags::WINDOW)
            .build(&mut window)
            .expect("window");
        window
    }

    fn set_units(h: HWND, u: &[u16]) {
        let mut z = u.to_vec();
        z.push(0);
        unsafe {
            SendMessageW(h, WM_SETTEXT, None, Some(LPARAM(z.as_ptr() as isize)));
        }
    }

    fn replace_sel(h: HWND, u: &[u16]) {
        const EM_REPLACESEL: u32 = 0x00C2;
        let mut z = u.to_vec();
        z.push(0);
        unsafe {
            SendMessageW(h, EM_REPLACESEL, Some(WPARAM(0)), Some(LPARAM(z.as_ptr() as isize)));
        }
    }

    fn char_unit(h: HWND, u: u16) {
        unsafe {
            SendMessageW(h, WM_CHAR, Some(WPARAM(u as usize)), Some(LPARAM(1)));
        }
    }

    fn inputs(window: &nwg::Window) -> (nwg::TextInput, nwg::TextInput, nwg::TextBox) {
        let mut plain = Default::default();
        nwg::TextInput::builder().parent(window).build(&mut plain).expect("input");
        let mut masked = Default::default();
        nwg::TextInput::builder().parent(window).password(Some('*')).build(&mut masked).expect("masked");
        let mut multi = Default::default();
        nwg::TextBox::builder().parent(window).build(&mut multi).expect("text box");
        (plain, masked, multi)
    }

    #[test]
    fn an_edit_field_keeps_a_lone_surrogate_and_nwg_reads_it_as_decoding_error() {
        // The behaviour this reader works around (nwg 1.0.13).
        let w = window();
        let (plain, masked, multi) = inputs(&w);
        for (handle, nwg_text) in [
            (plain.handle, Box::new(|| plain.text()) as Box<dyn Fn() -> String>),
            (masked.handle, Box::new(|| masked.text())),
            (multi.handle, Box::new(|| multi.text())),
        ] {
            let h = hwnd(handle);
            set_units(h, &[0x61, 0xD83D, 0x62]);
            assert_eq!(nwg_text(), "Decoding error", "WM_SETTEXT");
            assert_eq!(read(handle), Err(FieldTextError::InvalidUtf16 { unit: 1, count: 1 }));
            set_units(h, &[]);
            replace_sel(h, &[0xDE00, 0x78]);
            assert_eq!(nwg_text(), "Decoding error", "EM_REPLACESEL");
            assert_eq!(read(handle), Err(FieldTextError::InvalidUtf16 { unit: 0, count: 1 }));
            set_units(h, &[]);
            char_unit(h, 0x6B);
            char_unit(h, 0xD83D);
            assert_eq!(nwg_text(), "Decoding error", "half of a typed emoji");
            assert!(matches!(read(handle), Err(FieldTextError::InvalidUtf16 { unit: 1, .. })));
            char_unit(h, 0xDE00);
            assert_eq!(read(handle).expect("whole emoji").as_str(), "k\u{1F600}");
            set_units(h, &[]);
            assert_eq!(read(handle).expect("empty").as_str(), "");
            set_units(h, &[0x61, 0xD83D]);
            assert_eq!(read_display(handle).expect("display").as_str(), "a\u{FFFD}");
        }
    }

    /// Every `.rs` file under `dir`, recursively.
    fn sources(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(dir).expect("src/ui") {
            let p = e.expect("entry").path();
            if p.is_dir() {
                sources(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }

    #[test]
    fn no_production_code_reads_a_field_through_nwg_text() {
        // nwg's text() turns a half emoji into "Decoding error" (and a failed
        // read into ""), which then became a password, a secret or a name.
        // Every field is read here. The only `.text()` left are the strict
        // RichEdit readers of the editor, which return a Result.
        const STRICT_EDITOR_READERS: [&str; 3] = ["steps.text()", "s.text()", "target.text()"];
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("ui");
        let mut files = Vec::new();
        sources(&root, &mut files);
        assert!(files.len() > 60, "the ui sources were not found");
        let mut found = Vec::new();
        for file in &files {
            let src = crate::read_source(file);
            let code = src.split("#[cfg(test)]").next().expect("code part");
            for (n, line) in code.lines().enumerate() {
                let mut rest = line;
                while let Some(i) = rest.find(".text()") {
                    let before = &rest[..i];
                    let receiver_start = before.rfind(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.')).map_or(0, |p| p + 1);
                    let call = &rest[receiver_start..i + ".text()".len()];
                    if !STRICT_EDITOR_READERS.contains(&call) {
                        found.push(format!("{}:{}: {}", file.display(), n + 1, line.trim()));
                    }
                    rest = &rest[i + ".text()".len()..];
                }
            }
        }
        assert!(found.is_empty(), "fields read with nwg text():\n{}", found.join("\n"));
    }

    /// The index of `needle` in `hay` after `from`, or a failure naming it.
    fn at(hay: &str, needle: &str, from: usize, file: &str) -> usize {
        from + hay[from..].find(needle).unwrap_or_else(|| panic!("{file}: `{needle}` not found"))
    }

    #[test]
    fn a_refusal_comes_before_any_comparison_decrypt_counter_or_state_change() {
        // Wiring a test with a fake cannot see: these dialogs are closures
        // in modal loops. The read must come first, so a refused field is
        // never compared, decrypted, counted as an attempt, nor flips a view.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("ui");
        let check = |file: &str, start: &str, steps: &[&str]| {
            let src = crate::read_source(root.join(file));
            let mut pos = at(&src, start, 0, file);
            for step in steps {
                pos = at(&src, step, pos, file);
            }
        };
        check(
            "editor/lock_ops.rs",
            "pub(super) fn lift_curtain(&self) {",
            &["lock_backoff_remaining.get() > 0", "read_masked(self.lock_input.handle", "curtain_password_ok(", "*a += 1"],
        );
        check(
            "editor/lock_ops.rs",
            "pub(super) fn on_unlock_overlay(&self) {",
            &["lock_backoff_remaining.get() > 0", "read_masked(self.lock_input.handle", "crate::crypto::decrypt("],
        );
        check("dialogs/unlock.rs", "if handle == btn_unlock.handle {", &["read_masked(txt_password.handle", "crate::crypto::decrypt(", "*att += 1"]);
        check(
            "dialogs/create_password.rs",
            "if handle == btn_ok.handle {",
            &["read_masked(txt_password.handle", "read_masked(txt_confirm.handle", "< min_length", "*pw != *confirm"],
        );
        check(
            "dialogs/ssh_keygen.rs",
            "nwg::Event::OnButtonClick if handle == btn_generate.handle => {",
            &["read_masked(txt_pass.handle", "read_masked(txt_confirm.handle", "*pass != *confirm"],
        );
        check("editor/helpers.rs", "pub(super) fn stego_password_dialog(", &["read_masked(txt_pw.handle", "read_masked(txt_confirm.handle", "*pw != *confirm"]);
        check("dialogs/confirm_password.rs", "if handle == btn_ok.handle {", &["read_masked(txt_password.handle", "constant_time_eq("]);
        check("dialogs/totp_verify.rs", "if handle == btn_verify.handle {", &["read_visible(txt_code.handle", "totp::verify(", "*att += 1"]);
        check("dialogs/secret_edit.rs", "if handle == btn_toggle.handle {", &["read_value(showing", "*vis = !*vis"]);
    }

    #[test]
    fn no_window_is_an_error_not_an_empty_value() {
        let none = nwg::ControlHandle::NoHandle;
        assert_eq!(read(none), Err(FieldTextError::NoWindow));
        assert_eq!(read_display(none), Err(FieldTextError::NoWindow));
        let mut told = None;
        assert!(read_masked(none, "test", |m| told = Some(m.to_string())).is_none());
        assert!(told.is_some_and(|m| !m.is_empty()));
    }

    #[test]
    fn a_refused_masked_field_is_emptied_and_the_user_told_without_content() {
        let w = window();
        let (_, masked, _) = inputs(&w);
        let h = hwnd(masked.handle);
        set_units(h, &[0x73, 0x33, 0xD800, 0x74]);
        let mut told = Vec::new();
        assert!(read_masked(masked.handle, "test", |m| told.push(m.to_string())).is_none());
        assert_eq!(told.len(), 1);
        assert_eq!(told[0], FieldTextError::InvalidUtf16 { unit: 0, count: 0 }.masked_message());
        assert!(!told[0].contains("s3"), "content shown");
        assert_eq!(read(masked.handle).expect("emptied").as_str(), "", "the field is cleared");
        // Fixed, then read: the value comes back, nothing is told.
        masked.set_text("s3cret");
        let ok = read_masked(masked.handle, "test", |_| panic!("told on a valid value")).expect("valid");
        assert_eq!(ok.as_str(), "s3cret");
    }

    #[test]
    fn a_refused_visible_field_keeps_its_text_and_selects_the_bad_character() {
        let w = window();
        let (plain, _, multi) = inputs(&w);
        // A multiline EDIT counts "\r\n" as two positions: the selection
        // must land on unit 4, not one early per line break.
        let mut u = units("a\r\nb");
        u.push(0xD800);
        u.extend(units("c"));
        set_units(hwnd(multi.handle), &u);
        let mut told = None;
        assert!(read_visible(multi.handle, "test", |m| told = Some(m.to_string())).is_none());
        assert_eq!(told.as_deref(), Some(FieldTextError::InvalidUtf16 { unit: 0, count: 0 }.visible_message().as_str()));
        assert_eq!(multi.selection(), 4..5);
        assert_eq!(read_units(hwnd(multi.handle)).expect("kept").as_slice(), u.as_slice(), "the text is kept");
        set_units(hwnd(plain.handle), &[0x6E, 0x61, 0xDC00]);
        assert!(read_visible(plain.handle, "test", |_| {}).is_none());
        assert_eq!(plain.selection(), 2..3);
    }
}
