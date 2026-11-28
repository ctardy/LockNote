// Prompts — stored prompts, copied from a side panel (issue 058)
//
// Format:
//   [LOCKNOTE_PROMPTS]
//   [PROMPT:Name:Category]
//   text (several lines)
//   [/PROMPT]
//   [/LOCKNOTE_PROMPTS]
//
// The block sits right after the tabs and before the vault (see
// `features::payload`). A line of a text that starts with a reserved marker of the
// payload (any block, see `features::markers`) is written with a `\x00` prefix.
//
// Recognition (issue 058, pass 2): versions up to 1.2.24 did not reserve
// `[LOCKNOTE_PROMPTS]`, so an old tab may hold that line unescaped anywhere.
// Every payload this version writes ends with the block once the vault (and
// what follows it) is removed, so the block is the LAST header line, its
// footer the first footer line after it, and only whitespace may follow that
// footer: the rule of the session block (`session_state::block_span`).
// Anything else is no block and stays text, nothing is dropped.
//
// The header line is `[PROMPT:` + name + `:` + category + `]`. The category
// is what follows the last `:` and the name what precedes it, so a name may
// hold `:` (and `]`) but a category may not hold `:` (`check_entry`). Unlike
// the vault, categories are free text: there is no list to check them
// against.
//
// The writer is the vault's. The reader is exact where the vault's is not: a
// text keeps its leading empty lines and the line breaks ending it.
//
// Known limit shared with every block: a line of text that already starts
// with `\x00` loses that character on reload (the escape prefix is not
// doubled), see `line_starting_with_nul_loses_it_like_every_block`.

use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::crypto::wiping::WipingString;
use crate::features::markers::{
    find_marker_line, rfind_loose_marker_line, rfind_marker_line, unescape_content, write_escaped, PROMPTS_FOOTER,
    PROMPTS_HEADER, PROMPT_END, PROMPT_PREFIX,
};

/// Largest text of one prompt, in bytes. Refused when typed or imported;
/// also the defensive bound of the reader for a `[PROMPT:…]` without its
/// `[/PROMPT]` (same ceiling as a vault value).
pub const MAX_PROMPT_TEXT: usize = 1024 * 1024;

/// Ceiling of the reader on the escaped text of one prompt: twice the text
/// ceiling, so a text at the ceiling whose lines all got a `\x00` is read.
const MAX_ESCAPED_TEXT: usize = 2 * MAX_PROMPT_TEXT;

/// Category of a prompt created without choosing one (the dialog shows the
/// translated name, `prompts.category.default`).
pub const DEFAULT_CATEGORY: &str = "General";

/// A stored prompt.
///
/// The `text` is wiped when the entry is dropped (memory-dump protection,
/// like a vault value). `name` and `category` are shown in the panel list
/// and are skipped by the zeroizer.
#[derive(Debug, Clone, PartialEq, Zeroize, ZeroizeOnDrop)]
pub struct PromptEntry {
    #[zeroize(skip)]
    pub name: String,
    pub text: String,
    #[zeroize(skip)]
    pub category: String,
}

impl PromptEntry {
    pub fn new(name: &str, text: &str, category: &str) -> Self {
        PromptEntry { name: name.to_string(), text: text.to_string(), category: category.to_string() }
    }
}

/// Why a prompt cannot be stored as it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptError {
    /// Empty name (after trimming).
    EmptyName,
    /// Empty category (after trimming).
    EmptyCategory,
    /// Empty text.
    EmptyText,
    /// The text is over `MAX_PROMPT_TEXT` bytes.
    TextTooLong,
    /// The name holds a line break or NUL: the header line would break.
    NameChar(char),
    /// The category holds `:`, a line break or NUL: the header line could
    /// not be split back into name and category.
    CategoryChar(char),
}

impl std::fmt::Display for PromptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyName => write!(f, "empty prompt name"),
            Self::EmptyCategory => write!(f, "empty prompt category"),
            Self::EmptyText => write!(f, "empty prompt text"),
            Self::TextTooLong => write!(f, "prompt text over {} bytes", MAX_PROMPT_TEXT),
            Self::NameChar(c) => write!(f, "forbidden character {:?} in a prompt name", c),
            Self::CategoryChar(c) => write!(f, "forbidden character {:?} in a prompt category", c),
        }
    }
}

