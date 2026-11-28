// Line operations: Alt+Up / Alt+Down move lines, Edit > Lines transforms
// the lines of the selection or of the whole tab (features::line_ops),
// Ctrl+D / Ctrl+Shift+K duplicate / delete the current line, F5 inserts at
// the selection.
//
// Positions are RichEdit character positions (UTF-16 code units, a line
// break = 1, see helpers.rs). The text comes from WM_GETTEXT, so breaks are
// "\r\n" or a form feed U+000C, which RichEdit shows as a line break; a lone
// "\r" or "\n" also ends a line (features::text_lines). Every change is one
// EM_REPLACESEL (undoable) over the touched lines: one undo step. A command
// that reorders or drops lines keeps each break in its place in the block,
// so a form feed is never turned into a CRLF nor dropped by Trim.

#[allow(unused_imports)]
use super::*;
use crate::features::line_ops::{self, LineOp};
use crate::features::text_lines;

/// A replacement of the RichEdit range `start..end` by `text`, then the
/// selection `sel`. An empty range with an empty text replaces nothing:
/// only the selection is set (`run_line_edit`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LineEdit {
    pub start: u32,
    pub end: u32,
    pub text: String,
    pub sel: (u32, u32),
}

/// Byte range of every line of `text`, line break excluded, as RichEdit
/// shows the lines (features::text_lines: "\r\n", "\r", "\n" or a form
/// feed). Always at least one (empty) line.
pub(super) fn line_ranges(text: &str) -> Vec<(usize, usize)> {
    text_lines::line_ranges(text)
}

/// The text that puts back the break between lines `i` and `i + 1` in an
/// EM_REPLACESEL: a form feed stays a form feed, any other break is the
/// paragraph break "\n".
fn break_after(text: &str, lines: &[(usize, usize)], i: usize) -> &'static str {
    if text[lines[i].1..lines[i + 1].0] == *"\u{c}" { "\u{c}" } else { "\n" }
}

/// `parts` joined by the breaks that follow lines `first`, `first + 1`, ...
/// of `text`, in that order: each break stays where it was in the block,
/// whatever the lines now between them. `parts` holds at most as many
/// lines as there are lines from `first` on.
fn join_with_breaks<S: AsRef<str>>(text: &str, lines: &[(usize, usize)], first: usize, parts: &[S]) -> String {
    let mut out = String::new();
    for (k, part) in parts.iter().enumerate() {
        if k > 0 {
            out.push_str(break_after(text, lines, first + k - 1));
        }
        out.push_str(part.as_ref());
    }
    out
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
    // Every break is one position ("\r\n" and a form feed alike).
    let shift = position_of(other, other.len()) + 1;
    // Both ends lie in the moved block: same shift, direction kept.
    let new_sel = if up { (sel.0 - shift, sel.1 - shift) } else { (sel.0 + shift, sel.1 + shift) };
    let start = position_of(text, lines[a].0);
    if order.iter().copied().eq((a..=b).map(line)) {
        // Identical lines swapped: the text stays as it is, only the
        // selection moves; no undo step, the note not marked modified.
        return Some(LineEdit { start, end: start, text: String::new(), sel: new_sel });
    }
    let replacement = join_with_breaks(text, &lines, a, &order);
    Some(LineEdit { start, end: position_of(text, lines[b].1), text: replacement, sel: new_sel })
}

/// Edit > Lines: apply `op` to the lines touched by the selection when it
/// spans 2 lines or more, to the whole tab otherwise. The whole tab leaves
/// out the empty line after a final break, as a Ctrl+A selection does
/// (column-0 rule). `Ok(None)` when the lines would not change; the
/// replaced lines end up selected.
pub(super) fn line_op_edit<K: Ord, E>(
    text: &str,
    sel: (u32, u32),
    op: LineOp,
    key: impl Fn(&str) -> Result<K, E>,
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
    let result = line_ops::apply(op, &block, key)?;
    if result.iter().map(String::as_str).eq(block.iter().copied()) {
        return Ok(None);
    }
    let start = position_of(text, lines[first].0);
    // `apply` never returns more lines than it was given.
    let replacement = join_with_breaks(text, &lines, first, &result);
    let end_sel = start + utf16_len(&replacement) as u32;
    Ok(Some(LineEdit {
        start,
        end: position_of(text, lines[last].1),
        text: replacement,
        sel: (start, end_sel),
    }))
}

/// `line_op_edit` with the user's locale order (LCMapStringEx sort keys,
/// the order of CompareStringEx), as the Edit > Lines menu runs it.
pub(super) fn user_locale_line_op_edit(
    text: &str,
    sel: (u32, u32),
    op: LineOp,
) -> Result<Option<LineEdit>, crate::ui::vault_panel::VaultTreeError> {
    line_op_edit(text, sel, op, crate::ui::vault_panel::sort_key_user_locale)
}

/// Index of the line holding the start of the RichEdit selection `sel`
/// (either direction), as Ctrl+D and Ctrl+Shift+K have always picked it.
/// The start must fall on a character of `text`, except one position past
/// the end: the final paragraph mark of Select All (as `case_edit`).
fn line_of_selection_start(text: &str, lines: &[(usize, usize)], sel: (u32, u32)) -> Result<usize, Utf16RangeError> {
    // position_of counts like utf16_len without building the offset map
    // (about 8 bytes per byte of text) on every Ctrl+D / Ctrl+Shift+K.
    let max = position_of(text, text.len());
    let lo = sel.0.min(sel.1);
    let lo = if lo == max + 1 { max } else { lo };
    Ok(line_of(lines, utf16_to_byte_exact(text, lo as usize)?))
}

/// Ctrl+D: insert a copy of the line holding the selection start under it,
/// caret at the end of the copy. A RichEdit position that does not fall on
/// the text is an error (nothing is edited).
pub(super) fn duplicate_line_edit(text: &str, sel: (u32, u32)) -> Result<Option<LineEdit>, Utf16RangeError> {
    let lines = line_ranges(text);
    let i = line_of_selection_start(text, &lines, sel)?;
    let line = &text[lines[i].0..lines[i].1];
    let at = position_of(text, lines[i].1);
    // "\n" is one paragraph break for EM_REPLACESEL; the line holds no break.
    let caret = at + 1 + position_of(line, line.len());
    Ok(Some(LineEdit { start: at, end: at, text: format!("\n{line}"), sel: (caret, caret) }))
}

/// Ctrl+Shift+K: remove the line holding the selection start with its
/// break (the break before it for the last line), caret where it was.
/// `Ok(None)` on an empty tab.
pub(super) fn delete_line_edit(text: &str, sel: (u32, u32)) -> Result<Option<LineEdit>, Utf16RangeError> {
    let lines = line_ranges(text);
    let i = line_of_selection_start(text, &lines, sel)?;
    let (start, end) = if i + 1 < lines.len() {
        (position_of(text, lines[i].0), position_of(text, lines[i + 1].0))
    } else if i > 0 {
        (position_of(text, lines[i - 1].1), position_of(text, lines[i].1))
    } else {
        (0, position_of(text, lines[i].1))
    };
    if start == end {
        return Ok(None);
    }
    Ok(Some(LineEdit { start, end, text: String::new(), sel: (start, start) }))
}

