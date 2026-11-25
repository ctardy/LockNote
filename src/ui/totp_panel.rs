// TotpPanel — Right-side panel for TOTP authenticator codes
//
// Shows a list of TOTP entries with live codes and countdown.
// Provides Copy, Add, and Remove actions.

use std::cell::RefCell;

use crate::features::totp::{self, TotpEntry};

/// Default width of the TOTP panel in pixels (resizable: `ui::side_panel`).
pub const TOTP_PANEL_WIDTH: u32 = 280;

/// TotpPanel holds the list of TOTP entries and panel visibility state.
pub struct TotpPanel {
    /// All TOTP entries.
    entries: RefCell<Vec<TotpEntry>>,
    /// Whether the panel is currently visible.
    visible: RefCell<bool>,
}

impl TotpPanel {
    /// Create a new TOTP panel (initially hidden).
    pub fn new() -> Self {
        TotpPanel {
            entries: RefCell::new(Vec::new()),
            visible: RefCell::new(false),
        }
    }

    /// Whether the TOTP panel is visible.
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

    /// Load entries into the panel.
    pub fn set_entries(&self, entries: Vec<TotpEntry>) {
        *self.entries.borrow_mut() = entries;
    }

    /// Get all entries.
    pub fn get_entries(&self) -> Vec<TotpEntry> {
        self.entries.borrow().clone()
    }

    /// Get the number of entries.
    pub fn count(&self) -> usize {
        self.entries.borrow().len()
    }

    /// Add a new entry.
    pub fn add_entry(&self, entry: TotpEntry) {
        self.entries.borrow_mut().push(entry);
    }

    /// Remove an entry by index.
    pub fn remove_entry(&self, index: usize) {
        let mut entries = self.entries.borrow_mut();
        if index < entries.len() {
            entries.remove(index);
        }
    }

    /// Get a specific entry by index.
    pub fn get_entry(&self, index: usize) -> Option<TotpEntry> {
        self.entries.borrow().get(index).cloned()
    }

    /// Format an entry for display: "Name  —  CODE"
    pub fn format_entry(entry: &TotpEntry) -> String {
        let code = totp::generate_code_now(&entry.secret)
            .unwrap_or_else(|| "------".to_string());
        format!("{}  \u{2014}  {}", entry.name, code)
    }

    /// Get the formatted list for the ListBox (with live codes).
    pub fn formatted_list(&self) -> Vec<String> {
        self.entries.borrow().iter().map(Self::format_entry).collect()
    }

    /// Get the countdown text.
    pub fn countdown_text(&self) -> String {
        let remaining = totp::remaining_seconds();
        format!("Next code in {} s", remaining)
    }

    /// Copy a TOTP code to the clipboard.
    /// Returns the entry name if successful.
    pub fn copy_code(&self, index: usize) -> Option<String> {
        let entries = self.entries.borrow();
        if let Some(entry) = entries.get(index) {
            if let Some(code) = totp::generate_code_now(&entry.secret) {
                use clipboard_win::{formats, set_clipboard};
                let _ = set_clipboard(formats::Unicode, &code);
                return Some(entry.name.clone());
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entries() -> Vec<TotpEntry> {
        vec![
            TotpEntry { name: "GitHub".to_string(), secret: "JBSWY3DPEHPK3PXP".to_string() },
            TotpEntry { name: "AWS".to_string(), secret: "JBSWY3DPEHPK3PXP".to_string() },
        ]
    }

    #[test]
    fn new_panel_is_hidden() {
        let panel = TotpPanel::new();
        assert!(!panel.is_visible());
        assert_eq!(panel.count(), 0);
    }

    #[test]
    fn toggle_visibility() {
        let panel = TotpPanel::new();
        panel.toggle_visible();
        assert!(panel.is_visible());
        panel.toggle_visible();
        assert!(!panel.is_visible());
    }

    #[test]
    fn set_and_get_entries() {
        let panel = TotpPanel::new();
        panel.set_entries(sample_entries());
        assert_eq!(panel.count(), 2);
        let entries = panel.get_entries();
        assert_eq!(entries[0].name, "GitHub");
    }

    #[test]
    fn add_entry() {
        let panel = TotpPanel::new();
        panel.add_entry(TotpEntry { name: "Test".to_string(), secret: "AAAA".to_string() });
        assert_eq!(panel.count(), 1);
    }

    #[test]
    fn remove_entry() {
        let panel = TotpPanel::new();
        panel.set_entries(sample_entries());
        panel.remove_entry(0);
        assert_eq!(panel.count(), 1);
        assert_eq!(panel.get_entries()[0].name, "AWS");
    }

    #[test]
    fn formatted_list_has_correct_count() {
        let panel = TotpPanel::new();
        panel.set_entries(sample_entries());
        let list = panel.formatted_list();
        assert_eq!(list.len(), 2);
        assert!(list[0].contains("GitHub"));
    }

    #[test]
    fn set_visible() {
        let panel = TotpPanel::new();
        panel.set_visible(true);
        assert!(panel.is_visible());
        panel.set_visible(false);
        assert!(!panel.is_visible());
    }

    #[test]
    fn get_entry_valid_index() {
        let panel = TotpPanel::new();
        panel.set_entries(sample_entries());
        let entry = panel.get_entry(0);
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().name, "GitHub");
    }

    #[test]
    fn get_entry_out_of_bounds() {
        let panel = TotpPanel::new();
        assert!(panel.get_entry(99).is_none());
    }

    #[test]
    fn remove_entry_out_of_bounds_no_panic() {
        let panel = TotpPanel::new();
        panel.set_entries(sample_entries());
        panel.remove_entry(99);
        assert_eq!(panel.count(), 2);
    }

    #[test]
    fn format_entry_contains_dash() {
        let entry = TotpEntry { name: "Test".to_string(), secret: "JBSWY3DPEHPK3PXP".to_string() };
        let formatted = TotpPanel::format_entry(&entry);
        assert!(formatted.contains("Test"));
        assert!(formatted.contains("\u{2014}")); // em dash
    }

    #[test]
    fn countdown_text_format() {
        let panel = TotpPanel::new();
        let text = panel.countdown_text();
        assert!(text.contains("s"));
        assert!(text.contains("Next code in"));
    }

    #[test]
    fn set_entries_replaces_previous() {
        let panel = TotpPanel::new();
        panel.set_entries(sample_entries());
        assert_eq!(panel.count(), 2);
        panel.set_entries(vec![TotpEntry { name: "Only".to_string(), secret: "AAAA".to_string() }]);
        assert_eq!(panel.count(), 1);
        assert_eq!(panel.get_entries()[0].name, "Only");
    }
}
