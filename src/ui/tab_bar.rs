// TabBar — Custom tab strip for multi-note support
//
// Since NWG does not have a native tab strip control like WinForms TabControl,
// this implements a row of clickable tab buttons using a simple approach:
// a horizontal container with dynamically created buttons.
//
// Properties:
//   - Height: 30px, docked at top
//   - Tab width: min 80px, max 200px (dynamic based on text width)
//   - Close button: 16x16px, visible on hover or active tab
//   - Add button (+): 28px wide
//   - Modified indicator: bullet character in title
//   - Context menu (right-click): Rename, Color submenu, Close

use std::cell::RefCell;

/// Represents a single tab in the tab bar.
#[derive(Clone, Debug)]
pub struct Tab {
    /// Display name of the tab.
    pub name: String,
    /// Whether this tab has unsaved changes.
    pub is_modified: bool,
    /// Optional color tint name (e.g., "blue", "red").
    pub color: Option<String>,
    /// The note content for this tab.
    pub content: String,
    /// The content as of the last load or save, in memory only (Tab >
    /// Compare with Last Save); empty for a tab never saved.
    pub saved_content: String,
}

impl Tab {
    pub fn new(name: &str) -> Self {
        Tab {
            name: name.to_string(),
            is_modified: false,
            color: None,
            content: String::new(),
            saved_content: String::new(),
        }
    }

    /// Returns the display text for the tab button.
    /// Prepends a bullet if modified.
    pub fn display_text(&self) -> String {
        if self.is_modified {
            format!("\u{2022} {}", self.name)
        } else {
            self.name.clone()
        }
    }

    /// Zeroize the note content buffer in place (keep name/color — they are
    /// not plaintext secrets and are reused on unlock to restore the UI).
    pub fn wipe_content(&mut self) {
        use zeroize::Zeroize;
        self.content.zeroize();
        self.saved_content.zeroize();
    }
}

/// Refused tab move (indices outside the tab list).
#[derive(Debug, PartialEq, Eq)]
pub enum TabMoveError {
    OutOfRange { from: usize, to: usize, len: usize },
}

/// Tab bar state.
pub struct TabBar {
    /// All open tabs.
    tabs: RefCell<Vec<Tab>>,
    /// Index of the currently active tab.
    active_index: RefCell<usize>,
    /// Height of the tab bar in pixels.
    pub height: u32,
}

/// Height of the tab bar in pixels.
const TAB_BAR_HEIGHT: u32 = crate::ui::tab_layout::TAB_STRIP_H as u32;
/// Width of the add (+) button in pixels.
const ADD_BUTTON_WIDTH: u32 = 28;
/// Width of the close (x) button in pixels.
const CLOSE_BUTTON_SIZE: u32 = 16;
/// Maximum number of tabs allowed.
pub const MAX_TABS: usize = 20;

impl TabBar {
    /// Create a new tab bar with one default tab.
    pub fn new() -> Self {
        let default_tab = Tab::new("Note 1");
        TabBar {
            tabs: RefCell::new(vec![default_tab]),
            active_index: RefCell::new(0),
            height: TAB_BAR_HEIGHT,
        }
    }

    /// Wipe the plaintext `content` of every tab. Used at auto-lock so a
    /// memory dump captured after the lock overlay appears does not still
    /// contain every note the user had open.
    pub fn wipe_contents(&self) {
        let mut tabs = self.tabs.borrow_mut();
        for t in tabs.iter_mut() {
            t.wipe_content();
        }
    }

    /// Create a tab bar with the given tabs. If empty, creates a default tab.
    pub fn with_tabs(tabs: Vec<Tab>) -> Self {
        let tabs = if tabs.is_empty() {
            vec![Tab::new("Note 1")]
        } else {
            tabs
        };
        TabBar {
            tabs: RefCell::new(tabs),
            active_index: RefCell::new(0),
            height: TAB_BAR_HEIGHT,
        }
    }

    /// Get the number of tabs.
    pub fn count(&self) -> usize {
        self.tabs.borrow().len()
    }

    /// Get the active tab index.
    pub fn active_index(&self) -> usize {
        *self.active_index.borrow()
    }

