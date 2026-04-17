// Settings module — User settings serialization/deserialization
//
// Format:
//   [LOCKNOTE_SETTINGS]
//   key=value
//   [/LOCKNOTE_SETTINGS]
//   <note content>

/// Save-on-close behavior.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CloseAction {
    Ask,
    Always,
    Never,
}

impl CloseAction {
    fn from_str(s: &str) -> Option<CloseAction> {
        match s.trim().to_lowercase().as_str() {
            "ask" => Some(CloseAction::Ask),
            "always" => Some(CloseAction::Always),
            "never" => Some(CloseAction::Never),
            _ => None,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            CloseAction::Ask => "ask",
            CloseAction::Always => "always",
            CloseAction::Never => "never",
        }
    }
}

/// Theme choice.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ThemeChoice {
    Dark,
    Light,
}

impl ThemeChoice {
    fn from_str(s: &str) -> Option<ThemeChoice> {
        match s.trim().to_lowercase().as_str() {
            "dark" => Some(ThemeChoice::Dark),
            "light" => Some(ThemeChoice::Light),
            _ => None,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            ThemeChoice::Dark => "dark",
            ThemeChoice::Light => "light",
        }
    }
}

/// User settings.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    // Public fields
    pub save_on_close: CloseAction,
    pub theme: ThemeChoice,
    pub word_wrap: bool,
    pub minimize_to_tray: bool,
    pub min_password_length: u32,
    pub font_family: String,
    pub font_size: f64,
    pub save_key: u32,       // Virtual key code for save shortcut (default 0x53 = 'S')
    pub save_modifiers: u32,  // Modifier bitmask: 0x01=Alt, 0x02=Ctrl, 0x04=Shift

    // Window position/size (persisted between sessions)
    pub window_width: u32,
    pub window_height: u32,
    pub window_x: i32,
    pub window_y: i32,
    pub window_maximized: bool,
}

fn parse_bool(s: &str) -> Option<bool> {
    match s.trim().to_lowercase().as_str() {
        "on" | "true" | "1" | "yes" => Some(true),
        "off" | "false" | "0" | "no" => Some(false),
        _ => None,
    }
}

fn bool_str(b: bool) -> &'static str {
    if b { "on" } else { "off" }
}

fn clamp_f64(v: f64, min: f64, max: f64) -> f64 {
    if v < min { min } else if v > max { max } else { v }
}

fn clamp_u32(v: u32, min: u32, max: u32) -> u32 {
    if v < min { min } else if v > max { max } else { v }
}

const HEADER: &str = "[LOCKNOTE_SETTINGS]";
const FOOTER: &str = "[/LOCKNOTE_SETTINGS]";

impl Settings {
    /// Default settings for the public build.
    pub fn default_public() -> Self {
        Settings {
            save_on_close: CloseAction::Ask,
            theme: ThemeChoice::Dark,
            word_wrap: true,
            minimize_to_tray: false,
            min_password_length: 4,
            font_family: "Consolas".to_string(),
            font_size: 11.0,
            save_key: 0x53, // 'S'
            save_modifiers: 0x02, // Ctrl
            window_width: 900,
            window_height: 650,
            window_x: i32::MIN,
            window_y: i32::MIN,
            window_maximized: false,
        }
    }

    /// Parse settings from input text. Returns (settings, remaining_note_text).
    /// If no header is found, all defaults are used and the entire input is note text.
    pub fn parse(input: &str) -> (Settings, String) {
        if input.is_empty() {
            return (Self::default_public(), String::new());
        }

        // Check for header
        let trimmed = input.trim_start();
        if !trimmed.starts_with(HEADER) {
            return (Self::default_public(), input.to_string());
        }

        // Find header end (after the header line)
        let header_start = match input.find(HEADER) {
            Some(pos) => pos,
            None => return (Self::default_public(), input.to_string()),
        };
        let after_header = header_start + HEADER.len();

        // Find footer
        let footer_pos = match input[after_header..].find(FOOTER) {
            Some(pos) => after_header + pos,
            None => {
                // No footer — treat entire input as note text
                return (Self::default_public(), input.to_string());
            }
        };

        let settings_block = &input[after_header..footer_pos];
        let after_footer = footer_pos + FOOTER.len();

        // Strip leading CR/LF after footer
        let mut note_start = after_footer;
        let bytes = input.as_bytes();
        while note_start < bytes.len() && (bytes[note_start] == b'\r' || bytes[note_start] == b'\n') {
            note_start += 1;
        }
        let note_text = &input[note_start..];

        let mut settings = Self::default_public();

        for line in settings_block.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(eq_pos) = line.find('=') {
                let key = line[..eq_pos].trim();
                let value = line[eq_pos + 1..].trim();
                settings.apply_key(key, value);
            }
        }

