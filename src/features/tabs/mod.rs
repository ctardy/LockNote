// Tab system — Multi-tab note serialization/deserialization
//
// Format:
//   [LOCKNOTE_TABS]
//   [TAB:Tab Name]
//   content
//   [TAB:Second Tab:blue]
//   content
//   [/LOCKNOTE_TABS]
//
// A line of a tab's text that starts with a reserved marker of the payload
// (any block, see `features::markers`) is written with a `\x00` prefix.

/// A single note tab.
#[derive(Debug, Clone)]
pub struct NoteTab {
    pub name: String,
    pub content: String,
    pub saved_content: String,
    pub color: Option<String>,
}

impl NoteTab {
    /// Create a new tab with the given name and empty content.
    pub fn new(name: &str) -> Self {
        NoteTab {
            name: name.to_string(),
            content: String::new(),
            saved_content: String::new(),
            color: None,
        }
    }

    /// Create a new tab with name, content, and optional color.
    pub fn with_content(name: &str, content: &str, color: Option<&str>) -> Self {
        NoteTab {
            name: name.to_string(),
            content: content.to_string(),
            saved_content: content.to_string(),
            color: color.map(|s| s.to_string()),
        }
    }

    /// Whether the tab has unsaved modifications.
    pub fn is_modified(&self) -> bool {
        self.content != self.saved_content
    }

    /// Mark the tab as clean (saved).
    pub fn mark_clean(&mut self) {
        self.saved_content = self.content.clone();
    }
}

/// Valid tab colors with their RGB values.
pub const TAB_COLORS: &[(&str, (u8, u8, u8))] = &[
    ("blue",   (66, 133, 244)),
    ("green",  (52, 168, 83)),
    ("red",    (234, 67, 53)),
    ("orange", (251, 188, 4)),
    ("purple", (171, 71, 188)),
    ("yellow", (255, 235, 59)),
    ("pink",   (236, 64, 122)),
    ("gray",   (158, 158, 158)),
];

use crate::features::markers::{escape_content, unescape_content, TABS_FOOTER, TABS_HEADER, TAB_PREFIX};

/// Serialize tabs into the stored format.
pub fn serialize(tabs: &[NoteTab]) -> String {
    if tabs.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    out.push_str(TABS_HEADER);
    out.push('\n');

    for tab in tabs {
        out.push_str(TAB_PREFIX);
        out.push_str(&tab.name);
        if let Some(ref color) = tab.color {
            out.push(':');
            out.push_str(color);
        }
        out.push_str("]\n");
        out.push_str(&escape_content(&tab.content));
        if !tab.content.is_empty() && !tab.content.ends_with('\n') {
            out.push('\n');
        }
    }

    out.push_str(TABS_FOOTER);
    out
}

/// True when `parse` reads `input` as a tabs block: the header starts the
/// text, leading whitespace ignored. Otherwise the text is one plain tab.
pub fn starts_with_block(input: &str) -> bool {
    input.trim_start().starts_with(TABS_HEADER)
}

/// Parse tabs from input. Legacy: no header means single tab "Note 1".
pub fn parse(input: &str) -> Vec<NoteTab> {
    if input.is_empty() {
        return vec![NoteTab::with_content("Note 1", "", None)];
    }

    if !starts_with_block(input) {
        // Legacy single-tab
        return vec![NoteTab::with_content("Note 1", input, None)];
    }

    // Find header
    let header_pos = match input.find(TABS_HEADER) {
        Some(pos) => pos,
        None => return vec![NoteTab::with_content("Note 1", input, None)],
    };
    let after_header = header_pos + TABS_HEADER.len();

    // Parse line by line (escaped lines start with \x00, so real markers are distinguishable)
    let block = &input[after_header..];

    let mut tabs: Vec<NoteTab> = Vec::new();
    let mut current_name: Option<String> = None;
    let mut current_color: Option<String> = None;
    let mut current_content = String::new();
    for line in block.split('\n') {
        // Check for real (unescaped) footer
        let trimmed_line = line.trim_start();
        if trimmed_line == TABS_FOOTER {
            break;
        }

        if trimmed_line.starts_with(TAB_PREFIX) && !line.starts_with('\x00') {
            // Check if it's a valid [TAB:name] or [TAB:name:color] line
            if let Some(end) = trimmed_line.find(']') {
                let inner = &trimmed_line[TAB_PREFIX.len()..end];

                // Save previous tab
                if let Some(name) = current_name.take() {
                    let content = strip_trailing_newline(&current_content);
                    tabs.push(NoteTab::with_content(&name, &unescape_content(&content), current_color.take().as_deref()));
                    current_content.clear();
                }

                // Parse name and optional color
                if let Some(colon_pos) = inner.rfind(':') {
                    let maybe_color = &inner[colon_pos + 1..];
                    if TAB_COLORS.iter().any(|(c, _)| *c == maybe_color) {
                        current_name = Some(inner[..colon_pos].to_string());
                        current_color = Some(maybe_color.to_string());
                    } else {
                        // Not a valid color — treat colon as part of name
                        current_name = Some(inner.to_string());
                        current_color = None;
                    }
                } else {
                    current_name = Some(inner.to_string());
                    current_color = None;
                }
                continue;
            }
        }

        if current_name.is_some() {
            if !current_content.is_empty() {
                current_content.push('\n');
            }
            current_content.push_str(line);
        }
    }

    // Save last tab
    if let Some(name) = current_name.take() {
        let content = strip_trailing_newline(&current_content);
        tabs.push(NoteTab::with_content(&name, &unescape_content(&content), current_color.take().as_deref()));
    }

    if tabs.is_empty() {
        vec![NoteTab::with_content("Note 1", "", None)]
    } else {
        tabs
    }
}

