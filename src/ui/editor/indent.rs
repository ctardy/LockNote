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
pub(super) fn line_start_at(text: &str, at: usize) -> usize {
    text[..at].rfind(is_break).map(|p| p + 1).unwrap_or(0)
}

/// Byte offset of the end of the line holding byte `at` (before its break).
pub(super) fn line_end_at(text: &str, at: usize) -> usize {
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
///   ending at column 0 of a line does not touch that line, and still ends
///   at column 0 of that line afterwards, so a second Tab touches the same
///   lines. This holds even when a single line is touched: a whole line
///   with its break (Home, then Shift+Down) is indented, not replaced by a
///   tab.
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
    let b0 = byte_at_clamped(text, sel_start.min(sel_end))?;
    let b1 = byte_at_clamped(text, sel_start.max(sel_end))?;

    let first = line_start_at(text, b0);
    let last_line_start = line_start_at(text, b1);
    // Decided before the offset map is built: plain Tab stays cheap.
    if last_line_start == first && dir == IndentDir::In {
        return Ok(IndentAction::Native);
    }
    let offsets = build_utf16_offset_map(text);
    let (s, e) = (offsets[b0] as u32, offsets[b1] as u32);

    if last_line_start == first {
        // Empty or one-line selection, Shift+Tab.
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
    // A column-0 end keeps the break after the last touched line selected.
    let tail = if b1 == last_line_start { e - end_u16 } else { 0 };
    if new_region == region {
        return Ok(IndentAction::Apply { replace: None, select: (start_u16, end_u16 + tail) });
    }
    let new_end = start_u16 + utf16_len(&new_region) as u32;
    Ok(IndentAction::Apply {
        replace: Some(Replacement { start: start_u16, end: end_u16, text: new_region }),
        select: (start_u16, new_end + tail),
    })
}

/// Byte offset of RichEdit position `pos`; a position past the end maps to
/// the end of the text.
fn byte_at_clamped(text: &str, pos: u32) -> Result<usize, Utf16RangeError> {
    match utf16_to_byte_exact(text, pos as usize) {
        Err(Utf16RangeError::OutOfBounds { .. }) => Ok(text.len()),
        other => other,
    }
}

/// What `run_indent` (and `lists.rs`) needs from the editor.
pub(super) trait IndentSteps {
    /// The editor text (strict for Tab / Shift+Tab, see `TextRead`).
    fn text(&self) -> Result<String, EditorTextError>;
    /// The text could not be read: nothing was changed, tell the user.
    fn tell_unreadable(&mut self, e: &EditorTextError);
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
    let (s, e) = steps.selection();
    // Tab on a caret: the control inserts it, no need to copy the text.
    if dir == IndentDir::In && s == e {
        return false;
    }
    let text = match steps.text() {
        Ok(text) => text,
        Err(err) => {
            // Consumed without change: the indented lines are rebuilt from
            // the text read, which would not be the editor's.
            steps.tell_unreadable(&err);
            return true;
        }
    };
    match block_indent(&text, s, e, dir) {
        Ok(IndentAction::Native) => false,
        Ok(IndentAction::Apply { replace: None, select }) => {
            // Nothing to remove: leave an unchanged selection alone, so a
            // selection made backwards keeps its direction.
            if select != (s.min(e), s.max(e)) {
                steps.set_selection(select.0, select.1);
            }
            true
        }
        Ok(IndentAction::Apply { replace: Some(r), select }) => {
            steps.set_selection(r.start, r.end);
            steps.replace_selection(&r.text);
            steps.mark_modified();
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

/// How `FormIndentSteps` reads the editor text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TextRead {
    /// Tab / Shift+Tab rewrite the selected lines from the text read: a
    /// lone surrogate refuses the edit (`EditorForm::get_content`).
    Strict,
    /// The list keys insert or remove markers only and use the text for
    /// positions (`EditorForm::display_content`).
    Display,
}

/// The editor behind `IndentSteps`.
pub(super) struct FormIndentSteps<'a> {
    pub(super) form: &'a EditorForm,
    pub(super) read: TextRead,
}

impl IndentSteps for FormIndentSteps<'_> {
    fn text(&self) -> Result<String, EditorTextError> {
        match self.read {
            TextRead::Strict => self.form.get_content(),
            TextRead::Display => self.form.display_content(),
        }
    }
    fn tell_unreadable(&mut self, e: &EditorTextError) {
        self.form.tell_unreadable("indent", e);
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
        run_indent(&mut FormIndentSteps { form: self, read: TextRead::Strict }, dir)
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
        // the selection still ends at column 0 of "cc"
        assert_eq!(a, IndentAction::Apply { replace: repl(0, 5, "\taa\n\tbb"), select: (0, 8) });
    }

    #[test]
    fn tab_whole_line_with_its_break_indents_that_line() {
        // A selection across one line break that ends at column 0 touches one line.
        let t = "aa\nbb";
        // The new selection keeps the break, so it still ends at column 0.
        let a = apply(t, 0, 3, IndentDir::In);
        assert_eq!(a, IndentAction::Apply { replace: repl(0, 2, "\taa"), select: (0, 4) });
    }

    #[test]
    fn shift_tab_whole_line_with_its_break_outdents_that_line() {
        let t = "\taa\nbb";
        let a = apply(t, 0, 4, IndentDir::Out);
        assert_eq!(a, IndentAction::Apply { replace: repl(0, 3, "aa"), select: (0, 3) });
    }

    #[test]
    fn block_end_at_column_zero_without_indent_keeps_selection() {
        assert_eq!(
            apply("aa\r\nbb", 0, 3, IndentDir::Out),
            IndentAction::Apply { replace: None, select: (0, 3) }
        );
    }

    #[test]
    fn block_on_large_text_maps_positions() {
        // Regression guard for the single offset map (not a timing assert).
        let t = "x\r\n".repeat(100_000);
        let len = utf16_len(&t) as u32;
        assert_eq!(apply(&t, len - 2, len - 1, IndentDir::In), IndentAction::Native);
        let a = apply(&t, 0, len, IndentDir::In);
        assert_eq!(result_text(&t, &a).matches('\t').count(), 100_000);
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
        Unreadable(EditorTextError),
    }

    #[test]
    fn seq_unreadable_text_is_told_consumed_and_changes_nothing() {
        // A lone surrogate in the editor: the lines would be rebuilt from a
        // text that is not the editor's. The key is consumed (no native tab
        // replacing the selection) and the user is told once.
        let err = EditorTextError::InvalidUtf16 { unit: 2, line: 1, count: 1 };
        for dir in [IndentDir::In, IndentDir::Out] {
            let mut f = Fake::new("ab\ncd", 0, 4);
            f.unreadable = Some(err.clone());
            assert!(run_indent(&mut f, dir), "{dir:?}: the key must be consumed");
            assert_eq!(f.calls, vec![Call::Unreadable(err.clone())], "{dir:?}");
            assert_eq!(f.text, "ab\ncd");
        }
    }

    struct Fake {
        text: String,
        sel: (u32, u32),
        calls: Vec<Call>,
        text_reads: std::cell::Cell<usize>,
        unreadable: Option<EditorTextError>,
    }

    impl Fake {
        fn new(text: &str, s: u32, e: u32) -> Self {
            Fake { text: text.to_string(), sel: (s, e), calls: Vec::new(), text_reads: Default::default(), unreadable: None }
        }
    }

    impl IndentSteps for Fake {
        fn text(&self) -> Result<String, EditorTextError> {
            self.text_reads.set(self.text_reads.get() + 1);
            match &self.unreadable {
                Some(e) => Err(e.clone()),
                None => Ok(self.text.clone()),
            }
        }
        fn tell_unreadable(&mut self, e: &EditorTextError) {
            self.calls.push(Call::Unreadable(e.clone()));
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
        // the common keystroke does not copy the whole note
        assert_eq!(f.text_reads.get(), 0);
    }

    #[test]
    fn seq_whole_line_tab_twice_never_loses_text() {
        // Home, Shift+Down, then Tab twice and Shift+Tab twice.
        let mut f = Fake::new("aa\nbb", 0, 3);
        assert!(run_indent(&mut f, IndentDir::In));
        assert!(run_indent(&mut f, IndentDir::In));
        assert_eq!(f.text, "\t\taa\nbb");
        assert_eq!(f.sel, (0, 5));
        assert!(run_indent(&mut f, IndentDir::Out));
        assert!(run_indent(&mut f, IndentDir::Out));
        assert_eq!(f.text, "aa\nbb");
        assert_eq!(f.sel, (0, 3));
    }

    #[test]
    fn seq_block_end_at_column_zero_tab_twice_keeps_lines() {
        let mut f = Fake::new("aa\r\nbb\r\ncc", 0, 6);
        assert!(run_indent(&mut f, IndentDir::In));
        assert!(run_indent(&mut f, IndentDir::In));
        assert_eq!(f.text, "\t\taa\r\n\t\tbb\r\ncc");
        assert_eq!(f.sel, (0, 10));
    }

    #[test]
    fn seq_shift_tab_without_indent_is_consumed_without_edit() {
        let mut f = Fake::new("abc", 1, 1);
        assert!(run_indent(&mut f, IndentDir::Out));
        assert!(f.calls.is_empty());
        assert_eq!(f.text, "abc");
    }

    #[test]
    fn seq_shift_tab_without_indent_keeps_backward_selection() {
        // anchor after the caret (Shift+Left): no set_selection to flip it
        let mut f = Fake::new("abc", 2, 1);
        assert!(run_indent(&mut f, IndentDir::Out));
        assert!(f.calls.is_empty());
        assert_eq!(f.sel, (2, 1));
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
        assert!(ev.contains("0x09 => TAB_SUPPRESS_CHAR.swap(false"));
        assert!(ev.contains("TAB_SUPPRESS_CHAR.swap(false"));
    }

    #[test]
    fn editor_char_handler_clears_suppress_flags_before_lock_check() {
        // A lock between the VK and its queued WM_CHAR must not leave a flag
        // set that would swallow the next real character after unlock.
        let ev = crate::read_source(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("src").join("ui").join("editor").join("events.rs"),
        );
        let arm = &ev[ev.find("0x0102 =>").expect("WM_CHAR arm")..];
        let blocked = arm.find("f_ed.input_blocked()").expect("lock check in WM_CHAR arm");
        for flag in ["TAB_SUPPRESS_CHAR", "BULLET_SUPPRESS_ENTER_CHAR", "CTRL_ENTER_SUPPRESS_CHAR"] {
            let swap = arm.find(&format!("{}.swap(false", flag)).expect(flag);
            assert!(swap < blocked, "{} cleared after the lock check", flag);
        }
    }
}
