// Bullet, numbered and checkbox lists in the editor.
//
// `enter_action` decides what plain Enter does from the text of the current
// line; `checkbox_toggle` decides what Ctrl+Enter does to it. `run_enter`
// and `run_toggle_checkbox` apply those decisions through `IndentSteps`
// (indent.rs), in RichEdit positions (UTF-16 code units, `\r\n` = 1).
// The editor key handler (events.rs) calls `handle_enter_bullet` on plain
// VK_RETURN; `handle_shortcut` calls `on_toggle_checkbox` on Ctrl+Enter.

#[allow(unused_imports)]
use super::*;
use super::indent::{line_end_at, line_start_at, FormIndentSteps, IndentSteps, TextRead};

/// The bullet of Ctrl+. and of bullet lists.
pub(super) const BULLET: &str = "• ";

/// What plain Enter does on a line.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum EnterAction {
    /// Not a list line (or an unusable one): the control inserts the break.
    Native,
    /// Insert a line break and this prefix at the caret.
    Continue(String),
    /// Empty list item: clear the line, no new line.
    EndList,
}

/// One edit inside a line: replace `remove` bytes at byte `at` with `insert`.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct LineEdit {
    pub at: usize,
    pub remove: usize,
    pub insert: &'static str,
}

/// Bytes of leading spaces and tabs.
fn indent_len(line: &str) -> usize {
    line.bytes().take_while(|b| *b == b' ' || *b == b'\t').count()
}

/// Bytes of a "• " or "- " bullet at the start of `s`, 0 without one.
fn bullet_len(s: &str) -> usize {
    if s.starts_with(BULLET) {
        BULLET.len()
    } else if s.starts_with("- ") {
        2
    } else {
        0
    }
}

/// A checkbox line: indentation and optional bullet (`lead` bytes), then
/// "[ ] ", "[x] " or "[X] ", then `rest`.
struct Checkbox<'a> {
    lead: usize,
    checked: bool,
    rest: &'a str,
}

fn parse_checkbox(line: &str) -> Option<Checkbox<'_>> {
    let i = indent_len(line);
    let lead = i + bullet_len(&line[i..]);
    let after = &line[lead..];
    [("[ ] ", false), ("[x] ", true), ("[X] ", true)]
        .iter()
        .find(|(p, _)| after.starts_with(p))
        .map(|(p, checked)| Checkbox { lead, checked: *checked, rest: &after[p.len()..] })
}

/// Most digits of a list number ("N. " or "N) ").
const MAX_NUMBER_DIGITS: usize = 9;

/// A numbered line: `indent`, number `n`, `punct` ('.' or ')'), a space,
/// then `rest`.
struct Numbered<'a> {
    indent: &'a str,
    n: u64,
    punct: char,
    rest: &'a str,
}

fn parse_numbered(line: &str) -> Option<Numbered<'_>> {
    let i = indent_len(line);
    let after = &line[i..];
    let digits = after.bytes().take_while(|b| b.is_ascii_digit()).count();
    if digits == 0 || digits > MAX_NUMBER_DIGITS {
        return None;
    }
    let punct = match after.as_bytes().get(digits..digits + 2) {
        Some([b'.', b' ']) => '.',
        Some([b')', b' ']) => ')',
        _ => return None,
    };
    // At most 9 ASCII digits: always fits a u64.
    let n = after[..digits].parse::<u64>().ok()?;
    Some(Numbered { indent: &line[..i], n, punct, rest: &after[digits + 2..] })
}

/// Decide what plain Enter does on `line` (the current line, without its
/// break):
///
/// - checkbox line ("[ ] ", "[x] ", "[X] " after optional indentation and
///   "• " or "- " bullet): continue with the same lead and "[ ] ";
/// - numbered line (indentation, 1 to 9 digits, ". " or ") "): continue
///   with the same indentation, N+1 and the same punctuation;
/// - bullet line ("• " at the start, as before): continue with "• ";
/// - any of these with nothing but blanks after the prefix: `EndList`;
/// - anything else: `Native`.
pub(super) fn enter_action(line: &str) -> EnterAction {
    if let Some(c) = parse_checkbox(line) {
        if c.rest.trim().is_empty() {
            return EnterAction::EndList;
        }
        return EnterAction::Continue(format!("{}[ ] ", &line[..c.lead]));
    }
    if let Some(n) = parse_numbered(line) {
        if n.rest.trim().is_empty() {
            return EnterAction::EndList;
        }
        return EnterAction::Continue(format!("{}{}{} ", n.indent, n.n + 1, n.punct));
    }
    if let Some(rest) = line.strip_prefix(BULLET) {
        if rest.trim().is_empty() {
            return EnterAction::EndList;
        }
        return EnterAction::Continue(BULLET.to_string());
    }
    EnterAction::Native
}

