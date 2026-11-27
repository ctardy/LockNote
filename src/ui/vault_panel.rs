// VaultPanel — Right-side panel for secret management
//
// Shows a one-level tree (category → secret names) and provides
// Add, Edit, Delete, Copy, and Reveal actions. Values are never shown in the
// tree: Reveal puts them in a dedicated read-only field for 5 seconds.

use std::cell::RefCell;

use crate::features::vault::SecretEntry;

/// Default width of the vault panel in pixels (resizable: `ui::side_panel`).
pub const VAULT_PANEL_WIDTH: u32 = 300;

/// VaultPanel holds the list of secrets and panel visibility state.
pub struct VaultPanel {
    /// All secrets in the vault.
    secrets: RefCell<Vec<SecretEntry>>,
    /// Whether the vault panel is currently visible.
    visible: RefCell<bool>,
    /// Index of the secret currently being revealed (if any).
    reveal_index: RefCell<Option<usize>>,
    /// Timestamp when reveal started (for 5-second auto-hide).
    reveal_start: RefCell<Option<std::time::Instant>>,
}

impl VaultPanel {
    /// Create a new vault panel (initially hidden).
    pub fn new() -> Self {
        VaultPanel {
            secrets: RefCell::new(Vec::new()),
            visible: RefCell::new(false),
            reveal_index: RefCell::new(None),
            reveal_start: RefCell::new(None),
        }
    }

    /// Whether the vault panel is visible.
    pub fn is_visible(&self) -> bool {
        *self.visible.borrow()
    }

    /// Toggle visibility.
    pub fn toggle_visible(&self) {
        let mut v = self.visible.borrow_mut();
        *v = !*v;
    }

    /// Set visibility.
    pub fn set_visible(&self, visible: bool) {
        *self.visible.borrow_mut() = visible;
    }

    /// Load secrets into the panel.
    pub fn set_secrets(&self, secrets: Vec<SecretEntry>) {
        *self.secrets.borrow_mut() = secrets;
        *self.reveal_index.borrow_mut() = None;
        *self.reveal_start.borrow_mut() = None;
    }

    /// Get all secrets.
    pub fn get_secrets(&self) -> Vec<SecretEntry> {
        self.secrets.borrow().clone()
    }

    /// Get the number of secrets.
    pub fn count(&self) -> usize {
        self.secrets.borrow().len()
    }

    /// Start revealing the secret at the given global index.
    pub fn reveal(&self, global_idx: usize) {
        *self.reveal_index.borrow_mut() = Some(global_idx);
        *self.reveal_start.borrow_mut() = Some(std::time::Instant::now());
    }

    /// Check if the reveal timer has expired (5 seconds).
    /// Returns true if reveal was active and has now expired (caller should refresh).
    pub fn check_reveal_expired(&self) -> bool {
        let start = *self.reveal_start.borrow();
        if let Some(start) = start {
            if start.elapsed() >= std::time::Duration::from_secs(5) {
                *self.reveal_index.borrow_mut() = None;
                *self.reveal_start.borrow_mut() = None;
                return true;
            }
        }
        false
    }

    /// Hide the revealed secret immediately.
    pub fn hide_reveal(&self) {
        *self.reveal_index.borrow_mut() = None;
        *self.reveal_start.borrow_mut() = None;
    }

    /// Add a new secret.
    pub fn add_secret(&self, entry: SecretEntry) {
        self.secrets.borrow_mut().push(entry);
    }

    /// Replace the secret at `global_idx` only if it is still `original`: the
    /// edit dialog is modal, and an import may have replaced the vault while
    /// it was open. `false` (nothing changed) otherwise.
    pub fn update_secret_if_unchanged(&self, global_idx: usize, original: &SecretEntry, entry: SecretEntry) -> bool {
        let mut secrets = self.secrets.borrow_mut();
        match secrets.get_mut(global_idx) {
            Some(slot) if slot == original => {
                *slot = entry;
                true
            }
            _ => false,
        }
    }

    /// Delete a secret by global index.
    pub fn delete_secret(&self, global_idx: usize) {
        let mut secrets = self.secrets.borrow_mut();
        if global_idx < secrets.len() {
            secrets.remove(global_idx);
        }
        drop(secrets);
        // Reset reveal if the revealed entry was deleted
        let ri = *self.reveal_index.borrow();
        if let Some(ri) = ri {
            if ri == global_idx {
                *self.reveal_index.borrow_mut() = None;
                *self.reveal_start.borrow_mut() = None;
            } else if ri > global_idx {
                *self.reveal_index.borrow_mut() = Some(ri - 1);
            }
        }
    }

