// Line operations: Alt+Up / Alt+Down move lines, Edit > Lines transforms
// the lines of the selection or of the whole tab (features::line_ops).
//
// Positions are RichEdit character positions (UTF-16 code units, a line
// break = 1, see helpers.rs). The text comes from WM_GETTEXT, so breaks are
// "\r\n"; a lone "\r" or "\n" also ends a line. Every change is one
// EM_REPLACESEL (undoable) over the touched lines: one undo step.

#[allow(unused_imports)]
use super::*;
use crate::features::line_ops::{self, LineOp};

/// A replacement of the RichEdit range `start..end` by `text`, then the
/// selection `sel`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LineEdit {
    pub start: u32,
    pub end: u32,
    pub text: String,
    pub sel: (u32, u32),
}

/// Byte range of every line of `text`, line break excluded. Breaks are
/// "\r\n", "\r" or "\n". Always at least one (empty) line.
pub(super) fn line_ranges(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut lines = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\r' => {
                lines.push((start, i));
                i += if bytes.get(i + 1) == Some(&b'\n') { 2 } else { 1 };
                start = i;
            }
            b'\n' => {
                lines.push((start, i));
                i += 1;
                start = i;
            }
            _ => i += 1,
        }
    }
    lines.push((start, bytes.len()));
    lines
}

/// RichEdit position of byte offset `b` (a char boundary) of `text`, counted
/// as `build_utf16_offset_map` does ("\r\n" = 1) without building the map.
fn position_of(text: &str, b: usize) -> u32 {
    let bytes = text.as_bytes();
    let mut pos = 0u32;
    for (i, ch) in text[..b].char_indices() {
        if ch == '\n' && i > 0 && bytes[i - 1] == b'\r' {
            continue;
        }
        pos += ch.len_utf16() as u32;
    }
    pos
}

/// Index of the line holding byte offset `b` (the last line starting at or
/// before it).
fn line_of(lines: &[(usize, usize)], b: usize) -> usize {
    lines.partition_point(|&(s, _)| s <= b).saturating_sub(1)
}

/// First and last line touched by the RichEdit selection `sel`. A
/// selection ending at column 0 of a line does not include that line.
pub(super) fn touched_lines(text: &str, lines: &[(usize, usize)], sel: (u32, u32)) -> (usize, usize) {
    let (lo, hi) = (sel.0.min(sel.1), sel.0.max(sel.1));
    let (b0, b1) = utf16_range_to_byte_range(text, lo as usize, hi as usize);
    let first = line_of(lines, b0);
    let mut last = line_of(lines, b1);
    if b1 > b0 && last > first && lines[last].0 == b1 {
        last -= 1;
    }
    (first, last)
}

/// Alt+Up (`up`) / Alt+Down: move the touched lines by one line, the
/// selection with them. `None` at the first / last line.
pub(super) fn move_lines_edit(text: &str, sel: (u32, u32), up: bool) -> Option<LineEdit> {
    let lines = line_ranges(text);
    let (first, last) = touched_lines(text, &lines, sel);
    let (a, b) = if up {
        if first == 0 {
            return None;
        }
        (first - 1, last)
    } else {
        if last + 1 >= lines.len() {
            return None;
        }
        (first, last + 1)
    };
    let line = |i: usize| &text[lines[i].0..lines[i].1];
    let block: Vec<&str> = (first..=last).map(line).collect();
    // The line above goes under the block, the line below over it.
    let (order, other): (Vec<&str>, &str) = if up {
        let mut v = block;
        v.push(line(a));
        (v, line(a))
    } else {
        let mut v = vec![line(b)];
        v.extend(block);
        (v, line(b))
    };
    let shift = position_of(other, other.len()) + 1;
    // Both ends lie in the moved block: same shift, direction kept.
    let new_sel = if up { (sel.0 - shift, sel.1 - shift) } else { (sel.0 + shift, sel.1 + shift) };
    Some(LineEdit {
        start: position_of(text, lines[a].0),
        end: position_of(text, lines[b].1),
        text: order.join("\n"),
        sel: new_sel,
    })
}

/// Edit > Lines: apply `op` to the lines touched by the selection when it
/// spans 2 lines or more, to the whole tab otherwise. The whole tab leaves
/// out the empty line after a final break, as a Ctrl+A selection does
/// (column-0 rule). `Ok(None)` when the lines would not change; the
/// replaced lines end up selected.
pub(super) fn line_op_edit<E>(
    text: &str,
    sel: (u32, u32),
    op: LineOp,
    cmp: impl Fn(&str, &str) -> Result<std::cmp::Ordering, E>,
) -> Result<Option<LineEdit>, E> {
    let lines = line_ranges(text);
    let (first, last) = match touched_lines(text, &lines, sel) {
        (f, l) if l > f => (f, l),
        _ => {
            let mut last = lines.len() - 1;
            if last > 0 && lines[last].0 == lines[last].1 {
                last -= 1;
            }
            (0, last)
        }
    };
    let block: Vec<&str> = (first..=last).map(|i| &text[lines[i].0..lines[i].1]).collect();
    let result = line_ops::apply(op, &block, cmp)?;
    if result.iter().map(String::as_str).eq(block.iter().copied()) {
        return Ok(None);
    }
    let start = position_of(text, lines[first].0);
    let replacement = result.join("\n");
    let end_sel = start + utf16_len(&replacement) as u32;
    Ok(Some(LineEdit {
        start,
        end: position_of(text, lines[last].1),
        text: replacement,
        sel: (start, end_sel),
    }))
}

