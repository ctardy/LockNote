// Reading the text of the note editor (a RichEdit) without nwg's decoding.
//
// nwg's `RichTextBox::text()` decodes WM_GETTEXT with
// `OsString::into_string().unwrap_or("Decoding error")` (nwg 1.0.13,
// base_helper.rs `from_utf16`): a note holding an unpaired surrogate came
// back as the literal "Decoding error", which a save or a tab switch then
// stored in place of the note. RichEdit does keep a lone surrogate (measured
// 2026-11-28 on Windows 10.0.26200: WM_SETTEXT, EM_REPLACESEL and WM_CHAR
// all store one; typing an emoji as two WM_CHAR passes through that state).
//
// A note is a Rust `String`, saved as UTF-8 in a frozen format: a lone
// surrogate cannot be stored without loss. So there are two readers:
//
// - `read_strict`: for everything that stores, exports, encrypts or moves
//   the text, or writes part of it back into the editor. Invalid UTF-16 is
//   an error (`EditorTextError::InvalidUtf16`); the caller refuses the action
//   and tells the user.
// - `read_display`: for what only shows something or computes positions
//   (status bar, gutter, highlights, caret moves, search results, print).
//   Each lone surrogate becomes one U+FFFD, which is also one UTF-16 unit,
//   so every position still matches the RichEdit's. Its text must never be
//   stored nor written back into the note.
//
// Both read with GetWindowTextLengthW + GetWindowTextW, as nwg did ("\r\n"
// line breaks), into a buffer wiped on drop: it holds the plaintext.

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{GetWindowTextLengthW, GetWindowTextW};
use zeroize::Zeroizing;

/// Why the editor text was not returned. Carries no note content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorTextError {
    /// The editor control has no window handle.
    NoWindow,
    /// GetWindowTextW copied nothing from a non-empty control, or more units
    /// than GetWindowTextLengthW announced (the text may be cut).
    ReadFailed { expected: usize, copied: usize },
    /// The text holds unpaired surrogates. `unit` is the index of the first
    /// one in the WM_GETTEXT text ("\r\n" breaks, not a RichEdit position),
    /// `line` its 1-based line, `count` how many there are in all.
    InvalidUtf16 { unit: usize, line: usize, count: usize },
}

fn t(key: &str) -> String {
    crate::features::i18n::get(key).to_string()
}

impl EditorTextError {
    /// Title of the message box telling the user.
    pub fn title() -> String {
        t("editor.text.title")
    }

    /// The message for a dialog: what is wrong, that nothing was written,
    /// what to do.
    pub fn user_message(&self) -> String {
        match self {
            EditorTextError::InvalidUtf16 { line, count, .. } => t("editor.text.invalid")
                .replace("{line}", &line.to_string())
                .replace("{count}", &count.to_string()),
            EditorTextError::NoWindow | EditorTextError::ReadFailed { .. } => t("editor.text.unreadable"),
        }
    }

    /// One line, for a status label (the Find dialog).
    pub fn short_message(&self) -> String {
        match self {
            EditorTextError::InvalidUtf16 { line, .. } => {
                t("editor.text.invalid.short").replace("{line}", &line.to_string())
            }
            EditorTextError::NoWindow | EditorTextError::ReadFailed { .. } => t("editor.text.unreadable.short"),
        }
    }
}

/// The UTF-16 units of the control's text, "\r\n" line breaks.
pub fn read_units(hwnd: HWND) -> Result<Zeroizing<Vec<u16>>, EditorTextError> {
    if hwnd.is_invalid() {
        return Err(EditorTextError::NoWindow);
    }
    let expected = unsafe { GetWindowTextLengthW(hwnd) }.max(0) as usize;
    // One unit more than announced: a copy that fills it means the length
    // was short and the text may be cut.
    let mut buf = Zeroizing::new(vec![0u16; expected + 2]);
    let copied = unsafe { GetWindowTextW(hwnd, &mut buf) }.max(0) as usize;
    if (copied == 0 && expected > 0) || copied > expected {
        return Err(EditorTextError::ReadFailed { expected, copied });
    }
    buf.truncate(copied);
    Ok(buf)
}

/// `units` as a `String`, or where its first unpaired surrogate is.
pub fn decode_strict(units: &[u16]) -> Result<String, EditorTextError> {
    String::from_utf16(units).map_err(|_| {
        let lone = lone_surrogates(units);
        let unit = lone.first().copied().unwrap_or(units.len());
        let line = 1 + units[..unit].iter().filter(|&&u| u == u16::from(b'\n')).count();
        EditorTextError::InvalidUtf16 { unit, line, count: lone.len() }
    })
}

