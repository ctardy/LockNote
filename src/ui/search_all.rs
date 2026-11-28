// Search in all tabs (Edit > Search All Tabs..., Ctrl+Shift+F): the pure
// search over a copy of the tabs, the vault names and the prompts. The
// dialog is `ui::dialogs::search_all`, the flow `editor::search_all_ops`.
//
// What is searched: the text of every tab (the active one as the editor
// holds it), the NAME and CATEGORY of the vault secrets — never their
// values, which are not even copied — and the name and text of the prompts.
// Without "Match case" the comparison ignores case the way the editor's
// highlight does (`case_insensitive_matches`), not accents.
//
// Tab matches carry RichEdit positions (UTF-16 code units, a line break of
// one unit, as `build_utf16_offset_map` counts them), so the match can be
// selected once the tab is shown. Matches do not overlap: the search goes on
// after the end of each match, like Find Next.
//
// Cost: the search runs on the UI thread at every refresh, so it stops at
// the cap (no tab is collected past it), counts lines and positions forward
// from the previous match, and reads only around a match for its excerpt.

use zeroize::Zeroizing;

use crate::features::prompts::PromptEntry;
use crate::ui::prompts_panel::PromptsListError;
use crate::ui::editor::case_insensitive_matches;

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

/// Byte ranges of the first `limit` matches of `query` in `text`, without
/// overlap.
fn match_ranges(text: &str, query: &str, match_case: bool, limit: usize) -> Vec<(usize, usize)> {
    if query.is_empty() {
        return Vec::new();
    }
    if match_case {
        return text.match_indices(query).take(limit).map(|(at, m)| (at, at + m.len())).collect();
    }
    let mut kept: Vec<(usize, usize)> = Vec::new();
    for (start, end) in case_insensitive_matches(text, query) {
        if kept.len() >= limit {
            break;
        }
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
        case_insensitive_matches(haystack, query).next().is_some()
    }
}

/// RichEdit positions (UTF-16 code units, `\r\n` as one) in `text[from..to]`,
/// byte offsets on char boundaries: the count of `build_utf16_offset_map`.
fn utf16_units(text: &str, from: usize, to: usize) -> usize {
    let bytes = text.as_bytes();
    text[from..to]
        .char_indices()
        .filter(|&(i, ch)| !(ch == '\n' && from + i > 0 && bytes[from + i - 1] == b'\r'))
        .map(|(_, ch)| ch.len_utf16())
        .sum()
}

/// Line breaks (`\r\n`, `\r`, `\n` or a form feed, one each) in `bytes`:
/// the lines RichEdit shows (features::text_lines).
fn line_breaks(bytes: &[u8]) -> usize {
    crate::features::text_lines::break_count(bytes)
}

/// The line holding `start..end` (byte offsets on char boundaries), cut to
/// `EXCERPT_CHARS` around the match, tabs as spaces, an ellipsis where it
/// was cut. Reads at most `EXCERPT_BEFORE` chars before the match and
/// `EXCERPT_CHARS` from there: a line may be megabytes long.
fn excerpt(text: &str, start: usize, end: usize) -> String {
    let is_break = crate::features::text_lines::is_break;
    // Back from the match: EXCERPT_BEFORE chars at most, not past the line
    // start. Cut when the line goes on before `from`.
    let (mut from, mut taken) = (start, 0);
    let mut back = text[..start].chars().rev();
    let cut_before = loop {
        match back.next() {
            None => break false,
            Some(c) if is_break(c) => break false,
            Some(_) if taken == EXCERPT_BEFORE => break true,
            Some(c) => {
                from -= c.len_utf8();
                taken += 1;
            }
        }
    };
    // Forward from `from`: EXCERPT_CHARS chars at most, up to the first line
    // break after the match. Cut when the line goes on after `to`.
    let (mut to, mut taken) = (from, 0);
    let mut forward = text[from..].chars();
    let cut_after = loop {
        match forward.next() {
            None => break false,
            Some(c) if is_break(c) && to >= end => break false,
            Some(_) if taken == EXCERPT_CHARS => break true,
            Some(c) => {
                to += c.len_utf8();
                taken += 1;
            }
        }
    };
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
        // One more than still fits: it tells that others exist.
        let budget = max.saturating_sub(results.hits.len()).saturating_add(1);
        let ranges = match_ranges(text, query, match_case, budget);
        let bytes = text.as_bytes();
        // Lines and positions counted forward from the previous match.
        let (mut counted_to, mut line, mut u16_at) = (0usize, 1usize, 0usize);
        for (start, end) in ranges {
            line += line_breaks(&bytes[counted_to..start]);
            u16_at += utf16_units(text, counted_to, start);
            counted_to = start;
            let hit = Hit::Tab {
                tab,
                name: t.name.clone(),
                line,
                excerpt: excerpt(text, start, end),
                start: u16_at,
                end: u16_at + utf16_units(text, start, end),
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
        Hit::Vault { category, name, .. } => fill(vault, &[("{category}", category), ("{name}", name)]),
        Hit::Prompt { name, .. } => fill(prompt, &[("{name}", name)]),
    }
}

/// `template` with each placeholder replaced by its value, in one pass over
/// the template: a value holding "{name}" is not filled again.
fn fill(template: &str, values: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    'scan: while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let tail = &rest[open..];
        for (key, value) in values {
            if tail.starts_with(key) {
                out.push_str(value);
                rest = &tail[key.len()..];
                continue 'scan;
            }
        }
        out.push('{');
        rest = &tail[1..];
    }
    out.push_str(rest);
    out
}

