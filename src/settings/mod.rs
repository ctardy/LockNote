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

/// Language of the interface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Language {
    En,
    Fr,
    Es,
    De,
}

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

/// TOTP entry.
//
// Debug is implemented manually below so `{:?}` redacts the base32 secret —
// a crash log or debug print must never leak a 2FA seed, but we still want
// assert_eq! etc. to work in tests. ZeroizeOnDrop wipes `secret` when the
// entry is dropped or replaced.
#[derive(Clone, PartialEq, zeroize::Zeroize, zeroize::ZeroizeOnDrop)]
pub struct TotpEntry {
    #[zeroize(skip)]
    pub name: String,
    pub secret: String,
}

impl std::fmt::Debug for TotpEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TotpEntry")
            .field("name", &self.name)
            .field("secret", &"<redacted>")
            .finish()
    }
}

/// User settings.
//
// Debug is implemented manually below so `{:?}` redacts the base32 2FA
// seed — a panic log must never leak it — while still allowing
// `assert_eq!` in tests and `{:?}` printing elsewhere.
#[derive(Clone, PartialEq)]
pub struct Settings {
    // Fields of every version (1.2.1 included)
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

    // Fields added by the former Pro edition (absent from 1.2.1 files)
    pub language: Language,
    pub active_tab: u32,
    pub auto_lock: u32,
    pub clipboard_clear: u32,
    pub auto_save: u32,
    pub show_line_numbers: bool,
    pub totp_2fa: bool,
    pub totp_2fa_secret: String,
    pub totp_entries: Vec<TotpEntry>,
    /// User vault categories (issue 052); empty = never customized, the
    /// 7 defaults apply. Stored as repeated `vault_category=` keys.
    pub vault_categories: Vec<String>,
    pub hotkey_modifiers: u32,
    pub hotkey_key: u32,
    pub max_attachment_size_mb: u32,
    pub max_total_attachments_mb: u32,
    /// Password generator options (keys `passgen_*`).
    pub passgen: crate::features::passgen::Options,
    /// Width of each right-side panel, set by dragging its grip (machine
    /// state, like the window geometry; see `ui::side_panel`).
    pub vault_panel_width: u32,
    pub totp_panel_width: u32,
    pub attach_panel_width: u32,
    pub prompts_panel_width: u32,
}

// Custom Debug: redact the 2FA seed. All other fields are fine to print
// (they're UI config — font, theme, intervals, etc.). We list every field
// explicitly instead of using `..` so a future sensitive field added to
// the struct will cause a compile error here — forcing the developer to
// decide whether it should be redacted or printed.
impl std::fmt::Debug for Settings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut ds = f.debug_struct("Settings");
        ds.field("save_on_close", &self.save_on_close)
            .field("theme", &self.theme)
            .field("word_wrap", &self.word_wrap)
            .field("minimize_to_tray", &self.minimize_to_tray)
            .field("min_password_length", &self.min_password_length)
            .field("font_family", &self.font_family)
            .field("font_size", &self.font_size)
            .field("save_key", &self.save_key)
            .field("save_modifiers", &self.save_modifiers)
            .field("window_width", &self.window_width)
            .field("window_height", &self.window_height)
            .field("window_x", &self.window_x)
            .field("window_y", &self.window_y)
            .field("window_maximized", &self.window_maximized);
        {
            ds.field("language", &self.language)
                .field("active_tab", &self.active_tab)
                .field("auto_lock", &self.auto_lock)
                .field("clipboard_clear", &self.clipboard_clear)
                .field("auto_save", &self.auto_save)
                .field("show_line_numbers", &self.show_line_numbers)
                .field("totp_2fa", &self.totp_2fa)
                .field("totp_2fa_secret", &"<redacted>")
                .field("totp_entries", &self.totp_entries)
                .field("vault_categories", &self.vault_categories)
                .field("hotkey_modifiers", &self.hotkey_modifiers)
                .field("hotkey_key", &self.hotkey_key)
                .field("max_attachment_size_mb", &self.max_attachment_size_mb)
                .field("max_total_attachments_mb", &self.max_total_attachments_mb)
                .field("passgen", &self.passgen)
                .field("vault_panel_width", &self.vault_panel_width)
                .field("totp_panel_width", &self.totp_panel_width)
                .field("attach_panel_width", &self.attach_panel_width)
                .field("prompts_panel_width", &self.prompts_panel_width);
        }
        ds.finish()
    }
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

/// The settings a new note starts with (issue 059, D9: those of the former
/// Pro edition, the only ones since 2.0). `Settings::parse` starts from
/// `parse_fallback` instead.
impl Default for Settings {
    fn default() -> Self {
        Settings {
            save_on_close: CloseAction::Ask,
            theme: ThemeChoice::Dark,
            word_wrap: true,
            minimize_to_tray: false,
            min_password_length: 6,
            font_family: "Consolas".to_string(),
            font_size: 11.0,
            save_key: 0x53, // 'S'
            save_modifiers: 0x02, // Ctrl
            window_width: 900,
            window_height: 650,
            window_x: i32::MIN,
            window_y: i32::MIN,
            window_maximized: false,
            language: Language::En,
            active_tab: 0,
            auto_lock: 15,
            clipboard_clear: 30,
            auto_save: 30,
            show_line_numbers: true,
            totp_2fa: false,
            totp_2fa_secret: String::new(),
            totp_entries: Vec::new(),
            vault_categories: Vec::new(),
            hotkey_modifiers: 6,
            hotkey_key: 76,
            max_attachment_size_mb: 10,
            max_total_attachments_mb: 50,
            passgen: crate::features::passgen::Options::default(),
            vault_panel_width: crate::ui::side_panel::default_width(crate::ui::side_panel::SidePanel::Vault),
            totp_panel_width: crate::ui::side_panel::default_width(crate::ui::side_panel::SidePanel::Totp),
            attach_panel_width: crate::ui::side_panel::default_width(crate::ui::side_panel::SidePanel::Attach),
            prompts_panel_width: crate::ui::side_panel::default_width(crate::ui::side_panel::SidePanel::Prompts),
        }
    }

}

