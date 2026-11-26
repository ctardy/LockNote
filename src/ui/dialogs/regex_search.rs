// Regular-expression Find / Replace (feature B3) — the pure part of it.
//
// RichEdit cannot search with a regex, so the dialog reads the editor text,
// searches it here and selects the result by RichEdit position.
//
// Positions: RichEdit counts UTF-16 code units and one position per line
// break (`\r\n` = 1, see `build_utf16_offset_map`). The text is searched with
// every line break turned into `\n`, so that `\n`, `^` and `$` (multi-line
// mode) see one break per line, and a byte offset maps to a position with
// the same table as the rest of the editor.
//
// Zero-length matches (`^`, `x*`, ...): Next / Previous never return the
// empty match the selection already is, and Replace All works from the list
// of matches taken once, so every operation progresses and terminates.

use regex::{Regex, RegexBuilder};

use crate::ui::editor::build_utf16_offset_map;

/// Upper bound of the compiled program (and of the DFA cache, when the
/// regex crate uses one): a pattern like `\w{1000}{1000}` is refused as
/// invalid instead of eating memory.
pub const REGEX_SIZE_LIMIT: usize = 1 << 20;

/// The dialog's check boxes that change the pattern.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RegexOptions {
    pub match_case: bool,
    pub whole_word: bool,
}

/// A match, in RichEdit positions.
pub type Span = (usize, usize);

/// Compile `pattern` for the dialog: multi-line, case-insensitive unless
/// Match case, wrapped in `\b(?: )\b` for Whole word. The pattern is
/// validated alone first: wrapping `a)|(b` would make it valid.
pub fn compile(pattern: &str, opts: RegexOptions) -> Result<Regex, regex::Error> {
    let build = |p: &str| {
        RegexBuilder::new(p)
            .multi_line(true)
            .case_insensitive(!opts.match_case)
            .size_limit(REGEX_SIZE_LIMIT)
            .dfa_size_limit(REGEX_SIZE_LIMIT)
            .build()
    };
    let plain = build(pattern)?;
    if !opts.whole_word {
        return Ok(plain);
    }
    build(&format!(r"\b(?:{})\b", pattern))
}

/// The editor text as searched: line breaks as `\n`, plus the byte ->
/// RichEdit position table.
pub struct Haystack {
    text: String,
    u16_map: Vec<usize>,
}

impl Haystack {
    /// `raw` is the editor text as Windows returns it (`\r\n`, or a lone
    /// `\r`, per line break).
    pub fn new(raw: &str) -> Self {
        let text = normalize_breaks(raw);
        let u16_map = build_utf16_offset_map(&text);
        Haystack { text, u16_map }
    }

    /// RichEdit position of a byte offset. `regex::Regex` only reports
    /// offsets on UTF-8 boundaries, where the table is defined.
    fn pos(&self, byte: usize) -> usize {
        self.u16_map[byte]
    }

    /// Every match, in order (the regex crate's non-overlapping iteration,
    /// which itself steps over empty matches).
    pub fn matches(&self, re: &Regex) -> Vec<Span> {
        re.find_iter(&self.text).map(|m| (self.pos(m.start()), self.pos(m.end()))).collect()
    }

    /// The replacement text of the match that is exactly `sel` (`$1`,
    /// `${name}` expanded), or `None` when `sel` is not a match.
    pub fn expand_at(&self, re: &Regex, sel: Span, replacement: &str) -> Option<String> {
        re.captures_iter(&self.text).find_map(|caps| {
            let m = caps.get(0)?;
            if (self.pos(m.start()), self.pos(m.end())) != sel {
                return None;
            }
            let mut out = String::new();
            caps.expand(replacement, &mut out);
            Some(out)
        })
    }

    /// What Replace All does, match by match: positions in the text as it
    /// is now, with the expanded replacement of each.
    pub fn replace_plan(&self, re: &Regex, replacement: &str) -> Vec<(Span, String)> {
        re.captures_iter(&self.text)
            .filter_map(|caps| {
                let m = caps.get(0)?;
                let mut out = String::new();
                caps.expand(replacement, &mut out);
                Some(((self.pos(m.start()), self.pos(m.end())), out))
            })
            .collect()
    }
}

/// `\r\n` and lone `\r` become `\n`: one character per RichEdit line break.
pub fn normalize_breaks(raw: &str) -> String {
    raw.replace("\r\n", "\n").replace('\r', "\n")
}

