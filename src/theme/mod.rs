// Theme module — Dark/Light theme system
//
// Provides two theme modes (Dark/Light) with complete color palettes,
// font specifications, and predefined tab colors.

use std::sync::atomic::{AtomicU8, Ordering};

// ---------------------------------------------------------------------------
// Color
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Convert to Win32 COLORREF (0x00BBGGRR).
    pub fn to_colorref(&self) -> u32 {
        (self.b as u32) << 16 | (self.g as u32) << 8 | self.r as u32
    }
}

// ---------------------------------------------------------------------------
// ThemePalette
// ---------------------------------------------------------------------------

pub struct ThemePalette {
    pub background: Color,
    pub surface: Color,
    pub surface_light: Color,
    pub border: Color,
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_muted: Color,
    pub accent: Color,
    pub accent_hover: Color,
    pub editor_background: Color,
    pub editor_text: Color,
    pub gutter_background: Color,
    pub gutter_text: Color,
    pub status_background: Color,
    pub status_text: Color,
    pub menu_background: Color,
    pub menu_text: Color,
    pub menu_hover: Color,
    pub input_background: Color,
    pub input_border: Color,
    pub button_background: Color,
    pub button_text: Color,
    pub button_secondary: Color,
    pub error_text: Color,
    pub match_highlight: Color,
}

// ---------------------------------------------------------------------------
// Static palettes
// ---------------------------------------------------------------------------

static DARK_PALETTE: ThemePalette = ThemePalette {
    background:        Color::new(30, 30, 30),
    surface:           Color::new(37, 37, 38),
    surface_light:     Color::new(45, 45, 48),
    border:            Color::new(60, 60, 60),
    text_primary:      Color::new(220, 220, 220),
    text_secondary:    Color::new(150, 150, 150),
    text_muted:        Color::new(140, 140, 140),   // RGAA: ≥4.5:1 on dark bg
    accent:            Color::new(86, 156, 214),
    accent_hover:      Color::new(120, 180, 230),
    editor_background: Color::new(30, 30, 30),
    editor_text:       Color::new(220, 220, 220),
    gutter_background: Color::new(30, 30, 30),
    gutter_text:       Color::new(140, 140, 140),   // RGAA: ≥4.5:1 on dark bg
    status_background: Color::new(0, 122, 204),
    status_text:       Color::new(255, 255, 255),
    menu_background:   Color::new(37, 37, 38),
    menu_text:         Color::new(220, 220, 220),
    menu_hover:        Color::new(62, 62, 64),
    input_background:  Color::new(45, 45, 48),
    input_border:      Color::new(60, 60, 60),
    button_background: Color::new(60, 60, 60),
    button_text:       Color::new(220, 220, 220),
    button_secondary:  Color::new(80, 80, 80),
    error_text:        Color::new(244, 71, 71),
    match_highlight:   Color::new(80, 80, 0),
};

static LIGHT_PALETTE: ThemePalette = ThemePalette {
    background:        Color::new(252, 252, 252),
    surface:           Color::new(243, 243, 243),
    surface_light:     Color::new(233, 233, 233),
    border:            Color::new(210, 210, 210),
    text_primary:      Color::new(30, 30, 30),
    text_secondary:    Color::new(100, 100, 100),
    text_muted:        Color::new(110, 110, 110),   // RGAA: ≥4.5:1 on light bg
    accent:            Color::new(0, 120, 212),
    accent_hover:      Color::new(0, 90, 180),
    editor_background: Color::new(255, 255, 255),
    editor_text:       Color::new(30, 30, 30),
    gutter_background: Color::new(243, 243, 243),
    gutter_text:       Color::new(110, 110, 110),   // RGAA: ≥4.5:1 on light bg
    status_background: Color::new(0, 122, 204),
    status_text:       Color::new(255, 255, 255),
    menu_background:   Color::new(243, 243, 243),
    menu_text:         Color::new(30, 30, 30),
    menu_hover:        Color::new(210, 210, 210),
    input_background:  Color::new(255, 255, 255),
    input_border:      Color::new(210, 210, 210),
    button_background: Color::new(225, 225, 225),
    button_text:       Color::new(30, 30, 30),
    button_secondary:  Color::new(200, 200, 200),
    error_text:        Color::new(200, 40, 40),
    match_highlight:   Color::new(255, 255, 0),
};