/// What the block needs to round-trip a prompt: a name and a category the
/// header line can carry, a text under the ceiling. Empty values are
/// structurally fine (the dialog refuses them, see `check_new_entry`).
pub fn check_entry(name: &str, text: &str, category: &str) -> Result<(), PromptError> {
    if let Some(c) = name.chars().find(|c| matches!(c, '\n' | '\r' | '\0')) {
        return Err(PromptError::NameChar(c));
    }
    if let Some(c) = category.chars().find(|c| matches!(c, ':' | '\n' | '\r' | '\0')) {
        return Err(PromptError::CategoryChar(c));
    }
    if text.len() > MAX_PROMPT_TEXT {
        return Err(PromptError::TextTooLong);
    }
    Ok(())
}

/// `check_entry` plus what the dialog asks for: a name, a category and a
/// text that are not empty (name and category trimmed).
pub fn check_new_entry(name: &str, text: &str, category: &str) -> Result<(), PromptError> {
    if name.trim().is_empty() {
        return Err(PromptError::EmptyName);
    }
    if category.trim().is_empty() {
        return Err(PromptError::EmptyCategory);
    }
    if text.is_empty() {
        return Err(PromptError::EmptyText);
    }
    check_entry(name, text, category)
}

/// Where the parser sees the prompts block in `input`: byte positions of its
/// header and of its footer. The header is the last line that is the header
/// and nothing else (issue 050 rule), the footer the first such line after
/// it, and only whitespace may follow the footer.
fn block_span(input: &str) -> Option<(usize, usize)> {
    let start = rfind_marker_line(input, PROMPTS_HEADER)?;
    let footer = find_marker_line(input, PROMPTS_FOOTER, start + PROMPTS_HEADER.len())?;
    if !input[footer + PROMPTS_FOOTER.len()..].trim().is_empty() {
        return None;
    }
    Some((start, footer))
}

/// True when `parse` takes the end of `input` for a prompts block, or could
/// once the reading chain has dropped line breaks around it (a header behind
/// a lone `\r` becomes a header line). For the decision made when writing
/// (`payload::compose`), not for reading. Same search as
/// `session_state::may_end_with_block`, with `\r` alone counted as a line
/// bound on both sides: the last footer, only whitespace after it, and a
/// header before it with no other footer line in between. The footer too,
/// unlike the session block: the reader of the block that follows trims the
/// `\n` then the `\r` ending this text, which can turn `[/LOCKNOTE_PROMPTS]\r\r`
/// into a footer line. Every block `parse` finds passes.
pub fn may_end_with_block(input: &str) -> bool {
    let Some(footer) = rfind_loose_marker_line(input, PROMPTS_FOOTER) else {
        return false;
    };
    if !input[footer + PROMPTS_FOOTER.len()..].trim().is_empty() {
        return false;
    }
    let before = &input[..footer];
    let after_previous_footer = match rfind_marker_line(before, PROMPTS_FOOTER) {
        Some(previous) => previous + PROMPTS_FOOTER.len(),
        None => 0,
    };
    match rfind_loose_marker_line(before, PROMPTS_HEADER) {
        Some(header) => header >= after_previous_footer,
        None => false,
    }
}