    /// (name, category) of the secrets, in storage order: what the command
    /// palette lists and checks, without copying any value.
    pub fn keys(&self) -> Vec<(String, String)> {
        self.secrets.borrow().iter().map(|s| (s.name.clone(), s.category.clone())).collect()
    }

    /// Get a secret by global index.
    pub fn get_secret(&self, global_idx: usize) -> Option<SecretEntry> {
        self.secrets.borrow().get(global_idx).cloned()
    }

    /// Copy a secret's value to the clipboard. `None` when there is no such
    /// secret; otherwise the copied text (for clipboard guard tracking) or
    /// why the write failed (issue #31).
    pub fn copy_secret_value(&self, global_idx: usize) -> Option<Result<String, String>> {
        let secrets = self.secrets.borrow();
        let entry = secrets.get(global_idx)?;
        let value = entry.value.clone();
        use clipboard_win::{formats, set_clipboard};
        if let Err(e) = set_clipboard(formats::Unicode, &value) {
            return Some(Err(e.to_string()));
        }
        // Tell Windows to keep this entry out of Clipboard History
        // (Win+V) and Cloud Clipboard — they'd otherwise persist the
        // secret across reboots and sync it to other devices.
        crate::features::clipboard_guard::exclude_current_clipboard_entry_from_history();
        Some(Ok(value))
    }
}

// ── Tree view model (issue 048) ────────────────────────────────────────
//
// The panel shows a one-level tree: every category of `CATEGORIES`
// (always, even empty), sorted on its English key, with its secrets
// underneath sorted by name for the user's locale. Sorting is display
// only: the stored order (and so the saved payload) never changes. A leaf
// label is the secret name only — a value never goes into the TreeView.

#[derive(Debug, PartialEq, Eq)]
pub enum VaultTreeError {
    UnknownCategory { index: usize, category: String },
    BadParam(isize),
    MissingParam,
    Compare(String),
}

/// What is selected in the tree.
#[derive(Debug, PartialEq, Eq)]
pub enum TreeSel {
    Nothing,
    Category(usize),
    Leaf(usize),
}

/// Enabled state of the panel buttons.
#[derive(Debug, PartialEq, Eq)]
pub struct VaultButtons {
    pub add: bool,
    pub edit: bool,
    pub delete: bool,
    pub copy: bool,
    pub reveal: bool,
}

//// One category of the tree: its index in the category list (lParam of
/// the node), its name, and its secrets.
#[derive(Debug, PartialEq, Eq)]
pub struct TreeGroup {
    pub category_index: usize,
    pub name: String,
    pub secrets: Vec<usize>,
}

/// Stable insertion sort with a fallible comparator (no sort_by: the
/// system comparison can fail).
fn stable_sort_by<T, F>(items: &mut [T], key: impl Fn(&T) -> String, cmp: &F) -> Result<(), VaultTreeError>
where
    F: Fn(&str, &str) -> Result<std::cmp::Ordering, VaultTreeError>,
{
    for i in 1..items.len() {
        let mut j = i;
        while j > 0 && cmp(&key(&items[j - 1]), &key(&items[j]))? == std::cmp::Ordering::Greater {
            items.swap(j - 1, j);
            j -= 1;
        }
    }
    Ok(())
}

/// Every category of `categories` (even empty) sorted by `cmp`, with the
/// indices of its secrets sorted by `cmp` then by storage index (stable).
pub fn tree_groups<F>(
    secrets: &[SecretEntry],
    categories: &[String],
    cmp: F,
) -> Result<Vec<TreeGroup>, VaultTreeError>
where
    F: Fn(&str, &str) -> Result<std::cmp::Ordering, VaultTreeError>,
{
    let mut groups: Vec<TreeGroup> = categories
        .iter()
        .enumerate()
        .map(|(i, c)| TreeGroup { category_index: i, name: c.clone(), secrets: Vec::new() })
        .collect();
    for (index, s) in secrets.iter().enumerate() {
        let Some(g) = groups.iter_mut().find(|g| g.name == s.category) else {
            return Err(VaultTreeError::UnknownCategory { index, category: s.category.clone() });
        };
        g.secrets.push(index);
    }
    stable_sort_by(&mut groups, |g| g.name.clone(), &cmp)?;
    for g in groups.iter_mut() {
        stable_sort_by(&mut g.secrets, |&i| secrets[i].name.clone(), &cmp)?;
    }
    Ok(groups)
}

