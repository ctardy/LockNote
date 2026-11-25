// AttachmentsPanel — Right-side panel for file attachments
//
// Shows a list of attached files with name and size.
// Provides Add, Open, Save As, and Delete actions.

use std::cell::RefCell;

use crate::features::attachments::{Attachment, format_size};

/// Default width of the attachments panel in pixels (resizable: `ui::side_panel`).
pub const ATTACHMENTS_PANEL_WIDTH: u32 = 280;

/// AttachmentsPanel holds the list of attachments and panel visibility state.
pub struct AttachmentsPanel {
    visible: RefCell<bool>,
}

impl AttachmentsPanel {
    pub fn new() -> Self {
        AttachmentsPanel {
            visible: RefCell::new(false),
        }
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

    /// Format an attachment for display in the list box: "filename (size)"
    pub fn format_entry(att: &Attachment) -> String {
        format!("{} ({})", att.name, format_size(att.size()))
    }

    /// Get the formatted list for the ListBox.
    pub fn formatted_list(attachments: &[Attachment]) -> Vec<String> {
        attachments.iter().map(Self::format_entry).collect()
    }

    /// Build a status string showing total count and size.
    pub fn status_text(attachments: &[Attachment]) -> String {
        if attachments.is_empty() {
            return String::new();
        }
        let total = crate::features::attachments::total_size(attachments);
        format!("{} file(s), {}", attachments.len(), format_size(total))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_visibility() {
        let panel = AttachmentsPanel::new();
        assert!(!panel.is_visible());
        panel.toggle_visible();
        assert!(panel.is_visible());
        panel.toggle_visible();
        assert!(!panel.is_visible());
    }

    #[test]
    fn test_format_entry() {
        let att = Attachment::new("test.pdf", vec![0; 2048]);
        let formatted = AttachmentsPanel::format_entry(&att);
        assert!(formatted.contains("test.pdf"));
        assert!(formatted.contains("2.0 KB"));
    }

    #[test]
    fn test_formatted_list() {
        let atts = vec![
            Attachment::new("a.txt", vec![0; 100]),
            Attachment::new("b.pdf", vec![0; 2048]),
        ];
        let list = AttachmentsPanel::formatted_list(&atts);
        assert_eq!(list.len(), 2);
        assert!(list[0].contains("a.txt"));
        assert!(list[1].contains("b.pdf"));
    }

    #[test]
    fn test_status_text_empty() {
        let text = AttachmentsPanel::status_text(&[]);
        assert_eq!(text, "");
    }

    #[test]
    fn test_status_text() {
        let atts = vec![
            Attachment::new("a.txt", vec![0; 100]),
            Attachment::new("b.txt", vec![0; 200]),
        ];
        let text = AttachmentsPanel::status_text(&atts);
        assert!(text.contains("2 file(s)"));
        assert!(text.contains("300 B"));
    }
}
