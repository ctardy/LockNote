// Block indent / outdent with Tab and Shift+Tab in the editor.
//
// `block_indent` decides what a Tab or Shift+Tab does to the text and the
// selection, in RichEdit positions (UTF-16 code units, `\r\n` = 1).
// `run_indent` applies that decision through `IndentSteps`: the whole block
// is rewritten with a single EM_REPLACESEL, so one Ctrl+Z undoes it.
// The editor key handler (events.rs) calls `handle_tab_indent` on VK_TAB.

#[allow(unused_imports)]
use super::*;

/// Tab (indent) or Shift+Tab (outdent).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum IndentDir {
    In,
    Out,
}

/// Replace the RichEdit range `start..end` with `text`.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Replacement {
    pub start: u32,
    pub end: u32,
    pub text: String,
}

/// What a Tab or Shift+Tab does.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum IndentAction {
    /// The key goes on to the control: Tab on an empty or one-line
    /// selection inserts a tab as before.
    Native,
    /// The key is consumed: optional replacement, then this selection.
    Apply { replace: Option<Replacement>, select: (u32, u32) },
}

/// Most leading spaces one Shift+Tab removes from a line without a leading tab.
const OUTDENT_SPACES: usize = 4;

fn is_break(c: char) -> bool {
    c == '\n' || c == '\r'
}

/// Byte offset of the start of the line holding byte `at`.
fn line_start_at(text: &str, at: usize) -> usize {
    text[..at].rfind(is_break).map(|p| p + 1).unwrap_or(0)
}

/// Byte offset of the end of the line holding byte `at` (before its break).
fn line_end_at(text: &str, at: usize) -> usize {
    text[at..].find(is_break).map(|p| at + p).unwrap_or(text.len())
}

/// Bytes of indentation one outdent removes from `line`: one leading tab,
/// or else up to `OUTDENT_SPACES` leading spaces. All ASCII, so the count is
/// also the number of RichEdit positions.
fn outdent_len(line: &str) -> usize {
    if line.starts_with('\t') {
        return 1;
    }
    line.bytes().take(OUTDENT_SPACES).take_while(|b| *b == b' ').count()
}

/// Split `s` into `(line, break)` pairs, the break being `\r\n`, `\r`, `\n`
/// or "" for the last line.
fn split_keep_breaks(s: &str) -> Vec<(&str, &str)> {
    let mut out = Vec::new();
    let mut rest = s;
    loop {
        match rest.find(is_break) {
            None => {
                out.push((rest, ""));
                return out;
            }
            Some(i) => {
                let blen = if rest[i..].starts_with("\r\n") { 2 } else { 1 };
                out.push((&rest[..i], &rest[i..i + blen]));
                rest = &rest[i + blen..];
            }
        }
    }
}