/// The tree for the search field: an empty query (spaces only) keeps every
/// category, empty ones too; otherwise only the secrets whose name contains
/// the query (`contains(name, query)`), and only the categories left with
/// one. Never the value (typing a password must not show which entry holds
/// it), nor the category name.
pub fn filter_groups<F>(
    groups: Vec<TreeGroup>,
    secrets: &[SecretEntry],
    query: &str,
    contains: &F,
) -> Result<Vec<TreeGroup>, VaultTreeError>
where
    F: Fn(&str, &str) -> Result<bool, VaultTreeError>,
{
    let query = query.trim();
    if query.is_empty() {
        return Ok(groups);
    }
    let mut kept = Vec::new();
    for mut g in groups {
        let mut matching = Vec::new();
        for i in g.secrets {
            if contains(&secrets[i].name, query)? {
                matching.push(i);
            }
        }
        if !matching.is_empty() {
            g.secrets = matching;
            kept.push(g);
        }
    }
    Ok(kept)
}

/// Whether the search `query` hides a secret named `name` (`filter_groups`):
/// a secret just added or edited must stay in view, so the search is cleared.
pub fn search_hides<F>(name: &str, query: &str, contains: &F) -> Result<bool, VaultTreeError>
where
    F: Fn(&str, &str) -> Result<bool, VaultTreeError>,
{
    let query = query.trim();
    Ok(!query.is_empty() && !contains(name, query)?)
}

/// `needle` found in `haystack` for the user's locale, ignoring case and
/// accents (the flags of `compare_names_user_locale`).
pub fn contains_name_user_locale(haystack: &str, needle: &str) -> Result<bool, VaultTreeError> {
    use windows::Win32::Foundation::{GetLastError, SetLastError, ERROR_SUCCESS, LPARAM};
    use windows::Win32::Globalization::{FindNLSStringEx, FIND_FROMSTART, LINGUISTIC_IGNORECASE, LINGUISTIC_IGNOREDIACRITIC};
    if needle.is_empty() {
        return Ok(true);
    }
    if haystack.is_empty() {
        return Ok(false);
    }
    let h: Vec<u16> = haystack.encode_utf16().collect();
    let n: Vec<u16> = needle.encode_utf16().collect();
    let flags = FIND_FROMSTART | LINGUISTIC_IGNORECASE.0 | LINGUISTIC_IGNOREDIACRITIC.0;
    // Null locale name = LOCALE_NAME_USER_DEFAULT. -1 is "not found" when the
    // last error is ERROR_SUCCESS, a failure otherwise.
    let found = unsafe {
        SetLastError(ERROR_SUCCESS);
        FindNLSStringEx(windows::core::PCWSTR::null(), flags, &h, &n, None, None, None, LPARAM(0))
    };
    if found >= 0 {
        return Ok(true);
    }
    let err = unsafe { GetLastError() };
    if err == ERROR_SUCCESS {
        Ok(false)
    } else {
        Err(VaultTreeError::Compare(format!("FindNLSStringEx failed: {:?}", err)))
    }
}

/// Leaf lParam: storage index + 1.
pub fn encode_leaf_param(index: usize) -> isize {
    index as isize + 1
}

/// Category node lParam: -(category index + 1). 0 is an inert node (error).
pub fn encode_category_param(index: usize) -> isize {
    -(index as isize) - 1
}

/// Decode the lParam of the selected item. `None` means the TreeView could
/// not be queried: an error, never silently "nothing".
pub fn decode_tree_param(p: Option<isize>, secrets_len: usize, categories_len: usize) -> Result<TreeSel, VaultTreeError> {
    match p {
        None => Err(VaultTreeError::MissingParam),
        Some(0) => Ok(TreeSel::Nothing),
        Some(v) if v > 0 && (v as usize) <= secrets_len => Ok(TreeSel::Leaf(v as usize - 1)),
        Some(v) if v < 0 && ((-v) as usize) <= categories_len => Ok(TreeSel::Category((-v) as usize - 1)),
        Some(v) => Err(VaultTreeError::BadParam(v)),
    }
}

/// Category index a dragged secret lands in (issue 053): the category node
/// itself, or the category of the secret it is dropped on. `None` = no
/// valid target (drop cancelled).
pub fn drop_category(target: &TreeSel, secrets: &[SecretEntry], categories: &[String]) -> Option<usize> {
    match *target {
        TreeSel::Category(k) if k < categories.len() => Some(k),
        TreeSel::Leaf(i) => {
            let cat = &secrets.get(i)?.category;
            categories.iter().position(|c| c == cat)
        }
        _ => None,
    }
}