/// Index of the match Next selects from selection `sel`: the first one at
/// or after the selection's end that is not the selection itself (an empty
/// match at the caret was already found), else the first one (wrap).
pub fn next_index(matches: &[Span], sel: Span) -> Option<usize> {
    matches
        .iter()
        .position(|&m| m.0 >= sel.1 && m != sel)
        .or_else(|| (!matches.is_empty()).then_some(0))
}

/// Index of the match Previous selects from selection `sel`: the last one
/// ending at or before the selection's start that is not the selection
/// itself, else the last one (wrap).
pub fn prev_index(matches: &[Span], sel: Span) -> Option<usize> {
    matches
        .iter()
        .rposition(|&m| m.1 <= sel.0 && m != sel)
        .or_else(|| matches.len().checked_sub(1))
}

// ── The flows, over the editor control ──────────────────────────────────

/// What the flows need from the editor (a RichEdit in the dialog, a fake in
/// the tests). Positions are RichEdit positions.
pub trait RegexTarget {
    /// The text as Windows returns it (`\r\n` line breaks).
    fn text(&self) -> String;
    fn selection(&self) -> Span;
    fn set_selection(&mut self, start: usize, end: usize);
    /// Undoable replacement of the selection; the caret ends after it.
    fn replace_selection(&mut self, text: &str);
}

/// Where the counter stands after a step: 1-based match number and total.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Found {
    pub current: usize,
    pub total: usize,
}

fn select_index(target: &mut dyn RegexTarget, matches: &[Span], index: Option<usize>) -> Option<Found> {
    let i = index?;
    let (a, b) = matches[i];
    target.set_selection(a, b);
    Some(Found { current: i + 1, total: matches.len() })
}

/// Next: select the next match (wrapping).
pub fn find_next(target: &mut dyn RegexTarget, re: &Regex) -> Option<Found> {
    let matches = Haystack::new(&target.text()).matches(re);
    let i = next_index(&matches, target.selection());
    select_index(target, &matches, i)
}

/// Previous: select the previous match (wrapping).
pub fn find_prev(target: &mut dyn RegexTarget, re: &Regex) -> Option<Found> {
    let matches = Haystack::new(&target.text()).matches(re);
    let i = prev_index(&matches, target.selection());
    select_index(target, &matches, i)
}

/// Replace: when the selection is a match, replace it (groups expanded);
/// then select the next match. A selection that is not a match is left
/// alone, as the replacement of its groups would mean nothing.
pub fn replace_one(target: &mut dyn RegexTarget, re: &Regex, replacement: &str) -> Option<Found> {
    let sel = target.selection();
    if let Some(text) = Haystack::new(&target.text()).expand_at(re, sel, replacement) {
        target.replace_selection(&text);
    }
    find_next(target, re)
}

/// Replace All: one undoable replacement per match, as the plain search
/// does, from the matches of the text before the first one. Returns the
/// number of replacements.
pub fn replace_all(target: &mut dyn RegexTarget, re: &Regex, replacement: &str) -> Result<usize, String> {
    let plan = Haystack::new(&target.text()).replace_plan(re, replacement);
    // Shift of the positions after the replacements already made.
    let mut delta: isize = 0;
    for (count, ((a, b), text)) in plan.iter().enumerate() {
        let start = shifted(*a, delta)?;
        let end = shifted(*b, delta)?;
        target.set_selection(start, end);
        target.replace_selection(text);
        let (_, caret) = target.selection();
        delta = caret as isize - *b as isize;
        if caret < start {
            return Err(format!("replace all: caret {caret} before replacement start {start} at #{count}"));
        }
    }
    Ok(plan.len())
}