/// Decide what Ctrl+Enter does on `line`: flip the mark of its checkbox
/// ("[ ]" <-> "[x]", "[X]" counts as checked), or else insert "[ ] " after
/// the indentation and the "• " or "- " bullet.
pub(super) fn checkbox_toggle(line: &str) -> LineEdit {
    if let Some(c) = parse_checkbox(line) {
        let insert = if c.checked { " " } else { "x" };
        return LineEdit { at: c.lead + 1, remove: 1, insert };
    }
    let i = indent_len(line);
    LineEdit { at: i + bullet_len(&line[i..]), remove: 0, insert: "[ ] " }
}

/// Apply plain Enter. Returns true when the key is consumed (the caller
/// then swallows the WM_CHAR that follows), false when the control must
/// insert the break.
pub(super) fn run_enter(steps: &mut impl IndentSteps) -> bool {
    let text = match steps.text() {
        Ok(text) => text,
        Err(err) => {
            // As for a caret that does not map: the control's own Enter is
            // harmless, and a dialog on every Enter would be worse. The
            // save tells the user if the text cannot be stored.
            eprintln!("ERROR list enter: native Enter, editor text unreadable err={:?}", err);
            return false;
        }
    };
    let (s, e) = steps.selection();
    if s != e {
        return false;
    }
    let caret = (s as usize).min(utf16_len(&text));
    let b = match utf16_to_byte_exact(&text, caret) {
        Ok(b) => b,
        Err(err) => {
            // The control's own Enter is harmless: let it act.
            eprintln!("ERROR list enter: caret={} not mappable: {:?}", s, err);
            return false;
        }
    };
    let line_start = line_start_at(&text, b);
    let line_end = line_end_at(&text, b);
    match enter_action(&text[line_start..line_end]) {
        EnterAction::Native => false,
        EnterAction::Continue(prefix) => {
            steps.replace_selection(&format!("\n{}", prefix));
            steps.mark_modified();
            true
        }
        EnterAction::EndList => {
            let offsets = build_utf16_offset_map(&text);
            steps.set_selection(offsets[line_start] as u32, offsets[line_end] as u32);
            steps.replace_selection("");
            steps.mark_modified();
            true
        }
    }
}

/// Apply Ctrl+Enter to the line holding the start of the selection. The
/// selection keeps its place in the text.
pub(super) fn run_toggle_checkbox(steps: &mut impl IndentSteps) {
    let text = match steps.text() {
        Ok(text) => text,
        Err(err) => return steps.tell_unreadable(&err),
    };
    let (s, e) = steps.selection();
    let len = utf16_len(&text) as u32;
    let (s, e) = (s.min(len), e.min(len));
    let b = match utf16_to_byte_exact(&text, s.min(e) as usize) {
        Ok(b) => b,
        Err(err) => {
            eprintln!("ERROR checkbox toggle: selection={}..{} not mappable: {:?}", s, e, err);
            return;
        }
    };
    let line_start = line_start_at(&text, b);
    let edit = checkbox_toggle(&text[line_start..line_end_at(&text, b)]);
    let offsets = build_utf16_offset_map(&text);
    let at = offsets[line_start + edit.at] as u32;
    let end = offsets[line_start + edit.at + edit.remove] as u32;
    let inserted = utf16_len(edit.insert) as u32;

    steps.set_selection(at, end);
    steps.replace_selection(edit.insert);
    steps.mark_modified();

    let shift = |p: u32| {
        if p >= end {
            p - (end - at) + inserted
        } else if p > at {
            at + inserted
        } else {
            p
        }
    };
    steps.set_selection(shift(s), shift(e));
}

impl EditorForm {
    /// On plain Enter inside the editor, continue or end a bullet, numbered
    /// or checkbox list. Returns true when the Enter was consumed.
    pub(super) fn handle_enter_bullet(&self) -> bool {
        // A consumed key skips handle_shortcut, which resets the timer.
        self.autolock.borrow_mut().reset();
        // Only markers are inserted or removed: the display text.
        run_enter(&mut FormIndentSteps { form: self, read: TextRead::Display })
    }