// ---------------------------------------------------------------------------
// ThemeMode + global state
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ThemeMode {
    Dark,
    Light,
}

/// Atomic backing store for the current theme mode (0 = Dark, 1 = Light).
static CURRENT_MODE: AtomicU8 = AtomicU8::new(0);

/// Set the global theme mode.
pub fn set_mode(mode: ThemeMode) {
    let val = match mode {
        ThemeMode::Dark => 0,
        ThemeMode::Light => 1,
    };
    CURRENT_MODE.store(val, Ordering::SeqCst);
}

/// Get the current global theme mode.
pub fn current_mode() -> ThemeMode {
    match CURRENT_MODE.load(Ordering::SeqCst) {
        0 => ThemeMode::Dark,
        _ => ThemeMode::Light,
    }
}

/// Return a reference to the palette for the given mode.
pub fn get_palette(mode: ThemeMode) -> &'static ThemePalette {
    match mode {
        ThemeMode::Dark => &DARK_PALETTE,
        ThemeMode::Light => &LIGHT_PALETTE,
    }
}

/// Return a reference to the palette for the current global mode.
pub fn current() -> &'static ThemePalette {
    get_palette(current_mode())
}

// ---------------------------------------------------------------------------
// FontSpec
// ---------------------------------------------------------------------------

pub struct FontSpec {
    pub family: String,
    pub size: f32,
    pub bold: bool,
}

impl FontSpec {
    pub fn ui() -> Self {
        Self { family: "Segoe UI".into(), size: 9.0, bold: false }
    }
    pub fn ui_bold() -> Self {
        Self { family: "Segoe UI".into(), size: 9.0, bold: true }
    }
    pub fn editor() -> Self {
        Self { family: "Consolas".into(), size: 11.0, bold: false }
    }
    pub fn status() -> Self {
        Self { family: "Segoe UI".into(), size: 8.5, bold: false }
    }
}

// ---------------------------------------------------------------------------
// Tab colors
// ---------------------------------------------------------------------------

struct TabColor {
    name: &'static str,
    color: Color,
}

static TAB_COLORS: &[TabColor] = &[
    TabColor { name: "blue",   color: Color::new(66, 133, 244) },
    TabColor { name: "green",  color: Color::new(52, 168, 83) },
    TabColor { name: "red",    color: Color::new(234, 67, 53) },
    TabColor { name: "orange", color: Color::new(251, 188, 4) },
    TabColor { name: "purple", color: Color::new(171, 71, 188) },
    TabColor { name: "yellow", color: Color::new(255, 235, 59) },
    TabColor { name: "pink",   color: Color::new(236, 64, 122) },
    TabColor { name: "gray",   color: Color::new(158, 158, 158) },
];

/// Retrieve a predefined tab color by name (case-sensitive).
pub fn get_tab_color(name: &str) -> Option<Color> {
    TAB_COLORS.iter().find(|tc| tc.name == name).map(|tc| tc.color)
}

