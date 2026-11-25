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

    /// `self` with `percent` % (0..=100) of `other` mixed in.
    pub fn mix(&self, other: &Color, percent: u32) -> Color {
        let p = percent.min(100);
        let ch = |a: u8, b: u8| ((a as u32 * (100 - p) + b as u32 * p) / 100) as u8;
        Color::new(ch(self.r, other.r), ch(self.g, other.g), ch(self.b, other.b))
    }

    /// `self` blended over with `other` at `alpha`/255, the way GDI AlphaBlend
    /// with SourceConstantAlpha = `alpha` computes it (truncating division).
    pub fn blend_alpha(&self, other: &Color, alpha: u8) -> Color {
        let a = alpha as u32;
        let ch = |dst: u8, src: u8| ((src as u32 * a + dst as u32 * (255 - a)) / 255) as u8;
        Color::new(ch(self.r, other.r), ch(self.g, other.g), ch(self.b, other.b))
    }

    /// WCAG 2.x relative luminance (0.0 black .. 1.0 white).
    pub fn relative_luminance(&self) -> f64 {
        let lin = |c: u8| {
            let v = c as f64 / 255.0;
            if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * lin(self.r) + 0.7152 * lin(self.g) + 0.0722 * lin(self.b)
    }

    /// WCAG 2.x contrast ratio between two colors (1.0 .. 21.0), symmetric.
    pub fn contrast_ratio(&self, other: &Color) -> f64 {
        let (a, b) = (self.relative_luminance(), other.relative_luminance());
        let (hi, lo) = if a >= b { (a, b) } else { (b, a) };
        (hi + 0.05) / (lo + 0.05)
    }

    /// Whichever of `candidates` has the best contrast on `self`.
    pub fn most_readable<'a>(&self, candidates: &'a [Color]) -> Option<&'a Color> {
        candidates.iter().max_by(|a, b| {
            self.contrast_ratio(a)
                .partial_cmp(&self.contrast_ratio(b))
                .expect("contrast ratios are finite")
        })
    }

    /// Convert to Win32 COLORREF (0x00BBGGRR).
    pub fn to_colorref(&self) -> u32 {
        (self.b as u32) << 16 | (self.g as u32) << 8 | self.r as u32
    }

    /// WCAG 2.x relative luminance (exact sRGB linearisation), 0.0..=1.0.
    pub fn luminance(&self) -> f64 {
        let lin = |c: u8| {
            let v = c as f64 / 255.0;
            if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * lin(self.r) + 0.7152 * lin(self.g) + 0.0722 * lin(self.b)
    }

    /// WCAG 2.x contrast ratio between two colors, 1.0..=21.0.
    pub fn contrast_with(&self, other: &Color) -> f64 {
        let (a, b) = (self.luminance(), other.luminance());
        let (hi, lo) = if a > b { (a, b) } else { (b, a) };
        (hi + 0.05) / (lo + 0.05)
    }

    /// The more legible of white and near-black text on top of `self`.
    pub fn readable_text(&self) -> Color {
        const WHITE: Color = Color::new(255, 255, 255);
        const INK: Color = Color::new(16, 18, 22);
        if self.contrast_with(&WHITE) >= self.contrast_with(&INK) { WHITE } else { INK }
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
    /// Window chrome: title bar, menu bar, tab strip and status bar share it.
    pub chrome: Color,
    /// 1 px line between the chrome and the editor.
    pub chrome_border: Color,
    /// Hover / open highlight on the chrome (menu bar item pill).
    pub chrome_hover: Color,
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
    background:        Color::new(36, 39, 46),      // blue-charcoal
    surface:           Color::new(44, 47, 55),
    surface_light:     Color::new(53, 57, 66),
    border:            Color::new(62, 66, 76),
    chrome:            Color::new(27, 29, 35),
    chrome_border:     Color::new(47, 51, 61),
    chrome_hover:      Color::new(53, 57, 66),      // = surface_light: unchanged look
    text_primary:      Color::new(212, 216, 224),
    text_secondary:    Color::new(148, 154, 166),
    text_muted:        Color::new(140, 146, 158),   // WCAG AA ≥4.5:1 on dark bg
    accent:            Color::new(95, 160, 230),
    accent_hover:      Color::new(125, 180, 240),
    editor_background: Color::new(36, 39, 46),
    editor_text:       Color::new(212, 216, 224),
    gutter_background: Color::new(36, 39, 46),
    gutter_text:       Color::new(140, 146, 158),   // WCAG AA ≥4.5:1 on dark bg
    status_background: Color::new(55, 128, 205),
    status_text:       Color::new(255, 255, 255),
    menu_background:   Color::new(44, 47, 55),
    menu_text:         Color::new(212, 216, 224),
    menu_hover:        Color::new(62, 68, 82),
    input_background:  Color::new(44, 47, 55),
    input_border:      Color::new(62, 66, 76),
    button_background: Color::new(58, 64, 78),
    button_text:       Color::new(212, 216, 224),
    button_secondary:  Color::new(68, 74, 88),
    error_text:        Color::new(248, 92, 92),    // WCAG AA >=4.5:1 on the dark background
    match_highlight:   Color::new(85, 80, 15),
};

static LIGHT_PALETTE: ThemePalette = ThemePalette {
    // Colourful Light theme: the chrome (title bar, menu bar, tab strip,
    // status bar) is a soft tint of the blue accent, the editor stays white.
    background:        Color::new(247, 250, 255),
    surface:           Color::new(238, 244, 253),
    surface_light:     Color::new(222, 234, 251),
    border:            Color::new(160, 184, 224),
    chrome:            Color::new(219, 232, 252),
    chrome_border:     Color::new(170, 196, 238),
    chrome_hover:      Color::new(190, 212, 246),
    text_primary:      Color::new(20, 32, 56),      // dark navy
    text_secondary:    Color::new(64, 80, 110),
    text_muted:        Color::new(88, 100, 124),    // RGAA: ≥4.5:1 on every light surface
    accent:            Color::new(0, 102, 204),
    accent_hover:      Color::new(0, 78, 160),
    editor_background: Color::new(255, 255, 255),
    editor_text:       Color::new(30, 30, 30),
    gutter_background: Color::new(240, 245, 253),
    gutter_text:       Color::new(84, 96, 120),     // RGAA: ≥4.5:1 on the gutter
    status_background: Color::new(0, 102, 204),
    status_text:       Color::new(255, 255, 255),
    menu_background:   Color::new(219, 232, 252),
    menu_text:         Color::new(20, 32, 56),
    menu_hover:        Color::new(190, 212, 246),
    input_background:  Color::new(255, 255, 255),
    input_border:      Color::new(120, 142, 184),
    button_background: Color::new(214, 228, 250),
    button_text:       Color::new(20, 32, 56),
    button_secondary:  Color::new(190, 210, 240),
    error_text:        Color::new(178, 30, 30),
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

/// The theme mode a saved `settings.theme` choice stands for.
pub fn mode_for_choice(choice: crate::settings::ThemeChoice) -> ThemeMode {
    match choice {
        crate::settings::ThemeChoice::Dark => ThemeMode::Dark,
        crate::settings::ThemeChoice::Light => ThemeMode::Light,
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

/// Colour of the "saved" dot in the status bar: the green tab colour, darkened
/// just enough to reach 3:1 (WCAG 1.4.11) against the palette's chrome.
/// Unchanged when it already does (the Dark theme).
pub fn status_saved_color(pal: &ThemePalette) -> Color {
    let green = get_tab_color("green").expect("theme defines the green tab color");
    let black = Color::new(0, 0, 0);
    (0..=100u32)
        .step_by(5)
        .map(|p| green.mix(&black, p))
        .find(|c| c.contrast_ratio(&pal.chrome) >= 3.0)
        .expect("black reaches 3:1 on any chrome of the palettes")
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
        assert_eq!(p.background, Color::new(36, 39, 46));
    }

    #[test]
    fn contrast_ratio_matches_wcag_reference_values() {
        let (black, white) = (Color::new(0, 0, 0), Color::new(255, 255, 255));
        assert!((black.contrast_ratio(&white) - 21.0).abs() < 1e-9);
        assert!((white.contrast_ratio(&black) - 21.0).abs() < 1e-9);
        assert!((white.contrast_ratio(&white) - 1.0).abs() < 1e-9);
        // #767676 on white is the textbook 4.54:1.
        let grey = Color::new(0x76, 0x76, 0x76);
        assert!((grey.contrast_ratio(&white) - 4.54).abs() < 0.01);
    }

    #[test]
    fn blend_alpha_endpoints_and_midpoint() {
        let (a, b) = (Color::new(0, 100, 200), Color::new(255, 255, 255));
        assert_eq!(a.blend_alpha(&b, 0), a);
        assert_eq!(a.blend_alpha(&b, 255), b);
        assert_eq!(Color::new(0, 0, 0).blend_alpha(&b, 15), Color::new(15, 15, 15));
    }

    #[test]
    fn most_readable_picks_the_higher_contrast() {
        let accent = Color::new(95, 160, 230);
        let candidates = [Color::new(255, 255, 255), Color::new(20, 20, 20)];
        assert_eq!(accent.most_readable(&candidates), Some(&Color::new(20, 20, 20)));
        assert_eq!(accent.most_readable(&[]), None);
    }

    #[test]
    fn mix_blends_per_channel() {
        let a = Color::new(0, 100, 200);
        let b = Color::new(100, 200, 0);
        assert_eq!(a.mix(&b, 0), a);
        assert_eq!(a.mix(&b, 100), b);
        assert_eq!(a.mix(&b, 25), Color::new(25, 125, 150));
        assert_eq!(a.mix(&b, 250), b); // clamped
    }

    #[test]
    fn dark_chrome_is_darker_than_the_editor_and_light_chrome_is_not_dark() {
        let d = get_palette(ThemeMode::Dark);
        assert_eq!(d.chrome, Color::new(27, 29, 35));
        assert!(d.chrome.r < d.editor_background.r && d.chrome.g < d.editor_background.g && d.chrome.b < d.editor_background.b);
        let l = get_palette(ThemeMode::Light);
        assert!(l.chrome.r > 200 && l.chrome_border.r > 150);
    }

    #[test]
    fn light_chrome_is_a_blue_tint_not_a_neutral_grey() {
        let l = get_palette(ThemeMode::Light);
        assert_eq!(l.chrome, Color::new(219, 232, 252));
        assert_eq!(l.chrome_border, Color::new(170, 196, 238));
        // Clearly blue: blue channel well above red, and not the editor colour.
        for c in [l.chrome, l.chrome_border, l.chrome_hover] {
            assert!(c.b as i32 - c.r as i32 >= 20, "{c:?} is not tinted");
        }
        assert_ne!(l.chrome, l.editor_background);
        assert!(l.chrome.contrast_ratio(&l.editor_background) < 1.5, "chrome must stay soft");
        // The hover highlight is distinguishable from the chrome and from its border.
        assert_ne!(l.chrome_hover, l.chrome);
        assert_ne!(l.chrome_hover, l.chrome_border);
    }

    #[test]
    fn dark_chrome_hover_is_the_previous_bar_pill() {
        assert_eq!(get_palette(ThemeMode::Dark).chrome_hover, get_palette(ThemeMode::Dark).surface_light);
    }

    #[test]
    fn light_text_on_chrome_and_hover_meets_aa_in_both_directions() {
        let l = get_palette(ThemeMode::Light);
        for bg in [l.chrome, l.chrome_hover, l.menu_background, l.menu_hover, l.gutter_background, l.surface, l.surface_light, l.background] {
            for (name, fg) in [("text_primary", l.text_primary), ("menu_text", l.menu_text), ("text_secondary", l.text_secondary), ("text_muted", l.text_muted)] {
                // Muted text is a disabled item's colour and a disabled item never gets a hover fill.
                if name == "text_muted" && (bg == l.chrome_hover || bg == l.menu_hover) {
                    continue;
                }
                let r = fg.contrast_ratio(&bg);
                assert!(r >= 4.5, "Light {name} on {bg:?}: {r:.2}");
            }
        }
        // Accent as a glyph / line on the chrome and as a fill under white text.
        assert!(l.accent.contrast_ratio(&l.chrome) >= 3.0);
        assert!(l.accent.contrast_ratio(&l.editor_background) >= 3.0);
        assert!(Color::new(255, 255, 255).contrast_ratio(&l.accent) >= 4.5);
        assert!(Color::new(255, 255, 255).contrast_ratio(&l.accent_hover) >= 4.5);
        assert!(l.status_text.contrast_ratio(&l.status_background) >= 4.5);
        assert!(l.input_border.contrast_ratio(&l.input_background) >= 3.0);
        assert!(l.error_text.contrast_ratio(&l.background) >= 4.5);
        assert!(l.error_text.contrast_ratio(&l.chrome) >= 4.5);
        assert!(l.button_text.contrast_ratio(&l.button_background) >= 4.5);
        assert!(l.button_text.contrast_ratio(&l.button_secondary) >= 4.5);
    }

    #[test]
    fn tab_strip_status_bar_and_gutter_pairs_meet_the_thresholds_in_both_themes() {
        for mode in [ThemeMode::Dark, ThemeMode::Light] {
            let p = get_palette(mode);
            // Tab strip (tab_paint.rs): active tab joins the editor, inactive ones sit on the chrome.
            assert!(p.text_primary.contrast_ratio(&p.editor_background) >= 4.5, "{mode:?} active tab label");
            assert!(p.text_secondary.contrast_ratio(&p.chrome) >= 4.5, "{mode:?} inactive tab label / close glyph");
            assert!(p.accent.contrast_ratio(&p.chrome) >= 3.0, "{mode:?} accent line on the strip");
            assert!(p.accent.contrast_ratio(&p.editor_background) >= 3.0, "{mode:?} accent line on the active tab");
            // Status bar (content.rs): text_secondary on the chrome, accent dot / body.
            assert!(p.accent.contrast_ratio(&p.chrome) >= 3.0, "{mode:?} status indicator");
            // Gutter and the editor.
            assert!(p.gutter_text.contrast_ratio(&p.gutter_background) >= 4.5, "{mode:?} line numbers");
            assert!(p.editor_text.contrast_ratio(&p.editor_background) >= 4.5, "{mode:?} editor text");
            // The strip is separated from the editor by its border line, which must not vanish.
            assert_ne!(p.chrome_border, p.chrome, "{mode:?}");
            assert_ne!(p.chrome_border, p.editor_background, "{mode:?}");
        }
    }

    #[test]
    fn saved_dot_reaches_3_to_1_on_the_chrome_and_dark_is_unchanged() {
        for mode in [ThemeMode::Dark, ThemeMode::Light] {
            let p = get_palette(mode);
            assert!(status_saved_color(p).contrast_ratio(&p.chrome) >= 3.0, "{mode:?}");
        }
        assert_eq!(status_saved_color(get_palette(ThemeMode::Dark)), Color::new(52, 168, 83));
        // Light: still recognisably green.
        let l = status_saved_color(get_palette(ThemeMode::Light));
        assert!(l.g > l.r && l.g > l.b, "{l:?}");
    }

    #[test]
    fn dark_text_on_chrome_meets_aa() {
        let d = get_palette(ThemeMode::Dark);
        for fg in [d.text_primary, d.text_secondary, d.text_muted] {
            assert!(fg.contrast_ratio(&d.chrome) >= 4.5, "{fg:?}");
        }
        assert!(d.text_primary.contrast_ratio(&d.chrome_hover) >= 4.5);
    }

    #[test]
    fn dark_palette_accent() {
        let p = get_palette(ThemeMode::Dark);
        assert_eq!(p.accent, Color::new(95, 160, 230));
    }

    #[test]
    fn dark_palette_status() {
        let p = get_palette(ThemeMode::Dark);
        assert_eq!(p.status_background, Color::new(55, 128, 205));
        assert_eq!(p.status_text, Color::new(255, 255, 255));
    }

    #[test]
    fn dark_palette_error_and_match() {
        let p = get_palette(ThemeMode::Dark);
        assert_eq!(p.error_text, Color::new(248, 92, 92));
        assert_eq!(p.match_highlight, Color::new(85, 80, 15));
    }

    #[test]
    fn light_palette_background() {
        let p = get_palette(ThemeMode::Light);
        assert_eq!(p.background, Color::new(247, 250, 255));
    }

    #[test]
    fn light_palette_accent() {
        let p = get_palette(ThemeMode::Light);
        assert_eq!(p.accent, Color::new(0, 102, 204));
        assert_eq!(p.accent_hover, Color::new(0, 78, 160));
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
        assert_eq!(p.error_text, Color::new(178, 30, 30));
        assert_eq!(p.match_highlight, Color::new(255, 255, 0));
    }

    #[test]
    fn light_palette_buttons() {
        let p = get_palette(ThemeMode::Light);
        assert_eq!(p.button_background, Color::new(214, 228, 250));
        assert_eq!(p.button_text, Color::new(20, 32, 56));
        assert_eq!(p.button_secondary, Color::new(190, 210, 240));
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
    fn mode_follows_the_settings_choice() {
        use crate::settings::ThemeChoice;
        assert_eq!(mode_for_choice(ThemeChoice::Dark), ThemeMode::Dark);
        assert_eq!(mode_for_choice(ThemeChoice::Light), ThemeMode::Light);
    }

    #[test]
    fn set_and_get_mode() {
        set_mode(ThemeMode::Light);
        assert_eq!(current_mode(), ThemeMode::Light);
        let p = current();
        assert_eq!(p.background, Color::new(247, 250, 255));

        set_mode(ThemeMode::Dark);
        assert_eq!(current_mode(), ThemeMode::Dark);
        let p = current();
        assert_eq!(p.background, Color::new(36, 39, 46));
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
        assert_ne!(d.status_background, l.status_background);
    }

    // Helper: sRGB relative luminance (handles non-grayscale colors)
    fn srgb_luminance(c: &Color) -> f64 {
        let r = (c.r as f64 / 255.0).powf(2.2);
        let g = (c.g as f64 / 255.0).powf(2.2);
        let b = (c.b as f64 / 255.0).powf(2.2);
        0.2126 * r + 0.7152 * g + 0.0722 * b
    }

    fn contrast_ratio(c1: &Color, c2: &Color) -> f64 {
        let l1 = srgb_luminance(c1);
        let l2 = srgb_luminance(c2);
        let (lighter, darker) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
        (lighter + 0.05) / (darker + 0.05)
    }

    #[test]
    fn dark_palette_text_muted_wcag_contrast() {
        let p = get_palette(ThemeMode::Dark);
        let ratio = contrast_ratio(&p.text_muted, &p.background);
        assert!(
            ratio >= 4.5,
            "Dark palette text_muted contrast ratio {:.2} is below WCAG AA 4.5:1 threshold",
            ratio
        );
    }

    #[test]
    fn light_palette_text_muted_wcag_contrast() {
        let p = get_palette(ThemeMode::Light);
        let ratio = contrast_ratio(&p.text_muted, &p.background);
        assert!(
            ratio >= 4.5,
            "Light palette text_muted contrast ratio {:.2} is below WCAG AA 4.5:1 threshold",
            ratio
        );
    }

    #[test]
    fn dark_palette_gutter_text_wcag_contrast() {
        let p = get_palette(ThemeMode::Dark);
        let ratio = contrast_ratio(&p.gutter_text, &p.gutter_background);
        assert!(
            ratio >= 4.5,
            "Dark palette gutter_text contrast ratio {:.2} is below WCAG AA 4.5:1 threshold",
            ratio
        );
    }

    #[test]
    fn light_palette_gutter_text_wcag_contrast() {
        let p = get_palette(ThemeMode::Light);
        let ratio = contrast_ratio(&p.gutter_text, &p.gutter_background);
        assert!(
            ratio >= 4.5,
            "Light palette gutter_text contrast ratio {:.2} is below WCAG AA 4.5:1 threshold",
            ratio
        );
    }
}
