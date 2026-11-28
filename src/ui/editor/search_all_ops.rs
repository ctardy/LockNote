// Search in all tabs (Edit > Search All Tabs..., Ctrl+Shift+F).
// Part of the EditorForm implementation. The search is `ui::search_all`,
// the dialog `ui::dialogs::search_all`.
//
// Editor-window rules: nothing while the input is blocked; the tabs, vault
// names and prompts are copied (tab texts in `Zeroizing`) and every borrow
// dropped before the dialog opens; its result is applied only if
// `still_valid(epoch)` and the input is not blocked. The sequence is behind
// `SearchAllSteps`, driven by a recording fake in the tests below.
//
// Logs: `search_all: open tab|vault|prompt` and counts, never the query, a
// tab text or a name.

#[allow(unused_imports)]
use super::*;

use crate::ui::search_all::{self as sa, Hit, Sources};

/// What the sequence needs from the window.
pub(super) trait SearchAllSteps {
    fn input_blocked(&self) -> bool;
    fn epoch(&self) -> u64;
    fn still_valid(&self, epoch: u64) -> bool;
    /// The copy the dialog searches (no borrow kept); `None` when it could
    /// not be made (the window told the user).
    fn collect_sources(&self) -> Option<Sources>;
    /// Show the dialog; the result the user activated, if any.
    fn show_dialog(&self, sources: Sources) -> Option<Hit>;
    /// Show tab `tab` and select `start..end` (RichEdit positions).
    fn go_to_tab(&self, tab: usize, start: usize, end: usize);
    /// Open the vault panel on the secret, or with its name searched.
    fn open_vault(&self, index: usize, name: &str, category: &str);
    /// Open the prompts panel on the prompt, or with its name searched.
    fn open_prompt(&self, index: usize, name: &str, category: &str);
}

/// The whole sequence: open the dialog on a copy, apply its result.
pub(super) fn run<S: SearchAllSteps>(s: &S) {
    if s.input_blocked() {
        return;
    }
    let epoch = s.epoch();
    let Some(sources) = s.collect_sources() else { return };
    let Some(hit) = s.show_dialog(sources) else { return };
    // A lock (curtain) while the dialog was open: the answer is dropped.
    if !s.still_valid(epoch) || s.input_blocked() {
        eprintln!("INFO search_all: result dropped, the note locked meanwhile");
        return;
    }
    match hit {
        Hit::Tab { tab, start, end, .. } => {
            eprintln!("INFO search_all: open tab");
            s.go_to_tab(tab, start, end);
        }
        Hit::Vault { index, name, category } => {
            eprintln!("INFO search_all: open vault");
            s.open_vault(index, &name, &category);
        }
        Hit::Prompt { index, name, category } => {
            eprintln!("INFO search_all: open prompt");
            s.open_prompt(index, &name, &category);
        }
    }
}

/// `SearchAllSteps` on the real window.
pub(super) struct FormSearchAllSteps<'a> {
    pub(super) form: &'a EditorForm,
}

impl SearchAllSteps for FormSearchAllSteps<'_> {
    fn input_blocked(&self) -> bool {
        self.form.input_blocked()
    }
    fn epoch(&self) -> u64 {
        self.form.lock_epoch()
    }
    fn still_valid(&self, epoch: u64) -> bool {
        self.form.still_valid(epoch)
    }
    fn collect_sources(&self) -> Option<Sources> {
        match self.form.search_all_sources() {
            Ok(sources) => Some(sources),
            Err(e) => {
                self.form.tell_unreadable("search_all", &e);
                None
            }
        }
    }
    fn show_dialog(&self, sources: Sources) -> Option<Hit> {
        let selection = match self.form.current_selection_text() {
            Ok(selection) => selection,
            Err(e) => {
                self.form.tell_unreadable("search_all", &e);
                return None;
            }
        };
        let initial = sa::initial_query(&selection);
        crate::ui::dialogs::search_all::SearchAllDialog::show(sources, initial, Some(self.form.window.handle))
    }
    fn go_to_tab(&self, tab: usize, start: usize, end: usize) {
        self.form.search_all_go_to_tab(tab, start, end);
    }
    fn open_vault(&self, index: usize, name: &str, category: &str) {
        self.form.search_all_open_vault(index, name, category);
    }
    fn open_prompt(&self, index: usize, name: &str, category: &str) {
        self.form.search_all_open_prompt(index, name, category);
    }
}

