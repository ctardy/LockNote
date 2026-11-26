// Search in all tabs (Edit > Search All Tabs..., Ctrl+Shift+F): the pure
// search over a copy of the tabs, the vault names and the prompts. The
// dialog is `ui::dialogs::search_all`, the flow `editor::search_all_ops`.
//
// What is searched: the text of every tab (the active one as the editor
// holds it), the NAME and CATEGORY of the vault secrets — never their
// values, which are not even copied — and the name and text of the prompts.
// Without "Match case" the comparison ignores case the way the editor's
// highlight does (`find_all_case_insensitive`), not accents.
//
// Tab matches carry RichEdit positions (UTF-16 code units, a line break of
// one unit: `build_utf16_offset_map`), so the match can be selected once the
// tab is shown. Matches do not overlap: the search goes on after the end of
// each match, like Find Next.

use zeroize::Zeroizing;

use crate::features::prompts::PromptEntry;
use crate::ui::editor::{build_utf16_offset_map, find_all_case_insensitive};

/// Results shown at most; one more line says the rest is not shown.
pub const MAX_RESULTS: usize = 1000;
/// Characters of the line kept before the match in an excerpt.
const EXCERPT_BEFORE: usize = 30;
/// Characters of an excerpt at most (before the ellipses).
const EXCERPT_CHARS: usize = 100;
const ELLIPSIS: char = '\u{2026}';

/// One tab: its name and its text (wiped when dropped).
pub struct TabText {
    pub name: String,
    pub text: Zeroizing<String>,
}

/// What is searched of a vault secret: no value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultName {
    pub name: String,
    pub category: String,
}

/// The copy the dialog searches, taken before it opens (no borrow of the
/// window's stores is held while it is shown).
pub struct Sources {
    pub tabs: Vec<TabText>,
    pub vault: Vec<VaultName>,
    pub prompts: Vec<PromptEntry>,
}

/// One result, with what is needed to show it again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hit {
    /// A match in tab `tab`: 1-based `line`, the `excerpt` of that line,
    /// `start..end` in RichEdit positions.
    Tab { tab: usize, name: String, line: usize, excerpt: String, start: usize, end: usize },
    /// The secret at storage index `index` (name or category matched).
    Vault { index: usize, category: String, name: String },
    /// The prompt at storage index `index` (name or text matched).
    Prompt { index: usize, category: String, name: String },
}

/// The results, in order: tabs (tab order, then position), vault, prompts
/// (storage order). `more`: others exist past `max`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Results {
    pub hits: Vec<Hit>,
    pub more: bool,
}

/// Byte ranges of the matches of `query` in `text`, without overlap.
fn match_ranges(text: &str, query: &str, match_case: bool) -> Vec<(usize, usize)> {
    if query.is_empty() {
        return Vec::new();
    }
    if match_case {
        return text.match_indices(query).map(|(at, m)| (at, at + m.len())).collect();
    }
    let mut kept: Vec<(usize, usize)> = Vec::new();
    for (start, end) in find_all_case_insensitive(text, query) {
        if kept.last().map_or(true, |&(_, last_end)| start >= last_end) {
            kept.push((start, end));
        }
    }
    kept
}

fn contains(haystack: &str, query: &str, match_case: bool) -> bool {
    if query.is_empty() {
        return false;
    }
    if match_case {
        haystack.contains(query)
    } else {
        !find_all_case_insensitive(haystack, query).is_empty()
    }
}

/// Line breaks (`\r\n`, `\r` or `\n`, one each) in `bytes`.
fn line_breaks(bytes: &[u8]) -> usize {
    let mut n = 0;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'\n' || (b == b'\r' && bytes.get(i + 1) != Some(&b'\n')) {
            n += 1;
        }
    }
    n
}