/// Parse the prompts block ending `input` (see `block_span`). Returns
/// (prompts, what precedes the block); without such a block, no prompt and
/// `input` unchanged.
pub fn parse(input: &str) -> (Vec<PromptEntry>, String) {
    let Some((start, footer)) = block_span(input) else {
        return (Vec::new(), input.to_string());
    };
    // Same trim as the vault reader: the line break `combine` put before the
    // block, and any other ending the text that precedes it.
    let before = input[..start].trim_end_matches('\n').trim_end_matches('\r');

    let mut prompts: Vec<PromptEntry> = Vec::new();
    // (name, category) of the prompt being read, and its text lines.
    let mut current: Option<(String, String)> = None;
    let mut lines: Vec<&str> = Vec::new();
    let mut escaped_len = 0usize;
    let finish = |current: &mut Option<(String, String)>, lines: &mut Vec<&str>, prompts: &mut Vec<PromptEntry>| {
        if let Some((name, category)) = current.take() {
            // `serialize` ends every text with a line break before
            // `[/PROMPT]`: the text is the lines in between, as they are.
            // Unescaped one line at a time: on a whole text ending with a
            // line break, `unescape_content` adds one (historical behaviour
            // the other blocks rely on). The lines are joined in a buffer
            // that grows without leaving copies, each unescaped line wiped
            // (issue #42).
            let mut text = WipingString::new();
            for (i, l) in lines.iter().enumerate() {
                if i > 0 {
                    text.push('\n');
                }
                text.push_str(&zeroize::Zeroizing::new(unescape_content(l)));
            }
            prompts.push(PromptEntry { name, text: text.into_string(), category });
        }
        lines.clear();
    };

    // The lines between the header line and the footer line: up to the `\n`
    // that opens the footer line (`block_span` found it at a line start).
    for line in input[start + PROMPTS_HEADER.len()..footer - 1].split('\n').skip(1) {
        let trimmed = line.trim_start();
        if trimmed.starts_with(PROMPT_PREFIX) && !line.starts_with('\x00') {
            let head = trimmed.strip_suffix('\r').unwrap_or(trimmed);
            if let Some(inner) = head.strip_suffix(']') {
                finish(&mut current, &mut lines, &mut prompts);
                let inner = &inner[PROMPT_PREFIX.len()..];
                // The category follows the last `:` (it never holds one).
                current = Some(match inner.rfind(':') {
                    Some(colon) => (inner[..colon].to_string(), inner[colon + 1..].to_string()),
                    None => (inner.to_string(), DEFAULT_CATEGORY.to_string()),
                });
                escaped_len = 0;
                continue;
            }
        }
        if trimmed == PROMPT_END {
            finish(&mut current, &mut lines, &mut prompts);
            continue;
        }
        if current.is_some() {
            escaped_len += line.len() + 1;
            if escaped_len > MAX_ESCAPED_TEXT {
                // A `[PROMPT:…]` without its end: drop it and stop here.
                eprintln!(
                    "WARN prompts: an unterminated prompt over {} bytes was dropped kept={}",
                    MAX_ESCAPED_TEXT,
                    prompts.len()
                );
                current = None;
                lines.clear();
                break;
            }
            lines.push(line);
        }
    }
    finish(&mut current, &mut lines, &mut prompts);

    (prompts, before.to_string())
}

/// Serialize prompts into the block (empty string for no prompt).
/// (`payload::compose` writes the block with `write_block`.)
#[cfg(test)]
pub fn serialize(prompts: &[PromptEntry]) -> String {
    let mut out = WipingString::new();
    write_block(&mut out, prompts);
    out.into_string()
}

/// The prompts block, appended to `out` (nothing for no prompt). `out`
/// grows without leaving copies of the texts (issue #42).
pub fn write_block(out: &mut WipingString, prompts: &[PromptEntry]) {
    if prompts.is_empty() {
        return;
    }
    out.push_str(PROMPTS_HEADER);
    out.push('\n');
    for prompt in prompts {
        out.push_str(PROMPT_PREFIX);
        out.push_str(&prompt.name);
        out.push(':');
        out.push_str(&prompt.category);
        out.push_str("]\n");
        // `escape_content` already ends a text ending with `\n` with one
        // more: every text is then followed by exactly one line break.
        write_escaped(out, &prompt.text);
        if !prompt.text.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(PROMPT_END);
        out.push('\n');
    }
    out.push_str(PROMPTS_FOOTER);
}