    /// Set the active tab by index.
    pub fn set_active(&self, index: usize) {
        let count = self.tabs.borrow().len();
        if index < count {
            *self.active_index.borrow_mut() = index;
        }
    }

    /// Get a clone of the active tab.
    pub fn active_tab(&self) -> Tab {
        let tabs = self.tabs.borrow();
        let idx = (*self.active_index.borrow()).min(tabs.len().saturating_sub(1));
        tabs[idx].clone()
    }

    /// Add a new tab and switch to it. Returns the new tab's index.
    pub fn add_tab(&self, name: &str) -> usize {
        let mut tabs = self.tabs.borrow_mut();
        let new_tab = Tab::new(name);
        tabs.push(new_tab);
        let new_index = tabs.len() - 1;
        *self.active_index.borrow_mut() = new_index;
        new_index
    }

    /// Close the tab at the given index. Returns false if it's the last tab.
    pub fn close_tab(&self, index: usize) -> bool {
        let mut tabs = self.tabs.borrow_mut();
        if tabs.len() <= 1 {
            return false; // Cannot close the last tab
        }
        if index >= tabs.len() {
            return false;
        }
        tabs.remove(index);
        let mut active = self.active_index.borrow_mut();
        if *active >= tabs.len() {
            *active = tabs.len() - 1;
        } else if *active > index {
            *active -= 1;
        }
        true
    }

    /// Move the tab at `from` so it ends up at index `to` (drag & drop).
    /// The active tab stays the same tab. `Ok(false)` when `from == to`.
    pub fn move_tab(&self, from: usize, to: usize) -> Result<bool, TabMoveError> {
        let mut tabs = self.tabs.borrow_mut();
        let len = tabs.len();
        if from >= len || to >= len {
            return Err(TabMoveError::OutOfRange { from, to, len });
        }
        if from == to {
            return Ok(false);
        }
        let mut active = self.active_index.borrow_mut();
        let tab = tabs.remove(from);
        tabs.insert(to, tab);
        // Follow the active tab through the shift.
        *active = if *active == from {
            to
        } else if from < *active && *active <= to {
            *active - 1
        } else if to <= *active && *active < from {
            *active + 1
        } else {
            *active
        };
        Ok(true)
    }

    /// Rename the tab at the given index.
    pub fn rename_tab(&self, index: usize, new_name: &str) {
        let mut tabs = self.tabs.borrow_mut();
        if index < tabs.len() {
            tabs[index].name = new_name.to_string();
        }
    }

    /// Set the color of the tab at the given index.
    pub fn set_tab_color(&self, index: usize, color: Option<&str>) {
        let mut tabs = self.tabs.borrow_mut();
        if index < tabs.len() {
            tabs[index].color = color.map(|s| s.to_string());
        }
    }

    /// Mark the tab at the given index as modified or clean.
    pub fn set_tab_modified(&self, index: usize, modified: bool) {
        let mut tabs = self.tabs.borrow_mut();
        if index < tabs.len() {
            tabs[index].is_modified = modified;
        }
    }

    /// Update the content of the tab at the given index.
    pub fn set_tab_content(&self, index: usize, content: &str) {
        let mut tabs = self.tabs.borrow_mut();
        if index < tabs.len() {
            tabs[index].content = content.to_string();
        }
    }

    /// Get the content of the tab at the given index.
    pub fn get_tab_content(&self, index: usize) -> String {
        let tabs = self.tabs.borrow();
        if index < tabs.len() {
            tabs[index].content.clone()
        } else {
            String::new()
        }
    }

    /// The content of the tab at `index` as of the last load or save.
    pub fn get_saved_content(&self, index: usize) -> Option<String> {
        self.tabs.borrow().get(index).map(|t| t.saved_content.clone())
    }

    /// After a successful save: every tab's content is the saved one.
    pub fn mark_all_saved(&self) {
        for t in self.tabs.borrow_mut().iter_mut() {
            t.saved_content.clone_from(&t.content);
        }
    }