fn strip_trailing_newline(s: &str) -> String {
    let mut result = s.to_string();
    if result.ends_with('\n') {
        result.pop();
        if result.ends_with('\r') {
            result.pop();
        }
    }
    result
}

/// Generate next tab name: "Note N" where N = max existing "Note N" + 1
pub fn next_tab_name(tabs: &[NoteTab]) -> String {
    let mut max_n: u32 = 0;
    for tab in tabs {
        if tab.name.starts_with("Note ") {
            if let Ok(n) = tab.name[5..].parse::<u32>() {
                if n > max_n {
                    max_n = n;
                }
            }
        }
    }
    format!("Note {}", max_n + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_legacy_no_header() {
        let tabs = parse("Hello world");
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].name, "Note 1");
        assert_eq!(tabs[0].content, "Hello world");
    }

    #[test]
    fn test_null_input() {
        let tabs = parse("");
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].name, "Note 1");
        assert_eq!(tabs[0].content, "");
    }

    #[test]
    fn test_single_tab() {
        let input = "[LOCKNOTE_TABS]\n[TAB:My Note]\nHello\n[/LOCKNOTE_TABS]";
        let tabs = parse(input);
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].name, "My Note");
        assert_eq!(tabs[0].content, "Hello");
        assert!(tabs[0].color.is_none());
    }

    #[test]
    fn test_multi_tabs() {
        let input = "[LOCKNOTE_TABS]\n[TAB:First]\nContent 1\n[TAB:Second:blue]\nContent 2\n[/LOCKNOTE_TABS]";
        let tabs = parse(input);
        assert_eq!(tabs.len(), 2);
        assert_eq!(tabs[0].name, "First");
        assert_eq!(tabs[0].content, "Content 1");
        assert!(tabs[0].color.is_none());
        assert_eq!(tabs[1].name, "Second");
        assert_eq!(tabs[1].content, "Content 2");
        assert_eq!(tabs[1].color.as_deref(), Some("blue"));
    }

    #[test]
    fn test_tab_colors() {
        for (color, _) in TAB_COLORS {
            let input = format!("[LOCKNOTE_TABS]\n[TAB:Test:{}]\nContent\n[/LOCKNOTE_TABS]", color);
            let tabs = parse(&input);
            assert_eq!(tabs[0].color.as_deref(), Some(*color));
        }
    }

    #[test]
    fn test_escaping_roundtrip() {
        let content = "Normal line\n[TAB:fake tab]\n[LOCKNOTE_TABS]\n[/LOCKNOTE_TABS]\nEnd";
        let tab = NoteTab::with_content("Test", content, None);
        let serialized = serialize(&[tab]);
        let tabs = parse(&serialized);
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].content, content);
    }

    #[test]
    fn test_roundtrip_multi_tabs() {
        let tabs = vec![
            NoteTab::with_content("Note 1", "First content\nMultiple lines", Some("blue")),
            NoteTab::with_content("Note 2", "Second content", None),
            NoteTab::with_content("Note 3", "Third content", Some("red")),
        ];
        let serialized = serialize(&tabs);
        let parsed = parse(&serialized);
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].name, "Note 1");
        assert_eq!(parsed[0].content, "First content\nMultiple lines");
        assert_eq!(parsed[0].color.as_deref(), Some("blue"));
        assert_eq!(parsed[1].name, "Note 2");
        assert_eq!(parsed[1].content, "Second content");
        assert!(parsed[1].color.is_none());
        assert_eq!(parsed[2].name, "Note 3");
        assert_eq!(parsed[2].content, "Third content");
        assert_eq!(parsed[2].color.as_deref(), Some("red"));
    }

    #[test]
    fn test_next_tab_name_empty() {
        assert_eq!(next_tab_name(&[]), "Note 1");
    }

    #[test]
    fn test_next_tab_name_sequential() {
        let tabs = vec![
            NoteTab::new("Note 1"),
            NoteTab::new("Note 2"),
        ];
        assert_eq!(next_tab_name(&tabs), "Note 3");
    }

    #[test]
    fn test_next_tab_name_gap() {
        let tabs = vec![
            NoteTab::new("Note 1"),
            NoteTab::new("Note 5"),
            NoteTab::new("Note 3"),
        ];
        assert_eq!(next_tab_name(&tabs), "Note 6");
    }

    #[test]
    fn test_next_tab_name_custom_names() {
        let tabs = vec![
            NoteTab::new("My Diary"),
            NoteTab::new("Passwords"),
        ];
        assert_eq!(next_tab_name(&tabs), "Note 1");
    }

    #[test]
    fn test_modified_flag() {
        let mut tab = NoteTab::with_content("Test", "original", None);
        assert!(!tab.is_modified());
        tab.content = "changed".to_string();
        assert!(tab.is_modified());
        tab.mark_clean();
        assert!(!tab.is_modified());
        assert_eq!(tab.saved_content, "changed");
    }

    #[test]
    fn test_empty_tabs_serialize() {
        let result = serialize(&[]);
        assert_eq!(result, "");
    }

    #[test]
    fn test_tab_with_empty_content() {
        let tab = NoteTab::with_content("Empty", "", None);
        let serialized = serialize(&[tab]);
        let parsed = parse(&serialized);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "Empty");
        assert_eq!(parsed[0].content, "");
    }

    #[test]
    fn test_tab_name_with_colon_not_color() {
        // A colon followed by a non-color string should be part of the name
        let input = "[LOCKNOTE_TABS]\n[TAB:My:Note]\nContent\n[/LOCKNOTE_TABS]";
        let tabs = parse(input);
        assert_eq!(tabs[0].name, "My:Note");
        assert!(tabs[0].color.is_none());
    }

    #[test]
    fn test_multiline_content() {
        let content = "Line 1\nLine 2\n\nLine 4\n\n\nLine 7";
        let tab = NoteTab::with_content("Test", content, None);
        let serialized = serialize(&[tab]);
        let parsed = parse(&serialized);
        assert_eq!(parsed[0].content, content);
    }

    #[test]
    fn tab_color_pink_spec() {
        // Verify pink color matches spec value (236, 64, 122)
        let pink = TAB_COLORS.iter().find(|(name, _)| *name == "pink");
        assert!(pink.is_some(), "Pink color not found in TAB_COLORS");
        let (name, rgb) = pink.unwrap();
        assert_eq!(*name, "pink");
        assert_eq!(*rgb, (236, 64, 122), "Pink color RGB values do not match spec");
    }

    #[test]
    fn test_unicode_tab_name() {
        let tab = NoteTab::with_content("Onglet café ☕", "contenu", Some("blue"));
        let serialized = serialize(&[tab.clone()]);
        let parsed = parse(&serialized);
        assert_eq!(parsed[0].name, "Onglet café ☕");
        assert_eq!(parsed[0].content, "contenu");
        assert_eq!(parsed[0].color, Some("blue".to_string()));
    }

    #[test]
    fn test_content_with_tab_markers() {
        // Content contains text that looks like tab markers
        let tab = NoteTab::with_content("Safe", "[TAB:Fake]\n[/LOCKNOTE_TABS]", None);
        let serialized = serialize(&[tab]);
        let parsed = parse(&serialized);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "Safe");
        assert!(parsed[0].content.contains("[TAB:Fake]"));
    }

    #[test]
    fn test_empty_content_tab() {
        let tab = NoteTab::with_content("Empty", "", None);
        let serialized = serialize(&[tab]);
        let parsed = parse(&serialized);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "Empty");
        assert_eq!(parsed[0].content, "");
    }

    #[test]
    fn test_many_tabs_roundtrip() {
        let tabs: Vec<NoteTab> = (1..=10)
            .map(|i| NoteTab::with_content(&format!("Tab {}", i), &format!("Content {}", i), None))
            .collect();
        let serialized = serialize(&tabs);
        let parsed = parse(&serialized);
        assert_eq!(parsed.len(), 10);
        for (i, tab) in parsed.iter().enumerate() {
            assert_eq!(tab.name, format!("Tab {}", i + 1));
            assert_eq!(tab.content, format!("Content {}", i + 1));
        }
    }

    #[test]
    fn test_is_modified_and_mark_clean() {
        let mut tab = NoteTab::with_content("T", "original", None);
        assert!(!tab.is_modified());
        tab.content = "changed".to_string();
        assert!(tab.is_modified());
        tab.mark_clean();
        assert!(!tab.is_modified());
        assert_eq!(tab.saved_content, "changed");
    }

    #[test]
    fn test_parse_plain_text_legacy() {
        let input = "Just plain text without any tab markers";
        let parsed = parse(input);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].content, input);
    }

    #[test]
    fn tab_colors_complete_set() {
        // Ensure all expected colors are present with correct specs
        let expected = [
            ("blue", (66, 133, 244)),
            ("green", (52, 168, 83)),
            ("red", (234, 67, 53)),
            ("orange", (251, 188, 4)),
            ("purple", (171, 71, 188)),
            ("yellow", (255, 235, 59)),
            ("pink", (236, 64, 122)),
            ("gray", (158, 158, 158)),
        ];
        assert_eq!(
            TAB_COLORS.len(),
            expected.len(),
            "TAB_COLORS count does not match expected"
        );
        for (exp_name, exp_rgb) in &expected {
            let found = TAB_COLORS.iter().find(|(name, _)| name == exp_name);
            assert!(found.is_some(), "Expected color {} not found", exp_name);
            let (_, rgb) = found.unwrap();
            assert_eq!(rgb, exp_rgb, "Color {} RGB mismatch", exp_name);
        }
    }
}
