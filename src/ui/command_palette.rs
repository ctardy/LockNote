// Command palette (Ctrl+Shift+P): one filter field over every menu command,
// every tab ("Tab: <name>") and every vault secret ("Vault: <name>").
//
// This module holds the pure parts: the command table shared by the menus
// and the palette, the menu-label parsing, the fuzzy scorer and the flow of
// one palette run (a trait, so the sequence is tested with a fake). The
// dialog lives in `dialogs::command_palette`, the window side in
// `editor::palette_ops`.
//
// Nothing here is stored: the entries are rebuilt from the menus, the tabs
// and the vault each time the palette opens. A secret's value never enters
// the palette, only its name and index; the copy goes through the vault
// panel's own path.

/// A command of the main menu. The menus and the palette dispatch through
/// `EditorForm::run_command`, so a command does the same from both.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandId {
    Save,
    Print,
    Export,
    BackupExport,
    BackupImport,
    ImportExe,
    Backups,
    Preferences,
    Security,
    TempFiles,
    Quit,
    Cut,
    Copy,
    Paste,
    PastePlain,
    Find,
    Replace,
    GotoLine,
    DuplicateLine,
    DeleteLine,
    SelectAll,
    InsertTimestamp,
    AlwaysOnTop,
    WordWrap,
    LineNumbers,
    NewTab,
    CloseTab,
    RenameTab,
    NextTab,
    PrevTab,
    Vault,
    Prompts,
    Totp,
    Attachments,
    PasswordGenerator,
    StegoHide,
    StegoExtract,
    SshGenerate,
    SshAgent,
    CheckUpdates,
    About,
}

/// One row of the command table: the command and the i18n key of its menu
/// item (label, access key and accelerator text).
#[derive(Clone, Copy, Debug)]
pub struct Command {
    pub id: CommandId,
    pub label_key: &'static str,
}

/// Every menu command, in menu order (File, Edit, View, Tab, Tools, Help):
/// the order of the palette when the filter is empty.
pub const COMMANDS: &[Command] = &[
    Command { id: CommandId::Save, label_key: "menu.file.save" },
    Command { id: CommandId::Print, label_key: "menu.file.print" },
    Command { id: CommandId::Export, label_key: "menu.file.export" },
    Command { id: CommandId::BackupExport, label_key: "menu.file.backup_export" },
    Command { id: CommandId::BackupImport, label_key: "menu.file.backup_import" },
    Command { id: CommandId::ImportExe, label_key: "menu.file.import_exe" },
    Command { id: CommandId::Backups, label_key: "menu.file.backups" },
    Command { id: CommandId::Preferences, label_key: "menu.settings.preferences" },
    Command { id: CommandId::Security, label_key: "menu.settings.security" },
    Command { id: CommandId::TempFiles, label_key: "menu.settings.tempfiles" },
    Command { id: CommandId::Quit, label_key: "menu.file.quit" },
    Command { id: CommandId::Cut, label_key: "menu.edit.cut" },
    Command { id: CommandId::Copy, label_key: "menu.edit.copy" },
    Command { id: CommandId::Paste, label_key: "menu.edit.paste" },
    Command { id: CommandId::PastePlain, label_key: "menu.edit.pasteplain" },
    Command { id: CommandId::Find, label_key: "menu.edit.find" },
    Command { id: CommandId::Replace, label_key: "menu.edit.replace" },
    Command { id: CommandId::GotoLine, label_key: "menu.edit.goto" },
    Command { id: CommandId::DuplicateLine, label_key: "menu.edit.dupline" },
    Command { id: CommandId::DeleteLine, label_key: "menu.edit.delline" },
    Command { id: CommandId::SelectAll, label_key: "menu.edit.selectall" },
    Command { id: CommandId::InsertTimestamp, label_key: "menu.edit.timestamp" },
    Command { id: CommandId::AlwaysOnTop, label_key: "menu.view.ontop" },
    Command { id: CommandId::WordWrap, label_key: "menu.view.wordwrap" },
    Command { id: CommandId::LineNumbers, label_key: "menu.view.linenumbers" },
    Command { id: CommandId::NewTab, label_key: "menu.tab.new" },
    Command { id: CommandId::CloseTab, label_key: "menu.tab.close" },
    Command { id: CommandId::RenameTab, label_key: "menu.tab.rename" },
    Command { id: CommandId::NextTab, label_key: "menu.tab.next" },
    Command { id: CommandId::PrevTab, label_key: "menu.tab.prev" },
    Command { id: CommandId::Vault, label_key: "menu.view.vault" },
    Command { id: CommandId::Prompts, label_key: "menu.tools.prompts" },
    Command { id: CommandId::Totp, label_key: "menu.tools.totp" },
    Command { id: CommandId::Attachments, label_key: "menu.tools.attachments" },
    Command { id: CommandId::PasswordGenerator, label_key: "menu.tools.passgen" },
    Command { id: CommandId::StegoHide, label_key: "menu.tools.stego_hide" },
    Command { id: CommandId::StegoExtract, label_key: "menu.tools.stego_extract" },
    Command { id: CommandId::SshGenerate, label_key: "menu.tools.ssh_generate" },
    Command { id: CommandId::SshAgent, label_key: "menu.tools.ssh_agent" },
    Command { id: CommandId::CheckUpdates, label_key: "menu.help.update" },
    Command { id: CommandId::About, label_key: "menu.help.about" },
];