/// Return all tab color names (useful for UI enumeration).
pub fn tab_color_names() -> Vec<&'static str> {
    TAB_COLORS.iter().map(|tc| tc.name).collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_palette_background() {
        let p = get_palette(ThemeMode::Dark);
        assert_eq!(p.background, Color::new(30, 30, 30));
    }

    #[test]
    fn dark_palette_accent() {
        let p = get_palette(ThemeMode::Dark);
        assert_eq!(p.accent, Color::new(86, 156, 214));
    }

    #[test]
    fn dark_palette_status() {
        let p = get_palette(ThemeMode::Dark);
        assert_eq!(p.status_background, Color::new(0, 122, 204));
        assert_eq!(p.status_text, Color::new(255, 255, 255));
    }

    #[test]
    fn dark_palette_error_and_match() {
        let p = get_palette(ThemeMode::Dark);
        assert_eq!(p.error_text, Color::new(244, 71, 71));
        assert_eq!(p.match_highlight, Color::new(80, 80, 0));
    }

    #[test]
    fn light_palette_background() {
        let p = get_palette(ThemeMode::Light);
        assert_eq!(p.background, Color::new(252, 252, 252));
    }

    #[test]
    fn light_palette_accent() {
        let p = get_palette(ThemeMode::Light);
        assert_eq!(p.accent, Color::new(0, 120, 212));
    }

    #[test]
    fn light_palette_editor() {
        let p = get_palette(ThemeMode::Light);
        assert_eq!(p.editor_background, Color::new(255, 255, 255));
        assert_eq!(p.editor_text, Color::new(30, 30, 30));
    }

    #[test]
    fn light_palette_error_and_match() {
        let p = get_palette(ThemeMode::Light);
        assert_eq!(p.error_text, Color::new(200, 40, 40));
        assert_eq!(p.match_highlight, Color::new(255, 255, 0));
    }

    #[test]
    fn light_palette_buttons() {
        let p = get_palette(ThemeMode::Light);
        assert_eq!(p.button_background, Color::new(225, 225, 225));
        assert_eq!(p.button_text, Color::new(30, 30, 30));
        assert_eq!(p.button_secondary, Color::new(200, 200, 200));
    }

    #[test]
    fn colorref_conversion() {
        let c = Color::new(0xAA, 0xBB, 0xCC);
        // COLORREF = 0x00CCBBAA
        assert_eq!(c.to_colorref(), 0x00CCBBAA);
    }

    #[test]
    fn colorref_black() {
        assert_eq!(Color::new(0, 0, 0).to_colorref(), 0);
    }

    #[test]
    fn colorref_white() {
        assert_eq!(Color::new(255, 255, 255).to_colorref(), 0x00FFFFFF);
    }

    #[test]
    fn colorref_red() {
        assert_eq!(Color::new(255, 0, 0).to_colorref(), 0x000000FF);
    }

    #[test]
    fn tab_color_lookup() {
        assert_eq!(get_tab_color("blue"), Some(Color::new(66, 133, 244)));
        assert_eq!(get_tab_color("green"), Some(Color::new(52, 168, 83)));
        assert_eq!(get_tab_color("red"), Some(Color::new(234, 67, 53)));
        assert_eq!(get_tab_color("orange"), Some(Color::new(251, 188, 4)));
        assert_eq!(get_tab_color("purple"), Some(Color::new(171, 71, 188)));
        assert_eq!(get_tab_color("yellow"), Some(Color::new(255, 235, 59)));
        assert_eq!(get_tab_color("pink"), Some(Color::new(236, 64, 122)));
        assert_eq!(get_tab_color("gray"), Some(Color::new(158, 158, 158)));
    }

    #[test]
    fn tab_color_unknown() {
        assert_eq!(get_tab_color("nope"), None);
    }

    #[test]
    fn tab_color_names_count() {
        assert_eq!(tab_color_names().len(), 8);
    }

    #[test]
    fn set_and_get_mode() {
        set_mode(ThemeMode::Light);
        assert_eq!(current_mode(), ThemeMode::Light);
        let p = current();
        assert_eq!(p.background, Color::new(252, 252, 252));

        set_mode(ThemeMode::Dark);
        assert_eq!(current_mode(), ThemeMode::Dark);
        let p = current();
        assert_eq!(p.background, Color::new(30, 30, 30));
    }

    #[test]
    fn font_spec_defaults() {
        let ui = FontSpec::ui();
        assert_eq!(ui.family, "Segoe UI");
        assert_eq!(ui.size, 9.0);
        assert!(!ui.bold);

        let bold = FontSpec::ui_bold();
        assert!(bold.bold);

        let editor = FontSpec::editor();
        assert_eq!(editor.family, "Consolas");
        assert_eq!(editor.size, 11.0);

        let status = FontSpec::status();
        assert_eq!(status.size, 8.5);
    }

    #[test]
    fn dark_and_light_differ() {
        let d = get_palette(ThemeMode::Dark);
        let l = get_palette(ThemeMode::Light);
        assert_ne!(d.background, l.background);
        assert_ne!(d.text_primary, l.text_primary);
        assert_ne!(d.editor_background, l.editor_background);
        // Status bar is the same in both
        assert_eq!(d.status_background, l.status_background);
    }
}