fn shifted(pos: usize, delta: isize) -> Result<usize, String> {
    usize::try_from(pos as isize + delta).map_err(|_| format!("replace all: position {pos} shifted by {delta} is negative"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(match_case: bool, whole_word: bool) -> RegexOptions {
        RegexOptions { match_case, whole_word }
    }

    fn re(p: &str) -> Regex {
        compile(p, opts(true, false)).expect("valid pattern")
    }

    // ── compile ──

    #[test]
    fn compile_is_multi_line() {
        let h = Haystack::new("ab\r\ncd\r\nab");
        assert_eq!(h.matches(&re("^ab$")), vec![(0, 2), (6, 8)]);
    }

    #[test]
    fn compile_ignores_case_unless_match_case() {
        let h = Haystack::new("Foo foo FOO");
        assert_eq!(h.matches(&compile("foo", opts(false, false)).unwrap()).len(), 3);
        assert_eq!(h.matches(&compile("foo", opts(true, false)).unwrap()), vec![(4, 7)]);
    }

    #[test]
    fn compile_whole_word_wraps_the_pattern() {
        let h = Haystack::new("cat concat cat_ cats cat");
        assert_eq!(h.matches(&compile("cat", opts(true, true)).unwrap()), vec![(0, 3), (21, 24)]);
        // The alternation stays inside the group.
        let h = Haystack::new("ab xb a b");
        assert_eq!(h.matches(&compile("a|b", opts(true, true)).unwrap()), vec![(6, 7), (8, 9)]);
    }

    #[test]
    fn compile_rejects_an_invalid_pattern() {
        assert!(compile("(abc", opts(false, false)).is_err());
        assert!(compile("[z-a]", opts(false, true)).is_err());
    }

    #[test]
    fn compile_whole_word_does_not_fix_an_invalid_pattern() {
        assert!(compile("a)|(?:b", opts(false, false)).is_err());
        assert!(compile("a)|(?:b", opts(false, true)).is_err());
    }

    #[test]
    fn compile_refuses_a_pattern_over_the_size_limit() {
        let err = compile(r"\w{1000}{1000}", opts(false, false)).unwrap_err();
        assert!(matches!(err, regex::Error::CompiledTooBig(_)), "{err:?}");
    }

    #[test]
    fn compile_handles_unicode_classes_and_case() {
        let h = Haystack::new("ÉTÉ été");
        assert_eq!(h.matches(&compile("été", opts(false, false)).unwrap()).len(), 2);
        assert_eq!(h.matches(&compile(r"\w+", opts(true, true)).unwrap()), vec![(0, 3), (4, 7)]);
    }

    // ── positions ──

    #[test]
    fn positions_count_utf16_units_with_emoji() {
        // 😀 is 2 UTF-16 units, é is 1.
        let h = Haystack::new("😀é x😀y");
        assert_eq!(h.matches(&re("x")), vec![(4, 5)]);
        assert_eq!(h.matches(&re("😀")), vec![(0, 2), (5, 7)]);
        assert_eq!(h.matches(&re(".y")), vec![(5, 8)]);
    }

    #[test]
    fn positions_count_one_per_line_break() {
        assert_eq!(normalize_breaks("a\r\nb\rc\nd"), "a\nb\nc\nd");
        let h = Haystack::new("a\r\nb\rc\nd");
        assert_eq!(h.matches(&re("d")), vec![(6, 7)]);
        assert_eq!(h.matches(&re(r"b\nc")), vec![(2, 5)]);
    }

    #[test]
    fn dollar_and_caret_match_at_crlf_line_breaks() {
        let h = Haystack::new("one
two");
        assert_eq!(h.matches(&re("e$")), vec![(2, 3)]);
        assert_eq!(h.matches(&re("o$")), vec![(6, 7)]);
        assert_eq!(h.matches(&re("^t")), vec![(4, 5)]);
    }

    // ── next / prev ──

    #[test]
    fn next_index_goes_forward_and_wraps() {
        let m = [(0, 2), (5, 7), (9, 11)];
        assert_eq!(next_index(&m, (0, 0)), Some(0));
        assert_eq!(next_index(&m, (0, 2)), Some(1));
        assert_eq!(next_index(&m, (3, 3)), Some(1));
        assert_eq!(next_index(&m, (9, 11)), Some(0));
        assert_eq!(next_index(&[], (0, 0)), None);
    }

    #[test]
    fn prev_index_goes_backward_and_wraps() {
        let m = [(0, 2), (5, 7), (9, 11)];
        assert_eq!(prev_index(&m, (9, 11)), Some(1));
        assert_eq!(prev_index(&m, (8, 8)), Some(1));
        assert_eq!(prev_index(&m, (0, 2)), Some(2));
        assert_eq!(prev_index(&[], (0, 0)), None);
    }

    #[test]
    fn next_index_steps_over_the_empty_match_it_is_on() {
        let m = [(0, 0), (3, 3), (6, 6)];
        assert_eq!(next_index(&m, (0, 0)), Some(1));
        assert_eq!(next_index(&m, (3, 3)), Some(2));
        assert_eq!(next_index(&m, (6, 6)), Some(0));
        // A single empty match: wraps onto itself instead of "not found".
        assert_eq!(next_index(&[(0, 0)], (0, 0)), Some(0));
    }

    #[test]
    fn prev_index_steps_over_the_empty_match_it_is_on() {
        let m = [(0, 0), (3, 3), (6, 6)];
        assert_eq!(prev_index(&m, (6, 6)), Some(1));
        assert_eq!(prev_index(&m, (3, 3)), Some(0));
        assert_eq!(prev_index(&m, (0, 0)), Some(2));
    }

    // ── replacement ──

    #[test]
    fn expand_at_supports_numbered_and_named_groups() {
        let h = Haystack::new("John Smith");
        let r = re(r"(?P<first>\w+) (\w+)");
        assert_eq!(h.expand_at(&r, (0, 10), "$2, ${first}").as_deref(), Some("Smith, John"));
        assert_eq!(h.expand_at(&r, (0, 4), "$2"), None);
    }

    #[test]
    fn expand_at_maps_positions_after_emoji() {
        let h = Haystack::new("😀 key=val");
        let r = re(r"(\w+)=(\w+)");
        assert_eq!(h.expand_at(&r, (3, 10), "$2=$1").as_deref(), Some("val=key"));
    }

    #[test]
    fn replace_plan_lists_every_match_with_its_text() {
        let h = Haystack::new("a1 b22");
        let plan = h.replace_plan(&re(r"(\d+)"), "<$1>");
        assert_eq!(plan, vec![((1, 2), "<1>".to_string()), ((4, 6), "<22>".to_string())]);
    }

    #[test]
    fn replace_plan_of_empty_matches_is_finite() {
        let h = Haystack::new("ab\r\ncd");
        let plan = h.replace_plan(&re("x*"), "-");
        let spans: Vec<Span> = plan.iter().map(|p| p.0).collect();
        assert_eq!(spans, vec![(0, 0), (1, 1), (2, 2), (3, 3), (4, 4), (5, 5)]);
    }

    // ── flows over a fake RichEdit ──

    /// Behaves like the RichEdit: `\r` line breaks inside, `\r\n` out,
    /// UTF-16 positions, the caret after a replacement.
    struct FakeEdit {
        units: Vec<u16>,
        sel: Span,
        replaces: usize,
    }

    impl FakeEdit {
        fn new(text: &str) -> Self {
            FakeEdit { units: normalize_breaks(text).replace('\n', "\r").encode_utf16().collect(), sel: (0, 0), replaces: 0 }
        }
        fn plain(&self) -> String {
            String::from_utf16(&self.units).unwrap().replace('\r', "\n")
        }
        fn selected(&self) -> String {
            String::from_utf16(&self.units[self.sel.0..self.sel.1]).unwrap()
        }
    }

    impl RegexTarget for FakeEdit {
        fn text(&self) -> String {
            String::from_utf16(&self.units).unwrap().replace('\r', "\r\n")
        }
        fn selection(&self) -> Span {
            self.sel
        }
        fn set_selection(&mut self, start: usize, end: usize) {
            assert!(start <= end && end <= self.units.len(), "selection {start}..{end} out of {}", self.units.len());
            self.sel = (start, end);
        }
        fn replace_selection(&mut self, text: &str) {
            let ins: Vec<u16> = normalize_breaks(text).replace('\n', "\r").encode_utf16().collect();
            let (a, b) = self.sel;
            self.units.splice(a..b, ins.iter().copied());
            self.sel = (a + ins.len(), a + ins.len());
            self.replaces += 1;
        }
    }

    #[test]
    fn find_next_walks_every_match_then_wraps() {
        let mut ed = FakeEdit::new("cat\r\n😀cat cat");
        let r = re("cat");
        let seen: Vec<_> = (0..4).map(|_| (find_next(&mut ed, &r), ed.selected())).collect();
        assert_eq!(seen[0], (Some(Found { current: 1, total: 3 }), "cat".to_string()));
        assert_eq!(seen[1].0, Some(Found { current: 2, total: 3 }));
        assert_eq!(seen[2].0, Some(Found { current: 3, total: 3 }));
        assert_eq!(seen[3].0, Some(Found { current: 1, total: 3 }));
    }

    #[test]
    fn find_prev_walks_backward_then_wraps() {
        let mut ed = FakeEdit::new("x1 x2 x3");
        let r = re(r"x\d");
        assert_eq!(find_prev(&mut ed, &r), Some(Found { current: 3, total: 3 }));
        assert_eq!(ed.selected(), "x3");
        assert_eq!(find_prev(&mut ed, &r), Some(Found { current: 2, total: 3 }));
        assert_eq!(ed.selected(), "x2");
    }

    #[test]
    fn find_next_and_prev_progress_on_empty_matches() {
        let mut ed = FakeEdit::new("a\r\nb\r\nc");
        let r = re("^");
        let fwd: Vec<Span> = (0..4).map(|_| { find_next(&mut ed, &r); ed.sel }).collect();
        assert_eq!(fwd, vec![(2, 2), (4, 4), (0, 0), (2, 2)]);
        let back: Vec<Span> = (0..3).map(|_| { find_prev(&mut ed, &r); ed.sel }).collect();
        assert_eq!(back, vec![(0, 0), (4, 4), (2, 2)]);
    }

    #[test]
    fn find_next_on_no_match_leaves_the_selection() {
        let mut ed = FakeEdit::new("abc");
        ed.sel = (1, 2);
        assert_eq!(find_next(&mut ed, &re("z")), None);
        assert_eq!(ed.sel, (1, 2));
    }

    #[test]
    fn replace_one_replaces_the_selected_match_then_selects_the_next() {
        let mut ed = FakeEdit::new("k1=v1, k2=v2");
        let r = re(r"(\w+)=(\w+)");
        assert_eq!(find_next(&mut ed, &r), Some(Found { current: 1, total: 2 }));
        assert_eq!(replace_one(&mut ed, &r, "$2=$1"), Some(Found { current: 2, total: 2 }));
        assert_eq!(ed.plain(), "v1=k1, k2=v2");
        assert_eq!(ed.selected(), "k2=v2");
        assert_eq!(replace_one(&mut ed, &r, "${2}=$1"), Some(Found { current: 1, total: 2 }));
        assert_eq!(ed.plain(), "v1=k1, v2=k2");
        assert_eq!(ed.replaces, 2);
    }

    #[test]
    fn replace_one_leaves_a_selection_that_is_not_a_match() {
        let mut ed = FakeEdit::new("abc a1");
        ed.sel = (0, 3);
        assert_eq!(replace_one(&mut ed, &re(r"a\d"), "X"), Some(Found { current: 1, total: 1 }));
        assert_eq!(ed.plain(), "abc a1");
        assert_eq!(ed.selected(), "a1");
        assert_eq!(ed.replaces, 0);
    }

    #[test]
    fn replace_one_on_empty_matches_progresses() {
        let mut ed = FakeEdit::new("a\r\nb");
        let r = re("^");
        // Caret at 0 is the empty match at the first line start.
        replace_one(&mut ed, &r, "> ");
        assert_eq!(ed.plain(), "> a\nb");
        replace_one(&mut ed, &r, "> ");
        assert_eq!(ed.plain(), "> a\n> b");
    }

    #[test]
    fn replace_all_replaces_every_match_once_per_step() {
        let mut ed = FakeEdit::new("a=1\r\n😀 bb=22\r\nc=3");
        let n = replace_all(&mut ed, &re(r"^(\w+)=(\d+)$"), "$2:$1").unwrap();
        assert_eq!(n, 2);
        assert_eq!(ed.replaces, 2);
        assert_eq!(ed.plain(), "1:a\n😀 bb=22\n3:c");
    }

    #[test]
    fn replace_all_shifts_positions_after_longer_and_shorter_texts() {
        let mut ed = FakeEdit::new("x é x 😀 x");
        assert_eq!(replace_all(&mut ed, &re("x"), "long😀").unwrap(), 3);
        assert_eq!(ed.plain(), "long😀 é long😀 😀 long😀");
        assert_eq!(replace_all(&mut ed, &re("long😀"), "").unwrap(), 3);
        assert_eq!(ed.plain(), " é  😀 ");
    }

    #[test]
    fn replace_all_terminates_on_empty_matches() {
        let mut ed = FakeEdit::new("ab\r\ncd");
        assert_eq!(replace_all(&mut ed, &re("^"), "# ").unwrap(), 2);
        assert_eq!(ed.plain(), "# ab\n# cd");
        let mut ed = FakeEdit::new("ab");
        assert_eq!(replace_all(&mut ed, &re("x*"), "-").unwrap(), 3);
        assert_eq!(ed.plain(), "-a-b-");
    }

    #[test]
    fn replace_all_with_a_group_holding_a_line_break() {
        let mut ed = FakeEdit::new("a\r\nb c\r\nd");
        assert_eq!(replace_all(&mut ed, &re(r"(\w)\n(\w)"), "$2\n$1").unwrap(), 2);
        assert_eq!(ed.plain(), "b\na d\nc");
    }

    #[test]
    fn replace_all_without_match_changes_nothing() {
        let mut ed = FakeEdit::new("abc");
        assert_eq!(replace_all(&mut ed, &re("z"), "y").unwrap(), 0);
        assert_eq!(ed.replaces, 0);
        assert_eq!(ed.plain(), "abc");
    }
}
