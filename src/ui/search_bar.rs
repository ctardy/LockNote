// SearchBar — Find and Find & Replace bar
//
// Two modes:
//   - Find only (Ctrl+F): search field + Find Next button
//   - Find & Replace (Ctrl+H): adds replace field + Replace / Replace All buttons
//
// Communication with editor is done via callbacks that operate on
// the RichTextBox content.

use std::cell::RefCell;

/// Search bar state and controls.
pub struct SearchBar {
    /// Whether the search bar is currently visible.
    visible: RefCell<bool>,
    /// Whether replace controls are shown.
    replace_mode: RefCell<bool>,
    /// The last search index for wrap-around.
    last_index: RefCell<usize>,
    /// The current search term.
    search_term: RefCell<String>,
    /// The current replace term.
    replace_term: RefCell<String>,
}

impl SearchBar {
    /// Create a new SearchBar (controls are not yet wired to a parent).
    pub fn new() -> Self {
        SearchBar {
            visible: RefCell::new(false),
            replace_mode: RefCell::new(false),
            last_index: RefCell::new(usize::MAX),
            search_term: RefCell::new(String::new()),
            replace_term: RefCell::new(String::new()),
        }
    }

    /// Show the search bar in the given mode.
    /// `replace`: if true, show replace controls.
    pub fn show(&self, replace: bool) {
        *self.visible.borrow_mut() = true;
        *self.replace_mode.borrow_mut() = replace;
    }

    /// Hide the search bar.
    pub fn hide(&self) {
        *self.visible.borrow_mut() = false;
    }

    /// Returns true if the search bar is currently visible.
    pub fn is_visible(&self) -> bool {
        *self.visible.borrow()
    }

    /// Returns true if in replace mode.
    pub fn is_replace_mode(&self) -> bool {
        *self.replace_mode.borrow()
    }

    /// Set the search term.
    pub fn set_search_term(&self, term: &str) {
        *self.search_term.borrow_mut() = term.to_string();
        // Use usize::MAX as sentinel: "no previous match, start from beginning"
        *self.last_index.borrow_mut() = usize::MAX;
    }

    /// Set the replace term.
    pub fn set_replace_term(&self, term: &str) {
        *self.replace_term.borrow_mut() = term.to_string();
    }

    /// Find the next occurrence in the given text, starting from `last_index + 1`.
    /// Returns `Some((start, end))` if found, or `None` if not found.
    ///
    /// The search is case-insensitive and wraps around to the beginning.
    pub fn find_next(&self, text: &str) -> Option<(usize, usize)> {
        let term = self.search_term.borrow();
        if term.is_empty() {
            return None;
        }

        let text_lower = text.to_lowercase();
        let term_lower = term.to_lowercase();
        let start_from = *self.last_index.borrow();

        // Search from last_index + 1 to end (usize::MAX means "start from 0")
        let search_start = if start_from == usize::MAX {
            0
        } else if start_from + 1 > text_lower.len() {
            0
        } else {
            start_from + 1
        };

        if let Some(pos) = text_lower[search_start..].find(&term_lower) {
            let abs_pos = search_start + pos;
            *self.last_index.borrow_mut() = abs_pos;
            return Some((abs_pos, abs_pos + term.len()));
        }

        // Wrap around: search from the beginning of the full text
        if search_start > 0 {
            if let Some(pos) = text_lower.find(&term_lower) {
                *self.last_index.borrow_mut() = pos;
                return Some((pos, pos + term.len()));
            }
        }

        None
    }

    /// Replace the current selection if it matches the search term,
    /// then find the next occurrence.
    ///
    /// Returns `Some((start, end))` of the next match after replacement,
    /// or `None` if no next match.
    pub fn replace_current(&self, text: &mut String, selection: (usize, usize)) -> Option<(usize, usize)> {
        let term = self.search_term.borrow();
        let replace = self.replace_term.borrow();

        if term.is_empty() {
            return None;
        }

        let (sel_start, sel_end) = selection;
        if sel_start <= text.len() && sel_end <= text.len() && sel_start <= sel_end {
            let selected = &text[sel_start..sel_end];
            if selected.to_lowercase() == term.to_lowercase() {
                // Perform replacement
                let new_text = format!(
                    "{}{}{}",
                    &text[..sel_start],
                    &*replace,
                    &text[sel_end..]
                );
                *text = new_text;

                // Adjust last_index to account for replacement length difference
                *self.last_index.borrow_mut() = sel_start + replace.len();
            }
        }

        // Find next
        self.find_next(text)
    }

