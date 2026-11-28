// Edit > Convert Case: UPPERCASE (Ctrl+Shift+U), lowercase (Ctrl+U), Title
// Case (menu only), on the selection or, with none, on the word under the
// caret (the double-click word, `expand_word_selection`).
//
// Positions are RichEdit character positions (UTF-16 code units, a line
// break = 1, see helpers.rs). The converted text can change length
// ("ß" -> "SS"): the selection is set over the new text. The change is one
// EM_REPLACESEL (undoable) through `run_line_edit` (line_ops.rs): one undo
// step.

#[allow(unused_imports)]
use super::*;
use super::line_ops::{run_line_edit, EditorLineSteps, LineEdit, LineEditError};
use crate::features::case_convert::{self, CaseOp};

/// The edit converting the selection `sel`, or the word under the caret
/// when it is empty. `Ok(None)` when there is nothing to convert (caret on
/// whitespace, empty tab) or the text would not change. A selection end
/// that does not fall on a character of `text` is an error, except one
/// position past the end: RichEdit's final paragraph mark, which WM_GETTEXT
/// leaves out (EM_SETSEL(0, -1), triple-click on the last line), is the end.
pub(super) fn case_edit(text: &str, sel: (u32, u32), op: CaseOp) -> Result<Option<LineEdit>, Utf16RangeError> {
    let max = utf16_len(text) as u32;
    let end_mark = |p: u32| if p == max + 1 { max } else { p };
    let (lo, hi) = (end_mark(sel.0.min(sel.1)), end_mark(sel.0.max(sel.1)));
    let (b0, b1) = if lo == hi {
        let caret = utf16_to_byte_exact(text, lo as usize)?;
        match expand_word_selection(text, caret, caret) {
            Some(range) => range,
            None => return Ok(None),
        }
    } else {
        (utf16_to_byte_exact(text, lo as usize)?, utf16_to_byte_exact(text, hi as usize)?)
    };
    let selected = &text[b0..b1];
    let converted = case_convert::convert(op, selected);
    if converted == selected {
        return Ok(None);
    }
    // WM_GETTEXT gives "\r\n"; EM_REPLACESEL takes "\n" as the paragraph
    // break (as line_ops.rs). Case mappings never produce "\r" or "\n".
    let replacement = converted.replace("\r\n", "\n");
    let offsets = build_utf16_offset_map(text);
    let start = offsets[b0] as u32;
    Ok(Some(LineEdit {
        start,
        end: offsets[b1] as u32,
        sel: (start, start + utf16_len(&replacement) as u32),
        text: replacement,
    }))
}