/// Decide what Tab (`In`) or Shift+Tab (`Out`) does with the selection
/// `sel_start..sel_end` (RichEdit positions; past-the-end clamps to the end).
///
/// - Selection across a line break: every touched line gets a leading tab
///   (`In`) or loses one leading tab, else up to 4 leading spaces (`Out`);
///   the selection then covers the touched lines entirely. A selection
///   ending at column 0 of a line does not touch that line.
/// - Empty or one-line selection: `In` is `Native` (the control inserts the
///   tab); `Out` removes one indentation level from the current line.
///
/// A position inside a surrogate pair is an error.
pub(super) fn block_indent(
    text: &str,
    sel_start: u32,
    sel_end: u32,
    dir: IndentDir,
) -> Result<IndentAction, Utf16RangeError> {
    let len = utf16_len(text) as u32;
    let (s, e) = (sel_start.min(sel_end).min(len), sel_start.max(sel_end).min(len));
    let b0 = utf16_to_byte_exact(text, s as usize)?;
    let b1 = utf16_to_byte_exact(text, e as usize)?;
    let offsets = build_utf16_offset_map(text);

    let first = line_start_at(text, b0);
    let last_line_start = line_start_at(text, b1);

    if last_line_start == first {
        // Empty or one-line selection.
        if dir == IndentDir::In {
            return Ok(IndentAction::Native);
        }
        let line = &text[first..line_end_at(text, first)];
        let k = outdent_len(line) as u32;
        if k == 0 {
            return Ok(IndentAction::Apply { replace: None, select: (s, e) });
        }
        let first_u16 = offsets[first] as u32;
        let shift = |p: u32| first_u16 + (p - first_u16).saturating_sub(k);
        return Ok(IndentAction::Apply {
            replace: Some(Replacement { start: first_u16, end: first_u16 + k, text: String::new() }),
            select: (shift(s), shift(e)),
        });
    }

    // Block: the last touched line ends before the break that precedes a
    // column-0 end, else at the end of the line holding the end.
    let last_end = if b1 == last_line_start {
        if text[..b1].ends_with("\r\n") { b1 - 2 } else { b1 - 1 }
    } else {
        line_end_at(text, b1)
    };

    let region = &text[first..last_end];
    let mut new_region = String::with_capacity(region.len() + 16);
    for (line, brk) in split_keep_breaks(region) {
        match dir {
            IndentDir::In => {
                new_region.push('\t');
                new_region.push_str(line);
            }
            IndentDir::Out => new_region.push_str(&line[outdent_len(line)..]),
        }
        new_region.push_str(brk);
    }

    let start_u16 = offsets[first] as u32;
    let end_u16 = offsets[last_end] as u32;
    if new_region == region {
        return Ok(IndentAction::Apply { replace: None, select: (start_u16, end_u16) });
    }
    let new_end = start_u16 + utf16_len(&new_region) as u32;
    Ok(IndentAction::Apply {
        replace: Some(Replacement { start: start_u16, end: end_u16, text: new_region }),
        select: (start_u16, new_end),
    })
}

/// What `run_indent` needs from the editor.
pub(super) trait IndentSteps {
    fn text(&self) -> String;
    /// The selection in RichEdit positions.
    fn selection(&self) -> (u32, u32);
    fn set_selection(&mut self, start: u32, end: u32);
    /// One undoable EM_REPLACESEL of the selection.
    fn replace_selection(&mut self, s: &str);
    fn mark_modified(&mut self);
}

/// Apply Tab / Shift+Tab. Returns true when the key is consumed (the
/// caller then swallows the WM_CHAR tab that follows), false when the
/// control must handle it.
pub(super) fn run_indent(steps: &mut impl IndentSteps, dir: IndentDir) -> bool {
    let text = steps.text();
    let (s, e) = steps.selection();
    match block_indent(&text, s, e, dir) {
        Ok(IndentAction::Native) => false,
        Ok(IndentAction::Apply { replace, select }) => {
            if let Some(r) = replace {
                steps.set_selection(r.start, r.end);
                steps.replace_selection(&r.text);
                steps.mark_modified();
            }
            steps.set_selection(select.0, select.1);
            true
        }
        Err(err) => {
            // Consumed without change: letting the control replace the
            // selection with a tab would be a surprise edit.
            eprintln!("ERROR indent: dir={:?} selection={}..{} not mappable: {:?}", dir, s, e, err);
            true
        }
    }
}

/// The editor behind `IndentSteps`.
struct FormIndentSteps<'a> {
    form: &'a EditorForm,
}

impl IndentSteps for FormIndentSteps<'_> {
    fn text(&self) -> String {
        self.form.get_content()
    }
    fn selection(&self) -> (u32, u32) {
        let sel = self.form.editor.selection();
        (sel.start, sel.end)
    }
    fn set_selection(&mut self, start: u32, end: u32) {
        self.form.editor.set_selection(start..end);
    }
    fn replace_selection(&mut self, s: &str) {
        self.form.replace_selection_utf16(s);
    }
    fn mark_modified(&mut self) {
        self.form.mark_modified();
    }
}

