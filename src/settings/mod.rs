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

/// Language (Pro only).
#[cfg(feature = "pro")]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Language {
    En,
    Fr,
    Es,
    De,
}

#[cfg(feature = "pro")]
impl Language {
    fn from_str(s: &str) -> Option<Language> {
        match s.trim().to_lowercase().as_str() {
            "en" => Some(Language::En),
            "fr" => Some(Language::Fr),
            "es" => Some(Language::Es),
            "de" => Some(Language::De),
            _ => None,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Language::En => "en",
            Language::Fr => "fr",
            Language::Es => "es",
            Language::De => "de",
        }
    }
}

/// TOTP entry (Pro only).
#[cfg(feature = "pro")]
#[derive(Debug, Clone, PartialEq)]
pub struct TotpEntry {
    pub name: String,
    pub secret: String,
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

    // Pro-only fields
    #[cfg(feature = "pro")]
    pub language: Language,
    #[cfg(feature = "pro")]
    pub active_tab: u32,
    #[cfg(feature = "pro")]
    pub auto_lock: u32,
    #[cfg(feature = "pro")]
    pub clipboard_clear: u32,
    #[cfg(feature = "pro")]
    pub auto_save: u32,
    #[cfg(feature = "pro")]
    pub show_line_numbers: bool,
    #[cfg(feature = "pro")]
    pub totp_2fa: bool,
    #[cfg(feature = "pro")]
    pub totp_2fa_secret: String,
    #[cfg(feature = "pro")]
    pub totp_entries: Vec<TotpEntry>,
    #[cfg(feature = "pro")]
    pub hotkey_modifiers: u32,
    #[cfg(feature = "pro")]
    pub hotkey_key: u32,
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
            minimize_to_tray: true,
            min_password_length: 4,
            font_family: "Consolas".to_string(),
            font_size: 11.0,
            #[cfg(feature = "pro")]
            language: Language::En,
            #[cfg(feature = "pro")]
            active_tab: 0,
            #[cfg(feature = "pro")]
            auto_lock: 15,
            #[cfg(feature = "pro")]
            clipboard_clear: 30,
            #[cfg(feature = "pro")]
            auto_save: 30,
            #[cfg(feature = "pro")]
            show_line_numbers: true,
            #[cfg(feature = "pro")]
            totp_2fa: false,
            #[cfg(feature = "pro")]
            totp_2fa_secret: String::new(),
            #[cfg(feature = "pro")]
            totp_entries: Vec::new(),
            #[cfg(feature = "pro")]
            hotkey_modifiers: 6,
            #[cfg(feature = "pro")]
            hotkey_key: 76,
        }
    }

    /// Default settings for the Pro build.
    #[cfg(feature = "pro")]
    pub fn default_pro() -> Self {
        let mut s = Self::default_public();
        s.min_password_length = 6;
        s
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
            #[cfg(feature = "pro")]
            "language" => {
                if let Some(v) = Language::from_str(value) {
                    self.language = v;
                }
            }
            #[cfg(feature = "pro")]
            "active_tab" => {
                if let Ok(v) = value.parse::<u32>() {
                    self.active_tab = v;
                }
            }
            #[cfg(feature = "pro")]
            "auto_lock" => {
                if let Ok(v) = value.parse::<u32>() {
                    self.auto_lock = v;
                }
            }
            #[cfg(feature = "pro")]
            "clipboard_clear" => {
                if let Ok(v) = value.parse::<u32>() {
                    self.clipboard_clear = v;
                }
            }
            #[cfg(feature = "pro")]
            "auto_save" => {
                if let Ok(v) = value.parse::<u32>() {
                    self.auto_save = v;
                }
            }
            #[cfg(feature = "pro")]
            "show_line_numbers" => {
                if let Some(v) = parse_bool(value) {
                    self.show_line_numbers = v;
                }
            }
            #[cfg(feature = "pro")]
            "totp_2fa" => {
                if let Some(v) = parse_bool(value) {
                    self.totp_2fa = v;
                }
            }
            #[cfg(feature = "pro")]
            "totp_2fa_secret" => {
                self.totp_2fa_secret = value.to_string();
            }
            #[cfg(feature = "pro")]
            "totp_entry" => {
                // Format: name|BASE32SECRET
                if let Some(pipe) = value.find('|') {
                    let name = value[..pipe].to_string();
                    let secret = value[pipe + 1..].to_string();
                    if !name.is_empty() && !secret.is_empty() {
                        self.totp_entries.push(TotpEntry { name, secret });
                    }
                }
            }
            #[cfg(feature = "pro")]
            "hotkey_modifiers" => {
                if let Ok(v) = value.parse::<u32>() {
                    self.hotkey_modifiers = v;
                }
            }
            #[cfg(feature = "pro")]
            "hotkey_key" => {
                if let Ok(v) = value.parse::<u32>() {
                    self.hotkey_key = v;
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

        #[cfg(feature = "pro")]
        {
            out.push_str(&format!("language={}\n", self.language.as_str()));
            out.push_str(&format!("active_tab={}\n", self.active_tab));
            out.push_str(&format!("auto_lock={}\n", self.auto_lock));
            out.push_str(&format!("clipboard_clear={}\n", self.clipboard_clear));
            out.push_str(&format!("auto_save={}\n", self.auto_save));
            out.push_str(&format!("show_line_numbers={}\n", bool_str(self.show_line_numbers)));
            out.push_str(&format!("totp_2fa={}\n", bool_str(self.totp_2fa)));
            out.push_str(&format!("totp_2fa_secret={}\n", self.totp_2fa_secret));
            for entry in &self.totp_entries {
                out.push_str(&format!("totp_entry={}|{}\n", entry.name, entry.secret));
            }
            out.push_str(&format!("hotkey_modifiers={}\n", self.hotkey_modifiers));
            out.push_str(&format!("hotkey_key={}\n", self.hotkey_key));
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
        assert!(s.minimize_to_tray);
        assert_eq!(s.min_password_length, 4);
        assert_eq!(s.font_family, "Consolas");
        assert_eq!(s.font_size, 11.0);
    }

    #[cfg(feature = "pro")]
    #[test]
    fn test_default_pro() {
        let s = Settings::default_pro();
        assert_eq!(s.min_password_length, 6);
        assert_eq!(s.language, Language::En);
        assert_eq!(s.active_tab, 0);
        assert_eq!(s.auto_lock, 15);
        assert_eq!(s.clipboard_clear, 30);
        assert_eq!(s.auto_save, 30);
        assert!(s.show_line_numbers);
        assert!(!s.totp_2fa);
        assert_eq!(s.totp_2fa_secret, "");
        assert!(s.totp_entries.is_empty());
        assert_eq!(s.hotkey_modifiers, 6);
        assert_eq!(s.hotkey_key, 76);
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

    #[cfg(feature = "pro")]
    #[test]
    fn test_pro_totp_entries() {
        let input = "[LOCKNOTE_SETTINGS]\ntotp_entry=Gmail|JBSWY3DPEHPK3PXP\ntotp_entry=AWS|KZXW6YTBOI======\n[/LOCKNOTE_SETTINGS]\nNote";
        let (s, _) = Settings::parse(input);
        assert_eq!(s.totp_entries.len(), 2);
        assert_eq!(s.totp_entries[0].name, "Gmail");
        assert_eq!(s.totp_entries[0].secret, "JBSWY3DPEHPK3PXP");
        assert_eq!(s.totp_entries[1].name, "AWS");
    }

    #[cfg(feature = "pro")]
    #[test]
    fn test_pro_roundtrip() {
        let mut s = Settings::default_pro();
        s.language = Language::Fr;
        s.active_tab = 2;
        s.auto_lock = 0;
        s.totp_2fa = true;
        s.totp_entries.push(TotpEntry { name: "Test".into(), secret: "SECRET".into() });

        let serialized = s.serialize("Pro note");
        let (s2, note) = Settings::parse(&serialized);
        assert_eq!(s2.language, Language::Fr);
        assert_eq!(s2.active_tab, 2);
        assert_eq!(s2.auto_lock, 0);
        assert!(s2.totp_2fa);
        assert_eq!(s2.totp_entries.len(), 1);
        assert_eq!(note, "Pro note");
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
}