/// `line_op_edit` with the user's locale order (CompareStringEx), as the
/// Edit > Lines menu runs it.
pub(super) fn user_locale_line_op_edit(
    text: &str,
    sel: (u32, u32),
    op: LineOp,
) -> Result<Option<LineEdit>, crate::ui::vault_panel::VaultTreeError> {
    line_op_edit(text, sel, op, crate::ui::vault_panel::compare_names_user_locale)
}

/// What applying a `LineEdit` does to the editor, so the sequence can be
/// tested with a fake.
pub(super) trait LineEditSteps {
    fn text(&self) -> String;
    fn selection(&self) -> (u32, u32);
    fn select(&self, start: u32, end: u32);
    /// Close the current typing undo group, so the edit is its own step.
    fn stop_group_typing(&self);
    /// EM_REPLACESEL with undo.
    fn replace_selection(&self, text: &str);
    fn mark_modified(&self);
}

/// Read the text and the selection, compute the edit, apply it as one
/// undoable replacement and mark the tab modified. Returns whether the text
/// changed; on error nothing was touched.
pub(super) fn run_line_edit<S, E>(
    s: &S,
    compute: impl FnOnce(&str, (u32, u32)) -> Result<Option<LineEdit>, E>,
) -> Result<bool, E>
where
    S: LineEditSteps,
{
    let text = s.text();
    let sel = s.selection();
    let Some(edit) = compute(&text, sel)? else {
        return Ok(false);
    };
    s.select(edit.start, edit.end);
    s.stop_group_typing();
    s.replace_selection(&edit.text);
    s.select(edit.sel.0, edit.sel.1);
    s.mark_modified();
    Ok(true)
}

/// `LineEditSteps` on the main editor (also used by case_ops.rs).
pub(super) struct EditorLineSteps<'a>(pub(super) &'a EditorForm);

impl LineEditSteps for EditorLineSteps<'_> {
    fn text(&self) -> String {
        self.0.get_content()
    }
    /// (anchor, active end) through TOM: EM_GETSEL loses the direction.
    fn selection(&self) -> (u32, u32) {
        let ends = super::tom::hwnd_of(self.0.editor.handle)
            .ok_or_else(|| "no editor window".to_string())
            .and_then(super::tom::selection_ends);
        match ends {
            Ok(ends) => ends,
            Err(e) => {
                // Only the direction is lost: the positions are the same.
                eprintln!("WARN lines: selection direction unreadable, kept forward: {e}");
                let sel = self.0.editor.selection();
                (sel.start, sel.end)
            }
        }
    }
    /// EM_SETSEL(start, end): `end` is the active end, even before `start`.
    fn select(&self, start: u32, end: u32) {
        self.0.editor.set_selection(start..end);
    }
    fn stop_group_typing(&self) {
        self.0.send_editor_message(0x0458, 0, 0); // EM_STOPGROUPTYPING = WM_USER + 88
    }
    fn replace_selection(&self, text: &str) {
        self.0.replace_selection_utf16(text);
    }
    fn mark_modified(&self) {
        self.0.mark_modified();
    }
}

impl EditorForm {
    /// Alt+Up / Alt+Down.
    pub(super) fn on_move_lines(&self, up: bool) {
        let r: Result<bool, std::convert::Infallible> =
            run_line_edit(&EditorLineSteps(self), |text, sel| Ok(move_lines_edit(text, sel, up)));
        let Ok(changed) = r;
        eprintln!("DEBUG lines: move up={} changed={}", up, changed);
        if changed {
            self.reset_occurrence_highlights();
            self.send_editor_message(0x00B7, 0, 0); // EM_SCROLLCARET
        }
    }

    /// EM_REPLACESEL formats the inserted lines like the start of the
    /// replaced range, which may be a highlighted occurrence. Forgetting
    /// the last term makes the next highlight tick clear and repaint all
    /// (as theme_switch.rs does), even when the selected term is unchanged.
    /// An empty term means nothing is highlighted: no pass is needed.
    fn reset_occurrence_highlights(&self) {
        if !self.last_highlighted.borrow().is_empty() {
            *self.last_highlighted.borrow_mut() = "\0".to_string();
        }
    }