/// Accelerator text of the save shortcut (configurable in the settings):
/// "Ctrl+S", "Ctrl+Shift+S"... Shown by the File menu and the palette.
pub fn save_shortcut_text(modifiers: u32, key: u32) -> String {
    let mut parts = Vec::new();
    if modifiers & 0x02 != 0 { parts.push("Ctrl".to_string()); }
    if modifiers & 0x04 != 0 { parts.push("Shift".to_string()); }
    if modifiers & 0x01 != 0 { parts.push("Alt".to_string()); }
    // The settings keep a virtual-key code; for A-Z and 0-9 it is the ASCII
    // code. Any other key is shown as its code, never as a stray character.
    let key_text = match char::from_u32(key) {
        Some(c) if c.is_ascii_uppercase() || c.is_ascii_digit() => c.to_string(),
        _ => format!("0x{:02X}", key),
    };
    parts.push(key_text);
    parts.join("+")
}

/// A menu label ("Paste as &Plain Text\tCtrl+Shift+V") as the palette shows
/// it: the text without the access-key marks ("&&" is a literal "&") and
/// the accelerator after the tab, if any.
pub fn menu_label_parts(label: &str) -> (String, Option<String>) {
    let (text, accel) = match label.split_once('\t') {
        Some((t, a)) => (t, Some(a.trim())),
        None => (label, None),
    };
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' {
            if chars.peek() == Some(&'&') {
                out.push('&');
                chars.next();
            }
            continue;
        }
        out.push(c);
    }
    (out, accel.filter(|a| !a.is_empty()).map(str::to_string))
}

/// What a palette row does when run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaletteTarget {
    Command(CommandId),
    /// Switch to the tab at `index`, which was named `name` when the palette
    /// opened.
    Tab { index: usize, name: String },
    /// Copy the secret at `index` (named `name` when the palette opened).
    Secret { index: usize, name: String },
}

/// One row of the palette.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaletteEntry {
    pub label: String,
    pub shortcut: Option<String>,
    pub target: PaletteTarget,
}

// Scoring weights. A match on the first character or a word start weighs
// more than one inside a word; a run of consecutive characters more than
// scattered ones; each skipped character costs a little, so the tightest
// match wins.
const SCORE_MATCH: i32 = 1;
const BONUS_FIRST: i32 = 15;
const BONUS_WORD_START: i32 = 10;
const BONUS_CONSECUTIVE: i32 = 5;
const PENALTY_GAP: i32 = 1;
const BONUS_PREFIX: i32 = 100;

fn fold(c: char) -> char {
    // `to_lowercase` always yields at least one char; the first one is
    // enough to compare ("İ" folds to "i").
    c.to_lowercase().next().unwrap_or(c)
}