impl EditorForm {
    /// Edit > Search All Tabs... / Ctrl+Shift+F.
    pub(super) fn on_search_all_tabs(&self) {
        run(&FormSearchAllSteps { form: self });
    }

    /// Copy what the dialog searches: every tab (the active one from the
    /// editor), the vault names and categories (no value), the prompts.
    fn search_all_sources(&self) -> Result<Sources, EditorTextError> {
        let active = self.tab_bar.borrow().active_index();
        let all = self.tab_bar.borrow().all_tabs();
        let mut tabs = Vec::with_capacity(all.len());
        for (i, mut tab) in all.into_iter().enumerate() {
            // Moved, not copied: the tab's own buffer is wiped with the copy.
            let stored = zeroize::Zeroizing::new(std::mem::take(&mut tab.content));
            // Searched and shown only: the display text of the editor (a
            // match's positions are the RichEdit's, one unit per unit).
            let text = if i == active { zeroize::Zeroizing::new(self.display_content()?) } else { stored };
            tabs.push(sa::TabText { name: std::mem::take(&mut tab.name), text });
        }
        // SecretEntry clones are wiped when dropped, at the end of this map.
        let vault = self
            .vault_panel
            .get_secrets()
            .iter()
            .map(|e| sa::VaultName { name: e.name.clone(), category: e.category.clone() })
            .collect();
        let prompts = self.prompts_panel.get_prompts();
        eprintln!("INFO search_all: dialog tabs={} vault={} prompts={}", tabs.len(), self.vault_panel.count(), prompts.len());
        Ok(Sources { tabs, vault, prompts })
    }

    /// Show tab `tab` (as the tab list does) and select the match.
    fn search_all_go_to_tab(&self, tab: usize, start: usize, end: usize) {
        let (count, active) = {
            let tb = self.tab_bar.borrow();
            (tb.count(), tb.active_index())
        };
        if tab >= count {
            eprintln!("ERROR search_all: tab {} not found count={}", tab, count);
            return;
        }
        let (Ok(start), Ok(end)) = (u32::try_from(start), u32::try_from(end)) else {
            eprintln!("ERROR search_all: position out of range");
            return;
        };
        if tab != active {
            // As the tab list switches: a text that cannot be stored stops
            // here, nothing changed (and the undo history is not carried
            // from one tab to the other).
            if let Err(e) = super::tab_ops::switch_editor_tab(&self.editor, &self.tab_bar, |tb| tb.set_active(tab)) {
                return self.tell_unreadable("search_all_go_to_tab", &e);
            }
            *self.detected_encoding.borrow_mut() = "UTF-8".to_string();
            self.update_title();
            self.update_status_bar();
            self.paint_tab_bar();
            self.layout_controls();
        }
        self.editor.set_selection(start..end);
        self.editor.set_focus();
        self.send_editor_message(0x00B7, 0, 0); // EM_SCROLLCARET
    }

    /// Open the vault panel on the secret found. When the entry at that
    /// index is no longer it, its name is searched instead.
    fn search_all_open_vault(&self, index: usize, name: &str, category: &str) {
        if !self.vault_panel.is_visible() {
            self.on_toggle_vault();
        }
        let current = self.vault_panel.get_secret(index);
        if sa::same_entry(current.as_ref().map(|e| (e.name.as_str(), e.category.as_str())), name, category) {
            self.show_vault_secret(index);
        } else {
            eprintln!("INFO search_all: vault entry moved, its name is searched");
            self.suppress_tree_events.set(true);
            self.vault_search.set_text(name);
            self.suppress_tree_events.set(false);
            self.refresh_vault_tree(None);
        }
        self.vault_tree.set_focus();
    }

