// PromptsPanel — Right-side panel of stored prompts (issue 058)
//
// A search field, a list sorted by category then name ("Category · Name"),
// a read-only preview of the selected prompt, and Add / Edit / Delete / Copy.
// Double-click on a row copies it. A prompt is not a secret: the copy is not
// cleared from the clipboard after a delay (unlike the vault).
//
// This module holds the state and the pure helpers; the controls live in
// `editor::prompts_ops`.

use std::cell::RefCell;
use std::cmp::Ordering;

use crate::features::prompts::PromptEntry;
use crate::ui::vault_panel::VaultTreeError;

/// The prompts of the note, the panel visibility, and which prompt each row
/// of the list shows.
pub struct PromptsPanel {
    prompts: RefCell<Vec<PromptEntry>>,
    visible: RefCell<bool>,
    /// Storage index of the prompt shown on each row, in display order.
    rows: RefCell<Vec<usize>>,
}

impl PromptsPanel {
    pub fn new() -> Self {
        PromptsPanel { prompts: RefCell::new(Vec::new()), visible: RefCell::new(false), rows: RefCell::new(Vec::new()) }
    }

    pub fn is_visible(&self) -> bool {
        *self.visible.borrow()
    }

    pub fn toggle_visible(&self) {
        let mut v = self.visible.borrow_mut();
        *v = !*v;
    }

    pub fn set_visible(&self, visible: bool) {
        *self.visible.borrow_mut() = visible;
    }

    /// Replace every prompt (load, import, lock). The rows are rebuilt by the
    /// next refresh; until then none is valid.
    pub fn set_prompts(&self, prompts: Vec<PromptEntry>) {
        *self.prompts.borrow_mut() = prompts;
        self.rows.borrow_mut().clear();
    }

    pub fn get_prompts(&self) -> Vec<PromptEntry> {
        self.prompts.borrow().clone()
    }

    pub fn count(&self) -> usize {
        self.prompts.borrow().len()
    }

    pub fn get(&self, index: usize) -> Option<PromptEntry> {
        self.prompts.borrow().get(index).cloned()
    }

    /// Names in storage order (the palette: no prompt text is copied).
    pub fn names(&self) -> Vec<String> {
        self.prompts.borrow().iter().map(|p| p.name.clone()).collect()
    }

    /// Append a prompt; returns its storage index.
    pub fn add(&self, entry: PromptEntry) -> usize {
        let mut prompts = self.prompts.borrow_mut();
        prompts.push(entry);
        prompts.len() - 1
    }

    /// Replace the prompt at `index` only if it is still `original`: a modal
    /// dialog was open in between, and an import may have replaced the list.
    /// `false` (nothing changed) otherwise.
    pub fn update_if_unchanged(&self, index: usize, original: &PromptEntry, entry: PromptEntry) -> bool {
        match self.prompts.borrow_mut().get_mut(index) {
            Some(slot) if slot == original => {
                *slot = entry;
                true
            }
            _ => false,
        }
    }

    /// Remove the prompt at `index` only if it is still `original` (see
    /// `update_if_unchanged`).
    pub fn delete_if_unchanged(&self, index: usize, original: &PromptEntry) -> bool {
        if self.prompts.borrow().get(index) != Some(original) {
            return false;
        }
        self.delete(index)
    }

    /// Remove the prompt at `index`. `false` when there is none.
    pub fn delete(&self, index: usize) -> bool {
        let mut prompts = self.prompts.borrow_mut();
        if index >= prompts.len() {
            return false;
        }
        prompts.remove(index);
        drop(prompts);
        self.rows.borrow_mut().clear();
        true
    }

    pub fn set_rows(&self, rows: Vec<usize>) {
        *self.rows.borrow_mut() = rows;
    }

    /// Storage index of the prompt on list row `row`.
    pub fn index_at_row(&self, row: usize) -> Option<usize> {
        self.rows.borrow().get(row).copied()
    }