/// Move a secret to `category`. `Ok(false)` when it is already there.
pub fn move_secret(secrets: &mut [SecretEntry], index: usize, category: &str) -> Result<bool, VaultTreeError> {
    let Some(s) = secrets.get_mut(index) else {
        return Err(VaultTreeError::BadParam(index as isize));
    };
    if s.category == category {
        return Ok(false);
    }
    s.category = category.to_string();
    Ok(true)
}

/// Only Add works without a secret selected.
pub fn button_states(sel: &TreeSel) -> VaultButtons {
    let leaf = matches!(sel, TreeSel::Leaf(_));
    VaultButtons { add: true, edit: leaf, delete: leaf, copy: leaf, reveal: leaf }
}

/// Storage index of the selected secret (what Copy, Edit, Reveal act on): a secret
/// only; a category or an empty selection gives none.
pub fn secret_index(sel: &TreeSel) -> Option<usize> {
    match *sel {
        TreeSel::Leaf(i) => Some(i),
        TreeSel::Category(_) | TreeSel::Nothing => None,
    }
}

/// "Copied" indicator text: the template's `{name}` replaced by the secret
/// name (never its value).
pub fn copied_message(template: &str, name: &str) -> String {
    template.replace("{name}", name)
}

/// Label of a leaf: the secret name only, revealed or not.
pub fn leaf_label(entry: &SecretEntry) -> String {
    entry.name.clone()
}

/// User-facing message for a refused category operation (issue 052).
pub fn category_error_message(e: &crate::features::vault::categories::CategoryError) -> String {
    use crate::features::i18n::get;
    use crate::features::vault::categories::{CategoryError as E, MAX_CATEGORIES, MAX_NAME_CHARS};
    match e {
        E::Empty => get("vault.cat.err.empty").to_string(),
        E::TooLong => get("vault.cat.err.toolong").replace("{max}", &MAX_NAME_CHARS.to_string()),
        E::ForbiddenChar(c) => {
            let shown = match c {
                '\n' | '\r' => "\\n".to_string(),
                '\0' => "\\0".to_string(),
                c => c.to_string(),
            };
            get("vault.cat.err.char").replace("{char}", &shown)
        }
        E::Duplicate(name) => get("vault.cat.err.duplicate").replace("{name}", name),
        E::TooMany => get("vault.cat.err.toomany").replace("{max}", &MAX_CATEGORIES.to_string()),
        E::GeneralIsFixed => get("vault.cat.err.general").to_string(),
        E::NotFound(name) => get("vault.cat.err.notfound").replace("{name}", name),
        E::Compare(msg) => get("vault.cat.err.compare").replace("{msg}", msg),
    }
}

/// Category duplicate check (issue 052): same name for the user's locale,
/// ignoring case and accents.
pub fn same_category_name(a: &str, b: &str) -> Result<bool, crate::features::vault::categories::CategoryError> {
    match compare_names_user_locale(a, b) {
        Ok(o) => Ok(o == std::cmp::Ordering::Equal),
        Err(e) => Err(crate::features::vault::categories::CategoryError::Compare(format!("{:?}", e))),
    }
}

/// Sort key of `s` for the user's locale, ignoring case and accents. Two
/// keys compare as bytes the way compare_names_user_locale compares the
/// strings (same flags), so a long list is sorted with one LCMapStringEx call
/// per item instead of one CompareStringEx call per comparison.
pub fn sort_key_user_locale(s: &str) -> Result<Vec<u8>, VaultTreeError> {
    #[cfg(test)]
    tests_locale_calls::SORT_KEYS.with(|c| c.set(c.get() + 1));
    use windows::Win32::Foundation::LPARAM;
    use windows::Win32::Globalization::{
        LCMapStringEx, LCMAP_SORTKEY, LINGUISTIC_IGNORECASE, LINGUISTIC_IGNOREDIACRITIC,
    };
    let w: Vec<u16> = s.encode_utf16().collect();
    // LCMapStringEx refuses an empty source; CompareStringEx orders the
    // empty string first, as the empty key does.
    if w.is_empty() {
        return Ok(Vec::new());
    }
    let flags = LCMAP_SORTKEY | LINGUISTIC_IGNORECASE.0 | LINGUISTIC_IGNOREDIACRITIC.0;
    let failed = || {
        VaultTreeError::Compare(format!(
            "LCMapStringEx failed: {:?}",
            windows::core::Error::from_thread()
        ))
    };
    // Null locale name = LOCALE_NAME_USER_DEFAULT. With LCMAP_SORTKEY the
    // sizes are in bytes and the key is a byte string.
    let needed = unsafe { LCMapStringEx(windows::core::PCWSTR::null(), flags, &w, None, None, None, LPARAM(0)) };
    if needed <= 0 {
        return Err(failed());
    }
    // windows-rs passes the slice length as the byte size: `needed` u16
    // elements give the `needed` bytes the call asked for.
    let mut buf = vec![0u16; needed as usize];
    let written = unsafe {
        LCMapStringEx(windows::core::PCWSTR::null(), flags, &w, Some(&mut buf), None, None, LPARAM(0))
    };
    if written <= 0 || written > needed {
        return Err(failed());
    }
    Ok(buf.iter().flat_map(|u| u.to_ne_bytes()).take(written as usize).collect())
}

