// The lines of a text as the editor shows them: the one definition of a
// line break shared by every command that works on lines (Ctrl+D,
// Ctrl+Shift+K, Alt+Up/Down, Edit > Lines, Ctrl+G, the status bar, Tab
// indent, lists, Go to Heading, Search All, Print).
//
// RichEdit (the text read back by WM_GETTEXT) ends a line at "\r\n", and
// also at a form feed U+000C, which it keeps in the text and shows as its
// own line break (V2). "\f\r\n" is two breaks. A lone "\r" or "\n" is a
// break too: WM_GETTEXT never returns one, but texts that did not come
// from the editor (a tab never shown, a test) may hold them. Vertical tab,
// U+2028 and U+2029 are read back as "\r\n" and so never appear; NEL
// (U+0085) ends no line. The live oracle is
// `ui::editor::line_ops::tests::live::live_line_ranges_match_richedit_lines`.
//
// Pure text work: no EM_* call, so word wrap (display lines) never changes
// what a line is. What is saved is unaffected: the session's caret line is
// still counted on '\n' (see `ui::editor::edit_ops::session_line_position`).

/// The form feed, a line break for RichEdit.
pub const FORM_FEED: char = '\u{c}';

/// A character that ends a line: CR, LF or form feed ("\r\n" is one break).
pub fn is_break(c: char) -> bool {
    c == '\r' || c == '\n' || c == FORM_FEED
}

/// Length in bytes of the line break starting at byte `i` of `bytes`: 2 for
/// "\r\n", 1 for a lone CR, LF or form feed, 0 when none starts there.
pub fn break_len_at(bytes: &[u8], i: usize) -> usize {
    match bytes.get(i) {
        Some(b'\r') if bytes.get(i + 1) == Some(&b'\n') => 2,
        Some(b'\r' | b'\n' | 0x0C) => 1,
        _ => 0,
    }
}

/// Byte range of every line of `text`, break excluded. Always at least
/// one (empty) line; a final break is followed by an empty line, as
/// RichEdit counts it.
pub fn line_ranges(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut lines = Vec::new();
    let (mut start, mut i) = (0, 0);
    while i < bytes.len() {
        match break_len_at(bytes, i) {
            0 => i += 1,
            n => {
                lines.push((start, i));
                i += n;
                start = i;
            }
        }
    }
    lines.push((start, bytes.len()));
    lines
}

/// Line breaks in `bytes` ("\r\n" counting once). A slice ending between
/// the CR and the LF of a "\r\n" counts that CR.
pub fn break_count(bytes: &[u8]) -> usize {
    let (mut n, mut i) = (0, 0);
    while i < bytes.len() {
        match break_len_at(bytes, i) {
            0 => i += 1,
            len => {
                n += 1;
                i += len;
            }
        }
    }
    n
}

/// Byte offset of the start of the line holding byte `at` (a char boundary).
pub fn line_start_at(text: &str, at: usize) -> usize {
    text[..at].rfind(is_break).map_or(0, |p| p + 1)
}

/// Byte offset of the end of the line holding byte `at`, before its break.
pub fn line_end_at(text: &str, at: usize) -> usize {
    text[at..].find(is_break).map_or(text.len(), |p| at + p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crlf_cr_lf_and_form_feed_each_end_a_line() {
        assert_eq!(line_ranges(""), vec![(0, 0)]);
        assert_eq!(line_ranges("a\r\nb\rc\nd\u{c}e"), vec![(0, 1), (3, 4), (5, 6), (7, 8), (9, 10)]);
        assert_eq!(line_ranges("a\u{c}"), vec![(0, 1), (2, 2)]);
    }

    #[test]
    fn a_form_feed_then_crlf_is_two_breaks() {
        assert_eq!(line_ranges("a\u{c}\r\nb"), vec![(0, 1), (2, 2), (4, 5)]);
        assert_eq!(break_count(b"a\x0C\r\nb"), 2);
    }

    #[test]
    fn nel_vertical_tab_and_separators_end_no_line() {
        // VT, LS and PS never reach the text (RichEdit reads them back as
        // CRLF); NEL stays and ends no line.
        for t in ["a\u{85}b", "a\u{b}b", "a\u{2028}b", "a\u{2029}b"] {
            assert_eq!(line_ranges(t), vec![(0, t.len())], "{t:?}");
            assert_eq!(break_count(t.as_bytes()), 0, "{t:?}");
        }
    }

    #[test]
    fn break_count_counts_crlf_once() {
        assert_eq!(break_count(b""), 0);
        assert_eq!(break_count(b"a\r\nb\rc\nd\x0Ce"), 4);
        assert_eq!(break_count(b"a\r"), 1);
    }

    #[test]
    fn line_start_and_end_around_any_break() {
        let t = "ab\u{c}cd\r\nef";
        assert_eq!((line_start_at(t, 0), line_end_at(t, 0)), (0, 2));
        assert_eq!((line_start_at(t, 4), line_end_at(t, 4)), (3, 5));
        assert_eq!((line_start_at(t, 8), line_end_at(t, 8)), (7, 9));
        // At a break: the line it ends.
        assert_eq!((line_start_at(t, 2), line_end_at(t, 2)), (0, 2));
    }

    #[test]
    fn break_len_at_each_kind() {
        let b = b"\r\n\r\n\x0Cx";
        assert_eq!(break_len_at(b, 0), 2);
        assert_eq!(break_len_at(b, 1), 1);
        assert_eq!(break_len_at(b, 4), 1);
        assert_eq!(break_len_at(b, 5), 0);
        assert_eq!(break_len_at(b, 6), 0);
    }
}
