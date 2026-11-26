// Outline of a tab: the Markdown headings listed by Edit > Go to Heading
// (Ctrl+Shift+O). Pure text work, nothing stored: the list is rebuilt from
// the active tab every time the dialog opens.

/// One heading of the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    /// Zero-based line of the heading text (the line before the underline
    /// for a setext heading), counted on '\n' like `apply_cursor_line`.
    pub line: usize,
    /// 1 to 6 for "#" to "######"; 1 for a "===" underline, 2 for "---".
    pub level: u8,
    /// The heading text, trimmed, without its "#" marks.
    pub title: String,
}

/// Spaces per level in the dialog's list.
const INDENT: usize = 4;

/// The line without the up to 3 spaces Markdown allows before a heading or
/// a fence; `None` when it is indented more (an indented code line).
fn unindented(line: &str) -> Option<&str> {
    let spaces = line.len() - line.trim_start_matches(' ').len();
    (spaces <= 3).then(|| &line[spaces..])
}

/// A "```" fence line (opening or closing).
fn is_fence(line: &str) -> bool {
    unindented(line).is_some_and(|l| l.starts_with("```"))
}

/// "#" to "######" followed by a space: (level, title). The closing "#"
/// run of "## Title ##" is dropped; an empty title is no heading.
fn atx(line: &str) -> Option<(u8, String)> {
    let l = unindented(line)?;
    let marks = l.len() - l.trim_start_matches('#').len();
    if !(1..=6).contains(&marks) {
        return None;
    }
    // '#' is one byte: `marks` is a char boundary.
    let rest = l[marks..].strip_prefix(' ')?;
    let mut title = rest.trim();
    let without_closing = title.trim_end_matches('#');
    if without_closing.is_empty() || without_closing.ends_with([' ', '\t']) {
        title = without_closing.trim_end();
    }
    (!title.is_empty()).then(|| (marks as u8, title.to_string()))
}

/// A setext underline: 3 or more "=" (level 1) or "-" (level 2), alone on
/// the line apart from spaces.
fn setext_level(line: &str) -> Option<u8> {
    let l = unindented(line)?.trim_end();
    let level = match l.chars().next()? {
        '=' => 1,
        '-' => 2,
        _ => return None,
    };
    let mark = if level == 1 { '=' } else { '-' };
    (l.len() >= 3 && l.chars().all(|c| c == mark)).then_some(level)
}

/// The headings of `text`, in order. Lines inside ``` fences are skipped;
/// an unclosed fence runs to the end of the text.
pub fn headings(text: &str) -> Vec<Heading> {
    let lines: Vec<&str> = text.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l)).collect();
    let mut out = Vec::new();
    let mut in_fence = false;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if is_fence(line) {
            in_fence = !in_fence;
        } else if in_fence {
            // Code: nothing is a heading.
        } else if let Some((level, title)) = atx(line) {
            out.push(Heading { line: i, level, title });
        } else if !line.trim().is_empty() && setext_level(line).is_none() {
            if let Some(level) = lines.get(i + 1).and_then(|next| setext_level(next)) {
                out.push(Heading { line: i, level, title: line.trim().to_string() });
                // The underline belongs to this heading.
                i += 1;
            }
        }
        i += 1;
    }
    out
}

/// Indexes into `all` of the headings whose title contains `query`
/// (case-insensitive, surrounding spaces ignored). An empty query keeps all.
pub fn filter(all: &[Heading], query: &str) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    all.iter()
        .enumerate()
        .filter(|(_, h)| query.is_empty() || h.title.to_lowercase().contains(&query))
        .map(|(i, _)| i)
        .collect()
}

