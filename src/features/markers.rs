// Markers — the reserved markers of the note payload, and the one escaping
// shared by every block (tabs, history, prompts, vault, attachments, session
// state).
//
// The payload is a stack of text blocks (see `features::payload`). User text sits
// inside some of them (a tab, a prompt, a vault value, an old snapshot), so two rules
// keep text and structure apart (issue 050):
//
//   * Writing: a line of user text that starts with ANY reserved marker gets
//     a `\x00` prefix, whatever block it is written in. Before, each block
//     escaped its own markers only, and a tab holding `[LOCKNOTE_VAULT]` was
//     cut there on reload.
//   * Reading: a block header or footer is recognized only when it is a whole
//     line of its own. Inside a line, or behind `\x00`, it is text.
//
// A line is what `\n` separates; a `\r` before the `\n` (text coming from the
// editor) belongs to the line and is ignored when comparing with a marker.

pub const TABS_HEADER: &str = "[LOCKNOTE_TABS]";
pub const TABS_FOOTER: &str = "[/LOCKNOTE_TABS]";
pub const TAB_PREFIX: &str = "[TAB:";

pub const HISTORY_HEADER: &str = "[LOCKNOTE_HISTORY]";
pub const HISTORY_FOOTER: &str = "[/LOCKNOTE_HISTORY]";
pub const SNAP_PREFIX: &str = "[SNAP:";
pub const SNAP_END: &str = "[/SNAP]";

pub const PROMPTS_HEADER: &str = "[LOCKNOTE_PROMPTS]";
pub const PROMPTS_FOOTER: &str = "[/LOCKNOTE_PROMPTS]";
pub const PROMPT_PREFIX: &str = "[PROMPT:";
pub const PROMPT_END: &str = "[/PROMPT]";

pub const VAULT_HEADER: &str = "[LOCKNOTE_VAULT]";
pub const VAULT_FOOTER: &str = "[/LOCKNOTE_VAULT]";
pub const SECRET_PREFIX: &str = "[SECRET:";
pub const SECRET_END: &str = "[/SECRET]";

pub const ATTACH_HEADER: &str = "[LOCKNOTE_ATTACHMENTS]";
pub const ATTACH_FOOTER: &str = "[/LOCKNOTE_ATTACHMENTS]";
pub const FILE_PREFIX: &str = "[FILE:";
pub const FILE_END: &str = "[/FILE]";

pub const STATE_HEADER: &str = "[LOCKNOTE_STATE]";
pub const STATE_FOOTER: &str = "[/LOCKNOTE_STATE]";

/// Every marker a line of user text must not start with. A new block adds
/// its markers here, and nowhere else.
pub const RESERVED: [&str; 21] = [
    TABS_HEADER,
    TABS_FOOTER,
    TAB_PREFIX,
    HISTORY_HEADER,
    HISTORY_FOOTER,
    SNAP_PREFIX,
    SNAP_END,
    PROMPTS_HEADER,
    PROMPTS_FOOTER,
    PROMPT_PREFIX,
    PROMPT_END,
    VAULT_HEADER,
    VAULT_FOOTER,
    SECRET_PREFIX,
    SECRET_END,
    ATTACH_HEADER,
    ATTACH_FOOTER,
    FILE_PREFIX,
    FILE_END,
    STATE_HEADER,
    STATE_FOOTER,
];

/// True when `line` starts with a reserved marker, leading whitespace
/// ignored (the block parsers ignore it too for their inner markers).
fn starts_with_reserved(line: &str) -> bool {
    let trimmed = line.trim_start();
    RESERVED.iter().any(|m| trimmed.starts_with(m))
}

/// Escape user text before it is written inside a block: every line starting
/// with a reserved marker gets a `\x00` prefix.
pub fn escape_content(content: &str) -> String {
    let mut out = String::with_capacity(content.len());
    for line in content.split('\n') {
        if starts_with_reserved(line) {
            out.push('\x00');
        }
        out.push_str(line);
        out.push('\n');
    }
    // Remove trailing newline we added
    if out.ends_with('\n') && !content.ends_with('\n') {
        out.pop();
    }
    out
}