    /// Replace all occurrences (case-insensitive).
    /// Returns the number of replacements made.
    pub fn replace_all(&self, text: &mut String) -> usize {
        let term = self.search_term.borrow();
        let replace = self.replace_term.borrow();

        if term.is_empty() {
            return 0;
        }

        let term_lower = term.to_lowercase();
        let text_lower = text.to_lowercase();
        let mut result = String::with_capacity(text.len());
        let mut count = 0usize;
        let mut pos = 0usize;

        while pos < text.len() {
            if let Some(found) = text_lower[pos..].find(&term_lower) {
                let abs = pos + found;
                result.push_str(&text[pos..abs]);
                result.push_str(&replace);
                pos = abs + term.len();
                count += 1;
            } else {
                result.push_str(&text[pos..]);
                break;
            }
        }

        if count > 0 {
            *text = result;
            *self.last_index.borrow_mut() = 0;
        }

        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_next_basic() {
        let sb = SearchBar::new();
        sb.set_search_term("hello");
        let text = "Say hello world, hello!";
        let result = sb.find_next(text);
        assert_eq!(result, Some((4, 9)));
        // Next
        let result = sb.find_next(text);
        assert_eq!(result, Some((17, 22)));
        // Wrap
        let result = sb.find_next(text);
        assert_eq!(result, Some((4, 9)));
    }

    #[test]
    fn find_next_case_insensitive() {
        let sb = SearchBar::new();
        sb.set_search_term("HELLO");
        let text = "Hello World";
        let result = sb.find_next(text);
        assert_eq!(result, Some((0, 5)));
    }

    #[test]
    fn find_next_not_found() {
        let sb = SearchBar::new();
        sb.set_search_term("xyz");
        let text = "Hello World";
        assert_eq!(sb.find_next(text), None);
    }

    #[test]
    fn find_next_empty_term() {
        let sb = SearchBar::new();
        sb.set_search_term("");
        assert_eq!(sb.find_next("anything"), None);
    }

    #[test]
    fn replace_all_basic() {
        let sb = SearchBar::new();
        sb.set_search_term("foo");
        sb.set_replace_term("bar");
        let mut text = "foo and FOO and Foo".to_string();
        let count = sb.replace_all(&mut text);
        assert_eq!(count, 3);
        assert_eq!(text, "bar and bar and bar");
    }

    #[test]
    fn replace_all_no_match() {
        let sb = SearchBar::new();
        sb.set_search_term("xyz");
        sb.set_replace_term("abc");
        let mut text = "Hello World".to_string();
        let count = sb.replace_all(&mut text);
        assert_eq!(count, 0);
        assert_eq!(text, "Hello World");
    }

    #[test]
    fn replace_all_empty_replacement() {
        let sb = SearchBar::new();
        sb.set_search_term("the ");
        sb.set_replace_term("");
        let mut text = "the cat and the dog".to_string();
        let count = sb.replace_all(&mut text);
        assert_eq!(count, 2);
        assert_eq!(text, "cat and dog");
    }

    #[test]
    fn replace_current_basic() {
        let sb = SearchBar::new();
        sb.set_search_term("hello");
        sb.set_replace_term("world");
        let mut text = "hello there hello".to_string();
        // Simulate: selection is on first "hello" (0..5)
        let next = sb.replace_current(&mut text, (0, 5));
        assert_eq!(text, "world there hello");
        // Next match should be the remaining "hello"
        assert!(next.is_some());
    }

    #[test]
    fn show_hide_toggle() {
        let sb = SearchBar::new();
        assert!(!sb.is_visible());
        sb.show(false);
        assert!(sb.is_visible());
        assert!(!sb.is_replace_mode());
        sb.show(true);
        assert!(sb.is_replace_mode());
        sb.hide();
        assert!(!sb.is_visible());
    }

    #[test]
    fn find_next_single_char() {
        let sb = SearchBar::new();
        sb.set_search_term("a");
        let text = "banana";
        assert_eq!(sb.find_next(text), Some((1, 2)));
        assert_eq!(sb.find_next(text), Some((3, 4)));
        assert_eq!(sb.find_next(text), Some((5, 6)));
        // Wrap around to first 'a'
        assert_eq!(sb.find_next(text), Some((1, 2)));
    }

    #[test]
    fn find_next_at_very_end() {
        let sb = SearchBar::new();
        sb.set_search_term("test");
        let text = "hello world test";
        let result = sb.find_next(text);
        assert_eq!(result, Some((12, 16)));
        // Only occurrence — wrap around should find it again
        let result = sb.find_next(text);
        assert_eq!(result, Some((12, 16)));
    }

    #[test]
    fn find_next_wrap_around() {
        let sb = SearchBar::new();
        sb.set_search_term("a");
        let text = "xax";
        assert_eq!(sb.find_next(text), Some((1, 2)));
        // Next search starts at 2, nothing after, wraps to find at 1 again
        assert_eq!(sb.find_next(text), Some((1, 2)));
    }

    #[test]
    fn find_next_changing_term_resets() {
        let sb = SearchBar::new();
        sb.set_search_term("hello");
        let text = "hello world hello";
        let result = sb.find_next(text);
        assert_eq!(result, Some((0, 5)));
        // Change term — should reset index so search starts from beginning
        sb.set_search_term("world");
        let result = sb.find_next(text);
        assert_eq!(result, Some((6, 11)));
    }

    #[test]
    fn replace_all_overlapping() {
        let sb = SearchBar::new();
        sb.set_search_term("aa");
        sb.set_replace_term("x");
        let mut text = "aaaa".to_string();
        let count = sb.replace_all(&mut text);
        assert_eq!(count, 2);
        assert_eq!(text, "xx");
    }

    #[test]
    fn replace_all_growing_replacement() {
        let sb = SearchBar::new();
        sb.set_search_term("a");
        sb.set_replace_term("abc");
        let mut text = "aaa".to_string();
        let count = sb.replace_all(&mut text);
        assert_eq!(count, 3);
        assert_eq!(text, "abcabcabc");
    }

    #[test]
    fn replace_all_empty_term() {
        let sb = SearchBar::new();
        sb.set_search_term("");
        sb.set_replace_term("x");
        let mut text = "hello".to_string();
        let count = sb.replace_all(&mut text);
        assert_eq!(count, 0);
        assert_eq!(text, "hello");
    }

    #[test]
    fn replace_current_no_match_selection() {
        let sb = SearchBar::new();
        sb.set_search_term("hello");
        sb.set_replace_term("world");
        let mut text = "hello there hello".to_string();
        // Selection doesn't match search term — should not replace
        let result = sb.replace_current(&mut text, (6, 11));
        assert_eq!(text, "hello there hello");
        // Should still find next occurrence
        assert!(result.is_some());
    }

    #[test]
    fn replace_current_empty_text() {
        let sb = SearchBar::new();
        sb.set_search_term("hello");
        sb.set_replace_term("world");
        let mut text = String::new();
        let result = sb.replace_current(&mut text, (0, 0));
        assert_eq!(result, None);
        assert!(text.is_empty());
    }

    #[test]
    fn find_next_unicode_case() {
        let sb = SearchBar::new();
        sb.set_search_term("CAFÉ");
        let text = "I love café";
        let result = sb.find_next(text);
        assert_eq!(result, Some((7, 12)));
    }

    #[test]
    fn find_next_empty_text() {
        let sb = SearchBar::new();
        sb.set_search_term("hello");
        assert_eq!(sb.find_next(""), None);
    }

    #[test]
    fn replace_all_case_preserving_count() {
        let sb = SearchBar::new();
        sb.set_search_term("hello");
        sb.set_replace_term("hi");
        let mut text = "Hello HELLO hello hElLo".to_string();
        let count = sb.replace_all(&mut text);
        assert_eq!(count, 4);
        assert_eq!(text, "hi hi hi hi");
    }

    #[test]
    fn find_next_term_longer_than_text() {
        let sb = SearchBar::new();
        sb.set_search_term("hello world");
        assert_eq!(sb.find_next("hi"), None);
    }

    #[test]
    fn new_search_bar_defaults() {
        let sb = SearchBar::new();
        assert!(!sb.is_visible());
        assert!(!sb.is_replace_mode());
    }
}
