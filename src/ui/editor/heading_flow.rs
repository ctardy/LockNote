// Edit > Go to Heading (Ctrl+Shift+O), behind a trait: read the active
// tab, list its headings, and move the caret to the chosen one only if no
// lock happened while the dialog was open (editor-window rules,
// CONTRIBUTING). `FormHeadingSteps` (edit_ops.rs) implements the steps on
// the window; the tests below drive the sequence with a recording fake.

use crate::features::outline::{self, Heading};

/// What the sequence needs from the window.
pub(super) trait HeadingSteps {
    /// The text of the active tab.
    fn text(&self) -> String;
    fn lock_epoch(&self) -> u64;
    /// Show the dialog; the index of the chosen heading, `None` if cancelled.
    fn pick(&mut self, headings: &[Heading]) -> Option<usize>;
    fn still_valid(&self, epoch: u64) -> bool;
    /// Caret at the start of the zero-based line, scrolled into view.
    fn move_caret(&mut self, line: u32);
}

/// How a run ended (logged by the caller).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum HeadingOutcome {
    Cancelled,
    /// The note locked while the dialog was open: nothing done.
    DroppedLocked,
    Moved { line: u32 },
    /// The dialog returned an index outside the list (a bug): nothing done.
    BadIndex { index: usize, count: usize },
}

pub(super) fn go_to_heading<S: HeadingSteps>(s: &mut S) -> HeadingOutcome {
    // Only the headings cross the dialog, not the tab's text.
    let headings = outline::headings(&s.text());
    let epoch = s.lock_epoch();
    let Some(index) = s.pick(&headings) else {
        return HeadingOutcome::Cancelled;
    };
    if !s.still_valid(epoch) {
        return HeadingOutcome::DroppedLocked;
    }
    let Some(line) = headings.get(index).and_then(|h| u32::try_from(h.line).ok()) else {
        return HeadingOutcome::BadIndex { index, count: headings.len() };
    };
    s.move_caret(line);
    HeadingOutcome::Moved { line }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Records the steps in order; `answer` is what the dialog returns,
    /// `lock_during_dialog` moves the epoch while it is open.
    struct Fake {
        text: String,
        answer: Option<usize>,
        lock_during_dialog: bool,
        epoch: u64,
        shown: Vec<String>,
        log: Vec<String>,
    }

    impl Fake {
        fn new(text: &str, answer: Option<usize>) -> Self {
            Fake { text: text.into(), answer, lock_during_dialog: false, epoch: 7, shown: Vec::new(), log: Vec::new() }
        }
    }

    impl HeadingSteps for Fake {
        fn text(&self) -> String {
            self.text.clone()
        }
        fn lock_epoch(&self) -> u64 {
            self.epoch
        }
        fn pick(&mut self, headings: &[Heading]) -> Option<usize> {
            self.log.push("pick".into());
            self.shown = headings.iter().map(|h| h.title.clone()).collect();
            if self.lock_during_dialog {
                self.epoch += 1;
            }
            self.answer
        }
        fn still_valid(&self, epoch: u64) -> bool {
            epoch == self.epoch
        }
        fn move_caret(&mut self, line: u32) {
            self.log.push(format!("move {}", line));
        }
    }

    #[test]
    fn choosing_a_heading_moves_the_caret_to_its_line() {
        let mut f = Fake::new("intro\n# One\ntext\nTwo\n---\n", Some(1));
        assert_eq!(go_to_heading(&mut f), HeadingOutcome::Moved { line: 3 });
        assert_eq!(f.shown, ["One", "Two"]);
        assert_eq!(f.log, ["pick", "move 3"]);
    }

    #[test]
    fn cancel_moves_nothing() {
        let mut f = Fake::new("# One", None);
        assert_eq!(go_to_heading(&mut f), HeadingOutcome::Cancelled);
        assert_eq!(f.log, ["pick"]);
    }

    /// The dialog opens even without headings (it says so itself).
    #[test]
    fn a_tab_without_headings_still_shows_the_dialog() {
        let mut f = Fake::new("plain text", None);
        assert_eq!(go_to_heading(&mut f), HeadingOutcome::Cancelled);
        assert!(f.shown.is_empty());
        assert_eq!(f.log, ["pick"]);
    }

    #[test]
    fn a_lock_during_the_dialog_drops_the_answer() {
        let mut f = Fake::new("# One", Some(0));
        f.lock_during_dialog = true;
        assert_eq!(go_to_heading(&mut f), HeadingOutcome::DroppedLocked);
        assert_eq!(f.log, ["pick"]);
    }

    /// Ctrl+Shift+O and Edit > Go to Heading run this flow; the dialog is
    /// counted as a modal and owned by the main window.
    #[test]
    fn shortcut_and_menu_run_the_flow_under_a_modal() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("ui").join("editor");
        let ops = crate::read_source(dir.join("edit_ops.rs"));
        let start = ops.find("pub(super) fn handle_shortcut").unwrap();
        let shortcut = &ops[start..start + ops[start..].find("\n    }\n").unwrap()];
        let shift_branch = &shortcut[shortcut.find("} else if ctrl && shift {").unwrap()..];
        let shift_branch = &shift_branch[..shift_branch.find("} else if !ctrl && !shift {").unwrap()];
        assert!(shift_branch.contains("0x4F => { self.on_goto_heading(); return true; }"), "{}", shift_branch);
        assert!(ops.contains("go_to_heading(&mut FormHeadingSteps { form: self })"));
        let pick = &ops[ops.find("fn pick(&mut self").unwrap()..];
        let pick = &pick[..pick.find("\n    }\n").unwrap()];
        assert!(pick.contains("let _modal = self.form.modal();") && pick.contains("Some(self.form.window.handle)"), "{}", pick);
        let events = crate::read_source(dir.join("events.rs"));
        assert!(events.contains("handle == f.menu_edit_goto_heading.handle {\n                            f.on_goto_heading();"));
    }

    #[test]
    fn an_index_outside_the_list_moves_nothing() {
        let mut f = Fake::new("# One", Some(5));
        assert_eq!(go_to_heading(&mut f), HeadingOutcome::BadIndex { index: 5, count: 1 });
        assert_eq!(f.log, ["pick"]);
    }
}