/// Calls of sort_key_user_locale and compare_names_user_locale made by the
/// current thread, tests only: the 100k-line sort test checks the menu path
/// does one LCMapStringEx key per line and no CompareStringEx (#40).
#[cfg(test)]
pub(crate) mod tests_locale_calls {
    thread_local! {
        pub(crate) static SORT_KEYS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
        pub(crate) static COMPARES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    }
    /// (sort keys, comparisons) made by this thread so far.
    pub(crate) fn snapshot() -> (u64, u64) {
        (SORT_KEYS.with(|c| c.get()), COMPARES.with(|c| c.get()))
    }
}

/// Compare two names with the user's locale, ignoring case and accents.
pub fn compare_names_user_locale(a: &str, b: &str) -> Result<std::cmp::Ordering, VaultTreeError> {
    #[cfg(test)]
    tests_locale_calls::COMPARES.with(|c| c.set(c.get() + 1));
    use windows::Win32::Globalization::{
        CompareStringEx, CSTR_EQUAL, CSTR_GREATER_THAN, CSTR_LESS_THAN, LINGUISTIC_IGNORECASE,
        LINGUISTIC_IGNOREDIACRITIC,
    };
    let wa: Vec<u16> = a.encode_utf16().collect();
    let wb: Vec<u16> = b.encode_utf16().collect();
    // Null locale name = LOCALE_NAME_USER_DEFAULT.
    let r = unsafe {
        CompareStringEx(
            windows::core::PCWSTR::null(),
            LINGUISTIC_IGNORECASE | LINGUISTIC_IGNOREDIACRITIC,
            &wa,
            &wb,
            None,
            None,
            None,
        )
    };
    match r {
        CSTR_LESS_THAN => Ok(std::cmp::Ordering::Less),
        CSTR_EQUAL => Ok(std::cmp::Ordering::Equal),
        CSTR_GREATER_THAN => Ok(std::cmp::Ordering::Greater),
        _ => Err(VaultTreeError::Compare(format!(
            "CompareStringEx failed: {:?}",
            windows::core::Error::from_thread()
        ))),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn sort_keys_order_like_compare_names_user_locale() {
        let names = [
            "", "a", "A", "\u{e0}", "b", "B", "e", "\u{e9}", "E", "f", "Zebra", "z\u{e8}bre", "10", "9",
            "a b", "a-b", "ab", "oeuf", "\u{153}uf", "ss", "\u{df}", "\u{1F600}", "\u{65e5}\u{672c}", " lead",
        ];
        for a in names {
            for b in names {
                let by_compare = compare_names_user_locale(a, b).expect("compare");
                let by_key = sort_key_user_locale(a).expect("key a").cmp(&sort_key_user_locale(b).expect("key b"));
                assert_eq!(by_key, by_compare, "{a:?} vs {b:?}");
            }
        }
    }

    use super::*;
    use crate::features::vault::CATEGORIES;

    #[test]
    fn edit_after_the_dialog_needs_the_same_secret() {
        let panel = VaultPanel::new();
        let original = SecretEntry::new("gmail", "pw", "Email");
        panel.add_secret(original.clone());
        assert!(panel.update_secret_if_unchanged(0, &original, SecretEntry::new("gmail", "pw2", "Email")));
        assert!(!panel.update_secret_if_unchanged(0, &original, SecretEntry::new("x", "x", "General")));
        assert_eq!(panel.get_secret(0), Some(SecretEntry::new("gmail", "pw2", "Email")));
        assert!(!panel.update_secret_if_unchanged(5, &original, SecretEntry::new("x", "x", "General")));
    }

    #[test]
    fn keys_list_every_secret_in_storage_order() {
        let panel = VaultPanel::new();
        assert!(panel.keys().is_empty());
        panel.add_secret(SecretEntry::new("gmail", "pw", "Email"));
        panel.add_secret(SecretEntry::new("aws", "key", "Cloud"));
        assert_eq!(panel.keys(), [("gmail".to_string(), "Email".to_string()), ("aws".to_string(), "Cloud".to_string())]);
    }

    fn raw(name: &str, category: &str) -> SecretEntry {
        let mut e = SecretEntry::new(name, "v", "General");
        e.category = category.to_string();
        e
    }

    fn names(secrets: &[SecretEntry], idx: &[usize]) -> Vec<String> {
        idx.iter().map(|&i| secrets[i].name.clone()).collect()
    }

    fn defaults() -> Vec<String> {
        CATEGORIES.iter().map(|c| c.to_string()).collect()
    }

    fn group<'a>(g: &'a [TreeGroup], cat: &str) -> &'a [usize] {
        &g.iter().find(|x| x.name == cat).expect(cat).secrets
    }

    fn groups(s: &[SecretEntry]) -> Vec<TreeGroup> {
        tree_groups(s, &defaults(), compare_names_user_locale).unwrap()
    }

    // ── Search field ──

    fn filtered(s: &[SecretEntry], query: &str) -> Vec<(String, Vec<String>)> {
        filter_groups(groups(s), s, query, &contains_name_user_locale)
            .unwrap()
            .into_iter()
            .map(|g| (g.name.clone(), names(s, &g.secrets)))
            .collect()
    }

    #[test]
    fn an_empty_search_shows_the_whole_tree() {
        let s = vec![raw("gmail", "Email"), raw("aws", "Cloud")];
        for q in ["", "   "] {
            let g = filter_groups(groups(&s), &s, q, &contains_name_user_locale).unwrap();
            assert_eq!(g, groups(&s), "query {:?}: every category, empty ones too", q);
        }
    }

    #[test]
    fn the_search_keeps_matching_names_and_their_categories_only() {
        let s = vec![
            raw("Gmail perso", "Email"),
            raw("aws", "Cloud"),
            raw("GMAIL pro", "Email"),
            raw("Émile banque", "Finance"),
        ];
        assert_eq!(
            filtered(&s, " gmail "),
            vec![("Email".to_string(), vec!["Gmail perso".to_string(), "GMAIL pro".to_string()])]
        );
        // Accents and case ignored, anywhere in the name.
        assert_eq!(filtered(&s, "EMILE"), vec![("Finance".to_string(), vec!["Émile banque".to_string()])]);
        assert!(filtered(&s, "nothing like it").is_empty());
    }

    #[test]
    fn the_search_never_looks_at_the_secret_value_or_the_category() {
        // Typing a password must not reveal which entry holds it.
        let s = vec![SecretEntry::new("gmail", "hunter2", "Email")];
        assert!(filtered(&s, "hunter2").is_empty());
        assert!(filtered(&s, "Email").is_empty());
    }

    #[test]
    fn a_secret_just_saved_that_the_search_hides_clears_the_search() {
        let c = &contains_name_user_locale;
        assert_eq!(search_hides("Gmail pro", "aws", c), Ok(true));
        assert_eq!(search_hides("Gmail pro", "GMAIL", c), Ok(false));
        assert_eq!(search_hides("Gmail pro", "  ", c), Ok(false), "no search, nothing hidden");
    }

    #[test]
    fn a_failed_comparison_is_an_error_not_an_empty_tree() {
        let s = vec![raw("gmail", "Email")];
        let fails = |_: &str, _: &str| -> Result<bool, VaultTreeError> { Err(VaultTreeError::Compare("boom".into())) };
        assert_eq!(filter_groups(groups(&s), &s, "g", &fails), Err(VaultTreeError::Compare("boom".into())));
    }

    #[test]
    fn seven_categories_even_empty_sorted() {
        let g = groups(&[]);
        let cats: Vec<&str> = g.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(cats, vec!["API Keys", "Cloud", "Crypto", "Email", "Finance", "General", "Passwords"]);
        assert!(g.iter().all(|x| x.secrets.is_empty()));
    }

    #[test]
    fn category_index_points_into_the_list() {
        let list = defaults();
        for x in groups(&[]) {
            assert_eq!(list[x.category_index], x.name);
        }
    }

    #[test]
    fn custom_categories_sorted_by_locale() {
        let list: Vec<String> = ["General", "zèbre", "Banque perso", "émeraude"].iter().map(|s| s.to_string()).collect();
        let g = tree_groups(&[], &list, compare_names_user_locale).unwrap();
        let cats: Vec<&str> = g.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(cats, vec!["Banque perso", "émeraude", "General", "zèbre"]);
    }

    #[test]
    fn each_secret_under_its_category() {
        let s = vec![raw("a", "Email"), raw("b", "Cloud"), raw("c", "Email")];
        let g = groups(&s);
        assert_eq!(group(&g, "Email"), &[0, 2]);
        assert_eq!(group(&g, "Cloud"), &[1]);
    }

    #[test]
    fn sort_case_insensitive() {
        let s = vec![raw("beta", "General"), raw("Alpha", "General")];
        assert_eq!(names(&s, group(&groups(&s), "General")), vec!["Alpha", "beta"]);
    }

    #[test]
    fn sort_accents() {
        let s = vec![raw("Émile", "General"), raw("alpha", "General"), raw("emile", "General"), raw("Zoe", "General")];
        assert_eq!(names(&s, group(&groups(&s), "General")), vec!["alpha", "Émile", "emile", "Zoe"]);
    }

    #[test]
    fn duplicate_names_stable() {
        let s = vec![raw("x", "General"), raw("a", "General"), raw("x", "General")];
        assert_eq!(group(&groups(&s), "General"), &[1, 0, 2]);
    }

    #[test]
    fn sort_does_not_touch_storage() {
        let s = vec![raw("b", "General"), raw("a", "General")];
        let before = crate::features::vault::combine("note", &s);
        let _ = groups(&s);
        assert_eq!(crate::features::vault::combine("note", &s), before);
        assert_eq!(s[0].name, "b");
    }

    #[test]
    fn unknown_category_is_error() {
        let s = vec![raw("a", "General"), raw("b", "Foo")];
        assert_eq!(
            tree_groups(&s, &defaults(), compare_names_user_locale),
            Err(VaultTreeError::UnknownCategory { index: 1, category: "Foo".into() })
        );
    }

    #[test]
    fn compare_error_propagates() {
        let s = vec![raw("a", "General"), raw("b", "General")];
        let failing = |_: &str, _: &str| Err(VaultTreeError::Compare("boom".into()));
        assert_eq!(tree_groups(&s, &defaults(), failing), Err(VaultTreeError::Compare("boom".into())));
    }

    #[test]
    fn param_roundtrip() {
        for i in [0usize, 1, 41] {
            assert_eq!(decode_tree_param(Some(encode_leaf_param(i)), 42, 7), Ok(TreeSel::Leaf(i)));
        }
        for k in [0usize, 6] {
            assert_eq!(decode_tree_param(Some(encode_category_param(k)), 3, 7), Ok(TreeSel::Category(k)));
        }
        assert_eq!(decode_tree_param(Some(0), 3, 7), Ok(TreeSel::Nothing));
        assert_eq!(decode_tree_param(None, 3, 7), Err(VaultTreeError::MissingParam));
        assert_eq!(decode_tree_param(Some(4), 3, 7), Err(VaultTreeError::BadParam(4)));
        assert_eq!(decode_tree_param(Some(-8), 3, 7), Err(VaultTreeError::BadParam(-8)));
    }

    #[test]
    fn buttons_by_selection() {
        let only_add = VaultButtons { add: true, edit: false, delete: false, copy: false, reveal: false };
        assert_eq!(button_states(&TreeSel::Nothing), only_add);
        assert_eq!(button_states(&TreeSel::Category(0)), only_add);
        assert_eq!(
            button_states(&TreeSel::Leaf(0)),
            VaultButtons { add: true, edit: true, delete: true, copy: true, reveal: true }
        );
    }

    #[test]
    fn selecting_a_secret_copies_it_but_not_a_category() {
        assert_eq!(secret_index(&TreeSel::Leaf(3)), Some(3));
        assert_eq!(secret_index(&TreeSel::Category(1)), None);
        assert_eq!(secret_index(&TreeSel::Nothing), None);
    }

    #[test]
    fn copied_message_has_name_only() {
        assert_eq!(copied_message("Copied: {name}", "Gmail"), "Copied: Gmail");
    }

    #[test]
    fn drop_on_category_or_secret() {
        let cats = defaults(); // General at 0, Email at 1, Cloud at 2
        let s = vec![raw("a", "Email"), raw("b", "Cloud")];
        assert_eq!(drop_category(&TreeSel::Category(2), &s, &cats), Some(2));
        assert_eq!(drop_category(&TreeSel::Leaf(1), &s, &cats), Some(2)); // onto a Cloud secret
        assert_eq!(drop_category(&TreeSel::Nothing, &s, &cats), None);
        assert_eq!(drop_category(&TreeSel::Category(99), &s, &cats), None);
        assert_eq!(drop_category(&TreeSel::Leaf(99), &s, &cats), None);
    }

    #[test]
    fn move_secret_between_categories() {
        let mut s = vec![raw("a", "Email"), raw("b", "Cloud")];
        assert_eq!(move_secret(&mut s, 0, "Cloud"), Ok(true));
        assert_eq!(s[0].category, "Cloud");
        assert_eq!(s[0].value, "v");
        assert_eq!(move_secret(&mut s, 0, "Cloud"), Ok(false)); // same category: no-op
        assert_eq!(move_secret(&mut s, 5, "Cloud"), Err(VaultTreeError::BadParam(5)));
    }

    #[test]
    fn category_errors_have_messages() {
        use crate::features::vault::categories::CategoryError as E;
        let _g = crate::features::i18n::test_support::lang_guard();
        crate::features::i18n::set_language(crate::features::i18n::Language::English);
        for e in [E::Empty, E::TooLong, E::ForbiddenChar(':'), E::Duplicate("Email".into()), E::TooMany,
                  E::GeneralIsFixed, E::NotFound("X".into()), E::Compare("x".into())] {
            let m = category_error_message(&e);
            assert!(!m.starts_with('['), "missing i18n key for {:?}: {}", e, m);
        }
        assert!(category_error_message(&E::Duplicate("Email".into())).contains("Email"));
        crate::features::i18n::set_language(crate::features::i18n::Language::French);
        assert!(!category_error_message(&E::GeneralIsFixed).starts_with('['));
    }

    #[test]
    fn same_name_ignores_case_and_accents() {
        assert_eq!(same_category_name("Émeraude", "emeraude"), Ok(true));
        assert_eq!(same_category_name("Banque", "banque"), Ok(true));
        assert_eq!(same_category_name("Banque", "Bank"), Ok(false));
    }

    #[test]
    fn leaf_label_never_contains_value() {
        let panel = VaultPanel::new();
        panel.set_secrets(vec![SecretEntry::new("Gmail", "pass123", "Email")]);
        panel.reveal(0);
        let label = leaf_label(&panel.get_secret(0).unwrap());
        assert_eq!(label, "Gmail");
        assert!(!label.contains("pass123"));
    }

    fn sample_secrets() -> Vec<SecretEntry> {
        vec![
            SecretEntry::new("Gmail", "pass123", "Email"),
            SecretEntry::new("AWS Key", "AKIA...", "Cloud"),
            SecretEntry::new("Bank PIN", "1234", "Finance"),
        ]
    }

    #[test]
    fn new_panel_is_hidden() {
        let panel = VaultPanel::new();
        assert!(!panel.is_visible());
        assert_eq!(panel.count(), 0);
    }

    #[test]
    fn toggle_visibility() {
        let panel = VaultPanel::new();
        panel.toggle_visible();
        assert!(panel.is_visible());
        panel.toggle_visible();
        assert!(!panel.is_visible());
    }

    #[test]
    fn set_and_get_secrets() {
        let panel = VaultPanel::new();
        panel.set_secrets(sample_secrets());
        assert_eq!(panel.count(), 3);
        let secrets = panel.get_secrets();
        assert_eq!(secrets[0].name, "Gmail");
    }

    #[test]
    fn reveal_not_expired_immediately() {
        let panel = VaultPanel::new();
        panel.set_secrets(sample_secrets());
        panel.reveal(0);
        assert!(!panel.check_reveal_expired());
    }

    #[test]
    fn add_secret() {
        let panel = VaultPanel::new();
        panel.set_secrets(sample_secrets());
        panel.add_secret(SecretEntry::new("New", "val", "General"));
        assert_eq!(panel.count(), 4);
    }

    #[test]
    fn delete_secret() {
        let panel = VaultPanel::new();
        panel.set_secrets(sample_secrets());
        panel.delete_secret(1);
        assert_eq!(panel.count(), 2);
        let secrets = panel.get_secrets();
        assert_eq!(secrets[0].name, "Gmail");
        assert_eq!(secrets[1].name, "Bank PIN");
    }

}