/// Undo `escape_content`: remove the `\x00` prefix of the lines that have one.
pub fn unescape_content(content: &str) -> String {
    let mut out = String::with_capacity(content.len());
    for line in content.split('\n') {
        if let Some(rest) = line.strip_prefix('\x00') {
            out.push_str(rest);
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    // Remove trailing newline we added
    if out.ends_with('\n') && !content.ends_with('\n') {
        out.pop();
    }
    out
}

/// What bounds a line when a marker line is looked for.
#[derive(Clone, Copy)]
enum LineBounds {
    /// The readers' rule: a line starts after `\n` and ends before `\n`, an
    /// optional `\r` before that `\n` being part of the line end.
    Lf,
    /// `\r` alone bounds a line too. Wider than what any reader recognizes,
    /// on purpose: see `find_loose_marker_line`.
    LfOrCr,
}

/// True when the line starting at byte `pos` of `input` is exactly `marker`,
/// a line being what `bounds` says.
fn is_marker_line_at(input: &str, pos: usize, marker: &str, bounds: LineBounds) -> bool {
    let before = if pos == 0 { None } else { Some(input.as_bytes()[pos - 1]) };
    let after = &input[pos + marker.len()..];
    match bounds {
        LineBounds::Lf => {
            if !matches!(before, None | Some(b'\n')) {
                return false;
            }
            let rest_of_line = match after.find('\n') {
                Some(end) => &after[..end],
                None => after,
            };
            rest_of_line.is_empty() || rest_of_line == "\r"
        }
        LineBounds::LfOrCr => {
            matches!(before, None | Some(b'\n') | Some(b'\r'))
                && matches!(after.as_bytes().first(), None | Some(b'\n') | Some(b'\r'))
        }
    }
}

fn find_line(input: &str, marker: &str, from: usize, bounds: LineBounds) -> Option<usize> {
    let mut search = from;
    while let Some(rel) = input[search..].find(marker) {
        let pos = search + rel;
        if is_marker_line_at(input, pos, marker, bounds) {
            return Some(pos);
        }
        search = pos + marker.len();
    }
    None
}

fn rfind_line(input: &str, marker: &str, bounds: LineBounds) -> Option<usize> {
    let mut end = input.len();
    while let Some(pos) = input[..end].rfind(marker) {
        if is_marker_line_at(input, pos, marker, bounds) {
            return Some(pos);
        }
        end = pos;
    }
    None
}

/// Byte position of the first line of `input`, at or after `from`, that is
/// exactly the block header or footer `marker` (an optional `\r` before the
/// line end is accepted). An occurrence inside a line, or escaped with
/// `\x00`, is text and is skipped. This is what the readers use.
pub fn find_marker_line(input: &str, marker: &str, from: usize) -> Option<usize> {
    find_line(input, marker, from, LineBounds::Lf)
}

/// Like `find_marker_line`, for the last such line of `input`.
pub fn rfind_marker_line(input: &str, marker: &str) -> Option<usize> {
    rfind_line(input, marker, LineBounds::Lf)
}

/// `find_marker_line` with `\r` alone counted as a line bound as well: the
/// marker sits between the start of the text, `\n` or `\r`, and the end of
/// the text, `\n` or `\r`. Every line `find_marker_line` finds is found, plus
/// the ones the reading chain can turn into a marker line before a reader
/// sees the text: `Settings::parse` drops the `\r` / `\n` opening the note
/// text, and each block reader trims `\n` then `\r` from the end of what
/// precedes its block. Not for the readers: for the decision, when writing,
/// that a plain text is safe to leave unescaped (`payload::compose`).
pub fn find_loose_marker_line(input: &str, marker: &str, from: usize) -> Option<usize> {
    find_line(input, marker, from, LineBounds::LfOrCr)
}

/// Like `find_loose_marker_line`, for the last such place of `input`.
pub fn rfind_loose_marker_line(input: &str, marker: &str) -> Option<usize> {
    rfind_line(input, marker, LineBounds::LfOrCr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_list_has_no_duplicate() {
        for (i, a) in RESERVED.iter().enumerate() {
            for b in &RESERVED[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn escape_prefixes_every_reserved_line_and_only_those() {
        for m in RESERVED {
            let text = format!("a\n{}\n  {}\n{}tail\r\nvoir {} ici\n-{}", m, m, m, m, m);
            let expected = format!("a\n\x00{}\n\x00  {}\n\x00{}tail\r\nvoir {} ici\n-{}", m, m, m, m, m);
            assert_eq!(escape_content(&text), expected, "{}", m);
            assert_eq!(unescape_content(&expected), text, "{}", m);
        }
    }

    #[test]
    fn escape_leaves_other_text_alone() {
        for text in ["", "plain", "a\r\nb", "[TAB", "[LOCKNOTE_SETTINGS]", "[/LOCKNOTE]", "x\n\n\ny"] {
            assert_eq!(escape_content(text), text, "{:?}", text);
            assert_eq!(unescape_content(text), text, "{:?}", text);
        }
    }

    // Both functions are the ones every block had before issue 050, byte for
    // byte: a text ending with a line break gets one more. The block parsers
    // count on it (`strip_trailing_newline`), and the files of the previous
    // versions were written that way, so it must not be "fixed" here.
    #[test]
    fn trailing_line_break_behaviour_is_the_historical_one() {
        assert_eq!(escape_content("a\n"), "a\n\n");
        assert_eq!(unescape_content("a\n"), "a\n\n");
        assert_eq!(escape_content("\n"), "\n\n");
    }

    #[test]
    fn escaped_text_never_holds_a_marker_line() {
        for m in RESERVED {
            for text in [
                m.to_string(),
                format!("{}\r", m),
                format!("a\n{}\nb", m),
                format!("a\r\n{}\r\nb", m),
                format!("{}\n{}\n{}", m, m, m),
            ] {
                let escaped = escape_content(&text);
                for any in RESERVED {
                    assert_eq!(find_marker_line(&escaped, any, 0), None, "{:?} in {:?}", any, escaped);
                    assert_eq!(rfind_marker_line(&escaped, any), None, "{:?} in {:?}", any, escaped);
                }
            }
        }
    }

    #[test]
    fn marker_line_is_a_whole_line() {
        let h = VAULT_HEADER;
        // Recognized: alone on its line, `\r` tolerated.
        assert_eq!(find_marker_line(h, h, 0), Some(0));
        assert_eq!(find_marker_line(&format!("{}\n", h), h, 0), Some(0));
        assert_eq!(find_marker_line(&format!("{}\r\n", h), h, 0), Some(0));
        assert_eq!(find_marker_line(&format!("{}\r", h), h, 0), Some(0));
        assert_eq!(find_marker_line(&format!("ab\n{}\ncd", h), h, 0), Some(3));
        assert_eq!(find_marker_line(&format!("ab\r\n{}\r\ncd", h), h, 0), Some(4));
        // Not recognized: inside a line, escaped, indented, followed by text.
        for text in [
            format!("voir {} ici", h),
            format!("voir {}", h),
            format!("{} ici", h),
            format!("{}x\n", h),
            format!("\x00{}\n", h),
            format!("a\n\x00{}\nb", h),
            format!("a\n {}\nb", h),
            format!("a\r{}\rb", h),
            format!("{}\r\r\n", h),
        ] {
            assert_eq!(find_marker_line(&text, h, 0), None, "{:?}", text);
            assert_eq!(rfind_marker_line(&text, h), None, "{:?}", text);
        }
    }

    #[test]
    fn first_and_last_marker_lines() {
        let h = STATE_HEADER;
        let text = format!("voir {} ici\n{}\nx\n\x00{}\n{}\nfin {}", h, h, h, h, h);
        let first = format!("voir {} ici\n", h).len();
        let last = first + format!("{}\nx\n\x00{}\n", h, h).len();
        assert_eq!(find_marker_line(&text, h, 0), Some(first));
        assert_eq!(find_marker_line(&text, h, first), Some(first));
        assert_eq!(find_marker_line(&text, h, first + 1), Some(last));
        assert_eq!(find_marker_line(&text, h, last + 1), None);
        assert_eq!(rfind_marker_line(&text, h), Some(last));
        // A footer is not found by its header and the other way round.
        assert_eq!(find_marker_line("[/LOCKNOTE_STATE]", STATE_HEADER, 0), None);
        assert_eq!(find_marker_line("", h, 0), None);
    }

    #[test]
    fn loose_marker_line_also_stops_at_a_lone_cr() {
        let h = VAULT_HEADER;
        // Everything the readers' search finds.
        for text in [
            h.to_string(),
            format!("{}\n", h),
            format!("{}\r\n", h),
            format!("{}\r", h),
            format!("ab\n{}\ncd", h),
            format!("ab\r\n{}\r\ncd", h),
        ] {
            let strict = find_marker_line(&text, h, 0);
            assert!(strict.is_some(), "{:?}", text);
            assert_eq!(find_loose_marker_line(&text, h, 0), strict, "{:?}", text);
            assert_eq!(rfind_loose_marker_line(&text, h), strict, "{:?}", text);
        }
        // Only the loose search: a lone `\r` on either side.
        for (text, pos) in [
            (format!("\r{}\r\nb", h), 1),
            (format!("\n\r{}\nb", h), 2),
            (format!("a\r{}\rb", h), 2),
            (format!("a\r\n{}\r\r\n", h), 3),
            (format!("a\r\n{}\r\r", h), 3),
        ] {
            assert_eq!(find_marker_line(&text, h, 0), None, "{:?}", text);
            assert_eq!(find_loose_marker_line(&text, h, 0), Some(pos), "{:?}", text);
            assert_eq!(rfind_loose_marker_line(&text, h), Some(pos), "{:?}", text);
        }
        // Neither: inside a line, escaped, indented, followed by other text.
        for text in [
            format!("voir {} ici", h),
            format!("a\r\n{} \r\n", h),
            format!("a\r\n{}\t\r\n", h),
            format!("a\r\n {}\r\n", h),
            format!("a\r \r{}x", h),
            format!("\x00{}\r\r", h),
            format!("a\n\x00{}\n", h),
        ] {
            assert_eq!(find_loose_marker_line(&text, h, 0), None, "{:?}", text);
            assert_eq!(rfind_loose_marker_line(&text, h), None, "{:?}", text);
        }
    }

    // Lines next to a lone `\r`, once escaped (tabs block, vault value): a
    // line whose first visible character is the marker is escaped even
    // behind a `\r` (`\r` is leading whitespace), so the readers' search
    // finds no marker line. The loose search is not a reader and does not
    // look at `\x00`: it is only run on plain text about to be written.
    #[test]
    fn escaped_lone_cr_lines_hold_no_marker_line_for_the_readers() {
        for m in RESERVED {
            for text in [format!("\r{}\r\nb", m), format!("a\r\n{}\r\r\n", m), format!("{}\r\r", m), format!("\r\r{}\r", m)] {
                let escaped = escape_content(&text);
                assert!(escaped.starts_with('\x00') || escaped.contains("\n\x00"), "{:?}", escaped);
                for any in RESERVED {
                    assert_eq!(find_marker_line(&escaped, any, 0), None, "{:?} in {:?}", any, escaped);
                    assert_eq!(rfind_marker_line(&escaped, any), None, "{:?} in {:?}", any, escaped);
                }
            }
        }
    }

    #[test]
    fn search_is_safe_on_multibyte_text() {
        let text = format!("é漢😀 {}\né\n{}\n😀", TABS_FOOTER, TABS_FOOTER);
        let pos = find_marker_line(&text, TABS_FOOTER, 0).unwrap();
        assert_eq!(&text[pos..pos + TABS_FOOTER.len()], TABS_FOOTER);
        assert_eq!(rfind_marker_line(&text, TABS_FOOTER), Some(pos));
    }
}
