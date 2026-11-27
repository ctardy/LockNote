// The undo history after a presentation change.
//
// The font (zoom, preferences), the theme colours and the occurrence
// highlights suspend RichEdit's undo history (TOM `tomSuspend`) so that they
// never become undo steps, then resume it (`tom::resume_undo`). When the
// resume fails, even after its retry, the history may stay suspended: the
// next edits may no longer be undoable. That failure reaches the window as
// `UndoError::NotResumed`, is noted here, and the user is told once, with
// `inform()`, from one place: the highlight timer tick
// (`tell_if_pending`).
//
//   * No dialog storm. Ctrl+wheel applies several fonts in a row and the
//     highlights run at every tick: the state remembers that the user was
//     told, and only a resume that works again (the history is back) arms a
//     new alert.
//   * No dialog in the middle of a flow. A message box runs its own loop,
//     where a lock can run; a zoom step or a theme switch must not go on
//     after it. They only note the result; the tick tells, holding the
//     alert while the note is locked or under the curtain, or while another
//     modal is open. The state is a `Cell` (no borrow) and is set to "told"
//     before the box opens, so the ticks that run inside the box's loop do
//     not open a second one.

#[allow(unused_imports)]
use super::*;

/// The undo history stayed suspended after a presentation change: later
/// edits may not be undoable. The text says which call failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct UndoStuck(pub(super) String);

impl std::fmt::Display for UndoStuck {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// How a presentation change left the undo history, when not as it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum UndoError {
    /// The history could not be suspended (or the editor reached): it was
    /// not touched, its state is the one it had before.
    NotSuspended(String),
    /// Suspended, then not resumed: it may stay suspended.
    NotResumed(UndoStuck),
}

impl std::fmt::Display for UndoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UndoError::NotSuspended(e) => write!(f, "the undo history could not be suspended: {e}"),
            UndoError::NotResumed(e) => write!(f, "the undo history could not be resumed: {e}"),
        }
    }
}

/// What the window knows of the undo history.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum UndoHistory {
    /// Resumed by the last presentation change (or never suspended).
    #[default]
    Live,
    /// Left suspended; the user was not told yet.
    Stuck,
    /// Left suspended; the user was told, no new alert until it comes back.
    StuckTold,
}

/// The state after a presentation change (`what`) reported `resumed`,
/// logged. A history that could not even be suspended keeps its state.
pub(super) fn after(state: UndoHistory, resumed: &Result<(), UndoError>, what: &str) -> UndoHistory {
    match (resumed, state) {
        (Ok(()), UndoHistory::Live) => UndoHistory::Live,
        (Ok(()), _) => {
            eprintln!("INFO undo: the undo history was resumed after {what}");
            UndoHistory::Live
        }
        (Err(e @ UndoError::NotSuspended(_)), state) => {
            eprintln!("ERROR undo: {e} for {what}; history state unchanged ({state:?})");
            state
        }
        (Err(e @ UndoError::NotResumed(_)), state) => {
            eprintln!("ERROR undo: {e} after {what}; edits may not be undoable (was {state:?})");
            match state {
                UndoHistory::StuckTold => UndoHistory::StuckTold,
                UndoHistory::Live | UndoHistory::Stuck => UndoHistory::Stuck,
            }
        }
    }
}

/// The window side of the alert (sequence tests use a fake).
pub(super) trait UndoAlertSteps {
    fn history(&self) -> UndoHistory;
    fn set_history(&self, history: UndoHistory);
    /// Not locked, no curtain, no other modal open.
    fn can_tell(&self) -> bool;
    /// Show the alert (a message box: its loop runs the timer ticks).
    fn tell(&self);
}

/// Record what a presentation change (`what`) did to the history. Never
/// shows anything: the flow that called it goes on undisturbed.
pub(super) fn note(steps: &impl UndoAlertSteps, resumed: Result<(), UndoError>, what: &str) {
    steps.set_history(after(steps.history(), &resumed, what));
}