        (settings, note_text.to_string())
    }

    fn apply_key(&mut self, key: &str, value: &str) {
        match key {
            "save_on_close" => {
                if let Some(v) = CloseAction::from_str(value) {
                    self.save_on_close = v;
                }
            }
            "theme" => {
                if let Some(v) = ThemeChoice::from_str(value) {
                    self.theme = v;
                }
            }
            "word_wrap" => {
                if let Some(v) = parse_bool(value) {
                    self.word_wrap = v;
                }
            }
            "minimize_to_tray" => {
                if let Some(v) = parse_bool(value) {
                    self.minimize_to_tray = v;
                }
            }
            "min_password_length" => {
                if let Ok(v) = value.parse::<u32>() {
                    self.min_password_length = clamp_u32(v, 1, 64);
                }
            }
            "font_family" => {
                if !value.is_empty() {
                    self.font_family = value.to_string();
                }
            }
            "font_size" => {
                if let Ok(v) = value.parse::<f64>() {
                    self.font_size = clamp_f64(v, 6.0, 72.0);
                }
            }
            "save_key" => {
                if let Ok(v) = value.parse::<u32>() {
                    if (0x41..=0x5A).contains(&v) {
                        self.save_key = v;
                    }
                }
            }
            "save_modifiers" => {
                if let Ok(v) = value.parse::<u32>() {
                    if v > 0 && v <= 0x07 {
                        self.save_modifiers = v;
                    }
                }
            }
            "window_width" => {
                if let Ok(v) = value.parse::<u32>() {
                    self.window_width = clamp_u32(v, 200, 8000);
                }
            }
            "window_height" => {
                if let Ok(v) = value.parse::<u32>() {
                    self.window_height = clamp_u32(v, 150, 8000);
                }
            }
            "window_x" => {
                if let Ok(v) = value.parse::<i32>() {
                    self.window_x = v;
                }
            }
            "window_y" => {
                if let Ok(v) = value.parse::<i32>() {
                    self.window_y = v;
                }
            }
            "window_maximized" => {
                if let Some(v) = parse_bool(value) {
                    self.window_maximized = v;
                }
            }
            _ => {
                // Unknown keys silently ignored
            }
        }
    }

    /// Serialize settings + note text into the stored format.
    pub fn serialize(&self, note_text: &str) -> String {
        let mut out = String::new();
        out.push_str(HEADER);
        out.push('\n');

        out.push_str(&format!("save_on_close={}\n", self.save_on_close.as_str()));
        out.push_str(&format!("theme={}\n", self.theme.as_str()));
        out.push_str(&format!("word_wrap={}\n", bool_str(self.word_wrap)));
        out.push_str(&format!("minimize_to_tray={}\n", bool_str(self.minimize_to_tray)));
        out.push_str(&format!("min_password_length={}\n", self.min_password_length));
        out.push_str(&format!("font_family={}\n", self.font_family));

        // Serialize font_size: avoid trailing zeros but keep one decimal
        if self.font_size == self.font_size.floor() {
            out.push_str(&format!("font_size={:.1}\n", self.font_size));
        } else {
            out.push_str(&format!("font_size={}\n", self.font_size));
        }
        out.push_str(&format!("save_key={}\n", self.save_key));
        out.push_str(&format!("save_modifiers={}\n", self.save_modifiers));

        // Window position/size (only if explicitly set)
        if self.window_x != i32::MIN {
            out.push_str(&format!("window_width={}\n", self.window_width));
            out.push_str(&format!("window_height={}\n", self.window_height));
            out.push_str(&format!("window_x={}\n", self.window_x));
            out.push_str(&format!("window_y={}\n", self.window_y));
            out.push_str(&format!("window_maximized={}\n", bool_str(self.window_maximized)));
        }

        out.push_str(FOOTER);
        out.push('\n');
        out.push_str(note_text);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_public() {
        let s = Settings::default_public();
        assert_eq!(s.save_on_close, CloseAction::Ask);
        assert_eq!(s.theme, ThemeChoice::Dark);
        assert!(s.word_wrap);
        assert!(!s.minimize_to_tray);
        assert_eq!(s.min_password_length, 4);
        assert_eq!(s.font_family, "Consolas");
        assert_eq!(s.font_size, 11.0);
    }

    #[test]
    fn test_parse_empty() {
        let (s, note) = Settings::parse("");
        assert_eq!(s, Settings::default_public());
        assert_eq!(note, "");
    }

    #[test]
    fn test_parse_no_header() {
        let input = "Hello, world!\nThis is my note.";
        let (s, note) = Settings::parse(input);
        assert_eq!(s, Settings::default_public());
        assert_eq!(note, input);
    }

    #[test]
    fn test_parse_basic_header() {
        let input = "[LOCKNOTE_SETTINGS]\ntheme=light\nsave_on_close=always\n[/LOCKNOTE_SETTINGS]\nMy note";
        let (s, note) = Settings::parse(input);
        assert_eq!(s.theme, ThemeChoice::Light);
        assert_eq!(s.save_on_close, CloseAction::Always);
        assert_eq!(note, "My note");
    }

    #[test]
    fn test_parse_unknown_keys_ignored() {
        let input = "[LOCKNOTE_SETTINGS]\nfuture_key=42\ntheme=light\n[/LOCKNOTE_SETTINGS]\nNote";
        let (s, note) = Settings::parse(input);
        assert_eq!(s.theme, ThemeChoice::Light);
        assert_eq!(note, "Note");
    }

    #[test]
    fn test_parse_invalid_numeric_uses_default() {
        let input = "[LOCKNOTE_SETTINGS]\nmin_password_length=abc\nfont_size=xyz\n[/LOCKNOTE_SETTINGS]\nNote";
        let (s, _) = Settings::parse(input);
        assert_eq!(s.min_password_length, 4); // default
        assert_eq!(s.font_size, 11.0); // default
    }

    #[test]
    fn test_font_size_clamping() {
        let input = "[LOCKNOTE_SETTINGS]\nfont_size=2.0\n[/LOCKNOTE_SETTINGS]\n";
        let (s, _) = Settings::parse(input);
        assert_eq!(s.font_size, 6.0);

        let input = "[LOCKNOTE_SETTINGS]\nfont_size=100.0\n[/LOCKNOTE_SETTINGS]\n";
        let (s, _) = Settings::parse(input);
        assert_eq!(s.font_size, 72.0);
    }

    #[test]
    fn test_password_length_clamping() {
        let input = "[LOCKNOTE_SETTINGS]\nmin_password_length=0\n[/LOCKNOTE_SETTINGS]\n";
        let (s, _) = Settings::parse(input);
        assert_eq!(s.min_password_length, 1);

        let input = "[LOCKNOTE_SETTINGS]\nmin_password_length=200\n[/LOCKNOTE_SETTINGS]\n";
        let (s, _) = Settings::parse(input);
        assert_eq!(s.min_password_length, 64);
    }

    #[test]
    fn test_close_actions() {
        for (val, expected) in [("ask", CloseAction::Ask), ("always", CloseAction::Always), ("never", CloseAction::Never)] {
            let input = format!("[LOCKNOTE_SETTINGS]\nsave_on_close={}\n[/LOCKNOTE_SETTINGS]\n", val);
            let (s, _) = Settings::parse(&input);
            assert_eq!(s.save_on_close, expected);
        }
    }

    #[test]
    fn test_roundtrip() {
        let mut s = Settings::default_public();
        s.theme = ThemeChoice::Light;
        s.save_on_close = CloseAction::Never;
        s.font_size = 14.5;
        s.font_family = "Courier New".to_string();
        s.word_wrap = false;

        let note = "Line 1\nLine 2\nLine 3";
        let serialized = s.serialize(note);
        let (s2, note2) = Settings::parse(&serialized);

        assert_eq!(s2.theme, ThemeChoice::Light);
        assert_eq!(s2.save_on_close, CloseAction::Never);
        assert_eq!(s2.font_size, 14.5);
        assert_eq!(s2.font_family, "Courier New");
        assert!(!s2.word_wrap);
        assert_eq!(note2, note);
    }

    #[test]
    fn test_multiline_note_content() {
        let note = "First line\n\nThird line\n\n\nSixth line";
        let s = Settings::default_public();
        let serialized = s.serialize(note);
        let (_, note2) = Settings::parse(&serialized);
        assert_eq!(note2, note);
    }

    #[test]
    fn test_strip_leading_crlf_after_footer() {
        let input = "[LOCKNOTE_SETTINGS]\ntheme=dark\n[/LOCKNOTE_SETTINGS]\r\n\nNote";
        let (_, note) = Settings::parse(input);
        assert_eq!(note, "Note");
    }

    #[test]
    fn test_serialize_format() {
        let s = Settings::default_public();
        let out = s.serialize("hello");
        assert!(out.starts_with("[LOCKNOTE_SETTINGS]\n"));
        assert!(out.contains("[/LOCKNOTE_SETTINGS]\n"));
        assert!(out.ends_with("hello"));
    }

    #[test]
    fn test_bool_parsing_variants() {
        for (val, expected) in [("on", true), ("off", false), ("true", true), ("false", false), ("1", true), ("0", false)] {
            let input = format!("[LOCKNOTE_SETTINGS]\nword_wrap={}\n[/LOCKNOTE_SETTINGS]\n", val);
            let (s, _) = Settings::parse(&input);
            assert_eq!(s.word_wrap, expected, "Failed for word_wrap={}", val);
        }
    }

    #[test]
    fn test_no_footer_returns_all_as_note() {
        let input = "[LOCKNOTE_SETTINGS]\ntheme=light\nSome text without footer";
        let (s, note) = Settings::parse(input);
        // No footer → defaults, entire input is note
        assert_eq!(s, Settings::default_public());
        assert_eq!(note, input);
    }

    #[test]
    fn test_crypto_chain_settings_then_note() {
        // Simulates what happens after decryption: settings block + note
        let settings = Settings::default_public();
        let note_content = "Sensitive data here\nWith multiple lines\nAnd special chars: <>&\"'";
        let combined = settings.serialize(note_content);

        let (parsed_settings, parsed_note) = Settings::parse(&combined);
        assert_eq!(parsed_settings.theme, settings.theme);
        assert_eq!(parsed_note, note_content);
    }

    #[test]
    fn test_parse_leading_whitespace_before_header() {
        // Edge case: whitespace before header should not panic
        let input = "   \n  [LOCKNOTE_SETTINGS]\ntheme=light\n[/LOCKNOTE_SETTINGS]\nNote";
        let (s, note) = Settings::parse(input);
        assert_eq!(s.theme, ThemeChoice::Light);
        assert_eq!(note, "Note");
    }

    #[test]
    fn test_parse_only_whitespace() {
        let input = "   \n  \n  ";
        let (s, note) = Settings::parse(input);
        assert_eq!(s, Settings::default_public());
        assert_eq!(note, input);
    }

    #[test]
    fn test_parse_unicode_note_content() {
        let note = "Café ☕ 日本語 Привет 🔒📝";
        let s = Settings::default_public();
        let serialized = s.serialize(note);
        let (_, parsed_note) = Settings::parse(&serialized);
        assert_eq!(parsed_note, note);
    }

    #[test]
    fn test_parse_empty_settings_block() {
        let input = "[LOCKNOTE_SETTINGS]\n[/LOCKNOTE_SETTINGS]\nJust a note";
        let (s, note) = Settings::parse(input);
        assert_eq!(s, Settings::default_public());
        assert_eq!(note, "Just a note");
    }

    // ── Window position/size tests ──

    #[test]
    fn test_window_defaults() {
        let s = Settings::default_public();
        assert_eq!(s.window_width, 900);
        assert_eq!(s.window_height, 650);
        assert_eq!(s.window_x, i32::MIN);
        assert_eq!(s.window_y, i32::MIN);
        assert!(!s.window_maximized);
    }

    #[test]
    fn test_window_not_serialized_when_default() {
        let s = Settings::default_public();
        let out = s.serialize("note");
        assert!(!out.contains("window_width"));
        assert!(!out.contains("window_x"));
    }

    #[test]
    fn test_window_serialized_when_set() {
        let mut s = Settings::default_public();
        s.window_x = 100;
        s.window_y = 200;
        s.window_width = 1024;
        s.window_height = 768;
        s.window_maximized = true;

        let out = s.serialize("note");
        assert!(out.contains("window_width=1024"));
        assert!(out.contains("window_height=768"));
        assert!(out.contains("window_x=100"));
        assert!(out.contains("window_y=200"));
        assert!(out.contains("window_maximized=on"));
    }

    #[test]
    fn test_window_roundtrip() {
        let mut s = Settings::default_public();
        s.window_x = 50;
        s.window_y = -10;
        s.window_width = 1200;
        s.window_height = 800;
        s.window_maximized = false;

        let serialized = s.serialize("data");
        let (s2, note) = Settings::parse(&serialized);
        assert_eq!(s2.window_x, 50);
        assert_eq!(s2.window_y, -10);
        assert_eq!(s2.window_width, 1200);
        assert_eq!(s2.window_height, 800);
        assert!(!s2.window_maximized);
        assert_eq!(note, "data");
    }

    #[test]
    fn test_window_width_clamped() {
        let input = "[LOCKNOTE_SETTINGS]\nwindow_width=50\nwindow_height=50\n[/LOCKNOTE_SETTINGS]\n";
        let (s, _) = Settings::parse(input);
        assert_eq!(s.window_width, 200);
        assert_eq!(s.window_height, 150);
    }

    #[test]
    fn test_window_negative_coordinates() {
        let input = "[LOCKNOTE_SETTINGS]\nwindow_x=-500\nwindow_y=-200\n[/LOCKNOTE_SETTINGS]\n";
        let (s, _) = Settings::parse(input);
        assert_eq!(s.window_x, -500);
        assert_eq!(s.window_y, -200);
    }

    #[test]
    fn test_window_backward_compatible() {
        // Old settings without window fields should use defaults
        let input = "[LOCKNOTE_SETTINGS]\ntheme=light\n[/LOCKNOTE_SETTINGS]\nOld note";
        let (s, _) = Settings::parse(input);
        assert_eq!(s.window_width, 900);
        assert_eq!(s.window_height, 650);
        assert_eq!(s.window_x, i32::MIN);
    }

    // ── Edge-case and robustness tests ──

    #[test]
    fn test_note_containing_settings_header() {
        // Note body literally contains "[LOCKNOTE_SETTINGS]" — should survive roundtrip
        let note = "Here is a fake header: [LOCKNOTE_SETTINGS] in my note";
        let s = Settings::default_public();
        let serialized = s.serialize(note);
        let (s2, note2) = Settings::parse(&serialized);
        assert_eq!(s2, s);
        assert_eq!(note2, note);
    }

    #[test]
    fn test_note_containing_settings_footer() {
        // Note body literally contains "[/LOCKNOTE_SETTINGS]"
        let note = "Some text [/LOCKNOTE_SETTINGS] more text";
        let s = Settings::default_public();
        let serialized = s.serialize(note);
        let (s2, note2) = Settings::parse(&serialized);
        assert_eq!(s2, s);
        assert_eq!(note2, note);
    }

    #[test]
    fn test_duplicate_keys_last_wins() {
        let input = "[LOCKNOTE_SETTINGS]\ntheme=dark\ntheme=light\n[/LOCKNOTE_SETTINGS]\n";
        let (s, _) = Settings::parse(input);
        assert_eq!(s.theme, ThemeChoice::Light);
    }

    #[test]
    fn test_key_with_equals_in_value() {
        // find('=') returns the first '=', so value should be "Courier=New"
        let input = "[LOCKNOTE_SETTINGS]\nfont_family=Courier=New\n[/LOCKNOTE_SETTINGS]\n";
        let (s, _) = Settings::parse(input);
        assert_eq!(s.font_family, "Courier=New");
    }

    #[test]
    fn test_empty_value() {
        // "font_family=" → empty value, should NOT override default due to !value.is_empty() guard
        let input = "[LOCKNOTE_SETTINGS]\nfont_family=\n[/LOCKNOTE_SETTINGS]\n";
        let (s, _) = Settings::parse(input);
        assert_eq!(s.font_family, "Consolas");
    }

    #[test]
    fn test_window_height_clamping() {
        let input = "[LOCKNOTE_SETTINGS]\nwindow_height=50\n[/LOCKNOTE_SETTINGS]\n";
        let (s, _) = Settings::parse(input);
        assert_eq!(s.window_height, 150);

        let input = "[LOCKNOTE_SETTINGS]\nwindow_height=9999\n[/LOCKNOTE_SETTINGS]\n";
        let (s, _) = Settings::parse(input);
        assert_eq!(s.window_height, 8000);
    }

    #[test]
    fn test_bool_yes_no_variants() {
        let input = "[LOCKNOTE_SETTINGS]\nword_wrap=yes\n[/LOCKNOTE_SETTINGS]\n";
        let (s, _) = Settings::parse(input);
        assert!(s.word_wrap);

        let input = "[LOCKNOTE_SETTINGS]\nword_wrap=no\n[/LOCKNOTE_SETTINGS]\n";
        let (s, _) = Settings::parse(input);
        assert!(!s.word_wrap);
    }

    #[test]
    fn test_invalid_bool_uses_default() {
        // word_wrap default is true; "maybe" is invalid and should keep default
        let input = "[LOCKNOTE_SETTINGS]\nword_wrap=maybe\n[/LOCKNOTE_SETTINGS]\n";
        let (s, _) = Settings::parse(input);
        assert!(s.word_wrap);
    }

    #[test]
    fn test_invalid_theme_uses_default() {
        // Default theme is Dark; "blue" is invalid
        let input = "[LOCKNOTE_SETTINGS]\ntheme=blue\n[/LOCKNOTE_SETTINGS]\n";
        let (s, _) = Settings::parse(input);
        assert_eq!(s.theme, ThemeChoice::Dark);
    }

    #[test]
    fn test_invalid_close_action_uses_default() {
        // Default is Ask; "sometimes" is invalid
        let input = "[LOCKNOTE_SETTINGS]\nsave_on_close=sometimes\n[/LOCKNOTE_SETTINGS]\n";
        let (s, _) = Settings::parse(input);
        assert_eq!(s.save_on_close, CloseAction::Ask);
    }

    #[test]
    fn test_settings_block_with_blank_lines() {
        let input = "[LOCKNOTE_SETTINGS]\n\ntheme=light\n\nword_wrap=off\n\n[/LOCKNOTE_SETTINGS]\nNote";
        let (s, note) = Settings::parse(input);
        assert_eq!(s.theme, ThemeChoice::Light);
        assert!(!s.word_wrap);
        assert_eq!(note, "Note");
    }

    #[test]
    fn test_very_long_note() {
        let note: String = "x".repeat(100_000);
        let s = Settings::default_public();
        let serialized = s.serialize(&note);
        let (s2, note2) = Settings::parse(&serialized);
        assert_eq!(s2, s);
        assert_eq!(note2.len(), 100_000);
        assert_eq!(note2, note);
    }

    #[test]
    fn test_note_with_only_newlines() {
        // Note text is just newlines — after footer stripping, leading \n are consumed
        let s = Settings::default_public();
        let serialized = s.serialize("\n\n\n\n");
        let (s2, note2) = Settings::parse(&serialized);
        assert_eq!(s2, s);
        // The serialize format is: ...FOOTER\n + note_text
        // On parse, leading \r\n after footer are stripped, so "\n\n\n\n" becomes ""
        // because the parser eats all leading newlines after the footer
        assert_eq!(note2, "");
    }

    #[test]
    fn test_crlf_in_settings_block() {
        // Settings block with \r\n line endings — .lines() handles both
        let input = "[LOCKNOTE_SETTINGS]\r\ntheme=light\r\nword_wrap=off\r\n[/LOCKNOTE_SETTINGS]\r\nMy note";
        let (s, note) = Settings::parse(input);
        assert_eq!(s.theme, ThemeChoice::Light);
        assert!(!s.word_wrap);
        assert_eq!(note, "My note");
    }
}