impl EditorForm {
    /// Edit > Convert Case > ..., Ctrl+Shift+U, Ctrl+U.
    pub(super) fn on_convert_case(&self, op: CaseOp) {
        match run_line_edit(&EditorLineSteps(self), |text, sel| case_edit(text, sel, op)) {
            Ok(changed) => {
                eprintln!("INFO case: op={:?} changed={}", op, changed);
                if changed {
                    self.send_editor_message(0x00B7, 0, 0); // EM_SCROLLCARET
                }
            }
            Err(LineEditError::Unreadable(e)) => self.tell_unreadable("convert_case", &e),
            // The selection comes from the RichEdit itself: a mismatch is a
            // bug, logged and shown; the text was not touched.
            Err(LineEditError::Compute(e)) => {
                eprintln!("ERROR case: op={:?} selection not mapped onto the text: {:?}", op, e);
                self.inform(&t("case.error.title"), &t("case.error.mapping").replace("{msg}", &format!("{:?}", e)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::line_ops::tests::Fake;

    fn edit(text: &str, sel: (u32, u32), op: CaseOp) -> LineEdit {
        case_edit(text, sel, op).expect("maps").expect("changes")
    }

    // ── case_edit ──

    #[test]
    fn selection_is_converted_and_stays_selected() {
        let e = edit("say hello now", (4, 9), CaseOp::Upper);
        assert_eq!((e.start, e.end, e.text.as_str(), e.sel), (4, 9, "HELLO", (4, 9)));
        // Reversed selection (anchor after the caret).
        let e = edit("say HELLO now", (9, 4), CaseOp::Lower);
        assert_eq!((e.start, e.end, e.text.as_str(), e.sel), (4, 9, "hello", (4, 9)));
        let e = edit("say hello now", (0, 13), CaseOp::Title);
        assert_eq!((e.text.as_str(), e.sel), ("Say Hello Now", (0, 13)));
    }

    #[test]
    fn longer_result_widens_the_selection() {
        // "ß" -> "SS": 6 positions become 7.
        let e = edit("x straße y", (2, 8), CaseOp::Upper);
        assert_eq!((e.start, e.end, e.text.as_str(), e.sel), (2, 8, "STRASSE", (2, 9)));
        // "İ" -> "i" + U+0307: 1 position becomes 2.
        let e = edit("aİb", (1, 2), CaseOp::Lower);
        assert_eq!((e.start, e.end, e.text.as_str(), e.sel), (1, 2, "i\u{0307}", (1, 3)));
    }

    #[test]
    fn surrogate_pairs_count_as_two_positions() {
        // "😀" is 2 UTF-16 units: "😀abc" spans 0..5.
        let e = edit("😀abc d", (0, 5), CaseOp::Title);
        assert_eq!((e.start, e.end, e.text.as_str(), e.sel), (0, 5, "😀Abc", (0, 5)));
        // A Deseret letter (outside the BMP) with case.
        let e = edit("x \u{10428}", (2, 4), CaseOp::Upper);
        assert_eq!((e.start, e.end, e.text.as_str(), e.sel), (2, 4, "\u{10400}", (2, 4)));
        // After an emoji, positions shift by 2.
        let e = edit("😀 straße", (3, 9), CaseOp::Upper);
        assert_eq!((e.start, e.end, e.text.as_str(), e.sel), (3, 9, "STRASSE", (3, 10)));
    }

    #[test]
    fn selection_ending_inside_a_surrogate_pair_is_an_error() {
        assert_eq!(case_edit("a😀", (0, 2), CaseOp::Upper), Err(Utf16RangeError::MidSurrogate { pos: 2 }));
        assert!(matches!(case_edit("ab", (0, 9), CaseOp::Upper), Err(Utf16RangeError::OutOfBounds { .. })));
    }

    #[test]
    fn caret_converts_the_double_click_word() {
        // Caret inside "hello".
        let e = edit("say hello now", (6, 6), CaseOp::Upper);
        assert_eq!((e.start, e.end, e.text.as_str(), e.sel), (4, 9, "HELLO", (4, 9)));
        // Caret right after a word, and right before one.
        assert_eq!(edit("abc def", (3, 3), CaseOp::Upper).text, "ABC");
        assert_eq!(edit("abc def", (4, 4), CaseOp::Upper).text, "DEF");
        // The word is whitespace-delimited, punctuation included.
        let e = edit("mail user@x.com now", (7, 7), CaseOp::Upper);
        assert_eq!((e.start, e.end, e.text.as_str()), (5, 15, "USER@X.COM"));
        // The same range as a double-click from that caret.
        let text = "mail user@x.com now";
        assert_eq!(expand_word_selection(text, 7, 7), Some((5, 15)));
    }

    #[test]
    fn caret_word_with_length_change() {
        let e = edit("ab straße", (5, 5), CaseOp::Upper);
        assert_eq!((e.start, e.end, e.text.as_str(), e.sel), (3, 9, "STRASSE", (3, 10)));
    }

    #[test]
    fn caret_on_whitespace_or_empty_tab_does_nothing() {
        assert_eq!(case_edit("ab  cd", (3, 3), CaseOp::Upper), Ok(None));
        assert_eq!(case_edit("", (0, 0), CaseOp::Upper), Ok(None));
        assert_eq!(case_edit("ab\r\n\r\ncd", (3, 3), CaseOp::Upper), Ok(None));
    }

    #[test]
    fn unchanged_text_is_none() {
        assert_eq!(case_edit("ABC", (0, 3), CaseOp::Upper), Ok(None));
        assert_eq!(case_edit("abc 42", (4, 4), CaseOp::Upper), Ok(None));
        assert_eq!(case_edit("Title Case", (0, 10), CaseOp::Title), Ok(None));
    }

    #[test]
    fn multi_line_selection_sends_paragraph_breaks_as_newlines() {
        // WM_GETTEXT gives "\r\n" (1 position); EM_REPLACESEL takes "\n".
        let e = edit("ab\r\ncd", (0, 5), CaseOp::Upper);
        assert_eq!((e.start, e.end, e.text.as_str(), e.sel), (0, 5, "AB\nCD", (0, 5)));
    }

    // ── run_line_edit with case_edit: the sequence, with a fake editor ──

    #[test]
    fn sequence_selects_replaces_once_then_selects_the_new_text() {
        let f = Fake::new("x straße y", (2, 8));
        assert_eq!(run_line_edit(&f, |t, s| case_edit(t, s, CaseOp::Upper)), Ok(true));
        assert_eq!(
            *f.log.borrow(),
            vec!["select:2..8", "stop_group", "replace:\"STRASSE\"", "select:2..9", "modified"]
        );
        assert_eq!(*f.text.borrow(), "x STRASSE y");
    }

    #[test]
    fn sequence_upper_then_lower_then_title_on_the_caret_word() {
        let f = Fake::new("say hello now", (6, 6));
        run_line_edit(&f, |t, s| case_edit(t, s, CaseOp::Upper)).unwrap();
        assert_eq!((f.text.borrow().as_str(), *f.sel.borrow()), ("say HELLO now", (4, 9)));
        // The word stays selected: the next command works on it.
        run_line_edit(&f, |t, s| case_edit(t, s, CaseOp::Lower)).unwrap();
        assert_eq!((f.text.borrow().as_str(), *f.sel.borrow()), ("say hello now", (4, 9)));
        run_line_edit(&f, |t, s| case_edit(t, s, CaseOp::Title)).unwrap();
        assert_eq!((f.text.borrow().as_str(), *f.sel.borrow()), ("say Hello now", (4, 9)));
    }

    #[test]
    fn sequence_without_change_or_with_error_touches_nothing() {
        let f = Fake::new("ABC", (0, 3));
        assert_eq!(run_line_edit(&f, |t, s| case_edit(t, s, CaseOp::Upper)), Ok(false));
        assert!(f.log.borrow().is_empty());
        let f = Fake::new("a😀", (0, 2));
        assert!(run_line_edit(&f, |t, s| case_edit(t, s, CaseOp::Upper)).is_err());
        assert!(f.log.borrow().is_empty());
        assert_eq!(*f.text.borrow(), "a😀");
    }

    #[test]
    fn sequence_multi_line_round_trip() {
        let f = Fake::new("straße\r\nnoël", (0, 11));
        run_line_edit(&f, |t, s| case_edit(t, s, CaseOp::Upper)).unwrap();
        assert_eq!((f.text.borrow().as_str(), *f.sel.borrow()), ("STRASSE\r\nNOËL", (0, 12)));
        run_line_edit(&f, |t, s| case_edit(t, s, CaseOp::Lower)).unwrap();
        assert_eq!((f.text.borrow().as_str(), *f.sel.borrow()), ("strasse\r\nnoël", (0, 12)));
    }

    // ── Code reading: wiring of the shortcuts and the menu ──

    fn src(file: &str) -> String {
        crate::read_source(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ui/editor").join(file))
    }

    #[test]
    fn ctrl_u_and_ctrl_shift_u_reach_the_conversion() {
        let shortcut = src("edit_ops.rs");
        let ctrl = shortcut.find("if ctrl && !shift {").expect("Ctrl block");
        let ctrl_shift = shortcut.find("} else if ctrl && shift {").expect("Ctrl+Shift block");
        let rest = shortcut.find("} else if !ctrl && !shift {").expect("plain block");
        let lower = shortcut.find("0x55 => { self.on_convert_case(CaseOp::Lower); return true; }").expect("Ctrl+U");
        let upper = shortcut.rfind("0x55 => { self.on_convert_case(CaseOp::Upper); return true; }").expect("Ctrl+Shift+U");
        assert!(ctrl < lower && lower < ctrl_shift, "Ctrl+U in the Ctrl block");
        assert!(ctrl_shift < upper && upper < rest, "Ctrl+Shift+U in the Ctrl+Shift block");
    }

    #[test]
    fn every_convert_case_menu_item_is_dispatched() {
        // The menus run through the command table (palette_ops.rs).
        let table = src("palette_ops.rs");
        for (id, item, op) in [
            ("CaseUpper", "menu_edit_case_upper", "CaseOp::Upper"),
            ("CaseLower", "menu_edit_case_lower", "CaseOp::Lower"),
            ("CaseTitle", "menu_edit_case_title", "CaseOp::Title"),
        ] {
            assert!(table.contains(&format!("{id} => &self.{item},")), "{item} not in the command table");
            assert!(table.contains(&format!("{id} => self.on_convert_case(crate::features::case_convert::{op}),")), "{id} -> {op}");
        }
    }

    #[test]
    fn select_all_covers_every_utf16_unit() {
        // nwg's len() counts chars: one unit short per astral character.
        let body = src("edit_ops.rs");
        let start = body.find("fn on_select_all").expect("on_select_all");
        let end = start + body[start..].find("\n    }\n").expect("end of on_select_all");
        let f = &body[start..end];
        assert!(!f.contains("self.editor.len()"), "on_select_all must not use the char count");
        assert!(f.contains("utf16_len"), "on_select_all must select utf16_len positions");
        assert_eq!(utf16_len("a😀b straße"), 11);
    }

    #[test]
    fn convert_case_error_is_reported_to_the_user() {
        let body = src("case_ops.rs");
        let start = body.find("fn on_convert_case").expect("on_convert_case");
        let err = start + body[start..].find("Err(LineEditError::Compute(e)) =>").expect("Err branch");
        let tail = &body[err..err + body[err..].find("\n        }\n").expect("end of match")];
        assert!(tail.contains("self.inform(") && tail.contains("case.error.mapping"), "Err branch informs");
        // An unreadable text (a lone surrogate) is told too.
        assert!(body[start..err].contains("Err(LineEditError::Unreadable(e)) => self.tell_unreadable(\"convert_case\", &e),"));
    }

    #[test]
    fn selection_one_past_the_end_is_the_end() {
        // RichEdit's final paragraph mark (not in WM_GETTEXT) can be selected.
        let e = edit("abc", (0, 4), CaseOp::Upper);
        assert_eq!((e.start, e.end, e.text.as_str(), e.sel), (0, 3, "ABC", (0, 3)));
        let e = edit("ab\r\ncd", (6, 3), CaseOp::Upper);
        assert_eq!((e.start, e.end, e.text.as_str(), e.sel), (3, 5, "CD", (3, 5)));
        // Further out stays an error.
        assert!(matches!(case_edit("abc", (0, 9), CaseOp::Upper), Err(Utf16RangeError::OutOfBounds { .. })));
        assert!(matches!(case_edit("abc", (5, 5), CaseOp::Upper), Err(Utf16RangeError::OutOfBounds { .. })));
    }

    // ── Live RichEdit: length change, selection and one undo step ──

    #[test]
    fn live_conversion_is_one_undo_step_and_selects_the_new_text() {
        use super::super::line_ops::tests::live::{make, send};
        use super::super::line_ops::LineEditSteps;
        let r = make("x straße😀\r\nnoël y");
        // "straße😀\r\nnoël" = 2..15 (emoji 2 units, break 1).
        r.select(2, 15);
        assert_eq!(run_line_edit(&r, |t, s| case_edit(t, s, CaseOp::Upper)), Ok(true));
        assert_eq!(r.text(), "x STRASSE😀\r\nNOËL y");
        assert_eq!(r.selection(), (2, 16));

        // Caret word ("y" at 17), then undo restores each step.
        r.select(17, 17);
        assert_eq!(run_line_edit(&r, |t, s| case_edit(t, s, CaseOp::Title)), Ok(true));
        assert_eq!(r.text(), "x STRASSE😀\r\nNOËL Y");
        assert_eq!(r.selection(), (17, 18));

        send(r.ed, 0x00C7, 0, 0); // EM_UNDO: the title case only
        assert_eq!(r.text(), "x STRASSE😀\r\nNOËL y");
        send(r.ed, 0x00C7, 0, 0); // EM_UNDO: the uppercase
        assert_eq!(r.text(), "x straße😀\r\nnoël y");
    }

    #[test]
    fn live_select_all_then_upper_converts_past_an_emoji() {
        use super::super::line_ops::tests::live::make;
        use super::super::line_ops::LineEditSteps;
        let r = make("a😀b straße");
        r.select(0, utf16_len(&r.text()) as u32);
        assert_eq!(run_line_edit(&r, |t, s| case_edit(t, s, CaseOp::Upper)), Ok(true));
        assert_eq!(r.text(), "A😀B STRASSE");
        assert_eq!(r.selection(), (0, 12));
    }

    #[test]
    fn live_whole_text_selection_with_minus_one_is_converted() {
        use super::super::line_ops::tests::live::{make, send};
        let r = make("abc");
        send(r.ed, 0x00B1, 0, -1); // EM_SETSEL(0, -1): everything
        assert_eq!(run_line_edit(&r, |t, s| case_edit(t, s, CaseOp::Upper)), Ok(true));
        assert_eq!(r.text(), "ABC");
        let r = make("ab\r\ncd");
        send(r.ed, 0x00B1, 0, -1);
        assert_eq!(run_line_edit(&r, |t, s| case_edit(t, s, CaseOp::Upper)), Ok(true));
        assert_eq!(r.text(), "AB\r\nCD");
    }
}