/// The text of a heading in the dialog's list: indented by its level.
pub fn list_label(h: &Heading) -> String {
    let depth = usize::from(h.level.saturating_sub(1));
    format!("{}{}", " ".repeat(depth * INDENT), h.title)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(line: usize, level: u8, title: &str) -> Heading {
        Heading { line, level, title: title.to_string() }
    }

    #[test]
    fn atx_levels_one_to_six() {
        let text = "# One\n## Two\n### Three\n#### Four\n##### Five\n###### Six";
        assert_eq!(
            headings(text),
            vec![h(0, 1, "One"), h(1, 2, "Two"), h(2, 3, "Three"), h(3, 4, "Four"), h(4, 5, "Five"), h(5, 6, "Six")]
        );
    }

    #[test]
    fn atx_needs_a_space_and_at_most_six_marks() {
        let text = "#NoSpace\n####### Seven\n#\n# \nnot # a heading\n#\tTab";
        assert!(headings(text).is_empty(), "{:?}", headings(text));
    }

    #[test]
    fn atx_closing_marks_and_spaces_are_dropped() {
        assert_eq!(headings("##   Title  ##  "), vec![h(0, 2, "Title")]);
        // A "#" glued to the word is part of the title.
        assert_eq!(headings("# C#"), vec![h(0, 1, "C#")]);
    }

    #[test]
    fn atx_allows_up_to_three_leading_spaces() {
        assert_eq!(headings("   # Indented"), vec![h(0, 1, "Indented")]);
        assert!(headings("    # Code").is_empty());
    }

    #[test]
    fn setext_equals_is_level_one_and_dashes_level_two() {
        let text = "Title\n=====\ntext\nSub\n---";
        assert_eq!(headings(text), vec![h(0, 1, "Title"), h(3, 2, "Sub")]);
    }

    #[test]
    fn setext_needs_three_marks_and_a_non_empty_line_above() {
        assert!(headings("Title\n==").is_empty());
        assert!(headings("\n---").is_empty());
        assert!(headings("---").is_empty());
        assert!(headings("Title\n=-=").is_empty());
        assert!(headings("Title\n--- x").is_empty());
        assert_eq!(headings("  Title  \n===   "), vec![h(0, 1, "Title")]);
    }

    /// A rule under a rule is not a heading, and an underline is used once.
    #[test]
    fn setext_underline_is_not_a_title_itself() {
        assert!(headings("---\n---").is_empty());
        assert_eq!(headings("Title\n---\n---"), vec![h(0, 2, "Title")]);
    }

    #[test]
    fn fenced_code_blocks_are_ignored() {
        let text = "# Before\n```rust\n# not a heading\nNot\n---\n```\n# After\n```\n# unclosed";
        assert_eq!(headings(text), vec![h(0, 1, "Before"), h(6, 1, "After")]);
    }

    #[test]
    fn a_fence_line_is_not_a_setext_title() {
        assert!(headings("```\n```\n---").is_empty());
        assert_eq!(headings("Title\n```\n---\n```"), vec![]);
    }

    /// The editor gives the text with "\r\n": the line numbers and titles
    /// are the same as with "\n".
    #[test]
    fn crlf_text_gives_the_same_lines() {
        let text = "intro\r\n# One\r\nTwo\r\n===\r\n";
        assert_eq!(headings(text), vec![h(1, 1, "One"), h(2, 1, "Two")]);
    }

    #[test]
    fn non_ascii_titles_are_kept_whole() {
        assert_eq!(headings("## \u{e9}t\u{e9} \u{1f600}"), vec![h(0, 2, "\u{e9}t\u{e9} \u{1f600}")]);
        assert_eq!(headings("\u{c9}t\u{e9}\n---"), vec![h(0, 2, "\u{c9}t\u{e9}")]);
    }

    #[test]
    fn empty_text_has_no_heading() {
        assert!(headings("").is_empty());
    }

    #[test]
    fn filter_is_case_insensitive_and_keeps_order() {
        let all = vec![h(0, 1, "Intro"), h(3, 2, "Install on Windows"), h(9, 2, "Usage"), h(12, 1, "\u{c9}TAT")];
        assert_eq!(filter(&all, ""), vec![0, 1, 2, 3]);
        assert_eq!(filter(&all, "   "), vec![0, 1, 2, 3]);
        assert_eq!(filter(&all, "IN"), vec![0, 1]);
        assert_eq!(filter(&all, " usage "), vec![2]);
        assert_eq!(filter(&all, "\u{e9}tat"), vec![3]);
        assert!(filter(&all, "zzz").is_empty());
        assert!(filter(&[], "a").is_empty());
    }

    #[test]
    fn list_label_is_indented_by_level() {
        assert_eq!(list_label(&h(0, 1, "Top")), "Top");
        assert_eq!(list_label(&h(0, 2, "Sub")), "    Sub");
        assert_eq!(list_label(&h(0, 6, "Deep")), format!("{}Deep", " ".repeat(20)));
    }
}