fn is_word_start(chars: &[char], i: usize) -> bool {
    if i == 0 {
        return true;
    }
    let prev = chars[i - 1];
    let cur = chars[i];
    !prev.is_alphanumeric() || (prev.is_lowercase() && cur.is_uppercase())
}

/// Fuzzy subsequence score of `query` in `candidate`, case-insensitive:
/// `None` when the characters of the query (spaces ignored) do not all
/// appear in order. Higher is better: a prefix first, then matches on word
/// starts and runs of consecutive characters. An empty query matches
/// everything with 0.
pub fn fuzzy_score(query: &str, candidate: &str) -> Option<i32> {
    let q: Vec<char> = query.chars().filter(|c| !c.is_whitespace()).map(fold).collect();
    if q.is_empty() {
        return Some(0);
    }
    let orig: Vec<char> = candidate.chars().collect();
    let c: Vec<char> = orig.iter().map(|&ch| fold(ch)).collect();
    if q.len() > c.len() {
        return None;
    }
    // best[j]: best score with the current query char matched at candidate
    // position j (None: impossible). Dynamic programming over the query.
    let bonus = |j: usize| -> i32 {
        let mut b = SCORE_MATCH;
        if j == 0 {
            b += BONUS_FIRST;
        } else if is_word_start(&orig, j) {
            b += BONUS_WORD_START;
        }
        b
    };
    let mut best: Vec<Option<i32>> = (0..c.len())
        .map(|j| (c[j] == q[0]).then(|| bonus(j) - PENALTY_GAP * j as i32))
        .collect();
    for &qc in &q[1..] {
        let mut next: Vec<Option<i32>> = vec![None; c.len()];
        for j in 0..c.len() {
            if c[j] != qc {
                continue;
            }
            let mut from: Option<i32> = None;
            for k in 0..j {
                let Some(prev) = best[k] else { continue };
                let link = if k + 1 == j { BONUS_CONSECUTIVE } else { -PENALTY_GAP * (j - k - 1) as i32 };
                let s = prev + link;
                if from.map_or(true, |f| s > f) {
                    from = Some(s);
                }
            }
            next[j] = from.map(|f| f + bonus(j));
        }
        best = next;
    }
    let score = best.into_iter().flatten().max()?;
    // The whole query typed as the start of the label (spaces kept).
    let typed: Vec<char> = query.trim_start().chars().map(fold).collect();
    let prefix = !typed.is_empty() && c.len() >= typed.len() && c[..typed.len()] == typed[..];
    Some(if prefix { score + BONUS_PREFIX } else { score })
}

/// Indices of the labels that match `query`, best first; equal scores keep
/// the order of the list (menu order, then tabs, then secrets).
pub fn rank(query: &str, labels: &[String]) -> Vec<usize> {
    let mut scored: Vec<(i32, usize)> = labels
        .iter()
        .enumerate()
        .filter_map(|(i, l)| fuzzy_score(query, l).map(|s| (s, i)))
        .collect();
    // Stable: ties stay in list order.
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter().map(|(_, i)| i).collect()
}

/// Text of a list row: the label, a tab, the shortcut (the list box aligns
/// it in a right-hand column).
pub fn row_text(entry: &PaletteEntry) -> String {
    match &entry.shortcut {
        Some(s) => format!("{}\t{}", entry.label, s),
        None => entry.label.clone(),
    }
}

/// The steps of one palette run, on the real window or a fake.
pub trait PaletteSteps {
    fn lock_epoch(&self) -> u64;
    fn still_valid(&self, epoch: u64) -> bool;
    /// Show the palette; the index of the chosen entry, `None` on Esc.
    fn show(&self, entries: &[PaletteEntry]) -> Option<usize>;
    /// Name of the tab at `index` now, `None` if there is none.
    fn tab_name(&self, index: usize) -> Option<String>;
    /// Name of the secret at `index` now, `None` if there is none.
    fn secret_name(&self, index: usize) -> Option<String>;
    fn run_command(&self, id: CommandId);
    fn switch_to_tab(&self, index: usize);
    fn copy_secret(&self, index: usize);
}