    /// List row showing the prompt at storage index `index`.
    pub fn row_of(&self, index: usize) -> Option<usize> {
        self.rows.borrow().iter().position(|&i| i == index)
    }
}

/// Why the list could not be built (the system comparison failed): the
/// vault's locale error, logged in its Debug form, shown in words.
#[derive(Debug, PartialEq, Eq)]
pub struct PromptsListError(pub VaultTreeError);

impl PromptsListError {
    /// The status line of the panel, in `lang`: `prompts.list.error` with
    /// the cause in words (`VaultTreeError::user_message`).
    pub fn status_text(&self, lang: crate::features::i18n::Language) -> String {
        crate::features::i18n::get_for(lang, "prompts.list.error").replace("{msg}", &self.0.user_message(lang))
    }
}

/// True when `query` is empty (spaces only) or found in the name or the text
/// of `entry` by `contains(haystack, needle)`.
pub fn matches<F>(entry: &PromptEntry, query: &str, contains: &F) -> Result<bool, PromptsListError>
where
    F: Fn(&str, &str) -> Result<bool, PromptsListError>,
{
    let query = query.trim();
    if query.is_empty() {
        return Ok(true);
    }
    Ok(contains(&entry.name, query)? || contains(&entry.text, query)?)
}

/// Storage indices of the prompts matching `query`, sorted by category then
/// by name with `cmp`, storage order for ties (stable).
pub fn visible_rows<F, C>(prompts: &[PromptEntry], query: &str, contains: &F, cmp: &C) -> Result<Vec<usize>, PromptsListError>
where
    F: Fn(&str, &str) -> Result<bool, PromptsListError>,
    C: Fn(&str, &str) -> Result<Ordering, PromptsListError>,
{
    let mut rows = Vec::new();
    for (i, p) in prompts.iter().enumerate() {
        if matches(p, query, contains)? {
            rows.push(i);
        }
    }
    // Insertion sort: stable, and the comparison can fail.
    for i in 1..rows.len() {
        let mut j = i;
        while j > 0 && order(&prompts[rows[j - 1]], &prompts[rows[j]], cmp)? == Ordering::Greater {
            rows.swap(j - 1, j);
            j -= 1;
        }
    }
    Ok(rows)
}

fn order<C>(a: &PromptEntry, b: &PromptEntry, cmp: &C) -> Result<Ordering, PromptsListError>
where
    C: Fn(&str, &str) -> Result<Ordering, PromptsListError>,
{
    match cmp(&a.category, &b.category)? {
        Ordering::Equal => cmp(&a.name, &b.name),
        o => Ok(o),
    }
}

/// Text of a list row: "Category · Name".
pub fn list_label(entry: &PromptEntry) -> String {
    format!("{} \u{00b7} {}", entry.category, entry.name)
}

/// Space between two buttons of the panel's button row.
pub const BUTTON_GAP: u32 = 4;
/// Width of a button at the default panel width (the row of 2.1).
const BUTTON_MAX_W: u32 = 58;
/// Narrowest button: the label stays readable in a narrowed panel.
const BUTTON_MIN_W: u32 = 30;

/// Width of each of the `count` buttons sharing a row `inner_w` wide (gaps
/// of `BUTTON_GAP`), between `BUTTON_MIN_W` and `BUTTON_MAX_W`.
pub fn button_width(inner_w: u32, count: u32) -> u32 {
    if count == 0 {
        return BUTTON_MAX_W;
    }
    let gaps = BUTTON_GAP * (count - 1);
    (inner_w.saturating_sub(gaps) / count).clamp(BUTTON_MIN_W, BUTTON_MAX_W)
}

/// Row a click landed on, from the `LB_ITEMFROMPOINT` result (row in the low
/// word, non-zero high word when the point is past the last row). `None`
/// when the click hit no row: a click on the empty part copies nothing.
pub fn clicked_row(item_from_point: u32, row_count: usize) -> Option<usize> {
    let row = (item_from_point & 0xFFFF) as usize;
    (item_from_point >> 16 == 0 && row < row_count).then_some(row)
}