/// What applying a `LineEdit` does to the editor, so the sequence can be
/// tested with a fake.
pub(super) trait LineEditSteps {
    /// The editor text, read strictly: the edit writes part of it back.
    fn text(&self) -> Result<String, EditorTextError>;
    fn selection(&self) -> (u32, u32);
    fn select(&self, start: u32, end: u32);
    /// Close the current typing undo group, so the edit is its own step.
    fn stop_group_typing(&self);
    /// EM_REPLACESEL with undo.
    fn replace_selection(&self, text: &str);
    fn mark_modified(&self);
}

/// Why `run_line_edit` changed nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum LineEditError<E> {
    /// The editor text is not valid UTF-16 or could not be read: a copied
    /// or moved line would not be the one in the editor.
    Unreadable(EditorTextError),
    /// `compute` failed.
    Compute(E),
}

/// Read the text and the selection, compute the edit, apply it as one
/// undoable replacement and mark the tab modified. Returns whether the text
/// changed; on error nothing was touched.
pub(super) fn run_line_edit<S, E>(
    s: &S,
    compute: impl FnOnce(&str, (u32, u32)) -> Result<Option<LineEdit>, E>,
) -> Result<bool, LineEditError<E>>
where
    S: LineEditSteps,
{
    let text = s.text().map_err(LineEditError::Unreadable)?;
    let sel = s.selection();
    let Some(edit) = compute(&text, sel).map_err(LineEditError::Compute)? else {
        return Ok(false);
    };
    if edit.start == edit.end && edit.text.is_empty() {
        // Nothing to replace (a move of identical lines): only the selection.
        s.select(edit.sel.0, edit.sel.1);
        return Ok(false);
    }
    s.select(edit.start, edit.end);
    s.stop_group_typing();
    s.replace_selection(&edit.text);
    s.select(edit.sel.0, edit.sel.1);
    s.mark_modified();
    Ok(true)
}

/// F5: replace the selection (or insert at the caret) with `inserted`, as
/// one undo step. No position is computed: EM_REPLACESEL takes the
/// selection in either direction, Select All's final mark included, and
/// leaves the caret after the insertion.
pub(super) fn insert_at_selection<S: LineEditSteps>(s: &S, inserted: &str) {
    s.stop_group_typing();
    s.replace_selection(inserted);
    s.mark_modified();
}

/// `LineEditSteps` on the main editor (also used by case_ops.rs).
pub(super) struct EditorLineSteps<'a>(pub(super) &'a EditorForm);