impl EditorForm {
    /// Tab (`outdent == false`) or Shift+Tab in the editor. Returns true when
    /// the key was consumed.
    pub(super) fn handle_tab_indent(&self, outdent: bool) -> bool {
        // A consumed key skips handle_shortcut, which resets the timer.
        self.autolock.borrow_mut().reset();
        let dir = if outdent { IndentDir::Out } else { IndentDir::In };
        run_indent(&mut FormIndentSteps { form: self }, dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(text: &str, s: u32, e: u32, dir: IndentDir) -> IndentAction {
        block_indent(text, s, e, dir).expect("mappable selection")
    }

    fn repl(start: u32, end: u32, text: &str) -> Option<Replacement> {
        Some(Replacement { start, end, text: text.to_string() })
    }

    /// The text after applying the replacement, for readable assertions.
    fn result_text(text: &str, a: &IndentAction) -> String {
        match a {
            IndentAction::Apply { replace: Some(r), .. } => {
                let (b0, b1) = utf16_range_to_byte_range(text, r.start as usize, r.end as usize);
                format!("{}{}{}", &text[..b0], r.text, &text[b1..])
            }
            _ => text.to_string(),
        }
    }

    // ── one-line selection ──

    #[test]
    fn tab_on_caret_is_native() {
        assert_eq!(apply("abc", 1, 1, IndentDir::In), IndentAction::Native);
    }

    #[test]
    fn tab_on_one_line_selection_is_native() {
        assert_eq!(apply("abc\ndef", 4, 6, IndentDir::In), IndentAction::Native);
    }

    #[test]
    fn tab_on_empty_text_is_native() {
        assert_eq!(apply("", 0, 0, IndentDir::In), IndentAction::Native);
    }

    #[test]
    fn shift_tab_removes_leading_tab_of_current_line() {
        // caret after "\tab" on line 2
        let a = apply("x\n\tabc", 5, 5, IndentDir::Out);
        assert_eq!(a, IndentAction::Apply { replace: repl(2, 3, ""), select: (4, 4) });
        assert_eq!(result_text("x\n\tabc", &a), "x\nabc");
    }

    #[test]
    fn shift_tab_removes_one_tab_only() {
        let a = apply("\t\tabc", 0, 0, IndentDir::Out);
        assert_eq!(a, IndentAction::Apply { replace: repl(0, 1, ""), select: (0, 0) });
    }

    #[test]
    fn shift_tab_removes_up_to_four_spaces() {
        let a = apply("      abc", 7, 7, IndentDir::Out);
        assert_eq!(a, IndentAction::Apply { replace: repl(0, 4, ""), select: (3, 3) });
        let a = apply("  abc", 3, 3, IndentDir::Out);
        assert_eq!(a, IndentAction::Apply { replace: repl(0, 2, ""), select: (1, 1) });
    }

    #[test]
    fn shift_tab_caret_inside_indent_goes_to_line_start() {
        let a = apply("    abc", 2, 2, IndentDir::Out);
        assert_eq!(a, IndentAction::Apply { replace: repl(0, 4, ""), select: (0, 0) });
    }

    #[test]
    fn shift_tab_prefers_tab_over_spaces() {
        // a leading tab is one level, the spaces after it stay
        let a = apply("\t    x", 0, 0, IndentDir::Out);
        assert_eq!(result_text("\t    x", &a), "    x");
    }

    #[test]
    fn shift_tab_without_indent_changes_nothing() {
        assert_eq!(
            apply("abc", 1, 2, IndentDir::Out),
            IndentAction::Apply { replace: None, select: (1, 2) }
        );
        assert_eq!(apply("", 0, 0, IndentDir::Out), IndentAction::Apply { replace: None, select: (0, 0) });
    }

    #[test]
    fn shift_tab_one_line_selection_shifts_both_ends() {
        let a = apply("\tabcdef", 2, 5, IndentDir::Out);
        assert_eq!(a, IndentAction::Apply { replace: repl(0, 1, ""), select: (1, 4) });
    }

    // ── block ──

    #[test]
    fn tab_block_indents_every_touched_line() {
        let t = "aa\nbb\ncc";
        // from inside "aa" to inside "bb"
        let a = apply(t, 1, 4, IndentDir::In);
        assert_eq!(a, IndentAction::Apply { replace: repl(0, 5, "\taa\n\tbb"), select: (0, 7) });
        assert_eq!(result_text(t, &a), "\taa\n\tbb\ncc");
    }

    #[test]
    fn tab_block_end_at_column_zero_excludes_that_line() {
        let t = "aa\nbb\ncc";
        // "aa\nbb\n" selected: ends at column 0 of "cc"
        let a = apply(t, 0, 6, IndentDir::In);
        assert_eq!(result_text(t, &a), "\taa\n\tbb\ncc");
        assert_eq!(a, IndentAction::Apply { replace: repl(0, 5, "\taa\n\tbb"), select: (0, 7) });
    }

    #[test]
    fn tab_whole_line_with_its_break_indents_that_line() {
        // A selection across one line break that ends at column 0 touches one line.
        let t = "aa\nbb";
        let a = apply(t, 0, 3, IndentDir::In);
        assert_eq!(a, IndentAction::Apply { replace: repl(0, 2, "\taa"), select: (0, 3) });
    }

    #[test]
    fn tab_block_indents_empty_lines_too() {
        let t = "a\n\nb";
        let a = apply(t, 0, 4, IndentDir::In);
        assert_eq!(result_text(t, &a), "\ta\n\t\n\tb");
    }

    #[test]
    fn shift_tab_block_outdents_every_touched_line() {
        let t = "\taa\n      bb\n  cc\ndd\n\tee";
        let a = apply(t, 1, 19, IndentDir::Out); // "aa" to "dd"
        assert_eq!(result_text(t, &a), "aa\n  bb\ncc\ndd\n\tee");
        match a {
            IndentAction::Apply { select, .. } => assert_eq!(select, (0, 13)),
            other => panic!("{:?}", other),
        }
    }

    #[test]
    fn shift_tab_block_without_indent_selects_lines_only() {
        let t = "aa\nbb\ncc";
        assert_eq!(
            apply(t, 1, 4, IndentDir::Out),
            IndentAction::Apply { replace: None, select: (0, 5) }
        );
    }

    #[test]
    fn block_crlf_counts_break_as_one() {
        // RichEdit positions: a=0 a=1 \r\n=2 b=3 b=4
        let t = "aa\r\nbb\r\ncc";
        let a = apply(t, 1, 4, IndentDir::In);
        assert_eq!(a, IndentAction::Apply { replace: repl(0, 5, "\taa\r\n\tbb"), select: (0, 7) });
        assert_eq!(result_text(t, &a), "\taa\r\n\tbb\r\ncc");
    }

    #[test]
    fn block_crlf_end_at_column_zero() {
        let t = "aa\r\nbb\r\ncc";
        let a = apply(t, 0, 6, IndentDir::In); // up to column 0 of "cc"
        assert_eq!(result_text(t, &a), "\taa\r\n\tbb\r\ncc");
    }

    #[test]
    fn block_lone_cr_breaks() {
        // RichEdit's own paragraph mark
        let t = "aa\rbb\rcc";
        let a = apply(t, 0, 7, IndentDir::In);
        assert_eq!(result_text(t, &a), "\taa\r\tbb\r\tcc");
        match a {
            IndentAction::Apply { select, .. } => assert_eq!(select, (0, 11)),
            other => panic!("{:?}", other),
        }
    }

    #[test]
    fn block_after_emoji_uses_utf16_positions() {
        // 😀 = 2 code units: positions 😀=0..2 \n=2 b=3
        let t = "😀\nb\nc";
        let a = apply(t, 0, 4, IndentDir::In);
        assert_eq!(a, IndentAction::Apply { replace: repl(0, 4, "\t😀\n\tb"), select: (0, 6) });
    }

    #[test]
    fn block_reversed_and_past_end_selection_is_clamped() {
        let t = "aa\nbb";
        let a = apply(t, 99, 1, IndentDir::In);
        assert_eq!(result_text(t, &a), "\taa\n\tbb");
    }

    #[test]
    fn block_selection_can_be_indented_again() {
        // the new selection touches exactly the same lines
        let t = "aa\nbb\ncc";
        let a = apply(t, 1, 4, IndentDir::In);
        let (t2, sel) = match &a {
            IndentAction::Apply { select, .. } => (result_text(t, &a), *select),
            other => panic!("{:?}", other),
        };
        let b = apply(&t2, sel.0, sel.1, IndentDir::In);
        assert_eq!(result_text(&t2, &b), "\t\taa\n\t\tbb\ncc");
        let c = apply(&result_text(&t2, &b), 0, 9, IndentDir::Out);
        assert_eq!(result_text(&result_text(&t2, &b), &c), "\taa\n\tbb\ncc");
    }

    #[test]
    fn mid_surrogate_is_error() {
        assert_eq!(
            block_indent("😀\nb", 1, 4, IndentDir::In),
            Err(Utf16RangeError::MidSurrogate { pos: 1 })
        );
    }

    // ── sequence (run_indent) ──

    #[derive(Debug, PartialEq)]
    enum Call {
        Select(u32, u32),
        Replace(String),
        Modified,
    }

    struct Fake {
        text: String,
        sel: (u32, u32),
        calls: Vec<Call>,
    }

    impl Fake {
        fn new(text: &str, s: u32, e: u32) -> Self {
            Fake { text: text.to_string(), sel: (s, e), calls: Vec::new() }
        }
    }

    impl IndentSteps for Fake {
        fn text(&self) -> String {
            self.text.clone()
        }
        fn selection(&self) -> (u32, u32) {
            self.sel
        }
        fn set_selection(&mut self, start: u32, end: u32) {
            self.sel = (start, end);
            self.calls.push(Call::Select(start, end));
        }
        fn replace_selection(&mut self, s: &str) {
            let (b0, b1) = utf16_range_to_byte_range(&self.text, self.sel.0 as usize, self.sel.1 as usize);
            self.text = format!("{}{}{}", &self.text[..b0], s, &self.text[b1..]);
            let p = self.sel.0 + utf16_len(s) as u32;
            self.sel = (p, p);
            self.calls.push(Call::Replace(s.to_string()));
        }
        fn mark_modified(&mut self) {
            self.calls.push(Call::Modified);
        }
    }

    #[test]
    fn seq_block_tab_is_one_replace_then_select() {
        let mut f = Fake::new("aa\nbb\ncc", 1, 4);
        assert!(run_indent(&mut f, IndentDir::In));
        assert_eq!(
            f.calls,
            vec![
                Call::Select(0, 5),
                Call::Replace("\taa\n\tbb".into()),
                Call::Modified,
                Call::Select(0, 7),
            ]
        );
        assert_eq!(f.text, "\taa\n\tbb\ncc");
        assert_eq!(f.sel, (0, 7));
    }

    #[test]
    fn seq_tab_twice_then_shift_tab_twice_restores_text() {
        let mut f = Fake::new("aa\r\n    bb\r\ncc", 1, 5);
        assert!(run_indent(&mut f, IndentDir::In));
        assert!(run_indent(&mut f, IndentDir::In));
        assert_eq!(f.text, "\t\taa\r\n\t\t    bb\r\ncc");
        assert!(run_indent(&mut f, IndentDir::Out));
        assert!(run_indent(&mut f, IndentDir::Out));
        assert_eq!(f.text, "aa\r\n    bb\r\ncc");
        // one more: only the spaces of line 2 go
        assert!(run_indent(&mut f, IndentDir::Out));
        assert_eq!(f.text, "aa\r\nbb\r\ncc");
        assert_eq!(f.sel, (0, 5));
    }

    #[test]
    fn seq_plain_tab_on_caret_leaves_editor_untouched() {
        let mut f = Fake::new("abc", 1, 1);
        assert!(!run_indent(&mut f, IndentDir::In));
        assert!(f.calls.is_empty());
    }

    #[test]
    fn seq_shift_tab_without_indent_is_consumed_without_edit() {
        let mut f = Fake::new("abc", 1, 1);
        assert!(run_indent(&mut f, IndentDir::Out));
        assert_eq!(f.calls, vec![Call::Select(1, 1)]);
        assert_eq!(f.text, "abc");
    }

    #[test]
    fn seq_unmappable_selection_is_consumed_without_edit() {
        let mut f = Fake::new("😀\nb", 1, 4);
        assert!(run_indent(&mut f, IndentDir::In));
        assert!(f.calls.is_empty());
        assert_eq!(f.text, "😀\nb");
    }

    // ── wiring ──

    #[test]
    fn editor_key_handler_routes_tab_and_swallows_its_char() {
        let ev = crate::read_source(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("src").join("ui").join("editor").join("events.rs"),
        );
        assert!(ev.contains("vk == 0x09 && !ctrl && f_ed.handle_tab_indent(shift)"));
        assert!(ev.contains("TAB_SUPPRESS_CHAR.store(true"));
        assert!(ev.contains("w_param == 0x09"));
        assert!(ev.contains("TAB_SUPPRESS_CHAR.swap(false"));
    }
}