    /// Switch to the next tab (wraps around).
    pub fn next_tab(&self) {
        let count = self.tabs.borrow().len();
        if count > 1 {
            let mut active = self.active_index.borrow_mut();
            *active = (*active + 1) % count;
        }
    }

    /// Switch to the previous tab (wraps around).
    pub fn prev_tab(&self) {
        let count = self.tabs.borrow().len();
        if count > 1 {
            let mut active = self.active_index.borrow_mut();
            *active = if *active == 0 { count - 1 } else { *active - 1 };
        }
    }

    /// Get all tabs as a vector of clones (for rendering).
    pub fn all_tabs(&self) -> Vec<Tab> {
        self.tabs.borrow().clone()
    }

    /// The tabs as a save or a backup writes them (name, text, color), built
    /// from the tabs in place: one copy of each text, wiped by the caller
    /// (issue #42: `all_tabs` cloned every tab, its saved text included, and
    /// the clone was dropped unwiped). `saved_content` stays empty: neither
    /// writes it.
    pub fn note_tabs(&self) -> Vec<crate::features::tabs::NoteTab> {
        self.tabs
            .borrow()
            .iter()
            .map(|t| crate::features::tabs::NoteTab {
                name: t.name.clone(),
                content: t.content.clone(),
                saved_content: String::new(),
                color: t.color.clone(),
            })
            .collect()
    }