/// The line holding `start..end` (byte offsets on char boundaries), cut to
/// `EXCERPT_CHARS` around the match, tabs as spaces, an ellipsis where it
/// was cut.
fn excerpt(text: &str, start: usize, end: usize) -> String {
    let line_start = text[..start].rfind(['\r', '\n']).map_or(0, |p| p + 1);
    let line_end = text[end..].find(['\r', '\n']).map_or(text.len(), |p| end + p);
    // EXCERPT_BEFORE chars back from the match, on a char boundary.
    let from = text[line_start..start]
        .char_indices()
        .rev()
        .take(EXCERPT_BEFORE)
        .last()
        .map_or(start, |(i, _)| line_start + i);
    let cut_before = from > line_start;
    let rest = &text[from..line_end];
    let to = rest.char_indices().nth(EXCERPT_CHARS).map_or(line_end, |(i, _)| from + i);
    let cut_after = to < line_end;
    let mut out = String::new();
    if cut_before {
        out.push(ELLIPSIS);
    }
    let body = text[from..to].replace('\t', " ");
    out.push_str(if cut_before { &body } else { body.trim_start() });
    if cut_after {
        out.push(ELLIPSIS);
    }
    out
}

/// Search `query` everywhere in `src`, at most `max` results. An empty
/// query finds nothing.
pub fn search(src: &Sources, query: &str, match_case: bool, max: usize) -> Results {
    let mut results = Results::default();
    if query.is_empty() {
        return results;
    }
    // Pushes a hit, or records that one did not fit; false = stop.
    let push = |results: &mut Results, hit: Hit| -> bool {
        if results.hits.len() >= max {
            results.more = true;
            return false;
        }
        results.hits.push(hit);
        true
    };
    for (tab, t) in src.tabs.iter().enumerate() {
        let text: &str = &t.text;
        let ranges = match_ranges(text, query, match_case);
        if ranges.is_empty() {
            continue;
        }
        let u16_map = build_utf16_offset_map(text);
        let bytes = text.as_bytes();
        let (mut counted_to, mut line) = (0usize, 1usize);
        for (start, end) in ranges {
            line += line_breaks(&bytes[counted_to..start]);
            counted_to = start;
            let hit = Hit::Tab {
                tab,
                name: t.name.clone(),
                line,
                excerpt: excerpt(text, start, end),
                start: u16_map[start],
                end: u16_map[end],
            };
            if !push(&mut results, hit) {
                return results;
            }
        }
    }
    for (index, v) in src.vault.iter().enumerate() {
        if contains(&v.name, query, match_case) || contains(&v.category, query, match_case) {
            let hit = Hit::Vault { index, category: v.category.clone(), name: v.name.clone() };
            if !push(&mut results, hit) {
                return results;
            }
        }
    }
    for (index, p) in src.prompts.iter().enumerate() {
        if contains(&p.name, query, match_case) || contains(&p.text, query, match_case) {
            let hit = Hit::Prompt { index, category: p.category.clone(), name: p.name.clone() };
            if !push(&mut results, hit) {
                return results;
            }
        }
    }
    results
}

/// The list line of a result: "<tab> : <line> : <excerpt>", or the
/// translated `vault` ("{category}", "{name}") / `prompt` ("{name}") text.
pub fn hit_label(hit: &Hit, vault: &str, prompt: &str) -> String {
    match hit {
        Hit::Tab { name, line, excerpt, .. } => format!("{} : {} : {}", name, line, excerpt),
        Hit::Vault { category, name, .. } => vault.replace("{category}", category).replace("{name}", name),
        Hit::Prompt { name, .. } => prompt.replace("{name}", name),
    }
}