/// How a palette run ended.
#[derive(Debug, PartialEq, Eq)]
pub enum PaletteOutcome {
    Cancelled,
    /// A lock (or the curtain) came while the palette was open.
    DroppedByLock,
    /// The chosen tab or secret was removed, moved or renamed meanwhile
    /// (the main window stays usable while the palette is open).
    Stale,
    Ran(PaletteTarget),
}

/// One palette run: show, then act on the choice only if no lock came in
/// between and the chosen tab or secret is still the one listed.
pub fn run_palette(steps: &impl PaletteSteps, entries: &[PaletteEntry]) -> PaletteOutcome {
    let epoch = steps.lock_epoch();
    let Some(chosen) = steps.show(entries) else { return PaletteOutcome::Cancelled };
    if !steps.still_valid(epoch) {
        return PaletteOutcome::DroppedByLock;
    }
    let Some(entry) = entries.get(chosen) else { return PaletteOutcome::Cancelled };
    match &entry.target {
        PaletteTarget::Command(id) => steps.run_command(*id),
        PaletteTarget::Tab { index, name } => {
            if steps.tab_name(*index).as_deref() != Some(name.as_str()) {
                return PaletteOutcome::Stale;
            }
            steps.switch_to_tab(*index);
        }
        PaletteTarget::Secret { index, name } => {
            if steps.secret_name(*index).as_deref() != Some(name.as_str()) {
                return PaletteOutcome::Stale;
            }
            steps.copy_secret(*index);
        }
    }
    PaletteOutcome::Ran(entry.target.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    fn labels(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    // ── fuzzy_score ──

    #[test]
    fn empty_query_matches_everything_with_zero() {
        assert_eq!(fuzzy_score("", "Save"), Some(0));
        assert_eq!(fuzzy_score("   ", ""), Some(0));
    }

    #[test]
    fn subsequence_in_order_matches_case_insensitively() {
        assert!(fuzzy_score("SVE", "save").is_some());
        assert!(fuzzy_score("dl", "Delete Line").is_some());
    }

    #[test]
    fn characters_out_of_order_or_missing_do_not_match() {
        assert_eq!(fuzzy_score("evas", "Save"), None);
        assert_eq!(fuzzy_score("x", "Save"), None);
        assert_eq!(fuzzy_score("saves", "Save"), None);
        assert_eq!(fuzzy_score("a", ""), None);
    }

    #[test]
    fn spaces_in_the_query_are_ignored_for_matching() {
        assert!(fuzzy_score("del line", "Delete Line").is_some());
        assert!(fuzzy_score("d l", "DeleteLine").is_some());
    }

    #[test]
    fn a_prefix_beats_a_match_inside_a_word() {
        let prefix = fuzzy_score("pa", "Paste").unwrap();
        let inside = fuzzy_score("pa", "Compare").unwrap();
        assert!(prefix > inside, "{prefix} <= {inside}");
    }

    #[test]
    fn word_starts_beat_scattered_letters() {
        // "dl": D(elete) L(ine) on two word starts, versus d..l inside words.
        let words = fuzzy_score("dl", "Delete Line").unwrap();
        let inside = fuzzy_score("dl", "Addle").unwrap();
        assert!(words > inside, "{words} <= {inside}");
        let tab = fuzzy_score("nt", "New Tab").unwrap();
        let mid = fuzzy_score("nt", "Print").unwrap();
        assert!(tab > mid, "{tab} <= {mid}");
    }

    #[test]
    fn the_best_alignment_is_found_not_the_first() {
        // Greedy would take the first 'l' of "Delete"; the word start of
        // "Line" scores more.
        let s = fuzzy_score("dli", "Delete Line").unwrap();
        let greedy_only = fuzzy_score("dli", "Deletli").unwrap();
        assert!(s > greedy_only, "{s} <= {greedy_only}");
    }

    #[test]
    fn consecutive_characters_beat_scattered_ones() {
        let run = fuzzy_score("ab", "xabx").unwrap();
        let gap = fuzzy_score("ab", "xaxb").unwrap();
        assert!(run > gap, "{run} <= {gap}");
    }

    #[test]
    fn non_ascii_labels_are_handled_on_char_boundaries() {
        // é is not e: "pref" has no plain 'e' between "pr" and the 'f'.
        assert!(fuzzy_score("pref", "Pr\u{e9}f\u{e9}rences").is_none());
        assert!(fuzzy_score("pr\u{c9}f", "Pr\u{e9}f\u{e9}rences").is_some());
        assert!(fuzzy_score("\u{1F512}", "\u{1F512} Lock").is_some());
        assert!(fuzzy_score("onglet", "Onglet : R&D \u{2014} \u{e9}t\u{e9}").is_some());
    }

    #[test]
    fn word_start_after_punctuation_and_camel_case() {
        let colon = fuzzy_score("w", "Tab: work").unwrap();
        let inside = fuzzy_score("w", "Tab: swork").unwrap();
        assert!(colon > inside);
        let camel = fuzzy_score("k", "sshKey").unwrap();
        let low = fuzzy_score("k", "sshkey").unwrap();
        assert!(camel > low);
    }

    // ── rank ──

    #[test]
    fn rank_filters_and_orders_best_first() {
        let l = labels(&["Print...", "Paste", "Compare", "Save"]);
        assert_eq!(rank("pa", &l), vec![1, 2]);
        assert_eq!(rank("zz", &l), Vec::<usize>::new());
    }

    #[test]
    fn rank_with_empty_query_keeps_the_list_order() {
        let l = labels(&["b", "a", "c"]);
        assert_eq!(rank("", &l), vec![0, 1, 2]);
    }

    #[test]
    fn rank_keeps_list_order_on_equal_scores() {
        let l = labels(&["Tab: one", "Tab: two", "Vault: tab"]);
        let r = rank("tab", &l);
        assert_eq!(&r[..2], &[0, 1]);
        assert_eq!(r.len(), 3);
    }

    // ── labels ──

    #[test]
    fn menu_label_loses_access_key_and_accelerator() {
        assert_eq!(menu_label_parts("Paste as &Plain Text\tCtrl+Shift+V"), ("Paste as Plain Text".into(), Some("Ctrl+Shift+V".into())));
        assert_eq!(menu_label_parts("&Vault Manager..."), ("Vault Manager...".into(), None));
        assert_eq!(menu_label_parts("Cu&t\tCtrl+X"), ("Cut".into(), Some("Ctrl+X".into())));
        assert_eq!(menu_label_parts("R&&D &Notes"), ("R&D Notes".into(), None));
        assert_eq!(menu_label_parts("Trailing &"), ("Trailing ".into(), None));
        assert_eq!(menu_label_parts("Empty accel\t"), ("Empty accel".into(), None));
        assert_eq!(menu_label_parts("Gestor de &b\u{f3}veda..."), ("Gestor de b\u{f3}veda...".into(), None));
    }

    #[test]
    fn save_shortcut_text_follows_the_settings() {
        assert_eq!(save_shortcut_text(0x02, 0x53), "Ctrl+S");
        assert_eq!(save_shortcut_text(0x02 | 0x04, 0x53), "Ctrl+Shift+S");
        assert_eq!(save_shortcut_text(0x01, 0x31), "Alt+1");
        assert_eq!(save_shortcut_text(0x02, 0x74), "Ctrl+0x74"); // F5, not "t"
        assert_eq!(save_shortcut_text(0x02, 0x0D), "Ctrl+0x0D");
        assert_eq!(save_shortcut_text(0x02, 0x11_0000), "Ctrl+0x110000");
    }

    #[test]
    fn row_text_puts_the_shortcut_after_a_tab() {
        let e = PaletteEntry { label: "Save".into(), shortcut: Some("Ctrl+S".into()), target: PaletteTarget::Command(CommandId::Save) };
        assert_eq!(row_text(&e), "Save\tCtrl+S");
        let t = PaletteEntry { label: "Tab: a".into(), shortcut: None, target: PaletteTarget::Tab { index: 0, name: "a".into() } };
        assert_eq!(row_text(&t), "Tab: a");
    }

    // ── command table ──

    #[test]
    fn every_command_is_listed_once_with_a_distinct_key() {
        let mut ids: Vec<String> = COMMANDS.iter().map(|c| format!("{:?}", c.id)).collect();
        let mut keys: Vec<&str> = COMMANDS.iter().map(|c| c.label_key).collect();
        let n = ids.len();
        ids.sort();
        ids.dedup();
        keys.sort();
        keys.dedup();
        assert_eq!(ids.len(), n, "a command is listed twice");
        assert_eq!(keys.len(), n, "two commands share a label");
    }

    #[test]
    fn every_command_label_exists_and_is_readable_in_every_language() {
        use crate::features::i18n::{get, set_language, Language};
        let _g = crate::features::i18n::test_support::lang_guard();
        for lang in [Language::English, Language::French, Language::Spanish, Language::German] {
            set_language(lang);
            for c in COMMANDS {
                let raw = get(c.label_key);
                assert!(!raw.starts_with('['), "{} missing in {:?}", c.label_key, lang);
                let (text, _) = menu_label_parts(raw);
                assert!(!text.trim().is_empty() && !text.contains('&') && !text.contains('\t'), "{}: {:?}", c.label_key, text);
            }
            for key in ["palette.title", "palette.tab", "palette.vault"] {
                assert!(!get(key).starts_with('['), "{key} missing in {lang:?}");
            }
            assert!(get("palette.tab").contains("{name}") && get("palette.vault").contains("{name}"), "{lang:?}");
        }
        set_language(Language::English);
    }

    // ── run_palette (sequence, with a fake window) ──

    #[derive(Default)]
    struct Fake {
        epoch: Cell<u64>,
        curtain: Cell<bool>,
        choice: Option<usize>,
        lock_while_open: bool,
        tabs: RefCell<Vec<String>>,
        secrets: RefCell<Vec<String>>,
        rename_tab_while_open: Option<(usize, String)>,
        drop_secret_while_open: Option<usize>,
        log: RefCell<Vec<String>>,
    }

    impl PaletteSteps for Fake {
        fn lock_epoch(&self) -> u64 {
            self.log.borrow_mut().push("epoch".into());
            self.epoch.get()
        }
        fn still_valid(&self, epoch: u64) -> bool {
            self.log.borrow_mut().push("still_valid".into());
            epoch == self.epoch.get() && !self.curtain.get()
        }
        fn show(&self, entries: &[PaletteEntry]) -> Option<usize> {
            self.log.borrow_mut().push(format!("show {}", entries.len()));
            if self.lock_while_open {
                self.epoch.set(self.epoch.get() + 1);
            }
            if let Some((i, n)) = &self.rename_tab_while_open {
                self.tabs.borrow_mut()[*i] = n.clone();
            }
            if let Some(i) = self.drop_secret_while_open {
                self.secrets.borrow_mut().remove(i);
            }
            self.choice
        }
        fn tab_name(&self, index: usize) -> Option<String> {
            self.tabs.borrow().get(index).cloned()
        }
        fn secret_name(&self, index: usize) -> Option<String> {
            self.secrets.borrow().get(index).cloned()
        }
        fn run_command(&self, id: CommandId) {
            self.log.borrow_mut().push(format!("run {:?}", id));
        }
        fn switch_to_tab(&self, index: usize) {
            self.log.borrow_mut().push(format!("tab {}", index));
        }
        fn copy_secret(&self, index: usize) {
            self.log.borrow_mut().push(format!("copy {}", index));
        }
    }

    fn entries() -> Vec<PaletteEntry> {
        vec![
            PaletteEntry { label: "Save".into(), shortcut: Some("Ctrl+S".into()), target: PaletteTarget::Command(CommandId::Save) },
            PaletteEntry { label: "Tab: a".into(), shortcut: None, target: PaletteTarget::Tab { index: 0, name: "a".into() } },
            PaletteEntry { label: "Tab: b".into(), shortcut: None, target: PaletteTarget::Tab { index: 1, name: "b".into() } },
            PaletteEntry { label: "Vault: k".into(), shortcut: None, target: PaletteTarget::Secret { index: 1, name: "k".into() } },
        ]
    }

    fn fake(choice: Option<usize>) -> Fake {
        Fake {
            choice,
            tabs: RefCell::new(vec!["a".into(), "b".into()]),
            secrets: RefCell::new(vec!["j".into(), "k".into()]),
            ..Default::default()
        }
    }

    #[test]
    fn a_command_runs_after_the_epoch_was_taken_before_the_dialog() {
        let f = fake(Some(0));
        assert_eq!(run_palette(&f, &entries()), PaletteOutcome::Ran(PaletteTarget::Command(CommandId::Save)));
        assert_eq!(*f.log.borrow(), ["epoch", "show 4", "still_valid", "run Save"]);
    }

    #[test]
    fn esc_runs_nothing() {
        let f = fake(None);
        assert_eq!(run_palette(&f, &entries()), PaletteOutcome::Cancelled);
        assert_eq!(*f.log.borrow(), ["epoch", "show 4"]);
    }

    #[test]
    fn a_lock_while_open_drops_the_choice() {
        let mut f = fake(Some(3));
        f.lock_while_open = true;
        assert_eq!(run_palette(&f, &entries()), PaletteOutcome::DroppedByLock);
        assert!(!f.log.borrow().iter().any(|l| l.starts_with("copy") || l.starts_with("run")));
    }

    #[test]
    fn the_curtain_drops_the_choice() {
        let f = fake(Some(0));
        f.curtain.set(true);
        assert_eq!(run_palette(&f, &entries()), PaletteOutcome::DroppedByLock);
        assert!(!f.log.borrow().iter().any(|l| l.starts_with("run")));
    }

    #[test]
    fn a_tab_switches_when_it_is_still_the_one_listed() {
        let f = fake(Some(2));
        assert_eq!(run_palette(&f, &entries()), PaletteOutcome::Ran(PaletteTarget::Tab { index: 1, name: "b".into() }));
        assert_eq!(f.log.borrow().last().map(String::as_str), Some("tab 1"));
    }

    #[test]
    fn a_tab_renamed_or_closed_while_open_is_not_switched_to() {
        let mut f = fake(Some(2));
        f.rename_tab_while_open = Some((1, "c".into()));
        assert_eq!(run_palette(&f, &entries()), PaletteOutcome::Stale);
        assert!(!f.log.borrow().iter().any(|l| l.starts_with("tab")));

        let f = fake(Some(2));
        f.tabs.borrow_mut().truncate(1);
        assert_eq!(run_palette(&f, &entries()), PaletteOutcome::Stale);
    }

    #[test]
    fn a_secret_is_copied_through_its_index_when_unchanged() {
        let f = fake(Some(3));
        assert_eq!(run_palette(&f, &entries()), PaletteOutcome::Ran(PaletteTarget::Secret { index: 1, name: "k".into() }));
        assert_eq!(f.log.borrow().last().map(String::as_str), Some("copy 1"));
    }

    #[test]
    fn a_secret_deleted_while_open_is_not_copied() {
        // Deleting "j" moves "k" to index 0: index 1 is gone, nothing is
        // copied (and never the wrong secret).
        let mut f = fake(Some(3));
        f.drop_secret_while_open = Some(0);
        assert_eq!(run_palette(&f, &entries()), PaletteOutcome::Stale);
        assert!(!f.log.borrow().iter().any(|l| l.starts_with("copy")));
    }

    #[test]
    fn an_out_of_range_choice_runs_nothing() {
        let f = fake(Some(99));
        assert_eq!(run_palette(&f, &entries()), PaletteOutcome::Cancelled);
        assert!(!f.log.borrow().iter().any(|l| l.starts_with("run") || l.starts_with("tab") || l.starts_with("copy")));
    }
}