    /// Edit > Lines > ...
    pub(super) fn on_line_op(&self, op: LineOp) {
        let r = run_line_edit(&EditorLineSteps(self), |text, sel| user_locale_line_op_edit(text, sel, op));
        match r {
            Ok(changed) => {
                eprintln!("INFO lines: op={:?} changed={}", op, changed);
                if changed {
                    self.reset_occurrence_highlights();
                    self.send_editor_message(0x00B7, 0, 0); // EM_SCROLLCARET
                }
            }
            Err(e) => {
                eprintln!("ERROR lines: op={:?} failed: {:?}", op, e);
                self.inform(&t("lines.error.title"), &t("lines.error.sort").replace("{msg}", &format!("{:?}", e)));
            }
        }
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use std::cell::RefCell;

    fn ci(a: &str, b: &str) -> Result<std::cmp::Ordering, String> {
        Ok(a.to_lowercase().cmp(&b.to_lowercase()))
    }

    // ── line_ranges / touched_lines ──

    #[test]
    fn line_ranges_split_on_every_break_kind() {
        assert_eq!(line_ranges(""), vec![(0, 0)]);
        assert_eq!(line_ranges("a\r\nb\rc\nd"), vec![(0, 1), (3, 4), (5, 6), (7, 8)]);
        assert_eq!(line_ranges("a\r\n"), vec![(0, 1), (3, 3)]);
    }

    #[test]
    fn touched_lines_of_a_caret_is_its_line() {
        let t = "aa\r\nbb\r\ncc";
        let l = line_ranges(t);
        assert_eq!(touched_lines(t, &l, (0, 0)), (0, 0));
        assert_eq!(touched_lines(t, &l, (4, 4)), (1, 1));
        assert_eq!(touched_lines(t, &l, (8, 8)), (2, 2));
    }

    #[test]
    fn selection_ending_at_column_zero_leaves_that_line_out() {
        let t = "aa\r\nbb\r\ncc";
        let l = line_ranges(t);
        // RichEdit positions: line 1 starts at 3, line 2 at 6.
        assert_eq!(touched_lines(t, &l, (1, 6)), (0, 1));
        assert_eq!(touched_lines(t, &l, (1, 7)), (0, 2));
        // A whole line with its break stays one line.
        assert_eq!(touched_lines(t, &l, (3, 6)), (1, 1));
        // Reversed selection (anchor after the caret).
        assert_eq!(touched_lines(t, &l, (7, 1)), (0, 2));
    }

    #[test]
    fn touched_lines_count_utf16_and_crlf_as_richedit() {
        // "😀" is 2 UTF-16 units, "\r\n" is 1 position.
        let t = "😀a\r\nb";
        let l = line_ranges(t);
        assert_eq!(touched_lines(t, &l, (4, 4)), (1, 1));
        assert_eq!(touched_lines(t, &l, (3, 3)), (0, 0));
    }

    // ── move_lines_edit ──

    #[test]
    fn move_up_swaps_with_the_line_above_and_keeps_the_caret_column() {
        let t = "one\r\ntwo\r\nthree";
        // Caret in "two" after "t" (position 5).
        let e = move_lines_edit(t, (5, 5), true).expect("moves");
        assert_eq!((e.start, e.end, e.text.as_str()), (0, 7, "two\none"));
        assert_eq!(e.sel, (1, 1));
    }

    #[test]
    fn move_down_swaps_with_the_line_below() {
        let t = "one\r\ntwo\r\nthree";
        let e = move_lines_edit(t, (1, 1), false).expect("moves");
        assert_eq!((e.start, e.end, e.text.as_str()), (0, 7, "two\none"));
        assert_eq!(e.sel, (5, 5));
    }

    #[test]
    fn move_is_a_no_op_at_the_edges() {
        let t = "one\r\ntwo";
        assert_eq!(move_lines_edit(t, (1, 1), true), None);
        assert_eq!(move_lines_edit(t, (5, 5), false), None);
        assert_eq!(move_lines_edit("", (0, 0), true), None);
        assert_eq!(move_lines_edit("", (0, 0), false), None);
    }

    #[test]
    fn move_carries_every_touched_line_and_the_selection() {
        let t = "a\r\nbb\r\ncc\r\nd";
        // Selection from "bb" (2) to inside "cc" (6): lines 1 and 2.
        let e = move_lines_edit(t, (2, 6), true).expect("moves");
        assert_eq!((e.start, e.end, e.text.as_str()), (0, 7, "bb\ncc\na"));
        assert_eq!(e.sel, (0, 4));
        let e = move_lines_edit(t, (2, 6), false).expect("moves");
        assert_eq!((e.start, e.end, e.text.as_str()), (2, 9, "d\nbb\ncc"));
        assert_eq!(e.sel, (4, 8));
    }

    #[test]
    fn move_ignores_the_line_where_the_selection_ends_at_column_zero() {
        let t = "a\r\nbb\r\ncc";
        // "bb" with its break: 2..5, line 2 not included.
        let e = move_lines_edit(t, (2, 5), true).expect("moves");
        assert_eq!((e.start, e.end, e.text.as_str()), (0, 4, "bb\na"));
        assert_eq!(e.sel, (0, 3));
        let e = move_lines_edit(t, (2, 5), false).expect("moves");
        assert_eq!((e.start, e.end, e.text.as_str()), (2, 7, "cc\nbb"));
        assert_eq!(e.sel, (5, 8));
    }

    #[test]
    fn move_past_a_wide_line_shifts_in_utf16_units() {
        let t = "😀\r\nx";
        let e = move_lines_edit(t, (4, 4), true).expect("moves");
        assert_eq!((e.start, e.end, e.text.as_str()), (0, 4, "x\n😀"));
        assert_eq!(e.sel, (1, 1));
    }

    #[test]
    fn move_down_into_a_trailing_empty_line() {
        let t = "a\r\n";
        let e = move_lines_edit(t, (0, 0), false).expect("moves");
        assert_eq!((e.start, e.end, e.text.as_str()), (0, 2, "\na"));
        assert_eq!(e.sel, (1, 1));
    }

    #[test]
    fn move_keeps_the_direction_of_a_backward_selection() {
        // Anchor in "cc" (6), caret in "bb" (2), as after Shift+Up.
        assert_eq!(move_lines_edit("a\r\nbb\r\ncc", (6, 2), true).expect("moves").sel, (4, 0));
        assert_eq!(move_lines_edit("a\r\nbb\r\ncc\r\nd", (6, 2), true).expect("moves").sel, (4, 0));
        assert_eq!(move_lines_edit("a\r\nbb\r\ncc\r\nd", (6, 2), false).expect("moves").sel, (8, 4));
    }

    #[test]
    fn position_of_counts_like_the_offset_map() {
        let t = "😀a\r\nb\rc\nd\r\n";
        let map = build_utf16_offset_map(t);
        for (b, _) in t.char_indices().chain(std::iter::once((t.len(), ' '))) {
            assert_eq!(position_of(t, b), map[b] as u32, "byte {b}");
        }
    }

    // ── line_op_edit ──

    #[test]
    fn whole_tab_leaves_out_the_empty_line_after_a_final_break() {
        let e = line_op_edit("b\r\na\r\n", (0, 0), LineOp::SortAscending, ci).unwrap().expect("changes");
        assert_eq!(e, LineEdit { start: 0, end: 3, text: "a\nb".into(), sel: (0, 3) });
        let e = line_op_edit("a\r\nb\r\n", (0, 0), LineOp::Join, ci).unwrap().expect("changes");
        assert_eq!((e.start, e.end, e.text.as_str()), (0, 3, "a b"));
        let e = line_op_edit("a\r\n\r\nb\r\n", (0, 0), LineOp::RemoveDuplicates, ci);
        assert_eq!(e, Ok(None), "the only empty line is the trailing one: no duplicate");
        // A tab made of one break alone: nothing to do.
        assert_eq!(line_op_edit("\r\n", (0, 0), LineOp::Join, ci), Ok(None));
    }

    #[test]
    fn whole_tab_and_select_all_give_the_same_edit() {
        let ops = [
            LineOp::SortAscending,
            LineOp::SortDescending,
            LineOp::RemoveDuplicates,
            LineOp::Join,
            LineOp::TrimTrailing,
        ];
        for t in ["b \r\na\r\nb \r\n", "b\r\na", "x\r\n\r\n", "c\r\nb\r\na\r\n\r\n"] {
            let all = (0, utf16_len(t) as u32);
            for op in ops {
                assert_eq!(line_op_edit(t, (0, 0), op, ci), line_op_edit(t, all, op, ci), "{t:?} {op:?}");
            }
        }
    }

    #[test]
    fn op_on_a_single_line_selection_covers_the_whole_tab() {
        let t = "b\r\nc\r\na";
        let e = line_op_edit(t, (0, 1), LineOp::SortAscending, ci).unwrap().expect("changes");
        assert_eq!((e.start, e.end, e.text.as_str()), (0, 5, "a\nb\nc"));
        assert_eq!(e.sel, (0, 5));
    }

    #[test]
    fn op_on_a_multi_line_selection_covers_only_its_lines() {
        let t = "z\r\nc\r\nb\r\na";
        // From "c" (2) to the start of "a" (6): lines 1 and 2 only.
        let e = line_op_edit(t, (2, 6), LineOp::SortAscending, ci).unwrap().expect("changes");
        assert_eq!((e.start, e.end, e.text.as_str()), (2, 5, "b\nc"));
        assert_eq!(e.sel, (2, 5));
    }

    #[test]
    fn op_that_changes_nothing_is_none() {
        assert_eq!(line_op_edit("a\r\nb", (0, 0), LineOp::SortAscending, ci), Ok(None));
        assert_eq!(line_op_edit("a\r\nb", (0, 0), LineOp::TrimTrailing, ci), Ok(None));
        assert_eq!(line_op_edit("", (0, 0), LineOp::Join, ci), Ok(None));
    }

    #[test]
    fn op_join_and_dedup_and_trim_on_the_tab() {
        let t = "a \r\n  b\r\na ";
        let e = line_op_edit(t, (0, 0), LineOp::Join, ci).unwrap().expect("changes");
        assert_eq!(e.text, "a  b a ");
        let e = line_op_edit(t, (0, 0), LineOp::RemoveDuplicates, ci).unwrap().expect("changes");
        assert_eq!((e.text.as_str(), e.sel), ("a \n  b", (0, 6)));
        let e = line_op_edit(t, (0, 0), LineOp::TrimTrailing, ci).unwrap().expect("changes");
        assert_eq!(e.text, "a\n  b\na");
    }

    #[test]
    fn op_sort_error_is_returned() {
        let failing = |_: &str, _: &str| -> Result<std::cmp::Ordering, String> { Err("x".into()) };
        assert_eq!(line_op_edit("b\r\na", (0, 0), LineOp::SortDescending, failing), Err("x".to_string()));
    }

    #[test]
    fn whole_tab_sort_of_100k_lines_in_the_user_locale_stays_fast() {
        // About 1.7M CompareStringEx calls, each re-encoding its two lines
        // to UTF-16: about 1.2 s in a debug build, kept under a budget.
        let t: Vec<String> = (0..100_000).map(|n| format!("line{}", 100_000 - n)).collect();
        let t = t.join("\r\n");
        let started = std::time::Instant::now();
        let e = user_locale_line_op_edit(&t, (0, 0), LineOp::SortAscending).expect("sorts").expect("changes");
        let took = started.elapsed();
        assert_eq!(e.text.split('\n').next(), Some("line1"));
        assert!(took < std::time::Duration::from_secs(2), "took {took:?}");
    }

    // ── run_line_edit: the sequence, with a fake editor ──

    /// A fake RichEdit: a text with "\r\n" breaks (as WM_GETTEXT returns
    /// them), positions counted like RichEdit, and a log of the calls.
    /// Also used by the case_ops.rs tests.
    pub(in crate::ui::editor) struct Fake {
        pub(in crate::ui::editor) text: RefCell<String>,
        pub(in crate::ui::editor) sel: RefCell<(u32, u32)>,
        pub(in crate::ui::editor) log: RefCell<Vec<String>>,
    }

    impl Fake {
        pub(in crate::ui::editor) fn new(text: &str, sel: (u32, u32)) -> Self {
            Fake { text: RefCell::new(text.to_string()), sel: RefCell::new(sel), log: RefCell::new(Vec::new()) }
        }
    }

    impl LineEditSteps for Fake {
        fn text(&self) -> String {
            self.text.borrow().clone()
        }
        fn selection(&self) -> (u32, u32) {
            *self.sel.borrow()
        }
        fn select(&self, start: u32, end: u32) {
            self.log.borrow_mut().push(format!("select:{}..{}", start, end));
            *self.sel.borrow_mut() = (start, end);
        }
        fn stop_group_typing(&self) {
            self.log.borrow_mut().push("stop_group".into());
        }
        fn replace_selection(&self, text: &str) {
            self.log.borrow_mut().push(format!("replace:{:?}", text));
            let current = self.text.borrow().clone();
            let (s, e) = *self.sel.borrow();
            let (b0, b1) = utf16_range_to_byte_range(&current, s as usize, e as usize);
            // RichEdit stores a "\n" as a paragraph break, read back as "\r\n".
            let inserted = text.replace('\n', "\r\n");
            *self.text.borrow_mut() = format!("{}{}{}", &current[..b0], inserted, &current[b1..]);
        }
        fn mark_modified(&self) {
            self.log.borrow_mut().push("modified".into());
        }
    }

    #[test]
    fn sequence_move_selects_replaces_once_then_restores_the_selection() {
        let f = Fake::new("one\r\ntwo\r\nthree", (5, 6));
        let r: Result<bool, ()> = run_line_edit(&f, |t, s| Ok(move_lines_edit(t, s, true)));
        assert_eq!(r, Ok(true));
        assert_eq!(
            *f.log.borrow(),
            vec!["select:0..7", "stop_group", "replace:\"two\\none\"", "select:1..2", "modified"]
        );
        assert_eq!(*f.text.borrow(), "two\r\none\r\nthree");
    }

    #[test]
    fn sequence_moving_twice_down_then_twice_up_restores_the_text() {
        let original = "a\r\nb\r\nc";
        let f = Fake::new(original, (0, 0));
        for up in [false, false] {
            run_line_edit(&f, |t, s| Ok::<_, ()>(move_lines_edit(t, s, up))).unwrap();
        }
        assert_eq!(*f.text.borrow(), "b\r\nc\r\na");
        assert_eq!(*f.sel.borrow(), (4, 4));
        // A third move down is a no-op: nothing is touched.
        let before = f.log.borrow().len();
        assert_eq!(run_line_edit(&f, |t, s| Ok::<_, ()>(move_lines_edit(t, s, false))), Ok(false));
        assert_eq!(f.log.borrow().len(), before);
        for up in [true, true] {
            run_line_edit(&f, |t, s| Ok::<_, ()>(move_lines_edit(t, s, up))).unwrap();
        }
        assert_eq!(*f.text.borrow(), original);
        assert_eq!(*f.sel.borrow(), (0, 0));
    }

    #[test]
    fn sequence_op_without_change_touches_nothing() {
        let f = Fake::new("a\r\nb", (0, 0));
        assert_eq!(run_line_edit(&f, |t, s| line_op_edit(t, s, LineOp::SortAscending, ci)), Ok(false));
        assert!(f.log.borrow().is_empty());
    }

    #[test]
    fn sequence_op_error_touches_nothing() {
        let f = Fake::new("b\r\na", (0, 0));
        let failing = |_: &str, _: &str| -> Result<std::cmp::Ordering, String> { Err("x".into()) };
        assert_eq!(run_line_edit(&f, |t, s| line_op_edit(t, s, LineOp::SortAscending, failing)), Err("x".to_string()));
        assert!(f.log.borrow().is_empty());
        assert_eq!(*f.text.borrow(), "b\r\na");
    }

    #[test]
    fn sequence_sort_of_a_tab_ending_with_a_break_keeps_the_break() {
        let f = Fake::new("b\r\na\r\n", (0, 0));
        assert_eq!(run_line_edit(&f, |t, s| line_op_edit(t, s, LineOp::SortAscending, ci)), Ok(true));
        assert_eq!(*f.text.borrow(), "a\r\nb\r\n");
    }

    #[test]
    fn sequence_sort_then_dedup_on_a_selection() {
        let f = Fake::new("keep\r\nb\r\na\r\nb\r\nlast", (5, 11));
        run_line_edit(&f, |t, s| line_op_edit(t, s, LineOp::SortAscending, ci)).unwrap();
        assert_eq!(*f.text.borrow(), "keep\r\na\r\nb\r\nb\r\nlast");
        // The sorted lines stay selected, so the next op works on them.
        assert_eq!(*f.sel.borrow(), (5, 10));
        run_line_edit(&f, |t, s| line_op_edit(t, s, LineOp::RemoveDuplicates, ci)).unwrap();
        assert_eq!(*f.text.borrow(), "keep\r\na\r\nb\r\nlast");
        assert_eq!(*f.sel.borrow(), (5, 8));
    }

    // ── Code reading: wiring of the shortcuts and the menu ──

    fn src(file: &str) -> String {
        crate::read_source(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ui/editor").join(file))
    }

    #[test]
    fn alt_arrows_reach_handle_shortcut_through_wm_syskeydown() {
        assert_eq!(alt_key_action(0x26), Some(AltKey::MoveLinesUp));
        assert_eq!(alt_key_action(0x28), Some(AltKey::MoveLinesDown));
        assert_eq!(alt_key_action(0x25), None);
        assert_eq!(alt_key_action(0x46), None);
        let events = src("events.rs");
        // Both the main window and the editor subclass route WM_SYSKEYDOWN,
        // and only the Alt shortcuts.
        assert!(events.contains("if msg == 0x0104 && alt_key_action(w_param as u32).is_some()"), "main window");
        assert!(events.contains("0x0104 => {"), "editor subclass");
        // In the editor subclass, only the Alt shortcuts are swallowed while
        // input is blocked: Alt+F4 and the menu keys stay with the system.
        let arm = &events[events.find("0x0104 => {").expect("editor subclass")..];
        let arm = &arm[..arm.find("0x0102 =>").expect("next arm")];
        let filter = arm.find("if alt_key_action(vk).is_some() {").expect("alt filter in the editor subclass");
        let blocked = arm.find("f_ed.input_blocked()").expect("blocked check");
        assert!(filter < blocked, "input_blocked must sit inside the alt_key_action check");
        assert!(arm.contains("if f_ed.handle_shortcut(vk)"), "editor subclass");
        let shortcut = src("edit_ops.rs");
        assert!(shortcut.contains("alt_key_action(vk)"));
        assert!(shortcut.contains("self.on_move_lines(true)") && shortcut.contains("self.on_move_lines(false)"));
    }

    #[test]
    fn line_edits_reset_the_occurrence_highlights() {
        // EM_REPLACESEL may carry a highlight background over the inserted
        // lines; forgetting the last term makes the next tick repaint them.
        let code = src("line_ops.rs");
        let code = &code[..code.find("#[cfg(test)]").expect("test module")];
        for f in ["fn on_move_lines", "fn on_line_op"] {
            let body = &code[code.find(f).unwrap_or_else(|| panic!("{f}"))..];
            let body = &body[..body.find("\n    }\n").expect("end of fn")];
            assert!(body.contains("self.reset_occurrence_highlights()"), "{f}");
        }
        let reset = &code[code.find("fn reset_occurrence_highlights").expect("reset fn")..];
        let reset = &reset[..reset.find("\n    }\n").expect("end of fn")];
        // Only when something is highlighted: an empty term keeps the early
        // return of the highlight tick, so no whole-text pass per Alt+Up.
        let guard = reset.find("if !self.last_highlighted.borrow().is_empty() {").expect("empty-term guard");
        let set = reset.find("*self.last_highlighted.borrow_mut() = \"\\0\".to_string()").expect("reset");
        assert!(guard < set);
    }

    #[test]
    fn highlight_pass_formats_with_the_undo_suspended() {
        // Every background change of the pass is inside without_undo, so it
        // never becomes an undo step ahead of a line op (one Ctrl+Z).
        let code = src("highlight.rs");
        let body = &code[code.find("fn update_occurrence_highlights").expect("pass")..];
        let body = &body[..body.find("\n    }\n").expect("end of fn")];
        let wrapped = body.find("super::tom::without_undo(hwnd, || {").expect("undo suspended");
        let end = wrapped + body[wrapped..].find("\n        });").expect("end of closure");
        let formats: Vec<usize> = body.match_indices("apply_char_format_background").map(|(i, _)| i).collect();
        assert_eq!(formats.len(), 2);
        assert!(formats.iter().all(|&i| wrapped < i && i < end), "format change outside without_undo");
        // The selection comes back with its direction (TOM, then EM_SETSEL).
        assert!(body.contains("super::tom::selection_ends(hwnd)"));
        assert!(body[end..].contains("self.send_editor_message(0x00B1, anchor as usize, active as isize)"));
    }

    #[test]
    fn every_lines_menu_item_is_dispatched() {
        // The menus run through the command table (palette_ops.rs).
        let table = src("palette_ops.rs");
        for (id, item, op) in [
            ("LinesSortAscending", "menu_edit_lines_sort_asc", "LineOp::SortAscending"),
            ("LinesSortDescending", "menu_edit_lines_sort_desc", "LineOp::SortDescending"),
            ("LinesRemoveDuplicates", "menu_edit_lines_dedup", "LineOp::RemoveDuplicates"),
            ("LinesJoin", "menu_edit_lines_join", "LineOp::Join"),
            ("LinesTrimTrailing", "menu_edit_lines_trim", "LineOp::TrimTrailing"),
        ] {
            assert!(table.contains(&format!("{id} => &self.{item},")), "{item} not in the command table");
            assert!(table.contains(&format!("{id} => self.on_line_op(crate::features::line_ops::{op}),")), "{id} -> {op}");
        }
    }

    // ── Live RichEdit: the conventions the edit relies on ──

    /// A real RICHEDIT50W, also used by the case_ops.rs tests.
    pub(in crate::ui::editor) mod live {
        use super::*;
        use windows::core::w;
        use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
        use windows::Win32::System::LibraryLoader::LoadLibraryW;
        use windows::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, GetWindowTextLengthW, GetWindowTextW, SendMessageW, SetWindowTextW,
            WINDOW_EX_STYLE, WINDOW_STYLE, WS_CHILD, WS_POPUP,
        };

        pub(in crate::ui::editor) struct Rich {
            top: HWND,
            pub(in crate::ui::editor) ed: HWND,
        }

        impl Drop for Rich {
            fn drop(&mut self) {
                unsafe {
                    let _ = DestroyWindow(self.top);
                }
            }
        }

        pub(in crate::ui::editor) fn send(ed: HWND, msg: u32, w: usize, l: isize) -> isize {
            unsafe { SendMessageW(ed, msg, Some(WPARAM(w)), Some(LPARAM(l))).0 }
        }

        pub(in crate::ui::editor) fn make(text: &str) -> Rich {
            unsafe {
                LoadLibraryW(w!("msftedit.dll")).expect("msftedit.dll");
                let top = CreateWindowExW(
                    WINDOW_EX_STYLE(0), w!("STATIC"), None, WS_POPUP, 0, 0, 400, 300, None, None, None, None,
                )
                .expect("host window");
                // ES_MULTILINE | ES_AUTOVSCROLL | ES_NOHIDESEL, as the editor.
                let ed = CreateWindowExW(
                    WINDOW_EX_STYLE(0), w!("RICHEDIT50W"), None,
                    WS_CHILD | WINDOW_STYLE(0x0004 | 0x0040 | 0x0100),
                    0, 0, 400, 300, Some(top), None, None, None,
                )
                .expect("RICHEDIT50W");
                let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
                SetWindowTextW(ed, windows::core::PCWSTR(wide.as_ptr())).expect("set text");
                send(ed, 0x00CD, 0, 0); // EM_EMPTYUNDOBUFFER
                Rich { top, ed }
            }
        }

        impl Rich {
            /// Shift+Right: the active end moves one character right.
            fn select_with_shift_right(&self) {
                crate::ui::editor::tom::extend_selection_right(self.ed).expect("MoveRight");
            }
        }

        impl LineEditSteps for Rich {
            fn text(&self) -> String {
                unsafe {
                    let len = GetWindowTextLengthW(self.ed) as usize;
                    let mut buf = vec![0u16; len + 1];
                    let n = GetWindowTextW(self.ed, &mut buf) as usize;
                    String::from_utf16(&buf[..n]).expect("utf-16")
                }
            }
            /// (anchor, active end), as EditorLineSteps reads it.
            fn selection(&self) -> (u32, u32) {
                crate::ui::editor::tom::selection_ends(self.ed).expect("TOM selection")
            }
            fn select(&self, start: u32, end: u32) {
                send(self.ed, 0x00B1, start as usize, end as isize); // EM_SETSEL
            }
            fn stop_group_typing(&self) {
                send(self.ed, 0x0458, 0, 0);
            }
            fn replace_selection(&self, text: &str) {
                let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
                send(self.ed, 0x00C2, 1, wide.as_ptr() as isize); // EM_REPLACESEL, undoable
            }
            fn mark_modified(&self) {}
        }

        #[test]
        fn live_move_and_sort_are_one_undo_step_each() {
            let r = make("one\r\ntwo😀\r\nthree");
            assert_eq!(r.text(), "one\r\ntwo😀\r\nthree", "WM_GETTEXT gives \\r\\n breaks");
            r.select(5, 5);
            assert_eq!(run_line_edit(&r, |t, s| Ok::<_, ()>(move_lines_edit(t, s, true))), Ok(true));
            assert_eq!(r.text(), "two😀\r\none\r\nthree");
            assert_eq!(r.selection(), (1, 1));

            assert_eq!(run_line_edit(&r, |t, s| line_op_edit(t, s, LineOp::SortDescending, ci)), Ok(true));
            assert_eq!(r.text(), "two😀\r\nthree\r\none");
            assert_eq!(r.selection(), (0, 15));

            send(r.ed, 0x00C7, 0, 0); // EM_UNDO: the sort only
            assert_eq!(r.text(), "two😀\r\none\r\nthree");
            send(r.ed, 0x00C7, 0, 0); // EM_UNDO: the move
            assert_eq!(r.text(), "one\r\ntwo😀\r\nthree");
        }

        /// What the clear step of update_occurrence_highlights sends: the
        /// whole text back to the auto background (EM_SETCHARFORMAT).
        fn clear_backgrounds(r: &Rich) {
            let len = utf16_len(&r.text()) as u32;
            r.select(0, len);
            let mut buf = [0u8; 116];
            buf[0..4].copy_from_slice(&116u32.to_le_bytes());
            buf[4..8].copy_from_slice(&0x04000000u32.to_le_bytes()); // CFM_BACKCOLOR
            buf[8..12].copy_from_slice(&0x04000000u32.to_le_bytes()); // CFE_AUTOBACKCOLOR
            send(r.ed, 0x0444, 1, buf.as_mut_ptr() as isize); // EM_SETCHARFORMAT, SCF_SELECTION
        }

        #[test]
        fn live_a_background_change_alone_is_an_undo_step() {
            // Why the highlight pass suspends the undo: in rich-text mode
            // the clear pass is recorded, and one Ctrl+Z undoes it instead
            // of the sort.
            let r = make("b\r\nc\r\na");
            assert_eq!(run_line_edit(&r, |t, s| line_op_edit(t, s, LineOp::SortAscending, ci)), Ok(true));
            clear_backgrounds(&r);
            send(r.ed, 0x00C7, 0, 0); // EM_UNDO
            assert_eq!(r.text(), "a\r\nb\r\nc", "the format change took the undo");
        }

        #[test]
        fn live_highlight_pass_after_a_line_op_keeps_one_undo_step() {
            // The highlight tick that follows a line op clears the
            // backgrounds over the whole text, undo suspended as
            // update_occurrence_highlights does; one Ctrl+Z still undoes
            // the op itself.
            use crate::ui::editor::tom::without_undo;
            let r = make("b\r\nc\r\na");
            assert_eq!(run_line_edit(&r, |t, s| line_op_edit(t, s, LineOp::SortAscending, ci)), Ok(true));
            assert_eq!(r.text(), "a\r\nb\r\nc");
            without_undo(r.ed, || clear_backgrounds(&r)).expect("undo suspended");
            send(r.ed, 0x00C7, 0, 0); // EM_UNDO
            assert_eq!(r.text(), "b\r\nc\r\na");

            let r = make("one\r\ntwo");
            r.select(5, 5);
            assert_eq!(run_line_edit(&r, |t, s| Ok::<_, ()>(move_lines_edit(t, s, true))), Ok(true));
            without_undo(r.ed, || clear_backgrounds(&r)).expect("undo suspended");
            send(r.ed, 0x00C7, 0, 0); // EM_UNDO
            assert_eq!(r.text(), "one\r\ntwo");
            // The undo is back on after the pass: the next edit is undoable.
            r.select(0, 0);
            r.replace_selection("x");
            send(r.ed, 0x00C7, 0, 0); // EM_UNDO
            assert_eq!(r.text(), "one\r\ntwo");
        }

        #[test]
        fn live_move_keeps_a_backward_selection_backward() {
            // Shift+Up from inside "cc" to inside "bb": anchor 6, active 2.
            let r = make("a\r\nbb\r\ncc\r\nd");
            r.select(6, 2);
            assert_eq!(r.selection(), (6, 2), "TOM gives the active end first");
            assert_eq!(run_line_edit(&r, |t, s| Ok::<_, ()>(move_lines_edit(t, s, true))), Ok(true));
            assert_eq!(r.text(), "bb\r\ncc\r\na\r\nd");
            assert_eq!(r.selection(), (4, 0), "the active end stays at the top");
            // Shift+Right moves the active end: the top one.
            r.select_with_shift_right();
            assert_eq!(r.selection(), (4, 1));
        }
    }
}