impl Settings {
    /// What `parse` gives the keys a file does not hold, and a file without
    /// settings: the defaults, but a minimum password length of 4, the value
    /// every build up to 1.2.27 fell back to. Kept so that existing files read
    /// back the same (issue 059, D9).
    fn parse_fallback() -> Self {
        Settings { min_password_length: 4, ..Self::default() }
    }

    /// Parse settings from input text. Returns (settings, remaining_note_text).
    /// If no header is found, all defaults are used and the entire input is note text.
    pub fn parse(input: &str) -> (Settings, String) {
        if input.is_empty() {
            return (Self::parse_fallback(), String::new());
        }

        // Check for header
        let trimmed = input.trim_start();
        if !trimmed.starts_with(HEADER) {
            return (Self::parse_fallback(), input.to_string());
        }

        // Find header end (after the header line)
        let header_start = match input.find(HEADER) {
            Some(pos) => pos,
            None => return (Self::parse_fallback(), input.to_string()),
        };
        let after_header = header_start + HEADER.len();

        // Find footer
        let footer_pos = match input[after_header..].find(FOOTER) {
            Some(pos) => after_header + pos,
            None => {
                // No footer — treat entire input as note text
                return (Self::parse_fallback(), input.to_string());
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

        let mut settings = Self::parse_fallback();

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
            "pbkdf2_iterations" => {
                // Ignored — KDF migrated from PBKDF2 to Argon2id, kept for backward compatibility
            }
            "font_family" => {
                if !value.is_empty() {
                    self.font_family = value.to_string();
                }
            }
            "password_hint" => {
                // Ignored — password hint feature removed, kept for backward compatibility
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
            "language" => {
                if let Some(v) = Language::from_str(value) {
                    self.language = v;
                }
            }
            "active_tab" => {
                if let Ok(v) = value.parse::<u32>() {
                    self.active_tab = v;
                }
            }
            "auto_lock" => {
                if let Ok(v) = value.parse::<u32>() {
                    self.auto_lock = v;
                }
            }
            "clipboard_clear" => {
                if let Ok(v) = value.parse::<u32>() {
                    self.clipboard_clear = v;
                }
            }
            "auto_save" => {
                if let Ok(v) = value.parse::<u32>() {
                    self.auto_save = v;
                }
            }
            "show_line_numbers" => {
                if let Some(v) = parse_bool(value) {
                    self.show_line_numbers = v;
                }
            }
            "totp_2fa" => {
                if let Some(v) = parse_bool(value) {
                    self.totp_2fa = v;
                }
            }
            "totp_2fa_secret" => {
                // A later line replaces the secret: the earlier one is wiped
                // in place first, never freed as it stands (issue #41).
                zeroize::Zeroize::zeroize(&mut self.totp_2fa_secret);
                self.totp_2fa_secret.push_str(value);
            }
            "totp_entry" => {
                // Format: name|BASE32SECRET. Checked before anything is
                // copied: the secret is copied once, straight into the entry
                // (wiped on drop), never into a String dropped when the name
                // is empty (issue #41).
                if let Some((name, secret)) = value.split_once('|') {
                    if !name.is_empty() && !secret.is_empty() {
                        self.totp_entries.push(TotpEntry { name: name.to_string(), secret: secret.to_string() });
                    }
                }
            }
            "vault_category" => {
                let name = value.trim();
                let valid = !name.is_empty()
                    && !name.chars().any(|c| crate::features::vault::categories::FORBIDDEN_CHARS.contains(&c));
                if valid {
                    self.vault_categories.push(name.to_string());
                } else {
                    eprintln!("settings: invalid vault_category ignored");
                }
            }
            "hotkey_modifiers" => {
                if let Ok(v) = value.parse::<u32>() {
                    self.hotkey_modifiers = v;
                }
            }
            "hotkey_key" => {
                if let Ok(v) = value.parse::<u32>() {
                    self.hotkey_key = v;
                }
            }
            "max_attachment_size_mb" => {
                if let Ok(v) = value.parse::<u32>() {
                    self.max_attachment_size_mb = clamp_u32(v, 1, 50);
                }
            }
            "max_total_attachments_mb" => {
                if let Ok(v) = value.parse::<u32>() {
                    self.max_total_attachments_mb = clamp_u32(v, 10, 200);
                }
            }
            "vault_panel_width" | "totp_panel_width" | "attach_panel_width" | "prompts_panel_width" => {
                use crate::ui::side_panel::{MAX_PANEL_W, MIN_PANEL_W};
                if let Ok(v) = value.parse::<u32>() {
                    let v = clamp_u32(v, MIN_PANEL_W, MAX_PANEL_W);
                    match key {
                        "vault_panel_width" => self.vault_panel_width = v,
                        "totp_panel_width" => self.totp_panel_width = v,
                        "attach_panel_width" => self.attach_panel_width = v,
                        _ => self.prompts_panel_width = v,
                    }
                }
            }
            "passgen_length" => {
                if let Ok(v) = value.parse::<u32>() {
                    self.passgen.length =
                        clamp_u32(v, crate::features::passgen::MIN_LENGTH, crate::features::passgen::MAX_LENGTH);
                }
            }
            "passgen_upper" => {
                if let Some(v) = parse_bool(value) {
                    self.passgen.upper = v;
                }
            }
            "passgen_lower" => {
                if let Some(v) = parse_bool(value) {
                    self.passgen.lower = v;
                }
            }
            "passgen_digits" => {
                if let Some(v) = parse_bool(value) {
                    self.passgen.digits = v;
                }
            }
            "passgen_symbols" => {
                if let Some(v) = parse_bool(value) {
                    self.passgen.symbols = v;
                }
            }
            "passgen_exclude_ambiguous" => {
                if let Some(v) = parse_bool(value) {
                    self.passgen.exclude_ambiguous = v;
                }
            }
            "passgen_symbol_set" => {
                // An empty value is a valid, empty list.
                match crate::features::passgen::normalize_symbols(value) {
                    Ok(set) => self.passgen.symbol_set = set,
                    Err(_) => eprintln!("settings: invalid passgen_symbol_set ignored"),
                }
            }
            _ => {
                // Unknown keys silently ignored
            }
        }
    }

    /// Serialize settings + note text into the stored format.
    ///
    /// The text holds the 2FA and TOTP secrets and the whole note: it is
    /// written into one buffer allocated at its exact size (a first pass
    /// counts the bytes), so no reallocation leaves a freed, unwiped copy,
    /// and the secrets go straight into it, never through a `format!`
    /// temporary (issue #41). The caller wraps the result in `Zeroizing`.
    pub fn serialize(&self, note_text: &str) -> String {
        let mut len = ByteCount(0);
        self.write_serialized(note_text, &mut len)
            .expect("counting bytes never fails");
        let mut out = String::with_capacity(len.0);
        self.write_serialized(note_text, &mut out)
            .expect("writing into a String never fails");
        debug_assert_eq!(out.capacity(), len.0, "the serialized text outgrew its buffer");
        out
    }

    /// The stored format, written into `w` piece by piece (`write!` formats
    /// straight into `w`, without a temporary). Called twice by `serialize`:
    /// to count, then to write.
    fn write_serialized<W: std::fmt::Write>(&self, note_text: &str, w: &mut W) -> std::fmt::Result {
        w.write_str(HEADER)?;
        w.write_char('\n')?;

        write!(w, "save_on_close={}\n", self.save_on_close.as_str())?;
        write!(w, "theme={}\n", self.theme.as_str())?;
        write!(w, "word_wrap={}\n", bool_str(self.word_wrap))?;
        write!(w, "minimize_to_tray={}\n", bool_str(self.minimize_to_tray))?;
        write!(w, "min_password_length={}\n", self.min_password_length)?;
        write!(w, "font_family={}\n", self.font_family)?;

        // Serialize font_size: avoid trailing zeros but keep one decimal
        if self.font_size == self.font_size.floor() {
            write!(w, "font_size={:.1}\n", self.font_size)?;
        } else {
            write!(w, "font_size={}\n", self.font_size)?;
        }
        write!(w, "save_key={}\n", self.save_key)?;
        write!(w, "save_modifiers={}\n", self.save_modifiers)?;

        // Window position/size (only if explicitly set)
        if self.window_x != i32::MIN {
            write!(w, "window_width={}\n", self.window_width)?;
            write!(w, "window_height={}\n", self.window_height)?;
            write!(w, "window_x={}\n", self.window_x)?;
            write!(w, "window_y={}\n", self.window_y)?;
            write!(w, "window_maximized={}\n", bool_str(self.window_maximized))?;
        }

        {
            write!(w, "language={}\n", self.language.as_str())?;
            write!(w, "active_tab={}\n", self.active_tab)?;
            write!(w, "auto_lock={}\n", self.auto_lock)?;
            write!(w, "clipboard_clear={}\n", self.clipboard_clear)?;
            write!(w, "auto_save={}\n", self.auto_save)?;
            write!(w, "show_line_numbers={}\n", bool_str(self.show_line_numbers))?;
            write!(w, "totp_2fa={}\n", bool_str(self.totp_2fa))?;
            // The secrets: written as they stand, no formatting (issue #41).
            w.write_str("totp_2fa_secret=")?;
            w.write_str(&self.totp_2fa_secret)?;
            w.write_char('\n')?;
            for entry in &self.totp_entries {
                w.write_str("totp_entry=")?;
                w.write_str(&entry.name)?;
                w.write_char('|')?;
                w.write_str(&entry.secret)?;
                w.write_char('\n')?;
            }
            for cat in &self.vault_categories {
                write!(w, "vault_category={}\n", cat)?;
            }
            write!(w, "hotkey_modifiers={}\n", self.hotkey_modifiers)?;
            write!(w, "hotkey_key={}\n", self.hotkey_key)?;
            write!(w, "max_attachment_size_mb={}\n", self.max_attachment_size_mb)?;
            write!(w, "max_total_attachments_mb={}\n", self.max_total_attachments_mb)?;
            write!(w, "passgen_length={}\n", self.passgen.length)?;
            write!(w, "passgen_upper={}\n", bool_str(self.passgen.upper))?;
            write!(w, "passgen_lower={}\n", bool_str(self.passgen.lower))?;
            write!(w, "passgen_digits={}\n", bool_str(self.passgen.digits))?;
            write!(w, "passgen_symbols={}\n", bool_str(self.passgen.symbols))?;
            write!(w, "passgen_exclude_ambiguous={}\n", bool_str(self.passgen.exclude_ambiguous))?;
            // ASCII punctuation only (no space, no line break): safe on one line.
            write!(w, "passgen_symbol_set={}\n", self.passgen.symbol_set)?;
            // Side panel widths: only once resized, so a note whose panels
            // were never resized keeps the header of earlier versions.
            use crate::ui::side_panel::{default_width, SidePanel};
            for (key, width, panel) in [
                ("vault_panel_width", self.vault_panel_width, SidePanel::Vault),
                ("totp_panel_width", self.totp_panel_width, SidePanel::Totp),
                ("attach_panel_width", self.attach_panel_width, SidePanel::Attach),
                ("prompts_panel_width", self.prompts_panel_width, SidePanel::Prompts),
            ] {
                if width != default_width(panel) {
                    write!(w, "{key}={width}\n")?;
                }
            }
        }

        w.write_str(FOOTER)?;
        w.write_char('\n')?;
        w.write_str(note_text)
    }
}

/// The first pass of `Settings::serialize`: counts the bytes it would write.
struct ByteCount(usize);

impl std::fmt::Write for ByteCount {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.0 += s.len();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default() {
        let s = Settings::default();
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

    /// Every field of the defaults a new note starts with (issue 059, D9:
    /// those of the 1.2.27 Pro build, the only ones from 2.0 on). Written out
    /// on purpose: a default that moves, or a field added without one, fails
    /// here.
    fn frozen_defaults() -> Settings {
        Settings {
            save_on_close: CloseAction::Ask,
            theme: ThemeChoice::Dark,
            word_wrap: true,
            minimize_to_tray: false,
            min_password_length: 6,
            font_family: "Consolas".to_string(),
            font_size: 11.0,
            save_key: 0x53,
            save_modifiers: 0x02,
            window_width: 900,
            window_height: 650,
            window_x: i32::MIN,
            window_y: i32::MIN,
            window_maximized: false,
            language: Language::En,
            active_tab: 0,
            auto_lock: 15,
            clipboard_clear: 30,
            auto_save: 30,
            show_line_numbers: true,
            totp_2fa: false,
            totp_2fa_secret: String::new(),
            totp_entries: Vec::new(),
            vault_categories: Vec::new(),
            hotkey_modifiers: 6,
            hotkey_key: 76,
            max_attachment_size_mb: 10,
            max_total_attachments_mb: 50,
            passgen: crate::features::passgen::Options {
                length: 16,
                upper: true,
                lower: true,
                digits: true,
                symbols: true,
                exclude_ambiguous: false,
                symbol_set: "!@#$%^&*()-_=+[]{};:,.?".to_string(),
            },
            vault_panel_width: 300,
            totp_panel_width: 280,
            attach_panel_width: 280,
            prompts_panel_width: 320,
        }
    }

    #[test]
    fn single_defaults_are_frozen() {
        assert_eq!(Settings::default(), frozen_defaults());
        assert_eq!(crate::features::autolock::AutoLock::default().timeout_minutes(), 15);
        assert_eq!(crate::features::autosave::AutoSave::default().interval_seconds(), 30);
        assert_eq!(crate::features::clipboard_guard::ClipboardGuard::default().timeout_seconds(), 30);
        assert_eq!(crate::features::hotkey::GlobalHotkey::default().display(), "Ctrl+Shift+L");
    }

    /// What `Settings::parse` gives the keys a file does not hold (and a file
    /// without settings): the defaults, but a minimum password length of 4,
    /// as in every build up to 1.2.27 (D9: kept, so that existing files read
    /// back the same).
    #[test]
    fn parse_fallback_is_frozen() {
        let mut expected = frozen_defaults();
        expected.min_password_length = 4;
        for input in ["", "plain note", "[LOCKNOTE_SETTINGS]\n[/LOCKNOTE_SETTINGS]\nnote", "[LOCKNOTE_SETTINGS]\nno footer"] {
            assert_eq!(Settings::parse(input).0, expected, "input {:?}", input);
        }
    }

    #[test]
    fn test_parse_empty() {
        let (s, note) = Settings::parse("");
        assert_eq!(s, Settings::parse_fallback());
        assert_eq!(note, "");
    }

    #[test]
    fn test_parse_no_header() {
        let input = "Hello, world!\nThis is my note.";
        let (s, note) = Settings::parse(input);
        assert_eq!(s, Settings::parse_fallback());
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
        let mut s = Settings::parse_fallback();
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
        let s = Settings::parse_fallback();
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
        let s = Settings::parse_fallback();
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
    fn vault_categories_roundtrip() {
        let mut s = Settings::default();
        assert!(s.vault_categories.is_empty()); // never customized
        s.vault_categories = vec!["General".into(), "Banque perso".into(), "Serveurs".into()];
        let (p, note) = Settings::parse(&s.serialize("note"));
        assert_eq!(p.vault_categories, s.vault_categories);
        assert_eq!(note, "note");
        let (p, _) = Settings::parse(&Settings::default().serialize("x"));
        assert!(p.vault_categories.is_empty());
    }

    #[test]
    fn test_pro_totp_entries() {
        let input = "[LOCKNOTE_SETTINGS]\ntotp_entry=Gmail|JBSWY3DPEHPK3PXP\ntotp_entry=AWS|KZXW6YTBOI======\n[/LOCKNOTE_SETTINGS]\nNote";
        let (s, _) = Settings::parse(input);
        assert_eq!(s.totp_entries.len(), 2);
        assert_eq!(s.totp_entries[0].name, "Gmail");
        assert_eq!(s.totp_entries[0].secret, "JBSWY3DPEHPK3PXP");
        assert_eq!(s.totp_entries[1].name, "AWS");
    }

    #[test]
    fn test_pro_roundtrip() {
        let mut s = Settings::default();
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
        assert_eq!(s, Settings::parse_fallback());
        assert_eq!(note, input);
    }

    #[test]
    fn test_crypto_chain_settings_then_note() {
        // Simulates what happens after decryption: settings block + note
        let settings = Settings::parse_fallback();
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
        assert_eq!(s, Settings::parse_fallback());
        assert_eq!(note, input);
    }

    #[test]
    fn test_parse_unicode_note_content() {
        let note = "Café ☕ 日本語 Привет 🔒📝";
        let s = Settings::parse_fallback();
        let serialized = s.serialize(note);
        let (_, parsed_note) = Settings::parse(&serialized);
        assert_eq!(parsed_note, note);
    }

    #[test]
    fn test_parse_empty_settings_block() {
        let input = "[LOCKNOTE_SETTINGS]\n[/LOCKNOTE_SETTINGS]\nJust a note";
        let (s, note) = Settings::parse(input);
        assert_eq!(s, Settings::parse_fallback());
        assert_eq!(note, "Just a note");
    }

    // ── Window position/size tests ──

    #[test]
    fn test_window_defaults() {
        let s = Settings::parse_fallback();
        assert_eq!(s.window_width, 900);
        assert_eq!(s.window_height, 650);
        assert_eq!(s.window_x, i32::MIN);
        assert_eq!(s.window_y, i32::MIN);
        assert!(!s.window_maximized);
    }

    #[test]
    fn test_window_not_serialized_when_default() {
        let s = Settings::parse_fallback();
        let out = s.serialize("note");
        assert!(!out.contains("window_width"));
        assert!(!out.contains("window_x"));
    }

    #[test]
    fn test_window_serialized_when_set() {
        let mut s = Settings::parse_fallback();
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
        let mut s = Settings::parse_fallback();
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

    // ── Side panel widths ──

    #[test]
    fn panel_widths_default_to_the_historical_widths() {
        let s = Settings::default();
        assert_eq!((s.vault_panel_width, s.totp_panel_width, s.attach_panel_width), (300, 280, 280));
        assert_eq!(s.prompts_panel_width, 320);
    }

    #[test]
    fn panel_widths_not_serialized_when_default() {
        let out = Settings::default().serialize("note");
        assert!(!out.contains("panel_width"), "{out}");
    }

    #[test]
    fn panel_widths_roundtrip_each_on_its_own() {
        let mut s = Settings::default();
        s.totp_panel_width = 412;
        let out = s.serialize("note");
        assert!(out.contains("totp_panel_width=412\n"));
        assert!(!out.contains("vault_panel_width") && !out.contains("attach_panel_width"));
        s.vault_panel_width = 640;
        s.attach_panel_width = 221;
        assert!(!s.serialize("note").contains("prompts_panel_width"));
        s.prompts_panel_width = 777;
        assert!(s.serialize("note").contains("prompts_panel_width=777\n"));
        let (back, note) = Settings::parse(&s.serialize("note"));
        assert_eq!((back.vault_panel_width, back.totp_panel_width, back.attach_panel_width), (640, 412, 221));
        assert_eq!(back.prompts_panel_width, 777);
        assert_eq!(note, "note");
    }

    #[test]
    fn panel_widths_are_bounded_when_read() {
        let input = "[LOCKNOTE_SETTINGS]\nvault_panel_width=10\ntotp_panel_width=999999\nattach_panel_width=0\nprompts_panel_width=9999999\n[/LOCKNOTE_SETTINGS]\n";
        let (s, _) = Settings::parse(input);
        assert_eq!(s.prompts_panel_width, crate::ui::side_panel::MAX_PANEL_W);
        assert_eq!(s.vault_panel_width, crate::ui::side_panel::MIN_PANEL_W);
        assert_eq!(s.totp_panel_width, crate::ui::side_panel::MAX_PANEL_W);
        assert_eq!(s.attach_panel_width, crate::ui::side_panel::MIN_PANEL_W);
    }

    #[test]
    fn panel_widths_invalid_or_absent_keep_the_default() {
        let input = "[LOCKNOTE_SETTINGS]\nvault_panel_width=wide\ntotp_panel_width=-5\nprompts_panel_width=x\n[/LOCKNOTE_SETTINGS]\n";
        let (s, _) = Settings::parse(input);
        assert_eq!((s.vault_panel_width, s.totp_panel_width, s.attach_panel_width), (300, 280, 280));
        assert_eq!(s.prompts_panel_width, 320);
    }

    #[test]
    fn passgen_options_roundtrip_with_every_symbol() {
        use crate::features::passgen::Options;
        // Every ASCII punctuation character, `=`, `[`, `]`, `"` and `\` included.
        let all: String = (0x21u8..0x7F).map(char::from).filter(|c| c.is_ascii_punctuation()).collect();
        let mut s = Settings::default();
        s.passgen = Options {
            length: 41,
            upper: false,
            lower: true,
            digits: false,
            symbols: true,
            exclude_ambiguous: true,
            symbol_set: all.clone(),
        };
        let (back, note) = Settings::parse(&s.serialize("the note"));
        assert_eq!(back.passgen, s.passgen);
        assert_eq!(note, "the note");

        // An empty list is a value, not "absent".
        s.passgen.symbol_set = String::new();
        s.passgen.symbols = false;
        assert_eq!(Settings::parse(&s.serialize("")).0.passgen, s.passgen);
    }

    #[test]
    fn passgen_defaults_apply_to_older_notes_and_bad_values() {
        use crate::features::passgen::{Options, MAX_LENGTH, MIN_LENGTH};
        // Written before the generator existed: no passgen_* key.
        let (s, _) = Settings::parse("[LOCKNOTE_SETTINGS]\ntheme=light\n[/LOCKNOTE_SETTINGS]\nOld note");
        assert_eq!(s.passgen, Options::default());

        // Out-of-range lengths are clamped; unreadable values keep the default.
        let parse = |body: &str| Settings::parse(&format!("[LOCKNOTE_SETTINGS]\n{body}\n[/LOCKNOTE_SETTINGS]\n")).0.passgen;
        assert_eq!(parse("passgen_length=3").length, MIN_LENGTH);
        assert_eq!(parse("passgen_length=500").length, MAX_LENGTH);
        assert_eq!(parse("passgen_length=abc").length, Options::default().length);
        assert_eq!(parse("passgen_upper=maybe").upper, Options::default().upper);
        // A list holding anything but punctuation is ignored as a whole.
        assert_eq!(parse("passgen_symbol_set=!a#").symbol_set, Options::default().symbol_set);
        // Duplicates are dropped.
        assert_eq!(parse("passgen_symbol_set=##--").symbol_set, "#-");
    }

    /// Issue #41: every field set to a value other than its default, both
    /// secrets included, the window placed and every panel resized.
    fn every_field_set() -> Settings {
        Settings {
            save_on_close: CloseAction::Never,
            theme: ThemeChoice::Light,
            word_wrap: false,
            minimize_to_tray: true,
            min_password_length: 12,
            font_family: "Courier New".to_string(),
            font_size: 12.5,
            save_key: 0x4B,
            save_modifiers: 0x05,
            window_width: 1234,
            window_height: 777,
            window_x: -1920,
            window_y: 48,
            window_maximized: true,
            language: Language::De,
            active_tab: 3,
            auto_lock: 7,
            clipboard_clear: 45,
            auto_save: 120,
            show_line_numbers: false,
            totp_2fa: true,
            totp_2fa_secret: "JBSWY3DPEHPK3PXP".to_string(),
            totp_entries: vec![
                TotpEntry { name: "Gmail".into(), secret: "GEZDGNBVGY3TQOJQ".into() },
                TotpEntry { name: "AWS root".into(), secret: "KZXW6YTBOI======".into() },
            ],
            vault_categories: vec!["Banque perso".into(), "Serveurs".into()],
            hotkey_modifiers: 3,
            hotkey_key: 0x4E,
            max_attachment_size_mb: 25,
            max_total_attachments_mb: 150,
            passgen: crate::features::passgen::Options {
                length: 41,
                upper: false,
                lower: true,
                digits: false,
                symbols: true,
                exclude_ambiguous: true,
                symbol_set: "!#-_=[]".to_string(),
            },
            vault_panel_width: 640,
            totp_panel_width: 412,
            attach_panel_width: 221,
            prompts_panel_width: 777,
        }
    }

    /// Note text of the characterisation: CRLF, non-ASCII, and lines that
    /// look like settings.
    const EVERY_FIELD_NOTE: &str = "Line one\r\nLigne deux \u{e9} \u{65e5}\n[LOCKNOTE_SETTINGS]\ntotp_2fa_secret=NOTE\n";

    /// Issue #41: the bytes `serialize` writes, pinned on the code before
    /// the issue (format frozen: never edit these literals).
    #[test]
    fn serialize_bytes_are_frozen() {
        let full: &[u8] = b"[LOCKNOTE_SETTINGS]\n\
            save_on_close=never\n\
            theme=light\n\
            word_wrap=off\n\
            minimize_to_tray=on\n\
            min_password_length=12\n\
            font_family=Courier New\n\
            font_size=12.5\n\
            save_key=75\n\
            save_modifiers=5\n\
            window_width=1234\n\
            window_height=777\n\
            window_x=-1920\n\
            window_y=48\n\
            window_maximized=on\n\
            language=de\n\
            active_tab=3\n\
            auto_lock=7\n\
            clipboard_clear=45\n\
            auto_save=120\n\
            show_line_numbers=off\n\
            totp_2fa=on\n\
            totp_2fa_secret=JBSWY3DPEHPK3PXP\n\
            totp_entry=Gmail|GEZDGNBVGY3TQOJQ\n\
            totp_entry=AWS root|KZXW6YTBOI======\n\
            vault_category=Banque perso\n\
            vault_category=Serveurs\n\
            hotkey_modifiers=3\n\
            hotkey_key=78\n\
            max_attachment_size_mb=25\n\
            max_total_attachments_mb=150\n\
            passgen_length=41\n\
            passgen_upper=off\n\
            passgen_lower=on\n\
            passgen_digits=off\n\
            passgen_symbols=on\n\
            passgen_exclude_ambiguous=on\n\
            passgen_symbol_set=!#-_=[]\n\
            vault_panel_width=640\n\
            totp_panel_width=412\n\
            attach_panel_width=221\n\
            prompts_panel_width=777\n\
            [/LOCKNOTE_SETTINGS]\n\
            Line one\r\n\
            Ligne deux \xc3\xa9 \xe6\x97\xa5\n\
            [LOCKNOTE_SETTINGS]\n\
            totp_2fa_secret=NOTE\n";
        assert_eq!(every_field_set().serialize(EVERY_FIELD_NOTE).as_bytes(), full);
        let defaults: &[u8] = b"[LOCKNOTE_SETTINGS]\n\
            save_on_close=ask\n\
            theme=dark\n\
            word_wrap=on\n\
            minimize_to_tray=off\n\
            min_password_length=6\n\
            font_family=Consolas\n\
            font_size=11.0\n\
            save_key=83\n\
            save_modifiers=2\n\
            language=en\n\
            active_tab=0\n\
            auto_lock=15\n\
            clipboard_clear=30\n\
            auto_save=30\n\
            show_line_numbers=on\n\
            totp_2fa=off\n\
            totp_2fa_secret=\n\
            hotkey_modifiers=6\n\
            hotkey_key=76\n\
            max_attachment_size_mb=10\n\
            max_total_attachments_mb=50\n\
            passgen_length=16\n\
            passgen_upper=on\n\
            passgen_lower=on\n\
            passgen_digits=on\n\
            passgen_symbols=on\n\
            passgen_exclude_ambiguous=off\n\
            passgen_symbol_set=!@#$%^&*()-_=+[]{};:,.?\n\
            [/LOCKNOTE_SETTINGS]\n";
        assert_eq!(Settings::default().serialize("").as_bytes(), defaults);
    }

    /// Issue #41: the pinned bytes read back to every field and the note.
    #[test]
    fn parse_of_the_frozen_bytes_gives_every_field() {
        let (s, note) = Settings::parse(&every_field_set().serialize(EVERY_FIELD_NOTE));
        assert_eq!(s, every_field_set());
        assert_eq!(s.totp_2fa_secret, "JBSWY3DPEHPK3PXP");
        assert_eq!(s.totp_entries[1].secret, "KZXW6YTBOI======");
        assert_eq!(note, EVERY_FIELD_NOTE);
    }

    /// Issue #41: how the secret keys are read, frozen before the change:
    /// the last 2FA secret wins (an empty one too), a TOTP entry needs a
    /// pipe, a name and a secret, splits at the first pipe, keeps the
    /// spaces around the pipe and may repeat.
    #[test]
    fn parse_of_the_secret_keys_is_frozen() {
        let parse = |body: &str| Settings::parse(&format!("[LOCKNOTE_SETTINGS]\n{body}\n[/LOCKNOTE_SETTINGS]\n")).0;
        assert_eq!(parse("totp_2fa_secret=AAAAAAAAAAAAAAAAAAAAAAAA\ntotp_2fa_secret=BB").totp_2fa_secret, "BB");
        assert_eq!(parse("totp_2fa_secret=BB\ntotp_2fa_secret=CCCCCCCCCCCCCCCCCCCCCCCCCC").totp_2fa_secret, "CCCCCCCCCCCCCCCCCCCCCCCCCC");
        assert_eq!(parse("totp_2fa_secret=BB\ntotp_2fa_secret=").totp_2fa_secret, "");
        assert_eq!(parse("totp_2fa_secret=  SPACED  ").totp_2fa_secret, "SPACED");
        let entries = |body: &str| -> Vec<(String, String)> {
            parse(body).totp_entries.iter().map(|e| (e.name.clone(), e.secret.clone())).collect()
        };
        assert!(entries("totp_entry=|SECRET\ntotp_entry=Name|\ntotp_entry=NoPipe\ntotp_entry=|\ntotp_entry=").is_empty());
        assert_eq!(entries("totp_entry=a|b|c"), [("a".to_string(), "b|c".to_string())]);
        assert_eq!(entries("totp_entry=  Name | SEC  "), [("Name ".to_string(), " SEC".to_string())]);
        assert_eq!(
            entries("totp_entry=X|ONE\ntotp_entry=|LOST\ntotp_entry=X|ONE"),
            [("X".to_string(), "ONE".to_string()), ("X".to_string(), "ONE".to_string())]
        );
    }

    /// Issue #41: the serialized text (secrets and the whole note) is
    /// written into one buffer allocated at its exact size: a buffer that
    /// grows leaves a freed, unwiped copy at every reallocation, and a
    /// larger one would not be what the callers wrap and wipe.
    #[test]
    fn serialize_allocates_the_exact_size_once() {
        let big_note = "x\u{e9}\r\n".repeat(50_000);
        let mut no_window = every_field_set();
        no_window.window_x = i32::MIN;
        for (s, note) in [
            (every_field_set(), EVERY_FIELD_NOTE),
            (every_field_set(), ""),
            (every_field_set(), big_note.as_str()),
            (no_window, "n"),
            (Settings::default(), ""),
            (Settings::parse_fallback(), big_note.as_str()),
        ] {
            let out = s.serialize(note);
            assert_eq!(out.capacity(), out.len(), "buffer not sized up front ({} bytes)", out.len());
        }
    }

    /// The source of `rel` (from the crate root), from `start` to the first
    /// `end` after it.
    fn source_region(rel: &str, start: &str, end: &str) -> String {
        let src = crate::read_source(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(rel));
        let at = src.find(start).unwrap_or_else(|| panic!("{start} in {rel}"));
        let len = src[at..].find(end).unwrap_or_else(|| panic!("end of {start} in {rel}"));
        src[at..at + len].to_string()
    }

    /// `first` occurs in `body`, and `then` after it.
    fn in_order(body: &str, first: &str, then: &str) -> bool {
        match body.find(first) {
            Some(i) => body[i + first.len()..].contains(then),
            None => false,
        }
    }

    /// Issue #41: the secrets go into the serialized text as they stand: no
    /// `format!` temporary in serialize (the exact-size test cannot see one:
    /// a temporary pushed into a buffer large enough leaves no trace in it).
    #[test]
    fn serialize_writes_the_secrets_without_temporaries() {
        let ser = source_region("src/settings/mod.rs", "    pub fn serialize(&self", "\nstruct ByteCount");
        assert!(!ser.contains("format!("), "a format! temporary in serialize");
        assert!(ser.contains("let mut out = String::with_capacity(len.0);"));
        assert!(ser.contains("w.write_str(&self.totp_2fa_secret)?;"), "the 2FA secret");
        assert!(ser.contains("w.write_str(&entry.secret)?;"), "the TOTP entry secrets");
        for line in ser.lines().filter(|l| l.contains("write!(")) {
            assert!(!line.contains("secret"), "a secret formatted: {line}");
        }
    }

    /// Issue #41: parse wipes a 2FA secret a later line replaces, and copies
    /// a TOTP secret once, straight into its entry, after the checks.
    #[test]
    fn parse_wipes_a_replaced_2fa_secret_and_copies_an_entry_secret_once() {
        let two_fa = source_region("src/settings/mod.rs", "            \"totp_2fa_secret\" => {", "            \"totp_entry\" => {");
        assert!(
            in_order(&two_fa, "zeroize::Zeroize::zeroize(&mut self.totp_2fa_secret);", "self.totp_2fa_secret.push_str(value);"),
            "the previous 2FA secret is not wiped before the new one is written"
        );
        let entry = source_region("src/settings/mod.rs", "            \"totp_entry\" => {", "            \"vault_category\" => {");
        assert!(entry.contains("value.split_once('|')"));
        assert!(
            in_order(&entry, "if !name.is_empty() && !secret.is_empty() {", "secret: secret.to_string() });"),
            "the secret is copied before the checks"
        );
        assert_eq!(entry.matches("to_string()").count(), 2, "copies in totp_entry (the name and the secret)");
        let code = source_region("src/settings/mod.rs", "impl Settings {", "#[cfg(test)]");
        assert!(!code.contains("self.totp_2fa_secret ="), "the 2FA secret is assigned, freeing the previous one");
    }

    /// Issue #41: the startup unlock moves the decrypted text, its
    /// decompressed form and the note into wrappers (wiped on drop), hands
    /// the note to the editor by a move, and wipes the 2FA secret when the
    /// TOTP check fails. Since #42 the decrypted text comes wrapped from
    /// `crypto::decrypt` through the unlock dialog (types pinned by
    /// `crypto::tests::issue42_decrypted_text_is_wrapped_up_to_the_callers`).
    #[test]
    fn the_startup_unlock_wipes_the_decrypted_text() {
        let body = source_region("src/ui/mod.rs", "fn run_session(", "\n}\n");
        assert!(in_order(
            &body,
            "Some((password, decrypted_text)) => {",
            "let decrypted_text = zeroize::Zeroizing::new(crate::features::compression::decompress(&decrypted_text));"
        ));
        assert!(body.contains(
            "crate::settings::Settings::parse(&decrypted_text);\n                    let mut note_text = zeroize::Zeroizing::new(note_text);"
        ));
        assert!(body.contains("editor::EditorForm::run(exe_path, password, std::mem::take(&mut *note_text), settings);"));
        assert!(in_order(&body, "zeroize::Zeroize::zeroize(&mut settings.totp_2fa_secret);", "return; // TOTP verification failed"));
        assert!(!body.contains(".clone())"), "a copy before a wrap");
    }

    /// Issue #41: both unlocks of the editor move the decrypted text, its
    /// decompressed form and the payload into wrappers, wiped on drop.
    #[test]
    fn the_editor_unlocks_wipe_the_decrypted_text() {
        let disk = source_region("src/ui/editor/lock_ops.rs", "    pub(super) fn unlock_from_disk(", "\n    }\n");
        assert!(in_order(
            &disk,
            "Some((password, decrypted_text)) => {",
            "let decrypted_text = Zeroizing::new(crate::features::compression::decompress(&decrypted_text));"
        ));
        assert!(disk.contains("crate::settings::Settings::parse(&decrypted_text);\n                let note_text = Zeroizing::new(note_text);"));
        let restore = source_region("src/ui/editor/lock_ops.rs", "    pub(super) fn restore_after_unlock(", "\n    }\n");
        assert!(restore.starts_with(
            "    pub(super) fn restore_after_unlock(&self, password: Zeroizing<String>, plaintext: Zeroizing<String>) {"
        ));
        assert!(restore.contains("let plaintext = Zeroizing::new(crate::features::compression::decompress(&plaintext));"));
        assert!(restore.contains("crate::settings::Settings::parse(&plaintext);\n        let note_text = Zeroizing::new(note_text);"));
        for body in [&disk, &restore] {
            assert!(!body.contains(".clone())"), "a copy before a wrap");
        }
    }

    /// Issue #41: the save wipes the editor text, the composed payload and
    /// the serialized text (each moved into its wrapper).
    #[test]
    fn the_save_wipes_the_payload_and_the_serialized_text() {
        let save = source_region("src/ui/editor/file_ops.rs", "    pub(super) fn save_note(", "\n    }\n");
        // The editor is read strictly: the text is wrapped as soon as it is read.
        assert!(save.contains("let note_text = match self.get_content() {\n            Ok(text) => Zeroizing::new(text),"));
        assert!(save.contains("Zeroizing::new(crate::features::payload::compose(&note_tabs, &prompts, &secrets, &attachments, &state))"));
        assert!(save.contains("let combined: Zeroizing<String> = Zeroizing::new(settings.serialize(&note_text));"));
        assert!(!save.contains(".clone())"), "a copy before a wrap");
    }

    /// Issue #41: the backup wipes the settings it serializes or reads: the
    /// portable copy of an export, the settings of an exe it migrates, and
    /// the settings text of a `.lnbak`.
    #[test]
    fn the_backup_wipes_the_settings_it_serializes_or_reads() {
        let export = source_region("src/features/backup/mod.rs", "fn to_json(", "\n}\n");
        assert!(export.contains(
            "let settings_text: Zeroizing<String> = Zeroizing::new(portable.serialize(\"\"));\n    zeroize::Zeroize::zeroize(&mut portable.totp_2fa_secret);"
        ));
        let migrate = source_region("src/features/backup/mod.rs", "pub fn decode_exe_payload(", "\n}\n");
        assert!(in_order(&migrate, "let portable = portable_settings(&settings);", "zeroize::Zeroize::zeroize(&mut settings.totp_2fa_secret);"));
        assert!(in_order(&migrate, "zeroize::Zeroize::zeroize(&mut settings.totp_2fa_secret);", "Ok(Backup {\n        settings: portable,"));
        let import = source_region("src/features/backup/mod.rs", "fn parse_v1(", "\n}\n");
        assert!(import.contains("let text = Zeroizing::new(text);\n            Settings::parse(&text).0"));
    }
}