    /// The tab names in order, without cloning any content (the command
    /// palette: no extra plaintext copy).
    pub fn names(&self) -> Vec<String> {
        self.tabs.borrow().iter().map(|t| t.name.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar_abc(active: usize) -> TabBar {
        let bar = TabBar::with_tabs(vec![Tab::new("A"), Tab::new("B"), Tab::new("C")]);
        bar.set_active(active);
        bar
    }

    fn order(bar: &TabBar) -> String {
        bar.all_tabs().iter().map(|t| t.name.as_str()).collect()
    }

    #[test]
    fn names_lists_the_tabs_in_order() {
        assert_eq!(bar_abc(1).names(), ["A", "B", "C"]);
        let bar = TabBar::with_tabs(vec![Tab::new("n"), Tab::new("n")]);
        assert_eq!(bar.names(), ["n", "n"]);
    }

    #[test]
    fn move_tab_forward_and_back() {
        let bar = bar_abc(0);
        assert_eq!(bar.move_tab(0, 2), Ok(true));
        assert_eq!(order(&bar), "BCA");
        assert_eq!(bar.move_tab(2, 0), Ok(true));
        assert_eq!(order(&bar), "ABC");
    }

    #[test]
    fn active_follows_the_moved_tab() {
        let bar = bar_abc(0);
        bar.move_tab(0, 2).unwrap();
        assert_eq!(bar.active_tab().name, "A");
        assert_eq!(bar.active_index(), 2);
    }

    #[test]
    fn active_stays_on_its_tab_when_another_moves_across_it() {
        let bar = bar_abc(1); // B active
        bar.move_tab(0, 2).unwrap(); // BCA
        assert_eq!(bar.active_tab().name, "B");
        let bar = bar_abc(1);
        bar.move_tab(2, 0).unwrap(); // CAB
        assert_eq!(bar.active_tab().name, "B");
    }

    #[test]
    fn move_keeps_content_color_and_modified() {
        let bar = bar_abc(0);
        bar.set_tab_content(0, "secret");
        bar.set_tab_color(0, Some("red"));
        bar.set_tab_modified(0, true);
        bar.move_tab(0, 1).unwrap();
        let t = &bar.all_tabs()[1];
        assert_eq!((t.name.as_str(), t.content.as_str(), t.color.as_deref(), t.is_modified), ("A", "secret", Some("red"), true));
    }

    #[test]
    fn move_to_same_place_is_a_noop_and_bad_index_an_error() {
        let bar = bar_abc(0);
        assert_eq!(bar.move_tab(1, 1), Ok(false));
        assert_eq!(order(&bar), "ABC");
        assert_eq!(bar.move_tab(3, 0), Err(TabMoveError::OutOfRange { from: 3, to: 0, len: 3 }));
        assert_eq!(bar.move_tab(0, 3), Err(TabMoveError::OutOfRange { from: 0, to: 3, len: 3 }));
        assert_eq!(order(&bar), "ABC");
    }

    #[test]
    fn new_tab_bar_has_one_tab() {
        let bar = TabBar::new();
        assert_eq!(bar.count(), 1);
        assert_eq!(bar.active_index(), 0);
        assert_eq!(bar.active_tab().name, "Note 1");
    }

    #[test]
    fn add_and_switch_tab() {
        let bar = TabBar::new();
        let idx = bar.add_tab("Note 2");
        assert_eq!(idx, 1);
        assert_eq!(bar.count(), 2);
        assert_eq!(bar.active_index(), 1);
        assert_eq!(bar.active_tab().name, "Note 2");
    }

    #[test]
    fn close_tab_not_last() {
        let bar = TabBar::new();
        bar.add_tab("Note 2");
        assert!(bar.close_tab(0));
        assert_eq!(bar.count(), 1);
        assert_eq!(bar.active_tab().name, "Note 2");
    }

    #[test]
    fn cannot_close_last_tab() {
        let bar = TabBar::new();
        assert!(!bar.close_tab(0));
        assert_eq!(bar.count(), 1);
    }

    #[test]
    fn rename_tab() {
        let bar = TabBar::new();
        bar.rename_tab(0, "Renamed");
        assert_eq!(bar.active_tab().name, "Renamed");
    }

    #[test]
    fn next_prev_tab() {
        let bar = TabBar::new();
        bar.add_tab("Note 2");
        bar.add_tab("Note 3");
        bar.set_active(0);

        bar.next_tab();
        assert_eq!(bar.active_index(), 1);
        bar.next_tab();
        assert_eq!(bar.active_index(), 2);
        bar.next_tab();
        assert_eq!(bar.active_index(), 0); // wrap

        bar.prev_tab();
        assert_eq!(bar.active_index(), 2); // wrap back
        bar.prev_tab();
        assert_eq!(bar.active_index(), 1);
    }

    #[test]
    fn modified_indicator() {
        let mut tab = Tab::new("Test");
        assert_eq!(tab.display_text(), "Test");
        tab.is_modified = true;
        assert_eq!(tab.display_text(), "\u{2022} Test");
    }

    #[test]
    fn tab_content_management() {
        let bar = TabBar::new();
        bar.set_tab_content(0, "Hello World");
        assert_eq!(bar.get_tab_content(0), "Hello World");
    }

    #[test]
    fn saved_content_follows_saves_not_edits() {
        let bar = TabBar::with_tabs(vec![Tab::new("A"), Tab::new("B")]);
        assert_eq!(bar.get_saved_content(0).as_deref(), Some(""));
        bar.set_tab_content(0, "one");
        bar.set_tab_content(1, "two");
        assert_eq!(bar.get_saved_content(0).as_deref(), Some(""));
        bar.mark_all_saved();
        bar.set_tab_content(0, "one edited");
        assert_eq!(bar.get_saved_content(0).as_deref(), Some("one"));
        assert_eq!(bar.get_saved_content(1).as_deref(), Some("two"));
        assert_eq!(bar.get_saved_content(2), None);
        // A new tab was never saved.
        let idx = bar.add_tab("C");
        assert_eq!(bar.get_saved_content(idx).as_deref(), Some(""));
    }

    /// The lock wipes the saved copy with the content.
    #[test]
    fn wipe_clears_the_saved_content_too() {
        let bar = TabBar::new();
        bar.set_tab_content(0, "secret");
        bar.mark_all_saved();
        bar.wipe_contents();
        assert_eq!(bar.get_tab_content(0), "");
        assert_eq!(bar.get_saved_content(0).as_deref(), Some(""));
    }

    #[test]
    fn tab_color() {
        let bar = TabBar::new();
        bar.set_tab_color(0, Some("blue"));
        let tab = bar.active_tab();
        assert_eq!(tab.color, Some("blue".to_string()));
        bar.set_tab_color(0, None);
        let tab = bar.active_tab();
        assert_eq!(tab.color, None);
    }

    #[test]
    fn close_adjusts_active_index() {
        let bar = TabBar::new();
        bar.add_tab("Note 2");
        bar.add_tab("Note 3");
        bar.set_active(2); // active = "Note 3"
        bar.close_tab(1); // close "Note 2"
        // Active was 2, after removing index 1, it should be 1
        assert_eq!(bar.active_index(), 1);
        assert_eq!(bar.active_tab().name, "Note 3");
    }

    #[test]
    fn with_tabs_empty_gets_default() {
        let bar = TabBar::with_tabs(vec![]);
        assert_eq!(bar.count(), 1);
        assert_eq!(bar.active_tab().name, "Note 1");
    }

    #[test]
    fn with_tabs_preserves_input() {
        let tabs = vec![
            Tab::new("Alpha"),
            Tab::new("Beta"),
        ];
        let bar = TabBar::with_tabs(tabs);
        assert_eq!(bar.count(), 2);
        assert_eq!(bar.all_tabs()[0].name, "Alpha");
        assert_eq!(bar.all_tabs()[1].name, "Beta");
    }

    #[test]
    fn active_tab_clamps_out_of_bounds_index() {
        let bar = TabBar::new();
        bar.add_tab("Note 2");
        // Force active_index to an invalid value by closing tabs
        bar.set_active(1);
        assert_eq!(bar.active_tab().name, "Note 2");
        // Close tab 1 — active should be clamped to the last valid index
        bar.close_tab(1);
        // active_tab() should not panic even if internal index was stale
        let tab = bar.active_tab();
        assert_eq!(tab.name, "Note 1");
    }

    #[test]
    fn active_tab_single_tab_always_works() {
        let bar = TabBar::new();
        // With only one tab, active_tab() should always return it
        let tab = bar.active_tab();
        assert_eq!(tab.name, "Note 1");
    }

    #[test]
    fn max_tabs_constant() {
        assert_eq!(MAX_TABS, 20);
    }

    #[test]
    fn close_tab_beyond_bounds_returns_false() {
        let bar = TabBar::new();
        bar.add_tab("Note 2");
        assert!(!bar.close_tab(5));
        assert_eq!(bar.count(), 2);
    }

    #[test]
    fn close_active_tab_selects_previous() {
        let bar = TabBar::new();
        bar.add_tab("Note 2");
        bar.add_tab("Note 3");
        bar.set_active(1); // "Note 2"
        bar.close_tab(1);
        // After closing index 1, active should stay at 1 (now "Note 3")
        assert_eq!(bar.active_index(), 1);
        assert_eq!(bar.active_tab().name, "Note 3");
    }

    #[test]
    fn close_last_index_moves_active_back() {
        let bar = TabBar::new();
        bar.add_tab("Note 2");
        bar.set_active(1);
        bar.close_tab(1);
        assert_eq!(bar.active_index(), 0);
        assert_eq!(bar.active_tab().name, "Note 1");
    }

    #[test]
    fn set_active_out_of_bounds_ignored() {
        let bar = TabBar::new();
        bar.set_active(99);
        assert_eq!(bar.active_index(), 0);
    }

    #[test]
    fn get_tab_content_out_of_bounds() {
        let bar = TabBar::new();
        assert_eq!(bar.get_tab_content(99), "");
    }

    /// Issue #42: what a save writes, one copy of each text (the saved text
    /// is not copied), in order, with names and colors.
    #[test]
    fn note_tabs_copy_name_text_and_color_only() {
        let mut a = Tab::new("A");
        a.content = "alpha".into();
        a.saved_content = "old alpha".into();
        let mut b = Tab::new("B");
        b.content = "beta".into();
        b.color = Some("blue".into());
        let bar = TabBar::with_tabs(vec![a, b]);
        let got: Vec<(String, String, String, Option<String>)> =
            bar.note_tabs().into_iter().map(|t| (t.name, t.content, t.saved_content, t.color)).collect();
        assert_eq!(
            got,
            vec![
                ("A".into(), "alpha".into(), String::new(), None),
                ("B".into(), "beta".into(), String::new(), Some("blue".into())),
            ]
        );
    }
}