/// Tell the user about a history left suspended, once, when nothing else
/// is in the way. Returns whether the alert was shown.
pub(super) fn tell_if_pending(steps: &impl UndoAlertSteps) -> bool {
    if steps.history() != UndoHistory::Stuck || !steps.can_tell() {
        return false;
    }
    // Told before the box opens: the ticks of its loop see it.
    steps.set_history(UndoHistory::StuckTold);
    eprintln!("WARN undo: telling the user the undo history stays suspended");
    steps.tell();
    true
}

impl EditorForm {
    /// Record what a presentation change (`what`) did to the undo history
    /// (`undo_watch`); shows nothing.
    pub(super) fn note_undo(&self, resumed: Result<(), UndoError>, what: &str) {
        note(&FormUndoAlert { form: self }, resumed, what);
    }

    /// The highlight timer tick: tell the user once about a history left
    /// suspended (held while locked or behind another modal).
    pub(super) fn tell_undo_stuck_if_pending(&self) {
        tell_if_pending(&FormUndoAlert { form: self });
    }
}

/// The steps of the alert on the window. The state is a `Cell`: no borrow
/// is held while the box is open.
struct FormUndoAlert<'a> {
    form: &'a EditorForm,
}

impl UndoAlertSteps for FormUndoAlert<'_> {
    fn history(&self) -> UndoHistory {
        self.form.undo_history.get()
    }
    fn set_history(&self, history: UndoHistory) {
        self.form.undo_history.set(history);
    }
    fn can_tell(&self) -> bool {
        !self.form.input_blocked() && !self.form.modal_open()
    }
    fn tell(&self) {
        // The box only informs: nothing is applied after it, so no epoch
        // check is needed.
        self.form.inform("LockNote", &t("editor.undo_stuck"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    fn stuck() -> Result<(), UndoError> {
        Err(UndoError::NotResumed(UndoStuck("Undo(tomResume) failed twice".into())))
    }

    fn not_suspended() -> Result<(), UndoError> {
        Err(UndoError::NotSuspended("EM_GETOLEINTERFACE returned no interface".into()))
    }

    #[test]
    fn a_stuck_history_is_told_once_until_it_comes_back() {
        use UndoHistory::*;
        assert_eq!(after(Live, &Ok(()), "x"), Live);
        assert_eq!(after(Live, &stuck(), "x"), Stuck);
        assert_eq!(after(Stuck, &stuck(), "x"), Stuck);
        assert_eq!(after(StuckTold, &stuck(), "x"), StuckTold, "told: no new alert");
        assert_eq!(after(Stuck, &Ok(()), "x"), Live, "back before the alert: nothing to tell");
        assert_eq!(after(StuckTold, &Ok(()), "x"), Live, "back: the next failure is told again");
    }

    #[test]
    fn a_history_that_could_not_be_suspended_keeps_its_state() {
        use UndoHistory::*;
        for state in [Live, Stuck, StuckTold] {
            assert_eq!(after(state, &not_suspended(), "x"), state);
        }
    }

    /// The window: its state, whether a box may open, and the boxes shown.
    /// `inside_box` runs while a box is open, as the timer ticks do in its
    /// message loop.
    #[derive(Default)]
    struct Fake {
        history: Cell<UndoHistory>,
        blocked: Cell<bool>,
        told: Cell<u32>,
        open: Cell<bool>,
        log: RefCell<Vec<String>>,
        inside_box: RefCell<Option<Box<dyn Fn(&Fake)>>>,
    }

    impl UndoAlertSteps for Fake {
        fn history(&self) -> UndoHistory {
            self.history.get()
        }
        fn set_history(&self, history: UndoHistory) {
            self.history.set(history);
        }
        fn can_tell(&self) -> bool {
            !self.blocked.get() && !self.open.get()
        }
        fn tell(&self) {
            self.told.set(self.told.get() + 1);
            self.log.borrow_mut().push("tell".into());
            self.open.set(true);
            let inside = self.inside_box.borrow_mut().take();
            if let Some(run) = inside {
                run(self);
            }
            self.open.set(false);
        }
    }

    /// One highlight timer tick: the highlights note their result, then the
    /// tick tells if needed (the order of events.rs).
    fn tick(fake: &Fake, highlights: Result<(), UndoError>) {
        note(fake, highlights, "the occurrence highlights");
        tell_if_pending(fake);
    }

    #[test]
    fn ctrl_wheel_steps_that_all_fail_give_one_alert_after_the_zoom() {
        let fake = Fake::default();
        // Five notches in one wheel message: five fonts, five failures.
        for _ in 0..5 {
            note(&fake, stuck(), "a font change");
            fake.log.borrow_mut().push("zoom step".into());
        }
        assert_eq!(fake.told.get(), 0, "no box in the middle of the zoom");
        tick(&fake, Ok(()));
        // The highlights resumed fine on that tick: the history is back.
        assert_eq!(fake.told.get(), 0, "a working resume before the alert: nothing to tell");

        for _ in 0..5 {
            note(&fake, stuck(), "a font change");
        }
        tell_if_pending(&fake);
        tell_if_pending(&fake);
        assert_eq!(fake.told.get(), 1);
        assert_eq!(fake.log.borrow().last().map(String::as_str), Some("tell"), "after every zoom step");
        assert_eq!(fake.history.get(), UndoHistory::StuckTold);
    }

    #[test]
    fn highlights_failing_at_every_tick_give_one_alert() {
        let fake = Fake::default();
        for _ in 0..50 {
            tick(&fake, stuck());
        }
        assert_eq!(fake.told.get(), 1);
    }

    #[test]
    fn ticks_inside_the_box_open_no_second_box() {
        let fake = Fake::default();
        *fake.inside_box.borrow_mut() = Some(Box::new(|f: &Fake| {
            for _ in 0..10 {
                tick(f, stuck());
            }
        }));
        tick(&fake, stuck());
        assert_eq!(fake.told.get(), 1);
        assert_eq!(fake.history.get(), UndoHistory::StuckTold);
    }

    #[test]
    fn the_history_coming_back_arms_a_new_alert() {
        let fake = Fake::default();
        tick(&fake, stuck());
        tick(&fake, Ok(()));
        assert_eq!(fake.history.get(), UndoHistory::Live);
        tick(&fake, stuck());
        assert_eq!(fake.told.get(), 2);
    }

    #[test]
    fn the_alert_waits_while_locked_or_behind_another_modal() {
        let fake = Fake::default();
        fake.blocked.set(true);
        tick(&fake, stuck());
        tick(&fake, stuck());
        assert_eq!(fake.told.get(), 0, "nothing over the lock screen or another box");
        assert_eq!(fake.history.get(), UndoHistory::Stuck, "still pending");
        fake.blocked.set(false);
        tick(&fake, stuck());
        assert_eq!(fake.told.get(), 1, "told once unlocked");
    }

    #[test]
    fn a_failed_suspension_neither_tells_nor_clears_the_alert() {
        let fake = Fake::default();
        tick(&fake, not_suspended());
        assert_eq!(fake.told.get(), 0);
        tick(&fake, stuck());
        tick(&fake, not_suspended());
        assert_eq!(fake.history.get(), UndoHistory::StuckTold, "not a sign the history is back");
        tick(&fake, stuck());
        assert_eq!(fake.told.get(), 1);
    }

    #[test]
    fn the_alert_exists_in_every_language() {
        use crate::features::i18n::{get, set_language, test_support::lang_guard, Language};
        let _g = lang_guard();
        set_language(Language::French);
        assert_eq!(
            get("editor.undo_stuck"),
            "L'historique d'annulation n'a pas pu \u{00ea}tre r\u{00e9}tabli : les prochaines modifications \
             pourraient ne pas pouvoir \u{00ea}tre annul\u{00e9}es. Enregistrez votre note."
        );
        let mut seen = Vec::new();
        for lang in [Language::English, Language::French, Language::Spanish, Language::German] {
            set_language(lang);
            let text = get("editor.undo_stuck").to_string();
            assert!(!text.starts_with('['), "{lang:?} has no text");
            assert!(!seen.contains(&text), "{lang:?} is not translated");
            seen.push(text);
        }
        set_language(Language::English);
    }

    // ── wiring, read from the code (crate::read_source) ──

    fn source(file: &str) -> String {
        crate::read_source(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ui/editor").join(file))
    }

    fn body_of(file: &str, fun: &str) -> String {
        let src = source(file);
        let start = src.find(fun).unwrap_or_else(|| panic!("{fun} in {file}"));
        let end = src[start..].find("\n    }\n").map_or(src.len(), |e| start + e);
        src[start..end].to_string()
    }

    #[test]
    fn every_suspension_reports_to_the_window() {
        let font = body_of("edit_ops.rs", "fn apply_font(");
        let set = font.find("surface::set_font(").expect("set_font");
        let noted = font.find("self.0.note_undo(applied.undo").expect("the font's undo result is noted");
        assert!(set < noted);

        let hl = body_of("highlight.rs", "pub(super) fn update_occurrence_highlights");
        assert!(hl.contains("self.note_undo(painted,"), "the highlights' undo result is noted");

        let theme = body_of("theme_switch.rs", "pub(super) fn apply_theme_live");
        assert!(theme.contains("let colours = surface::retheme(&self.editor);"));
        assert!(theme.contains("self.note_undo(colours,"), "the colours' undo result is noted");

        let build = source("build.rs");
        assert!(build.contains("let install_undo = surface::install(&editor);"));
        assert!(build.contains("undo_watch::after(undo_watch::UndoHistory::Live, &install_undo,"));
    }

    #[test]
    fn the_resume_failure_is_no_longer_only_logged() {
        let surface = source("surface.rs");
        assert!(!surface.contains("fn resume_undo("), "one resume, in tom.rs");
        assert!(surface.contains("pub(super) fn set_font(hwnd: HWND, hfont: isize) -> Result<FontApplied, String>"));
        assert!(surface.contains("fn apply_palette(hwnd: HWND, pal: &ThemePalette) -> Result<(), UndoError>"));
        let tom = source("tom.rs");
        assert!(tom.contains("pub(super) fn without_undo<R>(hwnd: HWND, f: impl FnOnce() -> R) -> Result<R, UndoError>"));
    }

    #[test]
    fn the_alert_is_shown_from_the_highlight_tick_only() {
        let ev = source("events.rs");
        let tick = ev.find("f.run_pending_requests();").expect("tick");
        let tell = ev.find("f.tell_undo_stuck_if_pending();").expect("the tick tells");
        assert!(tick < tell && tell - tick < 300, "in the highlight timer block, after the requests");
        // Nothing else shows it: the zoom loop and the theme switch only note.
        for file in ["edit_ops.rs", "highlight.rs", "theme_switch.rs", "zoom.rs", "surface.rs", "tom.rs", "prefs_ops.rs"] {
            assert!(!source(file).contains("editor.undo_stuck"), "{file} shows the alert itself");
            assert!(!source(file).contains("tell_undo_stuck_if_pending"), "{file} shows the alert itself");
        }
        let glue = source("undo_watch.rs");
        let tell_body = &glue[glue.find("fn tell(&self) {\n        // The box").expect("form tell")..];
        assert!(tell_body.contains("self.form.inform(\"LockNote\", &t(\"editor.undo_stuck\"))"));
        let can = &glue[glue.find("fn can_tell(&self) -> bool {\n        !self.form").expect("form gate")..];
        assert!(can.contains("!self.form.input_blocked() && !self.form.modal_open()"));
    }
}
