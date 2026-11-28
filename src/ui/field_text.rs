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

    /// The message for a hidden field that is kept on a refusal (a stored
    /// secret the user may not be able to type again): Show selects it.
    pub fn hidden_message(&self) -> String {
        match self {
            FieldTextError::InvalidUtf16 { .. } => t("field.text.invalid.hidden"),
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

    pub fn kind(&self) -> &'static str {
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

/// The field's raw UTF-16 units, undecoded, in a buffer wiped on drop:
/// for moving a value that may hold a lone surrogate into another field
/// without losing it (`write_units`). Never shown nor stored as such.
pub fn read_raw(handle: nwg::ControlHandle) -> Result<Zeroizing<Vec<u16>>, FieldTextError> {
    Ok(read_units(hwnd(handle))?)
}

/// Replace the field's text with `units`, as they are (a lone surrogate
/// included), through a null-terminated buffer wiped on drop.
pub fn write_units(handle: nwg::ControlHandle, units: &[u16]) -> Result<(), FieldTextError> {
    let h = hwnd(handle);
    if h.is_invalid() {
        return Err(FieldTextError::NoWindow);
    }
    let mut z = Zeroizing::new(Vec::with_capacity(units.len() + 1));
    z.extend_from_slice(units);
    z.push(0);
    set_text_units(h, &z);
    Ok(())
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

/// Read a field as it is typed (strength meter, live link): `None` until
/// the text is whole. Between the two WM_CHAR of an emoji the field holds
/// half of it, so an invalid character is waited out quietly; nothing is
/// cleared nor told (the action buttons refuse it loudly). A missing window
/// or a failed read is logged at WARN (no content), never dropped silently.
pub fn read_typing(handle: nwg::ControlHandle, field: &str) -> Option<Zeroizing<String>> {
    match read(handle) {
        Ok(s) => Some(s),
        Err(FieldTextError::InvalidUtf16 { .. }) => None,
        Err(e) => {
            eprintln!("WARN field: typed field unreadable field={} reason={}", field, e.kind());
            None
        }
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

/// Read a hidden field that must be kept on a refusal (a stored secret the
/// user may not be able to type again). On a refusal: log it (no content),
/// leave the field as it is, focus it, pass the message to `tell` (Show
/// reveals the invalid character selected), return `None`: the caller
/// stops there and copies or saves nothing.
pub fn read_kept(handle: nwg::ControlHandle, field: &str, tell: impl FnOnce(&str)) -> Option<Zeroizing<String>> {
    match read(handle) {
        Ok(s) => Some(s),
        Err(e) => {
            eprintln!("WARN field: hidden field refused, kept field={} reason={}", field, e.kind());
            let h = hwnd(handle);
            if !h.is_invalid() {
                focus(h);
            }
            tell(&e.hidden_message());
            None
        }
    }
}

/// Select the first lone surrogate of a visible field, focus it and pass
/// its message to `tell`. `false` when the field holds none.
pub fn select_invalid(handle: nwg::ControlHandle, tell: impl FnOnce(&str)) -> bool {
    match read(handle) {
        Err(e @ FieldTextError::InvalidUtf16 { unit, .. }) => {
            let h = hwnd(handle);
            select_unit(h, unit);
            focus(h);
            tell(&e.visible_message());
            true
        }
        _ => false,
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
    set_text_units(h, &[0u16]);
}

/// WM_SETTEXT with `z`, which must end with a 0 unit.
fn set_text_units(h: HWND, z: &[u16]) {
    use windows::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_SETTEXT};
    debug_assert_eq!(z.last(), Some(&0));
    // SAFETY: a null-terminated buffer that outlives the synchronous call.
    unsafe {
        SendMessageW(h, WM_SETTEXT, None, Some(LPARAM(z.as_ptr() as isize)));
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
            for m in [e.masked_message(), e.visible_message(), e.typing_message(), e.hidden_message(), r.masked_message(), r.visible_message(), r.hidden_message()] {
                assert!(!m.starts_with("field."), "{lang:?}: untranslated {m}");
                assert!(!m.chars().any(|c| c.is_ascii_digit()), "{lang:?}: {m}");
                assert!(!m.contains('{'), "{lang:?}: {m}");
            }
            assert_ne!(e.masked_message(), e.visible_message());
            assert_ne!(e.typing_message(), e.visible_message());
            assert_ne!(e.hidden_message(), e.masked_message(), "a kept field is not said to be cleared");
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

    /// `src` with every `#[cfg(test)]` item (a `mod`, `fn`, `use`, ...)
    /// blanked out, line breaks kept so line numbers stay true. Production
    /// code after a test item is still scanned. String, char and comment
    /// braces are skipped.
    fn production_code(src: &str) -> String {
        let b = src.as_bytes();
        let mut out = b.to_vec();
        let mut from = 0;
        while let Some(off) = src[from..].find("#[cfg(test)]") {
            let start = from + off;
            let mut i = start + "#[cfg(test)]".len();
            let mut depth = 0usize;
            let end = loop {
                assert!(i < b.len(), "unterminated #[cfg(test)] item at byte {start}");
                match b[i] {
                    b'/' if b.get(i + 1) == Some(&b'/') => {
                        while i < b.len() && b[i] != b'\n' {
                            i += 1;
                        }
                        continue;
                    }
                    b'"' => {
                        // A raw string r#"..."# or a plain one.
                        let raw = i > 0 && (b[i - 1] == b'#' || b[i - 1] == b'r');
                        i += 1;
                        while i < b.len() && b[i] != b'"' {
                            if b[i] == b'\\' && !raw {
                                i += 1;
                            }
                            i += 1;
                        }
                    }
                    b'\'' if matches!(b.get(i + 1), Some(b'{') | Some(b'}') | Some(b'"')) && b.get(i + 2) == Some(&b'\'') => i += 2,
                    b';' if depth == 0 => break i,
                    b'{' => depth += 1,
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            break i;
                        }
                    }
                    _ => {}
                }
                i += 1;
            };
            for c in &mut out[start..=end] {
                if *c != b'\n' {
                    *c = b' ';
                }
            }
            from = end + 1;
        }
        String::from_utf8(out).expect("only ASCII bytes were replaced")
    }

    #[test]
    fn production_code_blanks_test_items_only() {
        let src = "fn a() {}\n#[cfg(test)]\nmod t { fn x() { let s = \"}\"; let c = '{'; } // }\n}\nfn b() { f.text() }\n#[cfg(test)]\nuse x::y;\nfn c() {}\n";
        let code = production_code(src);
        assert_eq!(code.lines().count(), src.lines().count());
        assert!(code.contains("fn a() {}") && code.contains("fn b() { f.text() }") && code.contains("fn c() {}"));
        assert!(!code.contains("mod t") && !code.contains("use x::y"));
    }

    /// The `.rs` files of src/ui, relative path with `/`, and their text.
    fn ui_sources() -> Vec<(String, String)> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("ui");
        let mut files = Vec::new();
        sources(&root, &mut files);
        assert!(files.len() > 60, "the ui sources were not found");
        files
            .iter()
            .map(|f| {
                let rel = f.strip_prefix(&root).expect("under src/ui").to_string_lossy().replace('\\', "/");
                (rel, crate::read_source(f))
            })
            .collect()
    }

    #[test]
    fn no_production_code_reads_a_field_through_nwg_text() {
        // nwg's text() turns a half emoji into "Decoding error" (and a failed
        // read into ""), which then became a password, a secret or a name.
        // Every field is read here. The only `.text()` left are the strict
        // RichEdit readers of the editor (traits returning a Result), each
        // allowed in its own file only.
        const STRICT_EDITOR_READERS: [(&str, &str); 6] = [
            ("dialogs/regex_search.rs", "target.text()"),
            ("editor/content.rs", "steps.text()"),
            ("editor/heading_flow.rs", "s.text()"),
            ("editor/indent.rs", "steps.text()"),
            ("editor/line_ops.rs", "s.text()"),
            ("editor/lists.rs", "steps.text()"),
        ];
        let mut found = Vec::new();
        for (rel, src) in ui_sources() {
            let code = production_code(&src);
            for (n, line) in code.lines().enumerate() {
                let mut rest = line;
                while let Some(i) = rest.find(".text()") {
                    let before = &rest[..i];
                    let receiver_start = before.rfind(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.')).map_or(0, |p| p + 1);
                    let call = &rest[receiver_start..i + ".text()".len()];
                    if !STRICT_EDITOR_READERS.contains(&(rel.as_str(), call)) {
                        found.push(format!("{}:{}: {}", rel, n + 1, line.trim()));
                    }
                    rest = &rest[i + ".text()".len()..];
                }
            }
        }
        assert!(found.is_empty(), "fields read with nwg text():\n{}", found.join("\n"));
    }

    #[test]
    fn no_production_code_reads_a_field_with_get_window_text_directly() {
        // A direct GetWindowTextW is allowed only where it reads a caption
        // or a title, never an input field (style.rs skips Edit controls).
        const ALLOWED: [&str; 5] = [
            "editor_text.rs",        // read_units: the strict reader itself
            "dialogs/style.rs",      // window_text: labels, buttons, check boxes
            "editor/events.rs",      // owner-drawn button caption
            "lock_card.rs",          // owner-drawn button caption
            "editor/modal_guard.rs", // dialog window title
        ];
        let mut found = Vec::new();
        for (rel, src) in ui_sources() {
            let code = production_code(&src);
            for (n, line) in code.lines().enumerate() {
                if line.contains("GetWindowTextW(") && !ALLOWED.contains(&rel.as_str()) {
                    found.push(format!("{}:{}: {}", rel, n + 1, line.trim()));
                }
            }
        }
        assert!(found.is_empty(), "direct GetWindowTextW reads:\n{}", found.join("\n"));
    }

    #[test]
    fn the_dialog_style_pass_never_reads_an_input_field() {
        // It used to copy every child's text, a pre-filled vault secret
        // included, into an unwiped String.
        let src = crate::read_source(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ui/dialogs/style.rs"));
        let code = production_code(&src);
        assert!(code.contains("text: if matches!(class, Class::Edit { .. }) { String::new() } else { window_text(h) },"));
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
        check(
            "dialogs/backup_password.rs",
            "if handle == btn_ok.handle {",
            &["read_masked(txt_password.handle", "pw.is_empty()", "*result_clone.borrow_mut() = Some(pw)"],
        );
        check(
            "dialogs/ssh_keygen.rs",
            "nwg::Event::OnButtonClick if handle == btn_load.handle => {",
            &["read_masked(txt_pass.handle", "*result_c.borrow_mut() = Some((i, pass))"],
        );
        check(
            "dialogs/password_generator.rs",
            "} else if handle == form.btn_copy.handle {",
            &["read_masked(form.txt_password.handle", "value.is_empty()", "clipboard_win::set_clipboard("],
        );
        check(
            "dialogs/password_generator.rs",
            "} else if for_field && handle == btn_use.handle {",
            &["read_masked(form.txt_password.handle", "value.is_empty()", "*used_h.borrow_mut() = Some(value)"],
        );
        check(
            "dialogs/totp_setup.rs",
            "} else if handle == btn_verify.handle {",
            &["read_visible(txt_code.handle", "read_visible(txt_secret.handle", "totp::verify("],
        );
        check(
            "dialogs/totp_setup.rs",
            "} else if handle == btn_confirm.handle && add {",
            &["read_visible(txt_secret.handle", "*result_clone.borrow_mut() = Some(TotpEntry"],
        );
        // secret_edit Show/Hide and OK: live sequence tests in secret_edit.rs.
    }

    #[test]
    fn a_field_read_while_typing_waits_for_a_whole_emoji_and_never_drops_a_failure_silently() {
        // No window: an error, never an empty value (logged at WARN).
        assert!(read_typing(nwg::ControlHandle::NoHandle, "test").is_none());
        let w = window();
        let (plain, _, _) = inputs(&w);
        // Half of an emoji typed as two WM_CHAR: nothing yet, quietly.
        set_units(hwnd(plain.handle), &[0x61, 0xD83D]);
        assert!(read_typing(plain.handle, "test").is_none());
        assert_eq!(read_units(hwnd(plain.handle)).expect("kept").as_slice(), &[0x61, 0xD83D], "nothing cleared while typing");
        // The second half arrives: the whole text.
        set_units(hwnd(plain.handle), &[0x61, 0xD83D, 0xDE00]);
        assert_eq!(read_typing(plain.handle, "test").expect("valid").as_str(), "a\u{1F600}");
    }

    #[test]
    fn typing_handlers_use_the_typing_reader_not_a_bare_let_ok() {
        // `let Ok(..) = read(..) else` drops NoWindow and ReadFailed silently.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("ui");
        for (file, field) in [
            ("dialogs/create_password.rs", "read_typing(txt_password.handle"),
            ("dialogs/ssh_keygen.rs", "read_typing(txt_pass.handle"),
            ("dialogs/totp_setup.rs", "read_typing(txt_secret.handle"),
        ] {
            let src = crate::read_source(root.join(file));
            assert!(src.contains(field), "{file}: {field} missing");
            assert!(
                !src.contains("let Ok(pw) = crate::ui::field_text::read(") && !src.contains("let Ok(typed) = crate::ui::field_text::read("),
                "{file}: a bare let Ok read is back"
            );
        }
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