impl LineEditSteps for EditorLineSteps<'_> {
    fn text(&self) -> Result<String, EditorTextError> {
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
        let r: Result<bool, LineEditError<std::convert::Infallible>> =
            run_line_edit(&EditorLineSteps(self), |text, sel| Ok(move_lines_edit(text, sel, up)));
        let changed = match r {
            Ok(changed) => changed,
            Err(LineEditError::Unreadable(e)) => return self.tell_unreadable("move_lines", &e),
            Err(LineEditError::Compute(never)) => match never {},
        };
        eprintln!("DEBUG lines: move up={} changed={}", up, changed);
        if changed {
            self.reset_occurrence_highlights();
        }
        // A move of identical lines changes no text but moves the caret.
        self.send_editor_message(0x00B7, 0, 0); // EM_SCROLLCARET
    }

    /// Ctrl+D (`duplicate_line_edit`) and Ctrl+Shift+K (`delete_line_edit`):
    /// one undoable EM_REPLACESEL. The line count changes, so the gutter
    /// width is laid out again. A selection that does not map onto the text
    /// leaves it untouched and is shown, as Convert Case does.
    pub(super) fn on_current_line_edit(&self, what: &str, compute: fn(&str, (u32, u32)) -> Result<Option<LineEdit>, Utf16RangeError>) {
        match run_line_edit(&EditorLineSteps(self), compute) {
            Ok(changed) => {
                eprintln!("INFO lines: {} changed={}", what, changed);
                if changed {
                    self.reset_occurrence_highlights();
                    self.layout_controls();
                    self.send_editor_message(0x00B7, 0, 0); // EM_SCROLLCARET
                }
            }
            Err(LineEditError::Unreadable(e)) => self.tell_unreadable(what, &e),
            Err(LineEditError::Compute(e)) => {
                eprintln!("ERROR lines: {} selection not mapped onto the text: {:?}", what, e);
                self.inform(&t("lines.error.title"), &t("lines.error.mapping").replace("{msg}", &format!("{:?}", e)));
            }
        }
    }

    /// EM_REPLACESEL formats the inserted lines like the start of the
    /// replaced range, which may be a highlighted occurrence. Forgetting
    /// the last term makes the next highlight tick clear and repaint all
    /// (as theme_switch.rs does), even when the selected term is unchanged.
    /// An empty term means nothing is highlighted: no pass is needed.
    pub(super) fn reset_occurrence_highlights(&self) {
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
            Err(LineEditError::Unreadable(e)) => self.tell_unreadable("line_op", &e),
            Err(LineEditError::Compute(e)) => {
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

    fn ci(s: &str) -> Result<String, String> {
        Ok(s.to_lowercase())
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
        let failing = |_: &str| -> Result<String, String> { Err("x".into()) };
        assert_eq!(line_op_edit("b\r\na", (0, 0), LineOp::SortDescending, failing), Err("x".to_string()));
    }

    #[test]
    fn whole_tab_sort_of_100k_lines_in_the_user_locale_stays_fast() {
        // One LCMapStringEx sort key per line, then byte comparisons. The
        // former CompareStringEx per pair (about 1.7M calls) took 1.2 s here
        // and 4.4 s on the CI runner. Issue #40: a 2 s wall-clock budget also
        // failed under load (4.1 s seen) for a sort doing the right work, so
        // the work is counted instead of timed, with the real locale key: one
        // key per line, and at most n * ceil(log2 n) key comparisons (the
        // merge sort bound, a quadratic sort would go far past it). The menu
        // path (user_locale_line_op_edit) must give the same edit with one
        // LCMapStringEx key per line and no CompareStringEx call, counted by
        // vault_panel::tests_locale_calls (this thread only).
        use std::cell::Cell;
        use std::cmp::Ordering;
        /// A locale sort key that counts its comparisons.
        struct Counted<'a>(Vec<u8>, &'a Cell<u64>);
        impl PartialEq for Counted<'_> {
            fn eq(&self, other: &Self) -> bool {
                self.cmp(other) == Ordering::Equal
            }
        }
        impl Eq for Counted<'_> {}
        impl PartialOrd for Counted<'_> {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }
        impl Ord for Counted<'_> {
            fn cmp(&self, other: &Self) -> Ordering {
                self.1.set(self.1.get() + 1);
                self.0.cmp(&other.0)
            }
        }

        const N: u64 = 100_000;
        let t: Vec<String> = (0..N).map(|n| format!("line{}", N - n)).collect();
        let t = t.join("\r\n");
        let keys = Cell::new(0u64);
        let comparisons = Cell::new(0u64);
        let counted_key = |s: &str| {
            keys.set(keys.get() + 1);
            crate::ui::vault_panel::sort_key_user_locale(s).map(|k| Counted(k, &comparisons))
        };
        let e = line_op_edit(&t, (0, 0), LineOp::SortAscending, counted_key).expect("sorts").expect("changes");
        assert_eq!(e.text.split('\n').next(), Some("line1"));
        assert_eq!(keys.get(), N, "one locale sort key per line");
        let ceil_log2 = u64::from(u64::BITS - (N - 1).leading_zeros());
        assert!(
            comparisons.get() <= N * ceil_log2,
            "{} key comparisons for {} lines, over n * ceil(log2 n) = {}",
            comparisons.get(),
            N,
            N * ceil_log2
        );
        use crate::ui::vault_panel::tests_locale_calls;
        let (keys_before, compares_before) = tests_locale_calls::snapshot();
        let menu = user_locale_line_op_edit(&t, (0, 0), LineOp::SortAscending).expect("sorts");
        let (keys_after, compares_after) = tests_locale_calls::snapshot();
        assert_eq!(menu, Some(e));
        assert_eq!(
            (keys_after - keys_before, compares_after - compares_before),
            (N, 0),
            "menu path: (LCMapStringEx keys, CompareStringEx calls) for {} lines",
            N
        );
    }

    // ── run_line_edit: the sequence, with a fake editor ──

    /// A fake RichEdit: a text with "\r\n" breaks (as WM_GETTEXT returns
    /// them), positions counted like RichEdit, and a log of the calls.
    /// Also used by the case_ops.rs tests.
    pub(in crate::ui::editor) struct Fake {
        pub(in crate::ui::editor) text: RefCell<String>,
        pub(in crate::ui::editor) sel: RefCell<(u32, u32)>,
        pub(in crate::ui::editor) log: RefCell<Vec<String>>,
        /// When set, reading the text fails with it (a lone surrogate).
        pub(in crate::ui::editor) unreadable: RefCell<Option<EditorTextError>>,
    }

    impl Fake {
        pub(in crate::ui::editor) fn new(text: &str, sel: (u32, u32)) -> Self {
            Fake {
                text: RefCell::new(text.to_string()),
                sel: RefCell::new(sel),
                log: RefCell::new(Vec::new()),
                unreadable: RefCell::new(None),
            }
        }
    }

    impl LineEditSteps for Fake {
        fn text(&self) -> Result<String, EditorTextError> {
            match self.unreadable.borrow().clone() {
                Some(e) => Err(e),
                None => Ok(self.text.borrow().clone()),
            }
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
            let (lo, hi) = (s.min(e), s.max(e));
            let (b0, b1) = utf16_range_to_byte_range(&current, lo as usize, hi as usize);
            // RichEdit stores a "\n" as a paragraph break, read back as "\r\n".
            // A lone form feed stays one: replace_selection_on sends it
            // through TOM (EM_REPLACESEL alone would make it a CRLF).
            let inserted = text.replace('\n', "\r\n");
            *self.text.borrow_mut() = format!("{}{}{}", &current[..b0], inserted, &current[b1..]);
            // As EM_REPLACESEL: the caret ends after the inserted text.
            let caret = build_utf16_offset_map(&current)[b0] as u32 + utf16_len(&inserted) as u32;
            *self.sel.borrow_mut() = (caret, caret);
        }
        fn mark_modified(&self) {
            self.log.borrow_mut().push("modified".into());
        }
    }

    #[test]
    fn sequence_move_selects_replaces_once_then_restores_the_selection() {
        let f = Fake::new("one\r\ntwo\r\nthree", (5, 6));
        let r: Result<bool, LineEditError<()>> = run_line_edit(&f, |t, s| Ok(move_lines_edit(t, s, true)));
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
    fn sequence_with_an_unreadable_text_touches_nothing() {
        // A lone surrogate in the editor: duplicating, moving or sorting
        // would write back a text that is not the editor's.
        let err = EditorTextError::InvalidUtf16 { unit: 4, line: 2, count: 1 };
        let f = Fake::new("one\r\ntwo", (5, 5));
        *f.unreadable.borrow_mut() = Some(err.clone());
        assert_eq!(run_line_edit(&f, duplicate_line_edit), Err(LineEditError::Unreadable(err.clone())));
        assert_eq!(run_line_edit(&f, delete_line_edit), Err(LineEditError::Unreadable(err.clone())));
        let r: Result<bool, LineEditError<()>> = run_line_edit(&f, |t, s| Ok(move_lines_edit(t, s, true)));
        assert_eq!(r, Err(LineEditError::Unreadable(err.clone())));
        assert_eq!(run_line_edit(&f, |t, s| line_op_edit(t, s, LineOp::SortAscending, ci)), Err(LineEditError::Unreadable(err)));
        assert!(f.log.borrow().is_empty(), "{:?}", f.log.borrow());
        assert_eq!(*f.text.borrow(), "one\r\ntwo");
    }

    #[test]
    fn sequence_op_error_touches_nothing() {
        let f = Fake::new("b\r\na", (0, 0));
        let failing = |_: &str| -> Result<String, String> { Err("x".into()) };
        assert_eq!(
            run_line_edit(&f, |t, s| line_op_edit(t, s, LineOp::SortAscending, failing)),
            Err(LineEditError::Compute("x".to_string()))
        );
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

    // ── Form feed U+000C: RichEdit shows it as a line break (V2) ──

    /// Shown as three lines a | b | c. RichEdit positions: a=0, FF=1, b=2,
    /// CRLF=3, c=4.
    const FF_TEXT: &str = "a\u{c}b\r\nc";

    #[test]
    fn line_ranges_split_on_a_form_feed_as_richedit() {
        assert_eq!(line_ranges(FF_TEXT), vec![(0, 1), (2, 3), (5, 6)]);
        assert_eq!(line_ranges("a\u{c}"), vec![(0, 1), (2, 2)]);
        assert_eq!(line_ranges("a\u{c}\u{c}b"), vec![(0, 1), (2, 2), (3, 4)]);
        // A form feed then CRLF: two breaks, not merged (EM_GETLINECOUNT = 3).
        assert_eq!(line_ranges("a\u{c}\r\nb"), vec![(0, 1), (2, 2), (4, 5)]);
        // NEL (U+0085) does not end a line in RichEdit.
        assert_eq!(line_ranges("a\u{85}b"), vec![(0, 4)]);
    }

    #[test]
    fn selection_ending_just_after_a_form_feed_leaves_that_line_out() {
        let t = "aa\u{c}bb\u{c}cc";
        let l = line_ranges(t);
        assert_eq!(touched_lines(t, &l, (1, 6)), (0, 1));
        assert_eq!(touched_lines(t, &l, (1, 7)), (0, 2));
        assert_eq!(touched_lines(t, &l, (3, 3)), (1, 1));
    }

    #[test]
    fn duplicate_and_delete_act_on_the_line_shown_after_a_form_feed() {
        // Caret on "b": only "b" is copied / removed.
        assert_eq!(after(duplicate_line_edit, FF_TEXT, (2, 2)), s("a\u{c}b\r\nb\r\nc", (5, 5)));
        assert_eq!(after(delete_line_edit, FF_TEXT, (2, 2)), s("a\u{c}c", (2, 2)));
        // Caret on "a": the line ends at the form feed.
        assert_eq!(after(duplicate_line_edit, FF_TEXT, (0, 0)), s("a\r\na\u{c}b\r\nc", (3, 3)));
        assert_eq!(after(delete_line_edit, FF_TEXT, (0, 0)), s("b\r\nc", (0, 0)));
    }

    #[test]
    fn move_across_a_form_feed_keeps_each_break_in_place() {
        // "b" over "a": the form feed stays between the first two lines.
        let e = move_lines_edit(FF_TEXT, (2, 2), true).expect("moves");
        assert_eq!((e.start, e.end, e.text.as_str(), e.sel), (0, 3, "b\u{c}a", (0, 0)));
        let e = move_lines_edit(FF_TEXT, (0, 0), false).expect("moves");
        assert_eq!((e.start, e.end, e.text.as_str(), e.sel), (0, 3, "b\u{c}a", (2, 2)));
        // "b" under "c": the CRLF stays a paragraph break ("\n").
        let e = move_lines_edit(FF_TEXT, (2, 2), false).expect("moves");
        assert_eq!((e.start, e.end, e.text.as_str(), e.sel), (2, 5, "c\nb", (4, 4)));
    }

    #[test]
    fn sequence_move_of_identical_lines_around_a_form_feed_changes_nothing() {
        // Two empty lines split by a form feed: the swap gives the same
        // text, so the text is not replaced (no undo step, not modified);
        // only the caret moves.
        let f = Fake::new("\u{c}", (0, 0));
        assert_eq!(run_line_edit(&f, |t, s| Ok::<_, ()>(move_lines_edit(t, s, false))), Ok(false));
        assert_eq!(*f.text.borrow(), "\u{c}");
        assert_eq!(*f.sel.borrow(), (1, 1));
        assert_eq!(*f.log.borrow(), vec!["select:1..1"]);
    }

    #[test]
    fn sequence_trim_down_to_a_lone_form_feed_replaces_once() {
        // " \f " trimmed is the lone form feed: one replacement, the result
        // selected. That it stays a form feed in RichEdit is the job of
        // replace_selection_on (live test below).
        let f = Fake::new(" \u{c} ", (0, 0));
        assert_eq!(run_line_edit(&f, |t, s| line_op_edit(t, s, LineOp::TrimTrailing, ci)), Ok(true));
        assert_eq!(*f.text.borrow(), "\u{c}");
        assert_eq!(*f.sel.borrow(), (0, 1));
        assert_eq!(f.log.borrow().iter().filter(|c| c.starts_with("replace:")).count(), 1, "{:?}", f.log.borrow());
    }

    #[test]
    fn trim_never_removes_a_form_feed() {
        // The form feed is a break, not trailing white space of "a".
        assert_eq!(line_op_edit("a\u{c}\r\nb", (0, 0), LineOp::TrimTrailing, ci), Ok(None));
        assert_eq!(line_op_edit("a \u{c}b", (0, 0), LineOp::TrimTrailing, ci).map(|e| e.map(|e| e.text)), Ok(Some("a\u{c}b".into())));
    }

    #[test]
    fn sort_and_dedup_keep_the_breaks_in_their_places() {
        let e = line_op_edit(FF_TEXT, (0, 0), LineOp::SortDescending, ci).unwrap().expect("sorted");
        assert_eq!((e.start, e.end, e.text.as_str(), e.sel), (0, 5, "c\u{c}b\na", (0, 5)));
        // Fewer lines: the first breaks of the block are kept, in order.
        let e = line_op_edit("a\u{c}a\r\nb", (0, 0), LineOp::RemoveDuplicates, ci).unwrap().expect("dedup");
        assert_eq!(e.text, "a\u{c}b");
    }

    #[test]
    fn sequence_line_commands_on_a_form_feed_text() {
        let f = Fake::new(FF_TEXT, (2, 2));
        assert_eq!(run_line_edit(&f, |t, s| Ok::<_, ()>(move_lines_edit(t, s, true))), Ok(true));
        assert_eq!(*f.text.borrow(), "b\u{c}a\r\nc");
        assert_eq!(*f.sel.borrow(), (0, 0));
        assert_eq!(run_line_edit(&f, duplicate_line_edit), Ok(true));
        assert_eq!(*f.text.borrow(), "b\r\nb\u{c}a\r\nc");
        // Caret after the copy, on the form feed: the copy and its break go.
        assert_eq!(run_line_edit(&f, delete_line_edit), Ok(true));
        assert_eq!(*f.text.borrow(), "b\r\na\r\nc");
    }

    // ── Ctrl+D / Ctrl+Shift+K / F5: RichEdit positions, never byte offsets ──

    type Compute = fn(&str, (u32, u32)) -> Result<Option<LineEdit>, Utf16RangeError>;

    /// Text and selection after `compute` ran on a fake editor.
    fn after(compute: Compute, text: &str, sel: (u32, u32)) -> (String, (u32, u32)) {
        let f = Fake::new(text, sel);
        run_line_edit(&f, compute).expect("maps");
        let text = f.text.borrow().clone();
        let sel = *f.sel.borrow();
        (text, sel)
    }

    fn s(text: &str, sel: (u32, u32)) -> (String, (u32, u32)) {
        (text.to_string(), sel)
    }

    #[test]
    fn duplicate_after_a_multi_byte_character() {
        // A caret after "é", "😀" or the bullet of Ctrl+. used to slice
        // inside the character: a panic, and the app closed (panic = abort).
        assert_eq!(after(duplicate_line_edit, "é", (1, 1)), s("é\r\né", (3, 3)));
        assert_eq!(after(duplicate_line_edit, "😀", (2, 2)), s("😀\r\n😀", (5, 5)));
        assert_eq!(after(duplicate_line_edit, "• ", (2, 2)), s("• \r\n• ", (5, 5)));
        assert_eq!(after(duplicate_line_edit, "x é y\r\nz", (3, 3)), s("x é y\r\nx é y\r\nz", (11, 11)));
    }

    #[test]
    fn duplicate_counts_a_line_break_as_one_position() {
        // Caret at the start of "c" (position 4): "c" is duplicated, not "b".
        assert_eq!(after(duplicate_line_edit, "a\r\nb\r\nc", (4, 4)), s("a\r\nb\r\nc\r\nc", (7, 7)));
        // Caret at the very end.
        assert_eq!(after(duplicate_line_edit, "ab\r\ncd", (5, 5)), s("ab\r\ncd\r\ncd", (8, 8)));
        // Caret on the empty line after a final break: that empty line.
        assert_eq!(after(duplicate_line_edit, "a\r\n", (2, 2)), s("a\r\n\r\n", (3, 3)));
        // Emoji and breaks before the caret.
        assert_eq!(after(duplicate_line_edit, "😀\r\né\r\nlast", (5, 5)), s("😀\r\né\r\nlast\r\nlast", (14, 14)));
    }

    #[test]
    fn duplicate_acts_on_the_line_of_the_selection_start() {
        // Today's rule, kept: the line holding the start, in both directions.
        assert_eq!(after(duplicate_line_edit, "aa\r\nbb\r\ncc", (1, 7)), s("aa\r\naa\r\nbb\r\ncc", (5, 5)));
        assert_eq!(after(duplicate_line_edit, "aa\r\nbb\r\ncc", (7, 1)), s("aa\r\naa\r\nbb\r\ncc", (5, 5)));
        // Select All (EM_SETSEL(0, -1)) ends one past the text (final mark).
        assert_eq!(after(duplicate_line_edit, "x😀\r\ny", (0, 6)), s("x😀\r\nx😀\r\ny", (7, 7)));
        assert_eq!(after(duplicate_line_edit, "", (0, 0)), s("\r\n", (1, 1)));
    }

    #[test]
    fn duplicate_inserts_only_the_copy() {
        // One insertion after the line, not a rebuild of the whole text.
        let e = duplicate_line_edit("a\r\nb😀", (3, 3)).expect("maps").expect("changes");
        assert_eq!(e, LineEdit { start: 5, end: 5, text: "\nb😀".into(), sel: (9, 9) });
    }

    #[test]
    fn delete_after_a_multi_byte_character() {
        assert_eq!(after(delete_line_edit, "é", (1, 1)), s("", (0, 0)));
        assert_eq!(after(delete_line_edit, "😀\r\nb", (2, 2)), s("b", (0, 0)));
        assert_eq!(after(delete_line_edit, "a\r\n• ", (4, 4)), s("a", (1, 1)));
    }

    #[test]
    fn delete_counts_a_line_break_as_one_position() {
        // Caret at the start of "c": "c" is deleted, not "b".
        assert_eq!(after(delete_line_edit, "a\r\nb\r\nc", (4, 4)), s("a\r\nb", (3, 3)));
        // Caret at the start of "b": "b" is deleted, not "a".
        assert_eq!(after(delete_line_edit, "a\r\nb", (2, 2)), s("a", (1, 1)));
        // Caret at the very end: the last line and its break go, no lone CR.
        assert_eq!(after(delete_line_edit, "a\r\nb", (3, 3)), s("a", (1, 1)));
        // The empty line after a final break.
        assert_eq!(after(delete_line_edit, "a\r\n", (2, 2)), s("a", (1, 1)));
        assert_eq!(after(delete_line_edit, "a\r\nb\r\nc", (0, 0)), s("b\r\nc", (0, 0)));
        assert_eq!(after(delete_line_edit, "\r\n", (0, 0)), s("", (0, 0)));
        assert_eq!(after(delete_line_edit, "abc", (1, 1)), s("", (0, 0)));
    }

    #[test]
    fn delete_acts_on_the_line_of_the_selection_start() {
        assert_eq!(after(delete_line_edit, "aa\r\nbb\r\ncc", (1, 7)), s("bb\r\ncc", (0, 0)));
        assert_eq!(after(delete_line_edit, "aa\r\nbb\r\ncc", (7, 1)), s("bb\r\ncc", (0, 0)));
        assert_eq!(after(delete_line_edit, "x😀\r\ny", (0, 6)), s("y", (0, 0)));
    }

    #[test]
    fn delete_removes_only_the_line() {
        let e = delete_line_edit("a\r\nbb\r\nc", (3, 3)).expect("maps").expect("changes");
        assert_eq!(e, LineEdit { start: 2, end: 5, text: String::new(), sel: (2, 2) });
    }

    #[test]
    fn delete_on_an_empty_tab_touches_nothing() {
        // No change, no undo step, the tab is not marked modified.
        let f = Fake::new("", (0, 0));
        assert_eq!(run_line_edit(&f, delete_line_edit), Ok(false));
        assert!(f.log.borrow().is_empty());
    }

    #[test]
    fn caret_not_on_the_text_is_an_error_and_nothing_changes() {
        // Inside a surrogate pair, or past the final mark: the text read
        // back does not match the control (a bug), so nothing is edited.
        for compute in [duplicate_line_edit as Compute, delete_line_edit] {
            assert_eq!(compute("a😀", (2, 2)), Err(Utf16RangeError::MidSurrogate { pos: 2 }));
            assert_eq!(compute("ab", (5, 5)), Err(Utf16RangeError::OutOfBounds { max: 5, len_u16: 2 }));
            let f = Fake::new("a😀", (2, 2));
            assert!(run_line_edit(&f, compute).is_err());
            assert!(f.log.borrow().is_empty());
        }
    }

    const TS: &str = "2026-11-27 10:00:00";

    fn insert(text: &str, sel: (u32, u32)) -> (String, (u32, u32)) {
        let f = Fake::new(text, sel);
        insert_at_selection(&f, TS);
        let text = f.text.borrow().clone();
        let sel = *f.sel.borrow();
        (text, sel)
    }

    #[test]
    fn insert_after_a_multi_byte_character() {
        assert_eq!(insert("é", (1, 1)), s(&format!("é{TS}"), (20, 20)));
        assert_eq!(insert("😀", (2, 2)), s(&format!("😀{TS}"), (21, 21)));
        assert_eq!(insert("• ", (2, 2)), s(&format!("• {TS}"), (21, 21)));
    }

    #[test]
    fn insert_counts_a_line_break_as_one_position() {
        assert_eq!(insert("ab\r\ncd", (5, 5)), s(&format!("ab\r\ncd{TS}"), (24, 24)));
        assert_eq!(insert("ab\r\ncd", (3, 3)), s(&format!("ab\r\n{TS}cd"), (22, 22)));
        assert_eq!(insert("", (0, 0)), s(TS, (19, 19)));
    }

    #[test]
    fn insert_replaces_the_selection_in_either_direction() {
        assert_eq!(insert("ab\r\ncd", (1, 4)), s(&format!("a{TS}d"), (20, 20)));
        assert_eq!(insert("ab\r\ncd", (4, 1)), s(&format!("a{TS}d"), (20, 20)));
        // Select All: the final mark past the text is replaced with the rest.
        assert_eq!(insert("x\r\ny😀", (0, 6)), s(TS, (19, 19)));
    }

    #[test]
    fn insert_is_one_replacement_of_the_selection() {
        // No read of the text, no position computed: EM_REPLACESEL replaces
        // the selection and leaves the caret after the insertion.
        let f = Fake::new("a\r\nb", (2, 2));
        insert_at_selection(&f, TS);
        assert_eq!(*f.log.borrow(), vec!["stop_group".to_string(), format!("replace:{TS:?}"), "modified".into()]);
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
    fn a_move_scrolls_the_caret_into_view_even_without_a_text_change() {
        // A move of identical lines (empty lines held down with Alt+Down)
        // changes no text but moves the caret: it is scrolled into view too.
        let code = src("line_ops.rs");
        let code = &code[..code.find("#[cfg(test)]").expect("test module")];
        let body = fn_body(code, "fn on_move_lines");
        let after_if = &body[body.find("if changed {").expect("changed branch")..];
        let after_if = &after_if[after_if.find("\n        }\n").expect("end of the changed branch")..];
        assert!(after_if.contains("self.send_editor_message(0x00B7, 0, 0); // EM_SCROLLCARET"), "{body}");
    }

    fn fn_body<'a>(code: &'a str, head: &str) -> &'a str {
        let body = &code[code.find(head).unwrap_or_else(|| panic!("{head}"))..];
        &body[..body.find("\n    }\n").unwrap_or_else(|| panic!("end of {head}"))]
    }

    #[test]
    fn line_commands_edit_through_one_replace_selection() {
        // Ctrl+D, Ctrl+Shift+K, F5 and Ctrl+G never slice the text with a
        // RichEdit position nor rebuild it with set_text (undo history and
        // scroll lost).
        let ed = src("edit_ops.rs");
        let ed = &ed[..ed.find("#[cfg(test)]").expect("test module")];
        for f in ["fn on_duplicate_line(", "fn on_delete_line(", "fn on_insert_timestamp(", "fn on_goto_line(", "fn apply_cursor_line(", "fn go_to_shown_line("] {
            let body = fn_body(ed, f);
            assert!(!body.contains("set_text("), "{f} rebuilds the text");
            assert!(!body.contains("text[") && !body.contains("&text[.."), "{f} slices the text");
        }
        assert!(fn_body(ed, "fn on_duplicate_line(").contains("self.on_current_line_edit(\"duplicate\", super::line_ops::duplicate_line_edit)"));
        assert!(fn_body(ed, "fn on_delete_line(").contains("self.on_current_line_edit(\"delete\", super::line_ops::delete_line_edit)"));
        let f5 = fn_body(ed, "fn on_insert_timestamp(");
        assert!(f5.contains("super::line_ops::insert_at_selection(&super::line_ops::EditorLineSteps(self), &timestamp)"));
        assert!(f5.contains("self.reset_occurrence_highlights()"), "F5 may land in a highlighted occurrence");
        assert!(f5.contains("self.send_editor_message(0x00B7, 0, 0)"));
        let goto = fn_body(ed, "fn on_goto_line(");
        assert!(goto.contains("current_line(&text, self.editor.selection().start)"));
        assert!(goto.contains("let max_lines = shown_line_count(&text);"));
        // Ctrl+G counts the lines RichEdit shows (a form feed ends one); the
        // session restore keeps the saved meaning: a count of LF (V2). Both
        // read the display text: positions only.
        assert!(goto.contains("self.go_to_shown_line(target_line - 1)"));
        let shown = fn_body(ed, "fn go_to_shown_line(");
        assert!(shown.contains("self.display_content()") && shown.contains("line_start_position(&text, line)"));
        let restore = fn_body(ed, "fn apply_cursor_line(");
        assert!(restore.contains("self.display_content()") && restore.contains("session_line_position(&text, line)"));
        // Ctrl+. puts the bullet on the line shown, a form feed ending it.
        let bullet = fn_body(ed, "fn on_toggle_bullet(");
        assert!(bullet.contains("text_lines::line_start_at(&text, byte_caret)") && bullet.contains("text_lines::line_end_at(&text, byte_caret)"));
        let heading = &ed[ed.find("impl super::heading_flow::HeadingSteps for FormHeadingSteps").expect("heading steps")..];
        assert!(fn_body(heading, "fn move_caret(").contains("self.form.go_to_shown_line(line)"));

        // The shared handler: highlights reset (the copy may carry one),
        // gutter width laid out again (the line count changed), caret
        // scrolled into view, and a mapping error shown, never swallowed.
        let lo = src("line_ops.rs");
        let lo = &lo[..lo.find("#[cfg(test)]").expect("test module")];
        let h = fn_body(lo, "fn on_current_line_edit(");
        assert!(h.contains("run_line_edit(&EditorLineSteps(self), compute)"));
        let changed = h.find("if changed {").expect("only after a change");
        for call in ["self.reset_occurrence_highlights()", "self.layout_controls()", "self.send_editor_message(0x00B7, 0, 0)"] {
            assert!(h[changed..].contains(call), "{call}");
        }
        assert!(h.contains("eprintln!(\"ERROR lines:") && h.contains("t(\"lines.error.mapping\")") && h.contains("self.inform("));
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
        use crate::ui::editor::live_rich_edit::LiveRichEdit;
        use windows::core::w;
        use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
        use windows::Win32::System::LibraryLoader::LoadLibraryW;
        use windows::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, SendMessageW, SetWindowTextW,
            WINDOW_EX_STYLE, WINDOW_STYLE, WS_CHILD, WS_POPUP,
        };

        pub(in crate::ui::editor) struct Rich {
            top: HWND,
            pub(in crate::ui::editor) ed: HWND,
            // Last: released once the control is destroyed (see live_rich_edit.rs).
            _live: LiveRichEdit,
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
            let live = crate::ui::editor::live_rich_edit::guard();
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
                Rich { top, ed, _live: live }
            }
        }

        impl Rich {
            /// The control's text, which the tests expect to be valid
            /// (an inherent method: `r.text()` in the asserts is this one).
            pub(in crate::ui::editor) fn text(&self) -> String {
                crate::ui::editor_text::read_strict(self.ed).expect("valid UTF-16")
            }

            /// Shift+Right: the active end moves one character right.
            fn select_with_shift_right(&self) {
                crate::ui::editor::tom::extend_selection_right(self.ed).expect("MoveRight");
            }
        }

        impl LineEditSteps for Rich {
            fn text(&self) -> Result<String, EditorTextError> {
                crate::ui::editor_text::read_strict(self.ed)
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
            /// The editor's own replacement (`EditorForm::replace_selection_utf16`).
            fn replace_selection(&self, text: &str) {
                crate::ui::editor::edit_ops::replace_selection_on(self.ed, text);
            }
            fn mark_modified(&self) {}
        }

        /// Tab / Shift+Tab on a live RichEdit, as `EditorIndentSteps`.
        impl crate::ui::editor::indent::IndentSteps for Rich {
            fn text(&self) -> String {
                LineEditSteps::text(self)
            }
            fn selection(&self) -> (u32, u32) {
                LineEditSteps::selection(self)
            }
            fn set_selection(&mut self, start: u32, end: u32) {
                LineEditSteps::select(self, start, end);
            }
            fn replace_selection(&mut self, s: &str) {
                LineEditSteps::replace_selection(self, s);
            }
            fn mark_modified(&mut self) {}
        }

        /// EM_REPLACESEL itself (undoable), without the editor's own rule.
        fn em_replacesel(r: &Rich, text: &str) {
            let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
            send(r.ed, 0x00C2, 1, wide.as_ptr() as isize);
        }

        /// EM_GETLINECOUNT and the EM_LINEINDEX of every line, word wrap
        /// off (EM_SETTARGETDEVICE 0, 1): hard lines only.
        fn richedit_line_starts(r: &Rich) -> Vec<u32> {
            send(r.ed, 0x0448, 0, 1); // EM_SETTARGETDEVICE: no word wrap
            let n = send(r.ed, 0x00BA, 0, 0); // EM_GETLINECOUNT
            (0..n).map(|i| send(r.ed, 0x00BB, i as usize, 0) as u32).collect() // EM_LINEINDEX
        }

        #[test]
        fn live_line_ranges_match_richedit_lines() {
            // The oracle of the shared line definition (V2): RichEdit ends a
            // line at CRLF and at a form feed U+000C, which WM_GETTEXT keeps;
            // VT, LS and PS come back as CRLF; NEL ends no line.
            for set in [
                "a\u{c}b", "a\u{c}", "a\u{c}\r\nb", "a\u{c}\u{c}b", "ab\u{c}cd\r\nef", "\u{c}\u{c}",
                "a\u{b}b", "a\u{2028}b", "a\u{2029}b", "a\rb", "a\nb", "a\u{85}b", "x\u{1F600}\u{c}y\r\n\r\nz",
            ] {
                let r = make(set);
                let text = r.text();
                let starts: Vec<u32> = line_ranges(&text).iter().map(|&(s, _)| position_of(&text, s)).collect();
                assert_eq!(starts, richedit_line_starts(&r), "{set:?} read back as {text:?}");
                assert!(!text.contains(['\u{b}', '\u{2028}', '\u{2029}']), "{text:?}");
            }
        }

        #[test]
        fn live_rtf_line_and_page_come_back_as_crlf_and_form_feed() {
            // A paste from a word processor: RTF \line (a soft break) is
            // read as CRLF, \page as a form feed.
            let r = make("");
            let rtf = b"{\\rtf1 a\\line b\\page c\\par d}\0";
            let st: [u32; 2] = [0, 0]; // SETTEXTEX: ST_DEFAULT, CP_ACP
            send(r.ed, 0x0461, st.as_ptr() as usize, rtf.as_ptr() as isize); // EM_SETTEXTEX
            let text = r.text();
            assert_eq!(text, "a\r\nb\u{c}c\r\nd");
            let starts: Vec<u32> = line_ranges(&text).iter().map(|&(s, _)| position_of(&text, s)).collect();
            assert_eq!(starts, richedit_line_starts(&r));
        }

        #[test]
        fn live_replace_selection_keeps_a_form_feed_unless_alone() {
            // Raw EM_REPLACESEL: a lone "\f" becomes a CRLF, a longer string
            // keeps it. Hence replace_selection_on (next test).
            for (inserted, read) in [
                ("\u{c}", "\r\n"), ("\u{c}\u{c}", "\u{c}\u{c}"), ("\u{c}b", "\u{c}b"), ("b\u{c}", "b\u{c}"), ("x\u{c}y\nz", "x\u{c}y\r\nz"),
            ] {
                let r = make("");
                em_replacesel(&r, inserted);
                assert_eq!(r.text(), read, "{inserted:?}");
            }
            // Inside a text too, and EM_SETTEXTEX (ST_SELECTION | ST_KEEPUNDO)
            // does the same.
            let r = make("a \u{c} b");
            r.select(1, 4);
            em_replacesel(&r, "\u{c}");
            assert_eq!(r.text(), "a\r\nb");
            let r = make(" \u{c} ");
            r.select(0, 3);
            let wide: Vec<u16> = "\u{c}".encode_utf16().chain(std::iter::once(0)).collect();
            let st: [u32; 2] = [1 | 2, 1200]; // ST_KEEPUNDO | ST_SELECTION, UTF-16
            send(r.ed, 0x0461, st.as_ptr() as usize, wide.as_ptr() as isize); // EM_SETTEXTEX
            assert_eq!(r.text(), "\r\n");
        }

        #[test]
        fn live_editor_replacement_keeps_a_lone_form_feed_in_one_undo_step() {
            // replace_selection_on: what every line command, Tab and list
            // edit sends. A lone form feed stays a form feed, caret after it,
            // and one Ctrl+Z puts the replaced text back.
            for (text, sel, caret) in [(" \u{c} ", (0, 3), 1), ("a \u{c} b", (1, 4), 2), ("", (0, 0), 1)] {
                let r = make(text);
                r.select(sel.0, sel.1);
                r.stop_group_typing();
                r.replace_selection("\u{c}");
                let after = text[..utf16_range_to_byte_range(text, 0, sel.0 as usize).1].to_string()
                    + "\u{c}"
                    + &text[utf16_range_to_byte_range(text, sel.1 as usize, sel.1 as usize).0..];
                assert_eq!(r.text(), after, "{text:?}");
                assert_eq!(r.selection(), (caret, caret), "{text:?}");
                send(r.ed, 0x00C7, 0, 0); // EM_UNDO
                assert_eq!(r.text(), text, "one undo step for {text:?}");
            }
        }

        #[test]
        fn live_trim_and_shift_tab_keep_a_form_feed_left_alone() {
            // Trim on the whole tab " \f " leaves ["", ""]: the replacement is
            // a lone form feed (refuter, V2 round 1).
            let r = make(" \u{c} ");
            r.select(0, 0);
            assert_eq!(run_line_edit(&r, |t, s| line_op_edit(t, s, LineOp::TrimTrailing, ci)), Ok(true));
            assert_eq!(r.text(), "\u{c}");
            assert_eq!(r.selection(), (0, 1));
            send(r.ed, 0x00C7, 0, 0); // EM_UNDO
            assert_eq!(r.text(), " \u{c} ");
            // " \f\r\n": the block [" ", ""] becomes ["", ""] (reviewer).
            let r = make(" \u{c}\r\n");
            r.select(0, 0);
            assert_eq!(run_line_edit(&r, |t, s| line_op_edit(t, s, LineOp::TrimTrailing, ci)), Ok(true));
            assert_eq!(r.text(), "\u{c}\r\n");
            // Shift+Tab on "\t\f\t", everything selected.
            let mut r = make("\t\u{c}\t");
            r.select(0, 3);
            assert!(crate::ui::editor::indent::run_indent(&mut r, crate::ui::editor::indent::IndentDir::Out));
            assert_eq!(LineEditSteps::text(&r), "\u{c}");
            send(r.ed, 0x00C7, 0, 0); // EM_UNDO
            assert_eq!(LineEditSteps::text(&r), "\t\u{c}\t");
        }

        #[test]
        fn live_line_commands_on_a_form_feed_text() {
            let r = make("a\u{c}b\r\nc");
            r.select(2, 2);
            assert_eq!(run_line_edit(&r, |t, s| Ok::<_, ()>(move_lines_edit(t, s, true))), Ok(true));
            assert_eq!(r.text(), "b\u{c}a\r\nc");
            assert_eq!(r.selection(), (0, 0));
            assert_eq!(run_line_edit(&r, |t, s| Ok::<_, ()>(move_lines_edit(t, s, false))), Ok(true));
            assert_eq!(r.text(), "a\u{c}b\r\nc");
            assert_eq!(r.selection(), (2, 2));
            assert_eq!(run_line_edit(&r, |t, s| line_op_edit(t, s, LineOp::SortDescending, ci)), Ok(true));
            assert_eq!(r.text(), "c\u{c}b\r\na");
            r.select(2, 2);
            assert_eq!(run_line_edit(&r, duplicate_line_edit), Ok(true));
            assert_eq!(r.text(), "c\u{c}b\r\nb\r\na");
            assert_eq!(run_line_edit(&r, delete_line_edit), Ok(true));
            assert_eq!(r.text(), "c\u{c}b\r\na");
            // Two empty lines around a form feed: the text stays as it is.
            let r = make("\u{c}");
            r.select(0, 0);
            assert_eq!(run_line_edit(&r, |t, s| Ok::<_, ()>(move_lines_edit(t, s, false))), Ok(false));
            assert_eq!(r.text(), "\u{c}");
            assert_eq!(r.selection(), (1, 1));
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

        // ── Ctrl+D / Ctrl+Shift+K / F5 on the real control ──

        fn em_getsel(r: &Rich) -> (u32, u32) {
            let (mut s, mut e) = (0u32, 0u32);
            send(r.ed, 0x00B0, &mut s as *mut u32 as usize, &mut e as *mut u32 as isize); // EM_GETSEL
            (s, e)
        }

        /// Keyboard typing (WM_CHAR), grouped by RichEdit into one undo step.
        fn type_chars(r: &Rich, s: &str) {
            for c in s.encode_utf16() {
                send(r.ed, 0x0102, c as usize, 0);
            }
        }

        fn undo(r: &Rich) {
            send(r.ed, 0x00C7, 0, 0); // EM_UNDO
        }

        fn redo(r: &Rich) {
            send(r.ed, 0x0454, 0, 0); // EM_REDO
        }

        fn select_all(r: &Rich) {
            send(r.ed, 0x00B1, 0, -1); // EM_SETSEL(0, -1), RichEdit's own Ctrl+A
        }

        #[test]
        fn live_richedit_counts_a_break_as_one_position() {
            // Probes that do not depend on a LineEdit: the counting the fix
            // relies on, read from the control itself.
            let r = make("ab\r\ncd😀");
            assert_eq!(r.text(), "ab\r\ncd😀", "WM_GETTEXT gives \\r\\n breaks");
            // A caret pushed past the end lands on the UTF-16 length with a
            // break counted once: 7, where the text holds 10 UTF-8 bytes.
            send(r.ed, 0x00B1, 100, 100);
            assert_eq!(em_getsel(&r), (7, 7));
            // Select All ends one past the text: the final paragraph mark.
            select_all(&r);
            assert_eq!(em_getsel(&r), (0, 8));
            // Four positions from the start are "ab", the break and "c".
            r.select(0, 4);
            r.replace_selection("");
            assert_eq!(r.text(), "d😀");
        }

        #[test]
        fn live_insert_after_an_accent_is_one_undo_step() {
            let r = make("é\r\nab");
            r.select(1, 1);
            insert_at_selection(&r, TS);
            assert_eq!(r.text(), format!("é{TS}\r\nab"));
            assert_eq!(em_getsel(&r), (20, 20), "caret after the timestamp");
            undo(&r);
            assert_eq!(r.text(), "é\r\nab");
            redo(&r);
            assert_eq!(r.text(), format!("é{TS}\r\nab"));
        }

        #[test]
        fn live_insert_over_a_selection_or_select_all() {
            let r = make("ab\r\ncd😀");
            r.select(4, 1); // backward, across the break
            insert_at_selection(&r, TS);
            assert_eq!(r.text(), format!("a{TS}d😀"));
            assert_eq!(em_getsel(&r), (20, 20));
            let r = make("x\r\ny😀");
            select_all(&r);
            insert_at_selection(&r, TS);
            assert_eq!(r.text(), TS);
            assert_eq!(em_getsel(&r), (19, 19));
        }

        #[test]
        fn live_typing_then_insert_then_typing_undo_one_step_each() {
            let r = make("");
            type_chars(&r, "hi");
            insert_at_selection(&r, TS);
            type_chars(&r, "z");
            assert_eq!(r.text(), format!("hi{TS}z"));
            undo(&r);
            assert_eq!(r.text(), format!("hi{TS}"), "the typing after only");
            undo(&r);
            assert_eq!(r.text(), "hi", "the timestamp only");
            undo(&r);
            assert_eq!(r.text(), "");
        }

        #[test]
        fn live_line_edits_refuse_a_lone_surrogate_and_change_nothing() {
            // Half of an emoji typed (WM_CHAR): RichEdit keeps it. Ctrl+D or
            // Alt+Up would copy or move a line read with it replaced.
            let r = make("one\r\ntwo");
            send(r.ed, 0x00B1, usize::MAX >> 1, -1); // EM_SETSEL end
            send(r.ed, 0x0102, 0xD83D, 0); // WM_CHAR
            let before = crate::ui::editor_text::read_units(r.ed).expect("units").to_vec();
            assert_eq!(before.last(), Some(&0xD83D), "RichEdit keeps the lone surrogate");
            let refused = |res: Result<bool, LineEditError<Utf16RangeError>>| {
                assert!(matches!(res, Err(LineEditError::Unreadable(EditorTextError::InvalidUtf16 { line: 2, count: 1, .. }))), "{res:?}");
            };
            refused(run_line_edit(&r, duplicate_line_edit));
            refused(run_line_edit(&r, delete_line_edit));
            assert_eq!(crate::ui::editor_text::read_units(r.ed).expect("units").to_vec(), before, "the editor changed");
        }

        #[test]
        fn live_duplicate_after_an_emoji_is_one_undo_step() {
            let r = make("a😀\r\nb");
            r.select(3, 3);
            assert_eq!(run_line_edit(&r, duplicate_line_edit), Ok(true));
            assert_eq!(r.text(), "a😀\r\na😀\r\nb", "one break between the lines, not two");
            assert_eq!(em_getsel(&r), (7, 7), "caret at the end of the copy");
            undo(&r);
            assert_eq!(r.text(), "a😀\r\nb");
            redo(&r);
            assert_eq!(r.text(), "a😀\r\na😀\r\nb");
        }

        #[test]
        fn live_delete_of_the_last_line_is_one_undo_step() {
            let r = make("a\r\nb\r\nc");
            r.select(4, 4);
            assert_eq!(run_line_edit(&r, delete_line_edit), Ok(true));
            assert_eq!(r.text(), "a\r\nb", "the caret line, not the one above");
            assert_eq!(em_getsel(&r), (3, 3));
            undo(&r);
            assert_eq!(r.text(), "a\r\nb\r\nc");
            redo(&r);
            assert_eq!(r.text(), "a\r\nb");
        }

        #[test]
        fn live_typing_around_duplicate_and_delete_undo_one_step_each() {
            let r = make("one\r\ntwo");
            r.select(7, 7);
            type_chars(&r, "!");
            assert_eq!(run_line_edit(&r, duplicate_line_edit), Ok(true));
            assert_eq!(r.text(), "one\r\ntwo!\r\ntwo!");
            type_chars(&r, "x");
            assert_eq!(run_line_edit(&r, delete_line_edit), Ok(true));
            assert_eq!(r.text(), "one\r\ntwo!");
            for expected in ["one\r\ntwo!\r\ntwo!x", "one\r\ntwo!\r\ntwo!", "one\r\ntwo!", "one\r\ntwo"] {
                undo(&r);
                assert_eq!(r.text(), expected);
            }
        }

        #[test]
        fn live_select_all_then_duplicate_or_delete() {
            let r = make("x😀\r\ny");
            select_all(&r);
            assert_eq!(run_line_edit(&r, duplicate_line_edit), Ok(true));
            assert_eq!(r.text(), "x😀\r\nx😀\r\ny");
            let r = make("x😀\r\ny");
            select_all(&r);
            assert_eq!(run_line_edit(&r, delete_line_edit), Ok(true));
            assert_eq!(r.text(), "y");
        }

        #[test]
        fn live_wm_settext_empties_the_undo_history() {
            // Why the commands no longer rebuild the text with set_text
            // (nwg's set_text is SetWindowTextW, a WM_SETTEXT).
            let r = make("abc");
            r.select(3, 3);
            r.replace_selection("d");
            assert_eq!(send(r.ed, 0x00C6, 0, 0), 1, "EM_CANUNDO after EM_REPLACESEL");
            let wide: Vec<u16> = "abcd!".encode_utf16().chain(std::iter::once(0)).collect();
            send(r.ed, 0x000C, 0, wide.as_ptr() as isize); // WM_SETTEXT
            assert_eq!(send(r.ed, 0x00C6, 0, 0), 0, "EM_CANUNDO after WM_SETTEXT");
        }

        #[test]
        fn live_duplicate_keeps_the_scroll() {
            let text: Vec<String> = (1..=100).map(|n| format!("line {n}")).collect();
            let r = make(&text.join("\r\n"));
            send(r.ed, 0x00B6, 0, 40); // EM_LINESCROLL: 40 lines down
            let first = send(r.ed, 0x00CE, 0, 0); // EM_GETFIRSTVISIBLELINE
            assert!(first > 0, "scrolled: {first}");
            let pos = crate::ui::editor::edit_ops::line_start_position(&r.text(), first as u32 + 2);
            r.select(pos, pos);
            assert_eq!(run_line_edit(&r, duplicate_line_edit), Ok(true));
            assert_eq!(send(r.ed, 0x00CE, 0, 0), first, "the view stays where it was");
            assert_eq!(r.text().lines().nth(first as usize + 3), Some(text[first as usize + 2].as_str()), "the copy is under its line");
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