/// "Copied" indicator: the template's `{name}` replaced by the prompt name.
pub fn copied_message(template: &str, name: &str) -> String {
    template.replace("{name}", name)
}

/// Categories to offer in the dialog: `default` first, then the categories
/// in use, each once (same name for `same`), sorted by `cmp`.
pub fn category_choices<S, C>(prompts: &[PromptEntry], default: &str, same: &S, cmp: &C) -> Result<Vec<String>, PromptsListError>
where
    S: Fn(&str, &str) -> Result<bool, PromptsListError>,
    C: Fn(&str, &str) -> Result<Ordering, PromptsListError>,
{
    let mut used: Vec<String> = Vec::new();
    for p in prompts {
        let mut known = same(&p.category, default)?;
        for u in &used {
            if known {
                break;
            }
            known = same(&p.category, u)?;
        }
        if !known {
            used.push(p.category.clone());
        }
    }
    for i in 1..used.len() {
        let mut j = i;
        while j > 0 && cmp(&used[j - 1], &used[j])? == Ordering::Greater {
            used.swap(j - 1, j);
            j -= 1;
        }
    }
    let mut out = vec![default.to_string()];
    out.extend(used);
    Ok(out)
}

/// A variable of a prompt text, expanded when the prompt is copied or
/// inserted into the note (nothing is stored: the text keeps `{date}`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptVar {
    /// `{date}`: YYYY-MM-DD.
    Date,
    /// `{time}`: HH:MM:SS.
    Time,
    /// `{datetime}`: "YYYY-MM-DD HH:MM:SS".
    DateTime,
    /// `{clipboard}`: the clipboard text, empty if none.
    Clipboard,
    /// `{tab}`: the name of the active tab.
    Tab,
}

/// The variables as written in a prompt, braces included.
const PROMPT_VARS: &[(&str, PromptVar)] = &[
    ("{date}", PromptVar::Date),
    ("{time}", PromptVar::Time),
    ("{datetime}", PromptVar::DateTime),
    ("{clipboard}", PromptVar::Clipboard),
    ("{tab}", PromptVar::Tab),
];

/// `text` with every variable replaced by `value(var)`. "{{" and "}}" give
/// a literal brace; any other brace, and an unknown `{xyz}`, is kept as is.
/// The values are not scanned again (a clipboard holding "{date}" stays).
pub fn expand_variables(text: &str, value: &mut dyn FnMut(PromptVar) -> String) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    'scan: while let Some(c) = rest.chars().next() {
        if rest.starts_with("{{") || rest.starts_with("}}") {
            out.push(c);
            rest = &rest[2..];
            continue;
        }
        if c == '{' {
            for (name, var) in PROMPT_VARS {
                if let Some(after) = rest.strip_prefix(name) {
                    out.push_str(&value(*var));
                    rest = after;
                    continue 'scan;
                }
            }
        }
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

/// Date and time of a "YYYY-MM-DD HH:MM:SS" timestamp (the Insert Timestamp
/// format). A text without a space is all date.
pub fn split_datetime(datetime: &str) -> (&str, &str) {
    datetime.split_once(' ').unwrap_or((datetime, ""))
}

/// What expanding and inserting a prompt needs from the window, real or
/// fake (sequence tests).
pub trait PromptSteps {
    /// Locked or under the curtain: nothing may change the note.
    fn blocked(&self) -> bool;
    fn prompt(&self, index: usize) -> Option<PromptEntry>;
    /// Now as "YYYY-MM-DD HH:MM:SS" (the Insert Timestamp format), on the
    /// local clock of the machine.
    fn now(&self) -> String;
    fn active_tab_name(&self) -> String;
    /// The clipboard text, empty if there is none.
    fn clipboard_text(&self) -> String;
    /// Replace the editor selection with `text` (one undo step).
    fn insert(&self, text: &str);
}

