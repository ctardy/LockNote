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

/// Why the list could not be built (the system comparison failed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptsListError(pub String);

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
    crate::ui::vault_panel::contains_name_user_locale(haystack, needle).map_err(|e| match e {
        crate::ui::vault_panel::VaultTreeError::Compare(msg) => PromptsListError(msg),
        other => PromptsListError(format!("{:?}", other)),
    })
}

/// Names compared for the user's locale, ignoring case and accents (the
/// vault's comparison).
pub fn compare_user_locale(a: &str, b: &str) -> Result<Ordering, PromptsListError> {
    crate::ui::vault_panel::compare_names_user_locale(a, b).map_err(|e| PromptsListError(format!("{:?}", e)))
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

    #[test]
    fn a_failing_comparison_is_an_error_not_an_empty_list() {
        let fail = |_: &str, _: &str| -> Result<bool, PromptsListError> { Err(PromptsListError("boom".into())) };
        assert_eq!(
            visible_rows(&sample(), "x", &fail, &compare_user_locale),
            Err(PromptsListError("boom".into()))
        );
        let bad_cmp = |_: &str, _: &str| -> Result<Ordering, PromptsListError> { Err(PromptsListError("cmp".into())) };
        assert!(visible_rows(&sample(), "", &contains_user_locale, &bad_cmp).is_err());
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
    fn newlines_for_the_controls_and_back() {
        assert_eq!(to_crlf("a\nb\n\nc"), "a\r\nb\r\n\r\nc");
        // Already CRLF stays CRLF (no \r\r\n).
        assert_eq!(to_crlf("a\r\nb\nc"), "a\r\nb\r\nc");
        assert_eq!(from_crlf("a\r\nb\r\n"), "a\nb\n");
        assert_eq!(from_crlf(&to_crlf("x\n\ny\n")), "x\n\ny\n");
    }
}