    /// Toggle or insert the checkbox of the current line (Ctrl+Enter).
    pub(super) fn on_toggle_checkbox(&self) {
        run_toggle_checkbox(&mut FormIndentSteps { form: self, read: TextRead::Display });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cont(prefix: &str) -> EnterAction {
        EnterAction::Continue(prefix.to_string())
    }

    /// `line` after `checkbox_toggle`.
    fn toggled(line: &str) -> String {
        let e = checkbox_toggle(line);
        format!("{}{}{}", &line[..e.at], e.insert, &line[e.at + e.remove..])
    }

    // ── enter_action: bullets (unchanged behaviour) ──

    #[test]
    fn enter_on_bullet_continues_with_bullet() {
        assert_eq!(enter_action("• milk"), cont("• "));
    }

    #[test]
    fn enter_on_empty_bullet_ends_list() {
        assert_eq!(enter_action("• "), EnterAction::EndList);
        assert_eq!(enter_action("•    "), EnterAction::EndList);
        assert_eq!(enter_action("• \t "), EnterAction::EndList);
    }

    #[test]
    fn enter_on_bullet_without_space_or_indented_is_native() {
        // as before: only "• " at the start of the line is a bullet
        assert_eq!(enter_action("•milk"), EnterAction::Native);
        assert_eq!(enter_action("\t• milk"), EnterAction::Native);
        assert_eq!(enter_action("  • milk"), EnterAction::Native);
        assert_eq!(enter_action("- milk"), EnterAction::Native);
    }

    #[test]
    fn enter_on_plain_line_is_native() {
        assert_eq!(enter_action(""), EnterAction::Native);
        assert_eq!(enter_action("hello"), EnterAction::Native);
        assert_eq!(enter_action("   "), EnterAction::Native);
    }

    // ── enter_action: numbered ──

    #[test]
    fn enter_on_numbered_dot_increments() {
        assert_eq!(enter_action("1. eggs"), cont("2. "));
        assert_eq!(enter_action("9. x"), cont("10. "));
    }

    #[test]
    fn enter_on_numbered_paren_keeps_punctuation() {
        assert_eq!(enter_action("3) eggs"), cont("4) "));
    }

    #[test]
    fn enter_on_numbered_keeps_indentation() {
        assert_eq!(enter_action("\t\t7. x"), cont("\t\t8. "));
        assert_eq!(enter_action("  \t1) x"), cont("  \t2) "));
    }

    #[test]
    fn enter_on_numbered_nine_digits_and_leading_zero() {
        assert_eq!(enter_action("999999999. x"), cont("1000000000. "));
        assert_eq!(enter_action("0. x"), cont("1. "));
        assert_eq!(enter_action("007. x"), cont("8. "));
    }

    #[test]
    fn enter_on_ten_digits_is_native() {
        assert_eq!(enter_action("1234567890. x"), EnterAction::Native);
    }

    #[test]
    fn enter_on_empty_numbered_ends_list() {
        assert_eq!(enter_action("4. "), EnterAction::EndList);
        assert_eq!(enter_action("  4)   "), EnterAction::EndList);
    }

    #[test]
    fn enter_on_number_lookalikes_is_native() {
        assert_eq!(enter_action("1.5 kg"), EnterAction::Native);
        assert_eq!(enter_action("1.x"), EnterAction::Native);
        assert_eq!(enter_action("1."), EnterAction::Native);
        assert_eq!(enter_action("1:"), EnterAction::Native);
        assert_eq!(enter_action("1 . x"), EnterAction::Native);
        assert_eq!(enter_action(". x"), EnterAction::Native);
        assert_eq!(enter_action("a. x"), EnterAction::Native);
        assert_eq!(enter_action("١. x"), EnterAction::Native); // non-ASCII digit
    }

    // ── enter_action: checkboxes ──

    #[test]
    fn enter_on_checkbox_continues_unchecked() {
        assert_eq!(enter_action("[ ] buy"), cont("[ ] "));
        assert_eq!(enter_action("[x] buy"), cont("[ ] "));
        assert_eq!(enter_action("[X] buy"), cont("[ ] "));
    }

    #[test]
    fn enter_on_checkbox_keeps_indentation_and_bullet() {
        assert_eq!(enter_action("\t[x] a"), cont("\t[ ] "));
        assert_eq!(enter_action("• [x] a"), cont("• [ ] "));
        assert_eq!(enter_action("- [ ] a"), cont("- [ ] "));
        assert_eq!(enter_action("  - [X] a"), cont("  - [ ] "));
        assert_eq!(enter_action("\t• [ ] a"), cont("\t• [ ] "));
    }

    #[test]
    fn enter_on_empty_checkbox_ends_list() {
        assert_eq!(enter_action("[ ] "), EnterAction::EndList);
        assert_eq!(enter_action("[x]   "), EnterAction::EndList);
        assert_eq!(enter_action("• [ ] "), EnterAction::EndList);
        assert_eq!(enter_action("\t- [X] \t"), EnterAction::EndList);
    }

    #[test]
    fn enter_on_checkbox_lookalikes() {
        // no space after the box: not a checkbox
        assert_eq!(enter_action("[ ]"), EnterAction::Native);
        assert_eq!(enter_action("[y] a"), EnterAction::Native);
        assert_eq!(enter_action("[] a"), EnterAction::Native);
        // a bullet whose text looks like a box stays a bullet
        assert_eq!(enter_action("• [x]"), cont("• "));
    }

    #[test]
    fn enter_with_non_ascii_text_after_prefix() {
        assert_eq!(enter_action("1. café 😀"), cont("2. "));
        assert_eq!(enter_action("[ ] 😀"), cont("[ ] "));
    }

    // ── checkbox_toggle ──

    #[test]
    fn toggle_checks_and_unchecks() {
        assert_eq!(toggled("[ ] a"), "[x] a");
        assert_eq!(toggled("[x] a"), "[ ] a");
        assert_eq!(toggled("[X] a"), "[ ] a");
    }

    #[test]
    fn toggle_after_indentation_and_bullet() {
        assert_eq!(toggled("\t• [ ] a"), "\t• [x] a");
        assert_eq!(toggled("  - [x] a"), "  - [ ] a");
        assert_eq!(checkbox_toggle("• [ ] a"), LineEdit { at: BULLET.len() + 1, remove: 1, insert: "x" });
    }

    #[test]
    fn toggle_inserts_box_after_indentation_and_bullet() {
        assert_eq!(toggled("task"), "[ ] task");
        assert_eq!(toggled(""), "[ ] ");
        assert_eq!(toggled("\t  task"), "\t  [ ] task");
        assert_eq!(toggled("• task"), "• [ ] task");
        assert_eq!(toggled("\t- task"), "\t- [ ] task");
        assert_eq!(toggled("[ ]"), "[ ] [ ]");
    }

    #[test]
    fn toggle_twice_restores_box() {
        assert_eq!(toggled(&toggled("• [ ] a")), "• [ ] a");
    }

    // ── sequence (run_enter, run_toggle_checkbox) ──

    #[derive(Debug, PartialEq)]
    enum Call {
        Select(u32, u32),
        Replace(String),
        Modified,
        Unreadable(EditorTextError),
    }

    struct Fake {
        text: String,
        sel: (u32, u32),
        calls: Vec<Call>,
        unreadable: Option<EditorTextError>,
    }

    impl Fake {
        fn new(text: &str, s: u32, e: u32) -> Self {
            Fake { text: text.to_string(), sel: (s, e), calls: Vec::new(), unreadable: None }
        }
        /// What the control does with an Enter the list logic left to it.
        fn native_enter(&mut self) {
            let (b0, b1) = utf16_range_to_byte_range(&self.text, self.sel.0 as usize, self.sel.1 as usize);
            self.text = format!("{}\r{}", &self.text[..b0], &self.text[b1..]);
            let p = self.sel.0 + 1;
            self.sel = (p, p);
        }
        /// The control typing `s` at the caret.
        fn type_text(&mut self, s: &str) {
            let (b0, b1) = utf16_range_to_byte_range(&self.text, self.sel.0 as usize, self.sel.1 as usize);
            self.text = format!("{}{}{}", &self.text[..b0], s, &self.text[b1..]);
            let p = self.sel.0 + utf16_len(s) as u32;
            self.sel = (p, p);
        }
        /// Enter as the editor key handler runs it.
        fn enter(&mut self) {
            if !run_enter(self) {
                self.native_enter();
            }
        }
    }

    impl IndentSteps for Fake {
        fn text(&self) -> Result<String, EditorTextError> {
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
            // RichEdit stores a paragraph break as a lone \r.
            let s = s.replace('\n', "\r");
            let (b0, b1) = utf16_range_to_byte_range(&self.text, self.sel.0 as usize, self.sel.1 as usize);
            self.text = format!("{}{}{}", &self.text[..b0], s, &self.text[b1..]);
            let p = self.sel.0 + utf16_len(&s) as u32;
            self.sel = (p, p);
            self.calls.push(Call::Replace(s.to_string()));
        }
        fn mark_modified(&mut self) {
            self.calls.push(Call::Modified);
        }
    }

    #[test]
    fn seq_unreadable_text_leaves_enter_to_the_control_and_tells_on_ctrl_enter() {
        let err = EditorTextError::ReadFailed { expected: 7, copied: 0 };
        let mut f = Fake::new("1. eggs", 7, 7);
        f.unreadable = Some(err.clone());
        assert!(!run_enter(&mut f), "Enter goes to the control, no dialog");
        assert!(f.calls.is_empty(), "{:?}", f.calls);
        run_toggle_checkbox(&mut f);
        assert_eq!(f.calls, vec![Call::Unreadable(err)]);
        assert_eq!(f.text, "1. eggs");
    }

    #[test]
    fn seq_enter_on_numbered_continues_then_empty_item_ends() {
        let mut f = Fake::new("1. eggs", 7, 7);
        assert!(run_enter(&mut f));
        assert_eq!(f.calls, vec![Call::Replace("\r2. ".into()), Call::Modified]);
        assert_eq!(f.text, "1. eggs\r2. ");
        assert_eq!(f.sel, (11, 11));
        f.calls.clear();
        // Enter on the empty "2. ": the prefix goes, no new line
        assert!(run_enter(&mut f));
        assert_eq!(
            f.calls,
            vec![Call::Select(8, 11), Call::Replace(String::new()), Call::Modified]
        );
        assert_eq!(f.text, "1. eggs\r");
        assert_eq!(f.sel, (8, 8));
        // next Enter is the control's own
        assert!(!run_enter(&mut f));
    }

    #[test]
    fn seq_typing_a_numbered_list() {
        let mut f = Fake::new("", 0, 0);
        f.type_text("\t1) a");
        f.enter();
        f.type_text("b");
        f.enter();
        f.type_text("c");
        f.enter();
        f.enter();
        f.type_text("end");
        assert_eq!(f.text, "\t1) a\r\t2) b\r\t3) c\rend");
    }

    #[test]
    fn seq_typing_a_checklist_then_toggling() {
        let mut f = Fake::new("", 0, 0);
        f.type_text("• [x] a");
        f.enter();
        f.type_text("b");
        f.enter();
        f.enter();
        f.type_text("z");
        assert_eq!(f.text, "• [x] a\r• [ ] b\rz");
        // Ctrl+Enter on line 2 (caret after "b") checks it, caret stays
        f.sel = (15, 15);
        run_toggle_checkbox(&mut f);
        assert_eq!(f.text, "• [x] a\r• [x] b\rz");
        assert_eq!(f.sel, (15, 15));
        // Ctrl+Enter on line 3 adds a box and moves the caret with the text
        f.sel = (17, 17);
        run_toggle_checkbox(&mut f);
        assert_eq!(f.text, "• [x] a\r• [x] b\r[ ] z");
        assert_eq!(f.sel, (21, 21));
    }

    #[test]
    fn seq_bullet_enter_is_unchanged() {
        let mut f = Fake::new("• a", 3, 3);
        assert!(run_enter(&mut f));
        assert_eq!(f.calls, vec![Call::Replace("\r• ".into()), Call::Modified]);
        f.calls.clear();
        assert!(run_enter(&mut f));
        assert_eq!(f.calls, vec![Call::Select(4, 6), Call::Replace(String::new()), Call::Modified]);
        assert_eq!(f.text, "• a\r");
    }

    #[test]
    fn seq_enter_mid_line_carries_the_rest() {
        // as with bullets: the break and prefix go in at the caret
        let mut f = Fake::new("1. abcd", 5, 5);
        assert!(run_enter(&mut f));
        assert_eq!(f.text, "1. ab\r2. cd");
    }

    #[test]
    fn seq_enter_with_selection_is_native() {
        let mut f = Fake::new("1. abcd", 3, 5);
        assert!(!run_enter(&mut f));
        assert!(f.calls.is_empty());
    }

    #[test]
    fn seq_enter_on_crlf_text_reads_the_current_line_only() {
        // RichEdit positions: "1. a"=0..4, \r\n=4, "b"=5
        let mut f = Fake::new("1. a\r\nb", 6, 6);
        assert!(!run_enter(&mut f));
        let mut f = Fake::new("x\r\n1. a\r\ny", 6, 6);
        assert!(run_enter(&mut f));
        assert_eq!(f.text, "x\r\n1. a\r2. \r\ny");
    }

    #[test]
    fn seq_enter_after_emoji_uses_utf16_positions() {
        // 😀 = 2 code units: "😀\r" = 0..3, "[ ] " = 3..7
        let mut f = Fake::new("😀\r[ ] ", 7, 7);
        assert!(run_enter(&mut f));
        assert_eq!(f.calls, vec![Call::Select(3, 7), Call::Replace(String::new()), Call::Modified]);
        assert_eq!(f.text, "😀\r");
    }

    #[test]
    fn seq_unmappable_caret_leaves_enter_to_the_control() {
        let mut f = Fake::new("😀", 1, 1);
        assert!(!run_enter(&mut f));
        assert!(f.calls.is_empty());
    }

    #[test]
    fn seq_caret_past_end_is_clamped() {
        let mut f = Fake::new("1. a", 99, 99);
        assert!(run_enter(&mut f));
        assert_eq!(f.text, "1. a\r2. ");
    }

    #[test]
    fn seq_toggle_is_one_replace_then_restores_selection() {
        let mut f = Fake::new("a\r[ ] task", 6, 9);
        run_toggle_checkbox(&mut f);
        assert_eq!(
            f.calls,
            vec![Call::Select(3, 4), Call::Replace("x".into()), Call::Modified, Call::Select(6, 9)]
        );
        assert_eq!(f.text, "a\r[x] task");
    }

    #[test]
    fn seq_toggle_insert_shifts_caret_after_point_only() {
        // caret inside the indentation stays
        let mut f = Fake::new("\t\ttask", 1, 1);
        run_toggle_checkbox(&mut f);
        assert_eq!(f.text, "\t\t[ ] task");
        assert_eq!(f.sel, (1, 1));
        // caret at the insertion point moves with the text
        let mut f = Fake::new("task", 0, 0);
        run_toggle_checkbox(&mut f);
        assert_eq!(f.sel, (4, 4));
    }

    #[test]
    fn seq_toggle_uses_line_of_selection_start() {
        let mut f = Fake::new("a\rb", 3, 0);
        run_toggle_checkbox(&mut f);
        assert_eq!(f.text, "[ ] a\rb");
        assert_eq!(f.sel, (7, 4));
    }

    #[test]
    fn seq_toggle_unmappable_selection_changes_nothing() {
        let mut f = Fake::new("😀", 1, 1);
        run_toggle_checkbox(&mut f);
        assert!(f.calls.is_empty());
        assert_eq!(f.text, "😀");
    }

    // ── wiring ──

    fn editor_src(file: &str) -> String {
        crate::read_source(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("src").join("ui").join("editor").join(file),
        )
    }

    #[test]
    fn ctrl_enter_toggles_checkbox_and_swallows_its_char() {
        let ops = editor_src("edit_ops.rs");
        assert!(ops.contains("0x0D => { self.on_toggle_checkbox(); return true; } // Ctrl+Enter"));
        let ev = editor_src("events.rs");
        assert!(ev.contains("if vk == 0x0D && ctrl {\n                                CTRL_ENTER_SUPPRESS_CHAR.store(true"));
        assert!(ev.contains("0x0A => CTRL_ENTER_SUPPRESS_CHAR.swap(false"));
        assert!(ev.contains("CTRL_ENTER_SUPPRESS_CHAR.swap(false"));
    }

    #[test]
    fn plain_enter_still_goes_through_list_logic() {
        let ev = editor_src("events.rs");
        assert!(ev.contains("vk == 0x0D && !ctrl && !shift && f_ed.handle_enter_bullet()"));
        // one implementation only
        assert!(!editor_src("edit_ops.rs").contains("fn handle_enter_bullet"));
    }
}