/// `units` with each unpaired surrogate replaced by U+FFFD: one unit for
/// one unit, so positions are kept. For display and positions only.
pub fn decode_display(units: &[u16]) -> String {
    String::from_utf16_lossy(units)
}

/// Indexes of the unpaired surrogates of `units`.
pub fn lone_surrogates(units: &[u16]) -> Vec<usize> {
    let mut lone = Vec::new();
    let mut i = 0;
    while i < units.len() {
        let u = units[i];
        if (0xD800..0xDC00).contains(&u) && units.get(i + 1).is_some_and(|n| (0xDC00..0xE000).contains(n)) {
            i += 2;
            continue;
        }
        if (0xD800..0xE000).contains(&u) {
            lone.push(i);
        }
        i += 1;
    }
    lone
}

/// The editor text, refused when it is not valid UTF-16. For every path
/// that stores, exports or writes the text back.
pub fn read_strict(hwnd: HWND) -> Result<String, EditorTextError> {
    decode_strict(&read_units(hwnd)?)
}

/// The editor text for display and positions (see `decode_display`).
pub fn read_display(hwnd: HWND) -> Result<String, EditorTextError> {
    Ok(decode_display(&read_units(hwnd)?))
}

/// The HWND of an nwg control (an invalid one when it has none).
pub fn hwnd_of(handle: native_windows_gui::ControlHandle) -> HWND {
    match handle {
        native_windows_gui::ControlHandle::Hwnd(h) => HWND(h as *mut _),
        _ => HWND(std::ptr::null_mut()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn units(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    #[test]
    fn valid_text_decodes_exactly_pairs_included() {
        let u = units("a\u{1F600}\r\nb");
        assert_eq!(decode_strict(&u), Ok("a\u{1F600}\r\nb".to_string()));
        assert!(lone_surrogates(&u).is_empty());
    }

    #[test]
    fn a_lone_high_or_low_surrogate_is_refused_with_its_place() {
        let mut u = units("l1\r\nl2");
        u.push(0xD83D);
        u.extend(units("x\r\n"));
        u.push(0xDE00);
        assert_eq!(decode_strict(&u), Err(EditorTextError::InvalidUtf16 { unit: 6, line: 2, count: 2 }));
        // A low half first, then a high half at the end.
        assert_eq!(
            decode_strict(&[0xDE00, 0x61, 0xD83D]),
            Err(EditorTextError::InvalidUtf16 { unit: 0, line: 1, count: 2 })
        );
        // Two high halves in a row: both lone.
        assert_eq!(lone_surrogates(&[0xD83D, 0xD83D, 0xDE00]), [0]);
    }

    #[test]
    fn the_substitute_string_never_comes_back() {
        // nwg returned this literal for any invalid text.
        let err = decode_strict(&[0x61, 0xD83D]).unwrap_err();
        assert!(matches!(err, EditorTextError::InvalidUtf16 { .. }));
        assert_ne!(decode_display(&[0x61, 0xD83D]), "Decoding error");
    }

    #[test]
    fn display_keeps_one_unit_per_unit() {
        let u = [0x61, 0xD83D, 0x0D, 0x0A, 0xDE00, 0xD83D, 0xDE00];
        let s = decode_display(&u);
        assert_eq!(s, "a\u{FFFD}\r\n\u{FFFD}\u{1F600}");
        assert_eq!(s.encode_utf16().count(), u.len(), "positions must not move");
    }

    #[test]
    fn messages_name_the_line_and_never_hold_text() {
        let _lang = crate::features::i18n::test_support::lang_guard();
        for lang in [
            crate::features::i18n::Language::English,
            crate::features::i18n::Language::French,
            crate::features::i18n::Language::Spanish,
            crate::features::i18n::Language::German,
        ] {
            crate::features::i18n::set_language(lang);
            let e = EditorTextError::InvalidUtf16 { unit: 9, line: 3, count: 2 };
            for m in [e.user_message(), e.short_message()] {
                assert!(m.contains('3') && !m.contains("{line}") && !m.contains("{count}"), "{lang:?}: {m}");
            }
            assert!(e.user_message().contains('2'), "{lang:?}: the count is missing");
            let r = EditorTextError::ReadFailed { expected: 4, copied: 0 };
            assert!(!r.user_message().is_empty() && !r.short_message().is_empty());
            assert!(!EditorTextError::title().starts_with("editor."), "{lang:?}: untranslated title");
        }
    }

    #[test]
    fn no_window_is_an_error_not_an_empty_note() {
        assert_eq!(read_strict(HWND(std::ptr::null_mut())), Err(EditorTextError::NoWindow));
        assert_eq!(read_display(HWND(std::ptr::null_mut())), Err(EditorTextError::NoWindow));
    }
}