/// The text of a prompt as copied or inserted: variables expanded, lines
/// ending with `\r\n`. The clock is read once (so `{date}` and `{time}`
/// agree) and the clipboard only when the text uses it, once.
pub fn expanded_text(steps: &dyn PromptSteps, text: &str) -> String {
    let mut now: Option<String> = None;
    let mut clipboard: Option<String> = None;
    let mut tab: Option<String> = None;
    let expanded = expand_variables(text, &mut |var| {
        match var {
            PromptVar::Date | PromptVar::Time | PromptVar::DateTime => {
                let now = now.get_or_insert_with(|| steps.now());
                let (date, time) = split_datetime(now);
                match var {
                    PromptVar::Date => date.to_string(),
                    PromptVar::Time => time.to_string(),
                    _ => now.clone(),
                }
            }
            PromptVar::Clipboard => clipboard.get_or_insert_with(|| steps.clipboard_text()).clone(),
            PromptVar::Tab => tab.get_or_insert_with(|| steps.active_tab_name()).clone(),
        }
    });
    to_crlf(&expanded)
}

/// How an "Insert into note" ended.
#[derive(Debug, PartialEq, Eq)]
pub enum InsertOutcome {
    /// Locked or under the curtain: nothing inserted.
    Blocked,
    /// No prompt at that index any more: nothing inserted.
    Gone,
    /// Inserted; the name of the prompt.
    Inserted(String),
}

/// "Insert into note": the prompt at `index`, expanded, replaces the
/// editor selection.
pub fn insert_prompt(steps: &dyn PromptSteps, index: usize) -> InsertOutcome {
    if steps.blocked() {
        return InsertOutcome::Blocked;
    }
    let Some(prompt) = steps.prompt(index) else { return InsertOutcome::Gone };
    let text = expanded_text(steps, &prompt.text);
    steps.insert(&text);
    InsertOutcome::Inserted(prompt.name.clone())
}

/// Text as Windows controls and the clipboard expect it: every line ends
/// with `\r\n` (`CF_UNICODETEXT`). Stored texts use `\n`.
pub fn to_crlf(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\n', "\r\n")
}

/// Text of the multi-line edit control (`\r\n`) as stored (`\n`).
pub fn from_crlf(text: &str) -> String {
    text.replace("\r\n", "\n")
}

/// `needle` found in `haystack` for the user's locale, ignoring case and
/// accents (the vault search, `vault_panel::contains_name_user_locale`).
pub fn contains_user_locale(haystack: &str, needle: &str) -> Result<bool, PromptsListError> {
    crate::ui::vault_panel::contains_name_user_locale(haystack, needle).map_err(PromptsListError)
}

/// Names compared for the user's locale, ignoring case and accents (the
/// vault's comparison).
pub fn compare_user_locale(a: &str, b: &str) -> Result<Ordering, PromptsListError> {
    crate::ui::vault_panel::compare_names_user_locale(a, b).map_err(PromptsListError)
}