    /// Open the prompts panel on the prompt found; the search is kept unless
    /// it hides it (as `show_vault_secret` does). When the entry at that
    /// index is no longer it, its name is searched instead.
    fn search_all_open_prompt(&self, index: usize, name: &str, category: &str) {
        if !self.prompts_panel.is_visible() {
            self.on_toggle_prompts();
        }
        let current = self.prompts_panel.get(index);
        if sa::same_entry(current.as_ref().map(|p| (p.name.as_str(), p.category.as_str())), name, category) {
            if let Some(entry) = current.as_ref() {
                // The rebuild below meets the same failures and shows them.
                match crate::ui::field_text::read_display(self.prompts_search.handle) {
                    Ok(query) => match sa::prompt_search_hides(entry, &query, &crate::ui::prompts_panel::contains_user_locale) {
                        Ok(true) => self.prompts_search.set_text(""),
                        Ok(false) => {}
                        Err(e) => eprintln!("WARN search_all: prompts search check failed err={:?}", e.0),
                    },
                    Err(e) => eprintln!("WARN search_all: prompts search field unreadable err={:?}", e),
                }
            }
            self.refresh_prompts_list(Some(index));
        } else {
            eprintln!("INFO search_all: prompt moved, its name is searched");
            self.prompts_search.set_text(name);
            self.refresh_prompts_list(None);
        }
        self.prompts_list.set_focus();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    /// Records the steps; `lock_in_dialog` simulates a lock (curtain) while
    /// the dialog is open, `answer` is what the dialog returns.
    struct Fake {
        log: RefCell<Vec<String>>,
        blocked: Cell<bool>,
        epoch: Cell<u64>,
        lock_in_dialog: bool,
        answer: Option<Hit>,
        /// The editor text cannot be read: no sources (the window told the user).
        unreadable: bool,
    }

    impl Fake {
        fn new(answer: Option<Hit>) -> Self {
            Fake {
                log: RefCell::new(Vec::new()),
                blocked: Cell::new(false),
                epoch: Cell::new(7),
                lock_in_dialog: false,
                answer,
                unreadable: false,
            }
        }
        fn log(&self) -> Vec<String> {
            self.log.borrow().clone()
        }
        fn push(&self, s: impl Into<String>) {
            self.log.borrow_mut().push(s.into());
        }
    }

    impl SearchAllSteps for Fake {
        fn input_blocked(&self) -> bool {
            self.blocked.get()
        }
        fn epoch(&self) -> u64 {
            self.epoch.get()
        }
        fn still_valid(&self, epoch: u64) -> bool {
            epoch == self.epoch.get()
        }
        fn collect_sources(&self) -> Option<Sources> {
            self.push("collect");
            if self.unreadable {
                return None;
            }
            Some(Sources {
                tabs: vec![sa::TabText { name: "A".into(), text: zeroize::Zeroizing::new("x".into()) }],
                vault: Vec::new(),
                prompts: Vec::new(),
            })
        }
        fn show_dialog(&self, sources: Sources) -> Option<Hit> {
            self.push(format!("dialog tabs={}", sources.tabs.len()));
            if self.lock_in_dialog {
                // The curtain: epoch moved, input blocked.
                self.epoch.set(self.epoch.get() + 1);
                self.blocked.set(true);
            }
            self.answer.clone()
        }
        fn go_to_tab(&self, tab: usize, start: usize, end: usize) {
            self.push(format!("tab {} {}..{}", tab, start, end));
        }
        fn open_vault(&self, index: usize, name: &str, category: &str) {
            self.push(format!("vault {} {} {}", index, category, name));
        }
        fn open_prompt(&self, index: usize, name: &str, category: &str) {
            self.push(format!("prompt {} {} {}", index, category, name));
        }
    }

    fn tab_hit() -> Hit {
        Hit::Tab { tab: 1, name: "B".into(), line: 3, excerpt: "e".into(), start: 10, end: 13 }
    }

    #[test]
    fn a_tab_result_shows_the_tab_and_selects_the_match() {
        let f = Fake::new(Some(tab_hit()));
        run(&f);
        assert_eq!(f.log(), ["collect", "dialog tabs=1", "tab 1 10..13"]);
    }

    #[test]
    fn a_vault_result_opens_the_vault_on_it() {
        let f = Fake::new(Some(Hit::Vault { index: 2, category: "Cloud".into(), name: "AWS".into() }));
        run(&f);
        assert_eq!(f.log(), ["collect", "dialog tabs=1", "vault 2 Cloud AWS"]);
    }

    #[test]
    fn a_prompt_result_opens_the_prompts_on_it() {
        let f = Fake::new(Some(Hit::Prompt { index: 0, category: "Code".into(), name: "Review".into() }));
        run(&f);
        assert_eq!(f.log(), ["collect", "dialog tabs=1", "prompt 0 Code Review"]);
    }

    #[test]
    fn without_sources_no_dialog_opens() {
        let mut f = Fake::new(Some(tab_hit()));
        f.unreadable = true;
        run(&f);
        assert_eq!(f.log(), ["collect"]);
    }

    #[test]
    fn closing_the_dialog_changes_nothing() {
        let f = Fake::new(None);
        run(&f);
        assert_eq!(f.log(), ["collect", "dialog tabs=1"]);
    }

    #[test]
    fn nothing_opens_while_the_input_is_blocked() {
        let f = Fake::new(Some(tab_hit()));
        f.blocked.set(true);
        run(&f);
        assert!(f.log().is_empty(), "{:?}", f.log());
    }

    #[test]
    fn a_lock_while_the_dialog_is_open_drops_the_result() {
        let mut f = Fake::new(Some(tab_hit()));
        f.lock_in_dialog = true;
        run(&f);
        assert_eq!(f.log(), ["collect", "dialog tabs=1"]);
    }

    /// An epoch moved without the input blocked (locked, then unlocked
    /// while the dialog was open): still dropped.
    #[test]
    fn a_lock_and_unlock_while_the_dialog_is_open_drops_the_result() {
        struct Relocked(Fake);
        impl SearchAllSteps for Relocked {
            fn input_blocked(&self) -> bool { self.0.input_blocked() }
            fn epoch(&self) -> u64 { self.0.epoch() }
            fn still_valid(&self, epoch: u64) -> bool { self.0.still_valid(epoch) }
            fn collect_sources(&self) -> Option<Sources> { self.0.collect_sources() }
            fn show_dialog(&self, sources: Sources) -> Option<Hit> {
                let hit = self.0.show_dialog(sources);
                self.0.epoch.set(self.0.epoch.get() + 1);
                hit
            }
            fn go_to_tab(&self, tab: usize, start: usize, end: usize) { self.0.go_to_tab(tab, start, end) }
            fn open_vault(&self, index: usize, name: &str, category: &str) { self.0.open_vault(index, name, category) }
            fn open_prompt(&self, index: usize, name: &str, category: &str) { self.0.open_prompt(index, name, category) }
        }
        let f = Relocked(Fake::new(Some(tab_hit())));
        run(&f);
        assert_eq!(f.0.log(), ["collect", "dialog tabs=1"]);
    }

    /// The real steps use the window's own checks and the shortcut and the
    /// menu reach the flow.
    #[test]
    fn the_flow_is_wired_to_the_window() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("ui").join("editor");
        let ops = crate::read_source(dir.join("search_all_ops.rs"));
        let code = &ops[..ops.find("#[cfg(test)]").expect("test module")];
        assert!(code.contains("self.form.still_valid(epoch)"));
        assert!(code.contains("self.form.input_blocked()"));
        assert!(code.contains("run(&FormSearchAllSteps { form: self })"));
        let edit = crate::read_source(dir.join("edit_ops.rs"));
        let shortcut = &edit[edit.find("pub(super) fn handle_shortcut").expect("handle_shortcut")..];
        let header = "} else if ctrl && shift {";
        let ctrl_shift = &shortcut[shortcut.find(header).expect("ctrl+shift branch") + header.len()..];
        let ctrl_shift = &ctrl_shift[..ctrl_shift.find("} else if").expect("end of branch")];
        assert!(ctrl_shift.contains("0x46 => { self.on_search_all_tabs(); return true; }"), "Ctrl+Shift+F");
        // The menu runs through the command table (palette_ops.rs).
        let table = crate::read_source(dir.join("palette_ops.rs"));
        assert!(table.contains("SearchAll => &self.menu_edit_search_all,"));
        assert!(table.contains("SearchAll => self.on_search_all_tabs(),"));
    }
}
