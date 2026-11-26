// Tab > Compare with Last Save, behind a trait: read the active tab's text
// as of the last load or save and its current text, then show their line
// diff in a read-only dialog, or say why there is none. Nothing is written
// and nothing comes back from the dialog, so a lock while it is open has
// nothing to drop (the lock closes it like any dialog). `FormCompareSteps`
// (tab_ops.rs) implements the steps on the window; the tests below drive
// the sequence with a recording fake.

use crate::features::line_diff::{self, Comparison, DiffError};
use zeroize::{Zeroize, Zeroizing};

/// What the sequence needs from the window.
pub(super) trait CompareSteps {
    /// (saved, current) text of the active tab; `None` when the tab bar has
    /// no entry for the active index (a bug).
    fn texts(&self) -> Option<(Zeroizing<String>, Zeroizing<String>)>;
    /// A message of the main window (`inform`).
    fn inform(&mut self, message: &str);
    /// The read-only diff dialog.
    fn show_diff(&mut self, lines: &[String]);
}

/// How a run ended (logged by the caller).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CompareOutcome {
    Identical,
    TooLarge { saved_lines: usize, current_lines: usize },
    Shown { lines: usize },
    /// No saved text for the active tab (a bug): the user is told.
    NoActiveTab,
    /// The diff could not be computed (a bug): the user is told.
    Failed(DiffError),
}

fn t(key: &str) -> String {
    crate::features::i18n::get(key).to_string()
}

pub(super) fn compare_with_last_save<S: CompareSteps>(s: &mut S) -> CompareOutcome {
    let Some((saved, current)) = s.texts() else {
        s.inform(&t("compare.failed"));
        return CompareOutcome::NoActiveTab;
    };
    match line_diff::compare(&saved, &current) {
        Ok(Comparison::Identical) => {
            s.inform(&t("compare.identical"));
            CompareOutcome::Identical
        }
        Ok(Comparison::TooLarge { saved_lines, current_lines }) => {
            s.inform(&t("compare.too_large").replace("{}", &line_diff::MAX_LINES.to_string()));
            CompareOutcome::TooLarge { saved_lines, current_lines }
        }
        Ok(Comparison::Diff(mut lines)) => {
            s.show_diff(&lines);
            let shown = lines.len();
            // The diff holds note text: wiped, not just freed.
            lines.iter_mut().for_each(|l| l.zeroize());
            CompareOutcome::Shown { lines: shown }
        }
        Err(e) => {
            s.inform(&t("compare.failed"));
            CompareOutcome::Failed(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Records the steps in order.
    struct Fake {
        texts: Option<(String, String)>,
        shown: Vec<String>,
        log: Vec<String>,
    }

    impl Fake {
        fn new(saved: &str, current: &str) -> Self {
            Fake { texts: Some((saved.into(), current.into())), shown: Vec::new(), log: Vec::new() }
        }
    }

    impl CompareSteps for Fake {
        fn texts(&self) -> Option<(Zeroizing<String>, Zeroizing<String>)> {
            self.texts.clone().map(|(a, b)| (Zeroizing::new(a), Zeroizing::new(b)))
        }
        fn inform(&mut self, message: &str) {
            self.log.push(format!("inform {}", message));
        }
        fn show_diff(&mut self, lines: &[String]) {
            self.log.push("dialog".into());
            self.shown = lines.to_vec();
        }
    }

    #[test]
    fn a_change_opens_the_dialog_with_the_diff() {
        let mut f = Fake::new("a\nb\n", "a\r\nc");
        assert_eq!(compare_with_last_save(&mut f), CompareOutcome::Shown { lines: 4 });
        assert_eq!(f.log, ["dialog"]);
        assert_eq!(f.shown, ["@@ -1,2 +1,2 @@", "  a", "- b", "+ c"]);
    }

    #[test]
    fn no_change_says_so_without_the_dialog() {
        let _g = crate::features::i18n::test_support::lang_guard();
        let mut f = Fake::new("a\nb", "a\r\nb");
        assert_eq!(compare_with_last_save(&mut f), CompareOutcome::Identical);
        assert_eq!(f.log, [format!("inform {}", t("compare.identical"))]);
    }

    #[test]
    fn a_tab_over_the_limit_is_not_diffed() {
        let _g = crate::features::i18n::test_support::lang_guard();
        let big = "x\n".repeat(line_diff::MAX_LINES + 1);
        let mut f = Fake::new("short", &big);
        assert_eq!(
            compare_with_last_save(&mut f),
            CompareOutcome::TooLarge { saved_lines: 1, current_lines: line_diff::MAX_LINES + 1 }
        );
        let msg = t("compare.too_large").replace("{}", "20000");
        assert!(msg.contains("20000"), "{msg}");
        assert_eq!(f.log, [format!("inform {}", msg)]);
    }

    /// A tab never saved (new, or imported from a backup) has an empty
    /// saved text: every line shows as added.
    #[test]
    fn a_never_saved_tab_shows_every_line_added() {
        let mut f = Fake::new("", "one\ntwo");
        assert_eq!(compare_with_last_save(&mut f), CompareOutcome::Shown { lines: 3 });
        assert_eq!(f.shown, ["@@ -0,0 +1,2 @@", "+ one", "+ two"]);
    }

    #[test]
    fn a_missing_tab_is_reported_not_ignored() {
        let _g = crate::features::i18n::test_support::lang_guard();
        let mut f = Fake::new("", "");
        f.texts = None;
        assert_eq!(compare_with_last_save(&mut f), CompareOutcome::NoActiveTab);
        assert_eq!(f.log, [format!("inform {}", t("compare.failed"))]);
    }

    /// Tab > Compare with Last Save runs this flow; the dialog is counted as
    /// a modal, owned by the main window, and gets no shortcut. The texts
    /// are copied before the dialog: no tab bar borrow is held across it.
    #[test]
    fn menu_runs_the_flow_under_a_modal() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("ui").join("editor");
        let ops = crate::read_source(dir.join("tab_ops.rs"));
        assert!(ops.contains("compare_with_last_save(&mut FormCompareSteps { form: self })"));
        let show = &ops[ops.find("fn show_diff(&mut self").unwrap()..];
        let show = &show[..show.find("\n    }\n").unwrap()];
        assert!(show.contains("let _modal = self.form.modal();") && show.contains("Some(self.form.window.handle)"), "{}", show);
        assert!(!show.contains("tab_bar"), "{}", show);
        let inform = &ops[ops.find("fn inform(&mut self").unwrap()..];
        let inform = &inform[..inform.find("\n    }\n").unwrap()];
        assert!(inform.contains("self.form.inform("), "{}", inform);
        // The menu runs through the command table (palette_ops.rs).
        let table = crate::read_source(dir.join("palette_ops.rs"));
        assert!(table.contains("CompareWithLastSave => &self.menu_tab_compare,"));
        assert!(table.contains("CompareWithLastSave => self.on_compare_with_last_save(),"));
        let shortcuts = crate::read_source(dir.join("edit_ops.rs"));
        assert!(!shortcuts.contains("on_compare_with_last_save"));
    }
}