/// Same category name for the user's locale, ignoring case and accents.
pub fn same_user_locale(a: &str, b: &str) -> Result<bool, PromptsListError> {
    Ok(compare_user_locale(a, b)? == Ordering::Equal)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(name: &str, text: &str, category: &str) -> PromptEntry {
        PromptEntry::new(name, text, category)
    }

    fn sample() -> Vec<PromptEntry> {
        vec![
            p("Résumé", "Résume ce texte en trois points", "Écriture"),
            p("bug", "Find the BUG in this code", "Code"),
            p("Alpha", "first", "code"),
            p("Zeta", "naïve café", "Autre"),
        ]
    }

    #[test]
    fn a_click_copies_only_the_row_under_the_pointer() {
        // LB_ITEMFROMPOINT: row in the low word, 1 in the high word when the
        // point is past the last row.
        assert_eq!(clicked_row(2, 4), Some(2));
        assert_eq!(clicked_row(0, 1), Some(0));
        assert_eq!(clicked_row(0x0001_0003, 4), None, "below the last row");
        assert_eq!(clicked_row(4, 4), None, "row out of the list");
        assert_eq!(clicked_row(0, 0), None, "empty list");
        assert_eq!(clicked_row(0xFFFF_FFFF, 4), None, "LB_ERR");
    }

    // After a modal: the edit or the delete applies only to the prompt the
    // dialog was opened on.
    #[test]
    fn edit_and_delete_after_a_modal_need_the_same_prompt() {
        let panel = PromptsPanel::new();
        panel.add(p("a", "t", "c"));
        let original = p("a", "t", "c");
        assert!(panel.update_if_unchanged(0, &original, p("a2", "t2", "c")));
        assert_eq!(panel.get(0), Some(p("a2", "t2", "c")));
        // Replaced meanwhile (import): nothing changes.
        assert!(!panel.update_if_unchanged(0, &original, p("x", "x", "x")));
        assert!(!panel.delete_if_unchanged(0, &original));
        assert_eq!(panel.get(0), Some(p("a2", "t2", "c")));
        // Gone meanwhile.
        assert!(!panel.update_if_unchanged(3, &original, p("x", "x", "x")));
        assert!(panel.delete_if_unchanged(0, &p("a2", "t2", "c")));
        assert_eq!(panel.count(), 0);
    }

    #[test]
    fn panel_state_add_update_delete() {
        let panel = PromptsPanel::new();
        assert!(!panel.is_visible());
        panel.toggle_visible();
        assert!(panel.is_visible());
        assert_eq!(panel.add(p("a", "t", "c")), 0);
        assert_eq!(panel.add(p("b", "t", "c")), 1);
        assert!(panel.update_if_unchanged(1, &p("b", "t", "c"), p("b2", "t2", "c")));
        assert!(!panel.update_if_unchanged(5, &p("b", "t", "c"), p("x", "x", "x")));
        assert_eq!(panel.get(1), Some(p("b2", "t2", "c")));
        panel.set_rows(vec![1, 0]);
        assert_eq!((panel.index_at_row(0), panel.row_of(0)), (Some(1), Some(1)));
        assert!(panel.delete(0));
        assert!(!panel.delete(3));
        assert_eq!(panel.count(), 1);
        // Rows are stale after a delete: none until the next refresh.
        assert_eq!(panel.index_at_row(0), None);
        panel.set_prompts(Vec::new());
        assert_eq!(panel.count(), 0);
    }

    #[test]
    fn sorted_by_category_then_name_ignoring_case_and_accents() {
        let rows = visible_rows(&sample(), "", &contains_user_locale, &compare_user_locale).unwrap();
        let labels: Vec<String> = rows.iter().map(|&i| list_label(&sample()[i])).collect();
        assert_eq!(labels, vec!["Autre · Zeta", "code · Alpha", "Code · bug", "Écriture · Résumé"]);
    }

    #[test]
    fn ties_keep_the_storage_order() {
        let prompts = vec![p("same", "1", "c"), p("SAME", "2", "C"), p("same", "3", "c")];
        let rows = visible_rows(&prompts, "", &contains_user_locale, &compare_user_locale).unwrap();
        assert_eq!(rows, vec![0, 1, 2]);
    }

    #[test]
    fn search_looks_at_name_and_text_ignoring_case_and_accents() {
        let s = sample();
        let names = |q: &str| -> Vec<String> {
            visible_rows(&s, q, &contains_user_locale, &compare_user_locale)
                .unwrap()
                .iter()
                .map(|&i| s[i].name.clone())
                .collect()
        };
        assert_eq!(names("resume"), vec!["Résumé"]); // name and text, accents
        assert_eq!(names("BUG"), vec!["bug"]); // case
        assert_eq!(names("cafe"), vec!["Zeta"]); // text only, accents
        assert_eq!(names("NAIVE"), vec!["Zeta"]);
        assert_eq!(names("  "), vec!["Zeta", "Alpha", "bug", "Résumé"]); // empty = all
        assert!(names("absent").is_empty());
        // The category is not searched (plan: name and text).
        assert!(names("Écriture").is_empty());
    }

    fn compare_failed(detail: &str) -> PromptsListError {
        PromptsListError(VaultTreeError::Compare(detail.into()))
    }

    #[test]
    fn a_failing_comparison_is_an_error_not_an_empty_list() {
        let fail = |_: &str, _: &str| -> Result<bool, PromptsListError> { Err(compare_failed("boom")) };
        assert_eq!(
            visible_rows(&sample(), "x", &fail, &compare_user_locale),
            Err(compare_failed("boom"))
        );
        let bad_cmp = |_: &str, _: &str| -> Result<Ordering, PromptsListError> { Err(compare_failed("cmp")) };
        assert!(visible_rows(&sample(), "", &contains_user_locale, &bad_cmp).is_err());
    }

    #[test]
    fn list_error_status_is_readable_in_every_language() {
        use crate::features::i18n::Language;
        let e = compare_failed("CompareStringEx failed: Error { code: HRESULT(0x80070057), message: \"Param\u{e8}tre incorrect.\" }");
        let mut seen = std::collections::HashSet::new();
        for lang in [Language::English, Language::French, Language::Spanish, Language::German] {
            let text = e.status_text(lang);
            for raw in ["Compare", "HRESULT", "0x", "{", "}", "["] {
                assert!(!text.contains(raw), "{lang:?} shows {raw:?}: {text}");
            }
            assert!(text.contains(&e.0.user_message(lang)), "{lang:?}: {text}");
            assert!(seen.insert(text.clone()), "{lang:?} has its own text: {text}");
        }
    }

    #[test]
    fn list_errors_are_logged_in_detail_and_shown_in_words() {
        let ops = crate::read_source(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ui/editor/prompts_ops.rs"));
        assert!(!ops.contains("&e.0)"), "the cause is never shown as stored");
        assert_eq!(ops.matches("self.prompts_status.set_text(&e.status_text(crate::features::i18n::current_language()));").count(), 2);
        assert!(ops.contains("eprintln!(\"ERROR prompts: list not built count={} err={:?}\", prompts.len(), e.0);"));
        assert!(ops.contains("eprintln!(\"ERROR prompts: categories not listed err={:?}\", e.0);"));
    }

    #[test]
    fn contains_user_locale_basics() {
        assert_eq!(contains_user_locale("Résumé du jour", "resume"), Ok(true));
        assert_eq!(contains_user_locale("abc", "ABC"), Ok(true));
        assert_eq!(contains_user_locale("abc", "abd"), Ok(false));
        assert_eq!(contains_user_locale("", "a"), Ok(false));
        assert_eq!(contains_user_locale("a", ""), Ok(true));
        assert_eq!(contains_user_locale("line1\nline2 😀", "LINE2 😀"), Ok(true));
    }

    #[test]
    fn category_choices_start_with_the_default_and_hold_each_category_once() {
        let s = sample();
        let got = category_choices(&s, "General", &same_user_locale, &compare_user_locale).unwrap();
        assert_eq!(got, vec!["General", "Autre", "Code", "Écriture"]);
        // A category equal to the default (case/accents) is not repeated.
        let got = category_choices(&[p("a", "t", "general")], "General", &same_user_locale, &compare_user_locale).unwrap();
        assert_eq!(got, vec!["General"]);
        let got = category_choices(&[], "Général", &same_user_locale, &compare_user_locale).unwrap();
        assert_eq!(got, vec!["Général"]);
    }

    #[test]
    fn list_label_and_copied_message() {
        assert_eq!(list_label(&p("Fix", "t", "Code")), "Code · Fix");
        assert_eq!(copied_message("Copied: {name}", "Fix"), "Copied: Fix");
    }

    #[test]
    fn five_buttons_share_the_row() {
        // Default panel (320 wide, 300 inside): the five fit.
        let w = button_width(300, 5);
        assert!(5 * w + 4 * BUTTON_GAP <= 300, "{w}");
        assert_eq!(w, 56);
        // Wide panel: the width of 2.1, no wider.
        assert_eq!(button_width(1000, 5), 58);
        // Narrowest panel (220 wide, 200 inside) and below: a floor.
        assert_eq!(button_width(200, 5), 36);
        assert_eq!(button_width(0, 5), 30);
        assert_eq!(button_width(300, 0), 58);
    }

    // ── variables (B6) ──

    fn fixed(var: PromptVar) -> String {
        match var {
            PromptVar::Date => "2026-11-26".into(),
            PromptVar::Time => "09:05:07".into(),
            PromptVar::DateTime => "2026-11-26 09:05:07".into(),
            PromptVar::Clipboard => "clip".into(),
            PromptVar::Tab => "Work".into(),
        }
    }

    fn exp(text: &str) -> String {
        expand_variables(text, &mut fixed)
    }

    #[test]
    fn every_variable_is_expanded() {
        assert_eq!(exp("{date}"), "2026-11-26");
        assert_eq!(exp("{time}"), "09:05:07");
        assert_eq!(exp("{datetime}"), "2026-11-26 09:05:07");
        assert_eq!(exp("{clipboard}"), "clip");
        assert_eq!(exp("{tab}"), "Work");
        assert_eq!(exp("On {date} at {time} in {tab}: {clipboard}{clipboard}"), "On 2026-11-26 at 09:05:07 in Work: clipclip");
        assert_eq!(exp("no variable"), "no variable");
        assert_eq!(exp(""), "");
    }

    #[test]
    fn doubled_braces_are_literal_braces() {
        assert_eq!(exp("{{date}}"), "{date}");
        assert_eq!(exp("{{"), "{");
        assert_eq!(exp("}}"), "}");
        assert_eq!(exp("{{{date}}}"), "{2026-11-26}");
        assert_eq!(exp("fn f() {{ x }}"), "fn f() { x }");
    }

    #[test]
    fn unknown_variables_and_lone_braces_stay_as_they_are() {
        assert_eq!(exp("{xyz}"), "{xyz}");
        assert_eq!(exp("{Date}"), "{Date}", "names are lower case");
        assert_eq!(exp("{ date }"), "{ date }");
        assert_eq!(exp("{date"), "{date");
        assert_eq!(exp("a { b } c"), "a { b } c");
        assert_eq!(exp("{"), "{");
        assert_eq!(exp("}"), "}");
        assert_eq!(exp("{x{date}}"), "{x2026-11-26}");
        assert_eq!(exp("{date}}"), "2026-11-26}");
    }

    #[test]
    fn values_are_not_expanded_again() {
        let mut v = |var: PromptVar| if var == PromptVar::Clipboard { "{date} {{".to_string() } else { fixed(var) };
        assert_eq!(expand_variables("[{clipboard}]", &mut v), "[{date} {{]");
    }

    #[test]
    fn non_ascii_text_around_variables() {
        assert_eq!(exp("é{date}😀{tab}ü"), "é2026-11-26😀Workü");
        assert_eq!(exp("😀{"), "😀{");
        assert_eq!(exp("{😀}"), "{😀}");
        let mut v = |var: PromptVar| if var == PromptVar::Tab { "Café ☕".to_string() } else { fixed(var) };
        assert_eq!(expand_variables("« {tab} »", &mut v), "« Café ☕ »");
    }

    #[test]
    fn datetime_splits_into_date_and_time() {
        assert_eq!(split_datetime("2026-11-26 09:05:07"), ("2026-11-26", "09:05:07"));
        assert_eq!(split_datetime("2026-11-26"), ("2026-11-26", ""));
        assert_eq!(split_datetime(""), ("", ""));
    }

    // ── expand and insert (sequence, with a fake window) ──

    #[derive(Default)]
    struct Fake {
        blocked: bool,
        prompts: Vec<PromptEntry>,
        clipboard: String,
        log: RefCell<Vec<String>>,
    }

    impl PromptSteps for Fake {
        fn blocked(&self) -> bool {
            self.log.borrow_mut().push("blocked?".into());
            self.blocked
        }
        fn prompt(&self, index: usize) -> Option<PromptEntry> {
            self.log.borrow_mut().push(format!("prompt {index}"));
            self.prompts.get(index).cloned()
        }
        fn now(&self) -> String {
            self.log.borrow_mut().push("now".into());
            "2026-11-26 09:05:07".into()
        }
        fn active_tab_name(&self) -> String {
            self.log.borrow_mut().push("tab".into());
            "Work".into()
        }
        fn clipboard_text(&self) -> String {
            self.log.borrow_mut().push("clipboard".into());
            self.clipboard.clone()
        }
        fn insert(&self, text: &str) {
            self.log.borrow_mut().push(format!("insert {text:?}"));
        }
    }

    #[test]
    fn insert_expands_then_inserts_once() {
        let f = Fake { prompts: vec![p("Log", "{date} {time}\n{tab}: ", "c")], ..Default::default() };
        assert_eq!(insert_prompt(&f, 0), InsertOutcome::Inserted("Log".into()));
        let log = f.log.borrow();
        assert_eq!(log.first().map(String::as_str), Some("blocked?"));
        assert_eq!(log.last().map(String::as_str), Some("insert \"2026-11-26 09:05:07\\r\\nWork: \""));
        assert_eq!(log.iter().filter(|l| l.starts_with("insert")).count(), 1, "one insertion, one undo step");
        assert_eq!(log.iter().filter(|l| *l == "now").count(), 1, "the clock is read once");
        assert!(!log.iter().any(|l| l == "clipboard"), "the clipboard is read only when used");
    }

    #[test]
    fn the_clipboard_is_read_once_when_used() {
        let f = Fake { prompts: vec![p("C", "{clipboard}|{clipboard}", "c")], clipboard: "x".into(), ..Default::default() };
        assert_eq!(insert_prompt(&f, 0), InsertOutcome::Inserted("C".into()));
        let log = f.log.borrow();
        assert_eq!(log.iter().filter(|l| *l == "clipboard").count(), 1);
        assert!(!log.iter().any(|l| l == "now" || l == "tab"), "{log:?}");
        assert_eq!(log.last().map(String::as_str), Some("insert \"x|x\""));
    }

    #[test]
    fn an_empty_clipboard_expands_to_nothing() {
        let f = Fake { prompts: vec![p("C", "[{clipboard}]", "c")], ..Default::default() };
        insert_prompt(&f, 0);
        assert_eq!(f.log.borrow().last().map(String::as_str), Some("insert \"[]\""));
    }

    #[test]
    fn nothing_is_inserted_when_locked_or_gone() {
        let f = Fake { blocked: true, prompts: vec![p("a", "t", "c")], ..Default::default() };
        assert_eq!(insert_prompt(&f, 0), InsertOutcome::Blocked);
        assert_eq!(*f.log.borrow(), ["blocked?"]);
        let f = Fake { prompts: vec![p("a", "t", "c")], ..Default::default() };
        assert_eq!(insert_prompt(&f, 1), InsertOutcome::Gone);
        assert!(!f.log.borrow().iter().any(|l| l.starts_with("insert")));
    }

    #[test]
    fn expanded_text_is_what_copy_puts_on_the_clipboard() {
        let f = Fake { clipboard: "old".into(), ..Default::default() };
        assert_eq!(expanded_text(&f, "a\n{{tab}} {tab}\n{clipboard}"), "a\r\n{tab} Work\r\nold");
        assert_eq!(expanded_text(&f, "plain"), "plain");
    }

    #[test]
    fn newlines_for_the_controls_and_back() {
        assert_eq!(to_crlf("a\nb\n\nc"), "a\r\nb\r\n\r\nc");
        // Already CRLF stays CRLF (no \r\r\n).
        assert_eq!(to_crlf("a\r\nb\nc"), "a\r\nb\r\nc");
        assert_eq!(from_crlf("a\r\nb\r\n"), "a\nb\n");
        assert_eq!(from_crlf(&to_crlf("x\n\ny\n")), "x\n\ny\n");
    }
}