/// The query the dialog opens with: the editor's selection when it is on
/// one line (the field has one line), empty otherwise.
pub fn initial_query(selection: &str) -> &str {
    if selection.contains(['\r', '\n']) {
        ""
    } else {
        selection
    }
}

/// Whether the prompts search `query` hides `entry` (the list's rule,
/// `prompts_panel::matches`): only then is it cleared to show the result.
pub fn prompt_search_hides<F>(entry: &PromptEntry, query: &str, contains: &F) -> Result<bool, PromptsListError>
where
    F: Fn(&str, &str) -> Result<bool, PromptsListError>,
{
    Ok(!crate::ui::prompts_panel::matches(entry, query, contains)?)
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
    fn a_form_feed_ends_a_line_as_richedit_shows_it() {
        // Shown as "one" | "two key"; the excerpt stops at the form feed.
        let src = sources(vec![tab("A", "one\u{c}two key\r\nthree")]);
        let r = search(&src, "key", true, MAX_RESULTS);
        assert_eq!(tab_hits(&r), vec![(0, 2, 8, 11)]);
        assert_eq!(excerpt("one\u{c}two key\u{c}three", 8, 11), "two key");
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

    /// Placeholders are filled from the template only, never inside a value
    /// already put in.
    #[test]
    fn labels_fill_the_template_in_one_pass() {
        let v = Hit::Vault { index: 0, category: "{name} keys".into(), name: "AWS".into() };
        assert_eq!(hit_label(&v, "Vault: {category} / {name}", ""), "Vault: {name} keys / AWS");
        let v = Hit::Vault { index: 0, category: "Cloud".into(), name: "{category}".into() };
        assert_eq!(hit_label(&v, "{name} ({category})", ""), "{category} (Cloud)");
        let p = Hit::Prompt { index: 0, category: "c".into(), name: "{name}".into() };
        assert_eq!(hit_label(&p, "", "Prompt: {name}"), "Prompt: {name}");
        // Unknown or unclosed braces are kept as they are.
        let p = Hit::Prompt { index: 0, category: "c".into(), name: "R".into() };
        assert_eq!(hit_label(&p, "", "{x} {name} {"), "{x} R {");
    }

    /// One very long line (pasted base64): each excerpt reads only around
    /// its match, not the whole line.
    #[test]
    fn very_long_line_stays_fast() {
        let src = sources(vec![tab("A", &"a".repeat(5_000_000))]);
        let t0 = std::time::Instant::now();
        let r = search(&src, "a", true, MAX_RESULTS);
        assert_eq!((r.hits.len(), r.more), (MAX_RESULTS, true));
        assert!(t0.elapsed() < std::time::Duration::from_secs(2), "{:?}", t0.elapsed());
    }

    #[test]
    fn excerpt_of_a_long_line_on_both_sides() {
        let text = format!("{}KEY{}", "x".repeat(1000), "y".repeat(1000));
        let want = format!("\u{2026}{}KEY{}\u{2026}", "x".repeat(EXCERPT_BEFORE), "y".repeat(EXCERPT_CHARS - EXCERPT_BEFORE - 3));
        assert_eq!(excerpt(&text, 1000, 1003), want);
        // The line edges inside the window.
        assert_eq!(excerpt("ab\r\nxKEYy\r\ncd", 5, 8), "xKEYy");
        assert_eq!(excerpt(&format!("{}\nKEY", "z".repeat(5000)), 5001, 5004), "KEY");
        // Exactly EXCERPT_BEFORE chars before, from the line start: no cut.
        let text = format!("q\n{}KEY", "x".repeat(EXCERPT_BEFORE));
        assert_eq!(excerpt(&text, 2 + EXCERPT_BEFORE, 5 + EXCERPT_BEFORE), format!("{}KEY", "x".repeat(EXCERPT_BEFORE)));
        // One more: cut.
        let text = format!("q\n{}KEY", "x".repeat(EXCERPT_BEFORE + 1));
        assert!(excerpt(&text, 3 + EXCERPT_BEFORE, 6 + EXCERPT_BEFORE).starts_with('\u{2026}'));
        // Line exactly EXCERPT_CHARS long from `from`: no cut after.
        let text = format!("KEY{}\nnext", "y".repeat(EXCERPT_CHARS - 3));
        assert_eq!(excerpt(&text, 0, 3), &text[..EXCERPT_CHARS]);
    }

    /// The cap stops the search: the rest of a big tab is not collected and
    /// later tabs, vault and prompts are not listed.
    #[test]
    fn the_cap_stops_the_search() {
        for match_case in [true, false] {
            let src = Sources {
                tabs: vec![tab("A", &"x ".repeat(2_000_000)), tab("B", "x")],
                vault: vec![VaultName { name: "x".into(), category: "General".into() }],
                prompts: vec![PromptEntry::new("x", "t", "c")],
            };
            let t0 = std::time::Instant::now();
            let r = search(&src, "x", match_case, MAX_RESULTS);
            let elapsed = t0.elapsed();
            assert_eq!((r.hits.len(), r.more), (MAX_RESULTS, true));
            assert!(r.hits.iter().all(|h| matches!(h, Hit::Tab { tab: 0, .. })));
            assert!(elapsed < std::time::Duration::from_secs(1), "match_case={} {:?}", match_case, elapsed);
        }
        // Positions stay RichEdit positions past line breaks and wide chars.
        let r = search(&sources(vec![tab("A", "\u{1F600}x\r\nx\nx")]), "x", true, 2);
        assert_eq!(tab_hits(&r), vec![(0, 1, 2, 3), (0, 2, 4, 5)]);
        assert!(r.more);
    }

    #[test]
    fn initial_query_only_from_a_single_line_selection() {
        assert_eq!(initial_query("key"), "key");
        assert_eq!(initial_query(""), "");
        assert_eq!(initial_query("one\r\ntwo"), "");
        assert_eq!(initial_query("one\rtwo"), "");
        assert_eq!(initial_query("one\ntwo"), "");
    }

    #[test]
    fn prompt_search_kept_when_it_shows_the_prompt() {
        let contains = |h: &str, n: &str| -> Result<bool, PromptsListError> { Ok(h.to_lowercase().contains(&n.to_lowercase())) };
        let review = PromptEntry::new("Review", "Find bugs", "Code");
        assert!(!prompt_search_hides(&review, "rev", &contains).unwrap());
        assert!(!prompt_search_hides(&review, "bugs", &contains).unwrap());
        assert!(!prompt_search_hides(&review, "  ", &contains).unwrap());
        assert!(prompt_search_hides(&review, "zzz", &contains).unwrap());
        let failing = |_: &str, _: &str| -> Result<bool, PromptsListError> { Err(PromptsListError(crate::ui::vault_panel::VaultTreeError::Compare("cmp".into()))) };
        assert!(prompt_search_hides(&review, "rev", &failing).is_err());
    }

    #[test]
    fn same_entry_needs_name_and_category() {
        assert!(same_entry(Some(("AWS", "Cloud")), "AWS", "Cloud"));
        assert!(!same_entry(Some(("AWS", "General")), "AWS", "Cloud"));
        assert!(!same_entry(Some(("GCP", "Cloud")), "AWS", "Cloud"));
        assert!(!same_entry(None, "AWS", "Cloud"));
    }
}