/// Whether the entry now at the result's index (`current`: name, category)
/// is still the one found: the list may have changed since the search.
pub fn same_entry(current: Option<(&str, &str)>, name: &str, category: &str) -> bool {
    current == Some((name, category))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(name: &str, text: &str) -> TabText {
        TabText { name: name.to_string(), text: Zeroizing::new(text.to_string()) }
    }

    fn sources(tabs: Vec<TabText>) -> Sources {
        Sources { tabs, vault: Vec::new(), prompts: Vec::new() }
    }

    fn tab_hits(r: &Results) -> Vec<(usize, usize, usize, usize)> {
        r.hits
            .iter()
            .filter_map(|h| match h {
                Hit::Tab { tab, line, start, end, .. } => Some((*tab, *line, *start, *end)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn every_match_in_every_tab_in_order() {
        let src = sources(vec![tab("A", "one key\ntwo key key"), tab("B", "nothing"), tab("C", "key")]);
        let r = search(&src, "key", true, MAX_RESULTS);
        assert_eq!(tab_hits(&r), vec![(0, 1, 4, 7), (0, 2, 12, 15), (0, 2, 16, 19), (2, 1, 0, 3)]);
        assert!(!r.more);
    }

    #[test]
    fn empty_query_finds_nothing() {
        let src = sources(vec![tab("A", "text")]);
        assert_eq!(search(&src, "", false, MAX_RESULTS), Results::default());
    }

    #[test]
    fn match_case_or_not() {
        let src = sources(vec![tab("A", "Key key KEY")]);
        assert_eq!(search(&src, "key", true, MAX_RESULTS).hits.len(), 1);
        assert_eq!(search(&src, "key", false, MAX_RESULTS).hits.len(), 3);
        // Case only, not accents (the highlight's rule).
        let src = sources(vec![tab("A", "\u{c9}t\u{e9} ete")]);
        assert_eq!(search(&src, "\u{e9}t\u{e9}", false, MAX_RESULTS).hits.len(), 1);
    }

    #[test]
    fn matches_do_not_overlap() {
        let src = sources(vec![tab("A", "aaaa")]);
        assert_eq!(tab_hits(&search(&src, "aa", true, MAX_RESULTS)), vec![(0, 1, 0, 2), (0, 1, 2, 4)]);
        assert_eq!(tab_hits(&search(&src, "AA", false, MAX_RESULTS)), vec![(0, 1, 0, 2), (0, 1, 2, 4)]);
    }

    /// The editor's text has `\r\n` (one position, like RichEdit); a tab
    /// read back from the payload may have `\n`. Same lines, same positions.
    #[test]
    fn positions_are_richedit_positions() {
        for text in ["ab\r\ncd x", "ab\ncd x", "ab\rcd x"] {
            let r = search(&sources(vec![tab("A", text)]), "x", true, MAX_RESULTS);
            assert_eq!(tab_hits(&r), vec![(0, 2, 6, 7)], "{:?}", text);
        }
        // Outside the BMP: two code units.
        let r = search(&sources(vec![tab("A", "\u{1F600}\u{e9}x")]), "x", true, MAX_RESULTS);
        assert_eq!(tab_hits(&r), vec![(0, 1, 3, 4)]);
    }

    #[test]
    fn line_numbers_count_every_kind_of_break() {
        let r = search(&sources(vec![tab("A", "x\r\nx\rx\nx\n\nx")]), "x", true, MAX_RESULTS);
        let lines: Vec<usize> = tab_hits(&r).iter().map(|h| h.1).collect();
        assert_eq!(lines, vec![1, 2, 3, 4, 6]);
    }

    #[test]
    fn excerpt_is_the_line_cut_around_the_match() {
        assert_eq!(excerpt("first\r\n\t  the key here\r\nlast", 13, 16), "the key here");
        let long = format!("{}KEY{}", "a".repeat(50), "b".repeat(200));
        let e = excerpt(&long, 50, 53);
        assert!(e.starts_with('\u{2026}') && e.ends_with('\u{2026}'), "{}", e);
        assert!(e.contains("KEY"));
        assert_eq!(e.chars().count(), EXCERPT_CHARS + 2);
        // Multi-byte text is cut on char boundaries.
        let wide = format!("{}KEY{}", "\u{e9}".repeat(50), "\u{1F600}".repeat(200));
        let at = wide.find("KEY").unwrap();
        let e = excerpt(&wide, at, at + 3);
        assert!(e.contains("KEY") && e.chars().count() == EXCERPT_CHARS + 2, "{}", e);
    }

    #[test]
    fn vault_name_or_category_never_the_value() {
        let src = Sources {
            tabs: Vec::new(),
            vault: vec![
                VaultName { name: "GitHub token".into(), category: "API Keys".into() },
                VaultName { name: "Bank".into(), category: "Finance".into() },
                VaultName { name: "Mail".into(), category: "Email".into() },
            ],
            prompts: Vec::new(),
        };
        let r = search(&src, "key", false, MAX_RESULTS);
        assert_eq!(r.hits, vec![Hit::Vault { index: 0, category: "API Keys".into(), name: "GitHub token".into() }]);
        let r = search(&src, "bank", false, MAX_RESULTS);
        assert_eq!(r.hits.len(), 1);
        // The type has no value field at all: nothing to search or leak.
        let _no_value = VaultName { name: String::new(), category: String::new() };
    }

    #[test]
    fn prompts_by_name_or_text() {
        let src = Sources {
            tabs: Vec::new(),
            vault: Vec::new(),
            prompts: vec![
                PromptEntry::new("Summary", "Summarise the text", "Writing"),
                PromptEntry::new("Review", "Find bugs in this code", "Code"),
                PromptEntry::new("Bug report", "Template", "Code"),
            ],
        };
        let r = search(&src, "bug", false, MAX_RESULTS);
        assert_eq!(
            r.hits,
            vec![
                Hit::Prompt { index: 1, category: "Code".into(), name: "Review".into() },
                Hit::Prompt { index: 2, category: "Code".into(), name: "Bug report".into() },
            ]
        );
        // The category alone does not match a prompt.
        assert!(search(&src, "Writing", true, MAX_RESULTS).hits.is_empty());
    }

    #[test]
    fn tabs_then_vault_then_prompts() {
        let src = Sources {
            tabs: vec![tab("Notes", "x")],
            vault: vec![VaultName { name: "x".into(), category: "General".into() }],
            prompts: vec![PromptEntry::new("x", "t", "c")],
        };
        let kinds: Vec<&str> = search(&src, "x", true, MAX_RESULTS)
            .hits
            .iter()
            .map(|h| match h {
                Hit::Tab { .. } => "tab",
                Hit::Vault { .. } => "vault",
                Hit::Prompt { .. } => "prompt",
            })
            .collect();
        assert_eq!(kinds, ["tab", "vault", "prompt"]);
    }

    #[test]
    fn capped_with_more_flag() {
        let src = Sources {
            tabs: vec![tab("A", &"x ".repeat(1500))],
            vault: vec![VaultName { name: "x".into(), category: "General".into() }],
            prompts: Vec::new(),
        };
        let r = search(&src, "x", true, MAX_RESULTS);
        assert_eq!(r.hits.len(), MAX_RESULTS);
        assert!(r.more);
        // Exactly at the cap: nothing hidden.
        let r = search(&sources(vec![tab("A", "x x")]), "x", true, 2);
        assert_eq!((r.hits.len(), r.more), (2, false));
        // The cap reached in the vault part.
        let r = search(&src, "x", true, 1500);
        assert_eq!((r.hits.len(), r.more), (1500, true));
    }

    #[test]
    fn labels() {
        let tab_hit = Hit::Tab { tab: 0, name: "Notes".into(), line: 12, excerpt: "the key".into(), start: 0, end: 1 };
        assert_eq!(hit_label(&tab_hit, "", ""), "Notes : 12 : the key");
        let v = Hit::Vault { index: 0, category: "Cloud".into(), name: "AWS".into() };
        assert_eq!(hit_label(&v, "Vault: {category} / {name}", ""), "Vault: Cloud / AWS");
        let p = Hit::Prompt { index: 0, category: "c".into(), name: "Review".into() };
        assert_eq!(hit_label(&p, "", "Prompt: {name}"), "Prompt: Review");
    }

    #[test]
    fn same_entry_needs_name_and_category() {
        assert!(same_entry(Some(("AWS", "Cloud")), "AWS", "Cloud"));
        assert!(!same_entry(Some(("AWS", "General")), "AWS", "Cloud"));
        assert!(!same_entry(Some(("GCP", "Cloud")), "AWS", "Cloud"));
        assert!(!same_entry(None, "AWS", "Cloud"));
    }
}