/// Append the prompts block to `tab_data` (what `payload::compose` writes,
/// for the tests that build a payload block by block).
#[cfg(test)]
pub fn combine(tab_data: &str, prompts: &[PromptEntry]) -> String {
    let block = serialize(prompts);
    if block.is_empty() {
        return tab_data.to_string();
    }
    if tab_data.is_empty() {
        return block;
    }
    format!("{}\n{}", tab_data, block)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(name: &str, text: &str, category: &str) -> PromptEntry {
        PromptEntry::new(name, text, category)
    }

    #[test]
    fn block_format_is_the_documented_one() {
        let out = serialize(&[p("Résumé", "line 1\nline 2", "Writing"), p("Fix", "x", "Code")]);
        assert_eq!(
            out,
            "[LOCKNOTE_PROMPTS]\n[PROMPT:Résumé:Writing]\nline 1\nline 2\n[/PROMPT]\n[PROMPT:Fix:Code]\nx\n[/PROMPT]\n[/LOCKNOTE_PROMPTS]"
        );
    }

    #[test]
    fn empty_list_writes_nothing() {
        assert_eq!(serialize(&[]), "");
        assert_eq!(combine("note", &[]), "note");
        assert_eq!(parse("just a note"), (Vec::new(), "just a note".to_string()));
        assert_eq!(parse(""), (Vec::new(), String::new()));
    }

    #[test]
    fn roundtrip_with_text_before() {
        let prompts = vec![p("A", "alpha", "General"), p("B", "multi\nline\n\ntext", "Code"), p("C", "", "Empty")];
        let combined = combine("[LOCKNOTE_TABS]\n[TAB:x]\ny\n[/LOCKNOTE_TABS]", &prompts);
        let (back, rest) = parse(&combined);
        assert_eq!(back, prompts);
        assert_eq!(rest, "[LOCKNOTE_TABS]\n[TAB:x]\ny\n[/LOCKNOTE_TABS]");
        let (back, rest) = parse(&combine("", &prompts));
        assert_eq!((back, rest), (prompts, String::new()));
    }

    // Texts the edit control produces (CRLF), trailing line breaks, Unicode,
    // tabs and NUL inside a line come back byte for byte.
    #[test]
    fn awkward_texts_roundtrip() {
        for text in [
            "a\r\nb\r\n",
            "a\n",
            "a\n\n",
            "\n",
            "\r\n",
            "\ttab\tindent",
            "nul \0 inside",
            "a\n \0 after a space",
            "é 漢 😀",
            "  [/PROMPT] indented and continued",
            "trailing CR\r",
        ] {
            let prompts = vec![p("first", text, "Cat"), p("second", "kept", "Cat")];
            let (back, rest) = parse(&combine("note", &prompts));
            assert_eq!(back, prompts, "{:?}", text);
            assert_eq!(rest, "note", "{:?}", text);
        }
    }

    // Every reserved marker at the start of a line of a text (the prompt's
    // own markers, the vault header, any block) is escaped and read back.
    #[test]
    fn text_lines_starting_with_any_marker_roundtrip() {
        for m in crate::features::markers::RESERVED {
            for text in [
                m.to_string(),
                format!("before\n{}\nafter", m),
                format!("before\r\n{}\r\nafter", m),
                format!("  {}x", m),
                format!("voir {} ici", m),
            ] {
                let prompts = vec![p("first", &text, "Cat"), p("second", "kept", "Cat")];
                let serialized = serialize(&prompts);
                // Nothing in the block but its own structure is a marker
                // line: one `[/PROMPT]` per prompt, no other marker.
                let body = &serialized[PROMPTS_HEADER.len()..serialized.len() - PROMPTS_FOOTER.len()];
                let end_lines = body.split('\n').filter(|l| *l == PROMPT_END).count();
                assert_eq!(end_lines, prompts.len(), "{:?}", serialized);
                for any in crate::features::markers::RESERVED.iter().filter(|m| **m != PROMPT_END) {
                    assert_eq!(find_marker_line(body, any, 0), None, "{:?} in {:?}", any, serialized);
                }
                let (back, rest) = parse(&combine("note", &prompts));
                assert_eq!(back, prompts, "{:?}", text);
                assert_eq!(rest, "note");
            }
        }
    }

    // Decision on `:` (same reading as `[SECRET:Name:Category]`): the
    // category is after the LAST `:`, so a name keeps its `:`; a category
    // with `:` is refused before it is written.
    #[test]
    fn colon_in_a_name_roundtrips_and_in_a_category_is_refused() {
        for name in ["host:port", "a:b:c", "ends with:", ":starts", "::"] {
            let prompts = vec![p(name, "t", "Cat")];
            assert_eq!(check_entry(name, "t", "Cat"), Ok(()));
            assert_eq!(parse(&serialize(&prompts)).0, prompts, "{:?}", name);
        }
        assert_eq!(check_entry("n", "t", "a:b"), Err(PromptError::CategoryChar(':')));
        assert_eq!(check_new_entry("n", "t", "a:b"), Err(PromptError::CategoryChar(':')));
    }

    #[test]
    fn brackets_in_name_and_category_roundtrip() {
        for (name, cat) in [("a]b", "c"), ("[x]", "[y]"), ("]", "]"), ("n", "c]d")] {
            let prompts = vec![p(name, "t", cat)];
            assert_eq!(check_entry(name, "t", cat), Ok(()));
            assert_eq!(parse(&serialize(&prompts)).0, prompts, "{:?} {:?}", name, cat);
        }
    }

    #[test]
    fn names_and_categories_that_would_break_the_header_are_refused() {
        for c in ['\n', '\r', '\0'] {
            assert_eq!(check_entry(&format!("a{}b", c), "t", "Cat"), Err(PromptError::NameChar(c)));
            assert_eq!(check_entry("n", "t", &format!("a{}b", c)), Err(PromptError::CategoryChar(c)));
        }
    }

    #[test]
    fn dialog_rules_refuse_empty_fields_explicitly() {
        assert_eq!(check_new_entry("", "t", "Cat"), Err(PromptError::EmptyName));
        assert_eq!(check_new_entry("   ", "t", "Cat"), Err(PromptError::EmptyName));
        assert_eq!(check_new_entry("n", "t", " "), Err(PromptError::EmptyCategory));
        assert_eq!(check_new_entry("n", "", "Cat"), Err(PromptError::EmptyText));
        assert_eq!(check_new_entry("n", "t", "Cat"), Ok(()));
        // Structure only: empty is fine (a backup may hold it).
        assert_eq!(check_entry("", "", ""), Ok(()));
    }

    #[test]
    fn text_over_the_ceiling_is_refused() {
        let at = "x".repeat(MAX_PROMPT_TEXT);
        assert_eq!(check_entry("n", &at, "c"), Ok(()));
        let over = "x".repeat(MAX_PROMPT_TEXT + 1);
        assert_eq!(check_entry("n", &over, "c"), Err(PromptError::TextTooLong));
        assert_eq!(check_new_entry("n", &over, "c"), Err(PromptError::TextTooLong));
    }

    // A text at the ceiling whose every line is escaped is still read back.
    #[test]
    fn text_at_the_ceiling_with_escapes_roundtrips() {
        let line = "[/PROMPT]\n";
        let mut text = line.repeat(MAX_PROMPT_TEXT / line.len());
        text.push_str(&"z".repeat(MAX_PROMPT_TEXT - text.len()));
        assert_eq!(check_entry("n", &text, "c"), Ok(()));
        let prompts = vec![p("n", &text, "c")];
        assert_eq!(parse(&serialize(&prompts)).0, prompts);
    }

    #[test]
    fn large_text_under_the_ceiling_roundtrips() {
        let big = "ligne [/PROMPT]\n".repeat(60_000);
        assert!(big.len() < MAX_PROMPT_TEXT);
        let prompts = vec![p("big", &big, "c"), p("after", "kept", "c")];
        assert_eq!(parse(&serialize(&prompts)).0, prompts);
    }

    // A malformed block (no `[/PROMPT]`, endless text) stops at the reader's
    // bound instead of growing without limit; the prompts read before are
    // kept. The bound leaves room for the `\x00` escapes of a text at the
    // ceiling (`text_at_the_ceiling_with_escapes_roundtrips`).
    #[test]
    fn unterminated_endless_text_stops_at_the_reader_bound() {
        let mut input = String::from("note\n[LOCKNOTE_PROMPTS]\n[PROMPT:ok:c]\nfine\n[/PROMPT]\n[PROMPT:bad:c]\n");
        input.push_str(&"y\n".repeat(MAX_PROMPT_TEXT + 10));
        input.push_str("[/LOCKNOTE_PROMPTS]");
        let (back, rest) = parse(&input);
        assert_eq!(back, vec![p("ok", "fine", "c")]);
        assert_eq!(rest, "note");
    }

    // Inside a whole block, a prompt without its `[/PROMPT]` ends at the
    // footer. A block without its footer is no block: the text stays text,
    // nothing is dropped.
    #[test]
    fn unclosed_prompt_is_kept_unclosed_block_is_text() {
        let (back, _) = parse("[LOCKNOTE_PROMPTS]\n[PROMPT:a:c]\ntext\n[/LOCKNOTE_PROMPTS]");
        assert_eq!(back, vec![p("a", "text", "c")]);
        let text = "note\n[LOCKNOTE_PROMPTS]\n[PROMPT:a:c]\ntext";
        assert_eq!(parse(text), (Vec::new(), text.to_string()));
    }

    // Issue 058, pass 2: 1.2.24 did not reserve `[LOCKNOTE_PROMPTS]`, so an
    // old note may hold that line unescaped anywhere. Every file this
    // version writes has the block at the very end of what is left once the
    // vault is removed: the block is the LAST header line, and its footer
    // ends the text (line breaks or spaces only after it).
    #[test]
    fn a_block_must_end_the_text() {
        let whole = "[LOCKNOTE_PROMPTS]\n[PROMPT:x:c]\nt\n[/PROMPT]\n[/LOCKNOTE_PROMPTS]";
        for text in [
            "before\n[LOCKNOTE_PROMPTS]\nafter".to_string(),
            "before\n[LOCKNOTE_PROMPTS]".to_string(),
            format!("before\n{}\nafter", whole),
            format!("before\r\n{}\r\nmore\r\n", whole.replace('\n', "\r\n")),
            "before\n[/LOCKNOTE_PROMPTS]".to_string(),
            "[LOCKNOTE_TABS]\n[TAB:A]\nx\n[LOCKNOTE_PROMPTS]\ny\n[TAB:B]\nz\n[/LOCKNOTE_TABS]".to_string(),
        ] {
            assert_eq!(parse(&text), (Vec::new(), text.clone()), "{:?}", text);
        }
        // Line breaks after the footer are tolerated.
        for tail in ["\n", "\r\n", "\n\n", "\r\n \t\r\n"] {
            let (back, rest) = parse(&format!("before\n{}{}", whole, tail));
            assert_eq!((back, rest.as_str()), (vec![p("x", "t", "c")], "before"), "{:?}", tail);
        }
    }

    // A stray header line left in the text by an old version is text; the
    // real block, after it, is read.
    #[test]
    fn the_last_header_line_is_the_block() {
        let prompts = vec![p("real", "kept", "c")];
        let old_text = "a\n[LOCKNOTE_PROMPTS]\nold text\n[/LOCKNOTE_PROMPTS]\nmore";
        let (back, rest) = parse(&combine(old_text, &prompts));
        assert_eq!(back, prompts);
        assert_eq!(rest, old_text);
    }

    // The write decision (`payload::plain_text_may_be_misread`): true for
    // every text `parse` takes a block from, and for one that a lone `\r`
    // could turn into such a text.
    #[test]
    fn may_end_with_block_matches_the_reader() {
        let whole = serialize(&[p("x", "t", "c")]);
        for text in [whole.clone(), format!("a\n{}", whole), format!("a\n{}\r\n", whole), format!("a\r{}", whole)] {
            assert!(may_end_with_block(&text), "{:?}", text);
        }
        for text in [
            "a\n[LOCKNOTE_PROMPTS]\nb".to_string(),
            "a\n[/LOCKNOTE_PROMPTS]".to_string(),
            format!("a\n{}\nmore", whole),
            "voir [LOCKNOTE_PROMPTS] ici\n[/LOCKNOTE_PROMPTS]".to_string(),
        ] {
            assert!(!may_end_with_block(&text), "{:?}", text);
            assert!(parse(&text).0.is_empty(), "{:?}", text);
        }
    }

    // A header line without a category (never written by this module) keeps
    // the whole inner text as the name, in the default category.
    #[test]
    fn header_without_category_takes_the_default() {
        let (back, _) = parse("[LOCKNOTE_PROMPTS]\n[PROMPT:only]\nt\n[/PROMPT]\n[/LOCKNOTE_PROMPTS]");
        assert_eq!(back, vec![p("only", "t", DEFAULT_CATEGORY)]);
    }

    // Issue 050 rule: the header is a whole line of its own; inside a line,
    // escaped or indented it is text.
    #[test]
    fn header_is_only_a_whole_line() {
        let rest = "\n[PROMPT:x:c]\nt\n[/PROMPT]\n[/LOCKNOTE_PROMPTS]";
        for head in ["voir [LOCKNOTE_PROMPTS] ici", "a\n\x00[LOCKNOTE_PROMPTS]", "a\n [LOCKNOTE_PROMPTS]", "[LOCKNOTE_PROMPTS]x", "a\r[LOCKNOTE_PROMPTS]\r"] {
            let text = format!("{}{}", head, rest);
            assert_eq!(parse(&text), (Vec::new(), text.clone()), "{:?}", text);
        }
    }

    // Known limit shared by every block (tabs, vault, prompts): the escape
    // prefix is `\x00` and a line that already starts with `\x00` is not
    // escaped, so reading drops that first `\x00`. Not specific to prompts,
    // reported with issue 058; kept here so a change is deliberate.
    #[test]
    fn line_starting_with_nul_loses_it_like_every_block() {
        let (back, _) = parse(&serialize(&[p("n", "a\n\0b", "c")]));
        assert_eq!(back[0].text, "a\nb");
        assert_eq!(crate::features::markers::unescape_content(&crate::features::markers::escape_content("\0b")), "b");
    }

    #[test]
    fn text_is_wiped_on_zeroize_name_and_category_kept() {
        let mut e = p("name", "secret text", "cat");
        e.zeroize();
        assert!(e.text.is_empty());
        assert_eq!((e.name.as_str(), e.category.as_str()), ("name", "cat"));
    }
}
