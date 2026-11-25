// ExportEncodingDialog — Choose encoding before exporting a note
//
// Displays a small modal with a ComboBox of available encodings.
// Returns Option<ExportEncoding>: chosen encoding, or None if cancelled.

use native_windows_gui as nwg;
use std::cell::RefCell;
use std::rc::Rc;

/// Supported export encodings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ExportEncoding {
    Utf8,
    Utf8Bom,
    Windows1252,
    Latin1,
}

impl ExportEncoding {
    /// Human-readable label shown in the ComboBox.
    pub fn label(&self) -> &'static str {
        match self {
            ExportEncoding::Utf8 => "UTF-8",
            ExportEncoding::Utf8Bom => "UTF-8 with BOM",
            ExportEncoding::Windows1252 => "Windows-1252",
            ExportEncoding::Latin1 => "ISO-8859-1 (Latin-1)",
        }
    }

    /// All available encodings, in ComboBox order.
    fn all() -> &'static [ExportEncoding] {
        &[
            ExportEncoding::Utf8,
            ExportEncoding::Utf8Bom,
            ExportEncoding::Windows1252,
            ExportEncoding::Latin1,
        ]
    }

    /// Encode a UTF-8 string into bytes using this encoding.
    /// Returns `(bytes, had_replacements)`.
    pub fn encode(&self, text: &str) -> (Vec<u8>, bool) {
        match self {
            ExportEncoding::Utf8 => (text.as_bytes().to_vec(), false),
            ExportEncoding::Utf8Bom => {
                let mut bytes = vec![0xEF, 0xBB, 0xBF];
                bytes.extend_from_slice(text.as_bytes());
                (bytes, false)
            }
            ExportEncoding::Windows1252 => {
                let (cow, _, had_errors) = encoding_rs::WINDOWS_1252.encode(text);
                (cow.into_owned(), had_errors)
            }
            ExportEncoding::Latin1 => {
                let mut bytes = Vec::with_capacity(text.len());
                let mut had_replacements = false;
                for ch in text.chars() {
                    if (ch as u32) <= 0xFF {
                        bytes.push(ch as u8);
                    } else {
                        bytes.push(b'?');
                        had_replacements = true;
                    }
                }
                (bytes, had_replacements)
            }
        }
    }
}

fn t(key: &str) -> String {
    crate::features::i18n::get(key).to_string()
}

pub struct ExportEncodingDialog;

impl ExportEncodingDialog {
    /// Show the encoding selection dialog.
    /// Returns `Some(encoding)` on OK, `None` on Cancel or window close.
    pub fn show() -> Option<ExportEncoding> {
        let mut window = Default::default();
        nwg::Window::builder()
            .size((320, 150))
            .position((350, 300))
            .title(&t("export.encoding.title"))
            .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
            .build(&mut window)
            .expect("Failed to build ExportEncodingDialog window");

        super::apply_dialog_theme(&window);

        let mut font = Default::default();
        nwg::Font::builder()
            .family("Segoe UI")
            .size(16)
            .build(&mut font)
            .expect("Failed to build font");

        // Label
        let mut lbl = Default::default();
        nwg::Label::builder()
            .text(&t("export.encoding.label"))
            .parent(&window)
            .position((20, 15))
            .size((280, 20))
            .font(Some(&font))
            .build(&mut lbl)
            .expect("Failed to build label");

        // ComboBox
        let labels: Vec<String> = ExportEncoding::all().iter().map(|e| e.label().to_string()).collect();
        let mut combo: nwg::ComboBox<String> = Default::default();
        nwg::ComboBox::builder()
            .collection(labels)
            .selected_index(Some(0))
            .parent(&window)
            .position((20, 42))
            .size((280, 28))
            .font(Some(&font))
            .build(&mut combo)
            .expect("Failed to build encoding combo");

        // OK button
        let mut btn_ok = Default::default();
        nwg::Button::builder()
            .text(&t("settings.ok"))
            .parent(&window)
            .position((120, 90))
            .size((80, 30))
            .font(Some(&font))
            .build(&mut btn_ok)
            .expect("Failed to build OK button");

        super::bind_enter_to_button(window.handle, btn_ok.handle);

        // Cancel button
        let mut btn_cancel = Default::default();
        nwg::Button::builder()
            .text(&t("settings.cancel"))
            .parent(&window)
            .position((215, 90))
            .size((80, 30))
            .font(Some(&font))
            .build(&mut btn_cancel)
            .expect("Failed to build Cancel button");

        let result: Rc<RefCell<Option<ExportEncoding>>> = Rc::new(RefCell::new(None));
        let window_handle = window.handle;
        let result_clone = result.clone();

        let handler = nwg::full_bind_event_handler(&window_handle, move |evt, _evt_data, handle| {
            match evt {
                nwg::Event::OnButtonClick => {
                    if handle == btn_ok.handle {
                        let idx = combo.selection().unwrap_or(0);
                        let encodings = ExportEncoding::all();
                        if idx < encodings.len() {
                            *result_clone.borrow_mut() = Some(encodings[idx]);
                        }
                        nwg::stop_thread_dispatch();
                    } else if handle == btn_cancel.handle {
                        nwg::stop_thread_dispatch();
                    }
                }
                nwg::Event::OnWindowClose => {
                    nwg::stop_thread_dispatch();
                }
                _ => {}
            }
        });

        nwg::dispatch_thread_events();
        nwg::unbind_event_handler(&handler);

        let out = result.borrow_mut().take();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_utf8() {
        let (bytes, lossy) = ExportEncoding::Utf8.encode("café");
        assert_eq!(bytes, "café".as_bytes());
        assert!(!lossy);
    }

    #[test]
    fn encode_utf8_bom() {
        let (bytes, lossy) = ExportEncoding::Utf8Bom.encode("Hi");
        assert_eq!(&bytes[..3], &[0xEF, 0xBB, 0xBF]);
        assert_eq!(&bytes[3..], "Hi".as_bytes());
        assert!(!lossy);
    }

    #[test]
    fn encode_windows_1252_basic() {
        let (bytes, lossy) = ExportEncoding::Windows1252.encode("café");
        // 'é' = 0xE9 in Windows-1252
        assert_eq!(bytes, vec![0x63, 0x61, 0x66, 0xE9]);
        assert!(!lossy);
    }

    #[test]
    fn encode_windows_1252_lossy() {
        // Japanese char not in Windows-1252
        let (_, lossy) = ExportEncoding::Windows1252.encode("日本語");
        assert!(lossy);
    }

    #[test]
    fn encode_latin1_basic() {
        let (bytes, lossy) = ExportEncoding::Latin1.encode("café");
        assert_eq!(bytes, vec![0x63, 0x61, 0x66, 0xE9]);
        assert!(!lossy);
    }

    #[test]
    fn encode_latin1_lossy() {
        let (bytes, lossy) = ExportEncoding::Latin1.encode("€"); // € = U+20AC, > 0xFF
        assert_eq!(bytes, vec![b'?']);
        assert!(lossy);
    }

    #[test]
    fn all_encodings_have_labels() {
        for enc in ExportEncoding::all() {
            assert!(!enc.label().is_empty());
        }
    }

    #[test]
    fn module_compiles() {
        let _ = std::mem::size_of::<ExportEncodingDialog>();
    }
}
