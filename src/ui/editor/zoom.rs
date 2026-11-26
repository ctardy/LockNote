// Editor zoom and live font (A2).
//
// Ctrl+wheel, Ctrl+= / Ctrl+Numpad+, Ctrl+- / Ctrl+Numpad- and Ctrl+0 change
// the size of the editor font by 1 pt, between ZOOM_MIN_PT and ZOOM_MAX_PT.
// The zoom lives for the session only: nothing is saved, the settings keep
// the user's size. A new font chosen in File > Settings > Preferences is
// applied at once and ends the zoom. Both go through `switch_font`, which
// gives the editor the font the way it got it at startup; the gutter, the
// line spacing and the current-line band read the editor's live metrics and
// follow on their own.
//
// RichEdit's own Ctrl+wheel zoom (EM_SETZOOM) is blocked: it scales the text
// without changing the font, so the gutter would no longer match the lines.

#[allow(unused_imports)]
use super::*;

/// Smallest size the zoom goes down to, in points.
pub(super) const ZOOM_MIN_PT: f64 = 6.0;
/// Largest size the zoom goes up to, in points.
pub(super) const ZOOM_MAX_PT: f64 = 48.0;
/// One notch of a mouse wheel (WHEEL_DELTA).
pub(super) const WHEEL_DELTA: i32 = 120;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ZoomStep {
    In,
    Out,
    /// Back to the size of the settings.
    Reset,
}

/// The zoom step of a key press, `None` for any other key. `ch` is the
/// character the key types without Shift in the active layout
/// (`key_char`): the main-row `=` / `+` and `-` are found by character,
/// since their key moves with the layout (`-` is the 6 key on a French
/// AZERTY keyboard, which has no VK_OEM_MINUS). The numpad keys and the 0
/// of the main row are found by key. Ctrl alone: with Alt too it is AltGr,
/// which types characters (`@` is AltGr+0 on a French keyboard, `}` is
/// AltGr+=).
pub(super) fn zoom_key(vk: u32, ch: Option<char>, ctrl: bool, shift: bool, alt: bool) -> Option<ZoomStep> {
    if !ctrl || shift || alt {
        return None;
    }
    match (vk, ch) {
        (0x6B, _) => Some(ZoomStep::In),         // VK_ADD
        (0x6D, _) => Some(ZoomStep::Out),        // VK_SUBTRACT
        (0x30, _) => Some(ZoomStep::Reset),      // 0 of the main row
        (_, Some('=' | '+')) => Some(ZoomStep::In),
        (_, Some('-')) => Some(ZoomStep::Out),
        _ => None,
    }
}

/// The character of a key from `MapVirtualKeyW(vk, MAPVK_VK_TO_CHAR)`:
/// `None` for 0 (no character) and for a dead key (top bit set), which
/// only prepares an accent.
pub(super) fn key_char(mapped: u32) -> Option<char> {
    if mapped == 0 || mapped & 0x8000_0000 != 0 {
        return None;
    }
    char::from_u32(mapped)
}

/// Size after `step`, from `current` (the size shown) and `base` (the size
/// of the settings). 1 pt per step, kept within ZOOM_MIN_PT..=ZOOM_MAX_PT; a
/// size already past a bound does not move further past it. Reset gives the
/// settings' size as it is, even outside the zoom range.
pub(super) fn zoomed_size(current: f64, base: f64, step: ZoomStep) -> f64 {
    match step {
        ZoomStep::Reset => base,
        ZoomStep::In if current >= ZOOM_MAX_PT => current,
        ZoomStep::In => (current + 1.0).clamp(ZOOM_MIN_PT, ZOOM_MAX_PT),
        ZoomStep::Out if current <= ZOOM_MIN_PT => current,
        ZoomStep::Out => (current - 1.0).clamp(ZOOM_MIN_PT, ZOOM_MAX_PT),
    }
}

/// Wheel rotation turned into zoom steps: `(steps, rest)`, one step per full
/// notch (positive = away from the user = zoom in), `rest` carried to the
/// next message so fine-grained wheels and touchpads zoom by notches too.
/// A turn in the other direction drops what was carried.
pub(super) fn wheel_steps(carried: i32, delta: i32) -> (i32, i32) {
    let carried = if carried != 0 && delta != 0 && carried.signum() != delta.signum() { 0 } else { carried };
    let total = carried.saturating_add(delta);
    (total / WHEEL_DELTA, total % WHEEL_DELTA)
}

/// The font the editor shows: the settings' family and size (`base_pt`),
/// and the size actually applied (`size_pt`, different while zoomed).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct EditorFontState {
    pub family: String,
    pub base_pt: f64,
    pub size_pt: f64,
}

impl EditorFontState {
    /// The font of the settings, not zoomed.
    pub(super) fn from_settings(family: &str, size_pt: f64) -> Self {
        Self { family: family.to_string(), base_pt: size_pt, size_pt }
    }

    /// The same font after a zoom step.
    pub(super) fn zoomed(&self, step: ZoomStep) -> Self {
        Self { size_pt: zoomed_size(self.size_pt, self.base_pt, step), ..self.clone() }
    }

    /// Whether this is the font of these settings (zoomed or not).
    pub(super) fn follows(&self, family: &str, size_pt: f64) -> bool {
        self.family == family && self.base_pt == size_pt
    }
}

/// The window side of a font change (sequence tests use a fake).
pub(super) trait FontSteps {
    /// Build the font and give it to the editor (line spacing and colours
    /// re-applied, the text, undo and modified state untouched).
    fn apply_font(&mut self, family: &str, size_pt: f64) -> Result<(), String>;
    /// Record the state the editor now shows.
    fn store(&mut self, state: EditorFontState);
    /// Let the gutter and the layout follow the new metrics.
    fn refresh(&mut self);
}

/// Bring the editor from `current` to `target`. The editor gets a new font
/// only when the family or the applied size differ; the state is stored once
/// the editor took the font, then the gutter and layout follow. Returns
/// whether the editor's font changed. On an error nothing is stored: the
/// editor keeps the font it had.
pub(super) fn switch_font(
    current: &EditorFontState,
    target: EditorFontState,
    steps: &mut impl FontSteps,
) -> Result<bool, String> {
    if *current == target {
        return Ok(false);
    }
    if current.family == target.family && current.size_pt == target.size_pt {
        // Only the settings' size moved (a zoom now equal to it): the
        // editor already shows this font.
        steps.store(target);
        return Ok(false);
    }
    steps.apply_font(&target.family, target.size_pt)?;
    steps.store(target);
    steps.refresh();
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_map_to_steps_with_ctrl_only() {
        for (vk, ch, step) in [
            (0xBB, Some('='), ZoomStep::In),  // US, French: the = key
            (0xBB, Some('+'), ZoomStep::In),  // German: the + key
            (0x6B, Some('+'), ZoomStep::In),  // numpad +
            (0xBD, Some('-'), ZoomStep::Out), // US: VK_OEM_MINUS
            (0x36, Some('-'), ZoomStep::Out), // French AZERTY: - is on the 6 key
            (0x6D, Some('-'), ZoomStep::Out), // numpad -
            (0x30, Some('0'), ZoomStep::Reset),
            (0x30, Some('à'), ZoomStep::Reset), // AZERTY: the 0 key types à
        ] {
            assert_eq!(zoom_key(vk, ch, true, false, false), Some(step), "vk {vk:#x} {ch:?}");
            assert_eq!(zoom_key(vk, ch, false, false, false), None, "vk {vk:#x} without Ctrl");
            assert_eq!(zoom_key(vk, ch, true, true, false), None, "vk {vk:#x} with Shift");
            // AltGr = Ctrl+Alt: AltGr+0 types @ on a French keyboard.
            assert_eq!(zoom_key(vk, ch, true, false, true), None, "vk {vk:#x} with AltGr");
        }
    }

    #[test]
    fn other_keys_are_not_zoom() {
        for (vk, ch) in [
            (0x31, Some('1')),
            (0x36, Some('6')), // US Ctrl+6
            (0x60, Some('0')), // numpad 0: not the reset key
            (0x46, Some('F')),
            (0x5A, Some('Z')),
            (0xBE, Some('.')),
            (0xBC, Some(',')),
            (0xBD, None),      // a key whose character is unknown
            (0xBB, Some(')')), // French: the ) key next to =
        ] {
            assert_eq!(zoom_key(vk, ch, true, false, false), None, "vk {vk:#x} {ch:?}");
        }
    }

    #[test]
    fn key_char_skips_dead_keys_and_keys_without_a_character() {
        assert_eq!(key_char(0x2D), Some('-'));
        assert_eq!(key_char(0x3D), Some('='));
        assert_eq!(key_char(0), None);
        assert_eq!(key_char(0x8000_005E), None, "dead ^ of a French keyboard");
    }

    #[test]
    fn a_step_is_one_point() {
        assert_eq!(zoomed_size(11.0, 11.0, ZoomStep::In), 12.0);
        assert_eq!(zoomed_size(11.0, 11.0, ZoomStep::Out), 10.0);
        assert_eq!(zoomed_size(13.5, 13.5, ZoomStep::In), 14.5);
        assert_eq!(zoomed_size(20.0, 11.0, ZoomStep::Reset), 11.0);
    }

    #[test]
    fn the_zoom_stays_within_its_bounds() {
        assert_eq!(zoomed_size(48.0, 11.0, ZoomStep::In), 48.0);
        assert_eq!(zoomed_size(47.5, 11.0, ZoomStep::In), 48.0);
        assert_eq!(zoomed_size(6.0, 11.0, ZoomStep::Out), 6.0);
        assert_eq!(zoomed_size(6.5, 11.0, ZoomStep::Out), 6.0);
        let mut size = 11.0;
        for _ in 0..100 {
            size = zoomed_size(size, 11.0, ZoomStep::In);
        }
        assert_eq!(size, ZOOM_MAX_PT);
        for _ in 0..100 {
            size = zoomed_size(size, 11.0, ZoomStep::Out);
        }
        assert_eq!(size, ZOOM_MIN_PT);
    }

    #[test]
    fn a_size_past_a_bound_never_moves_further_out() {
        // The settings accept up to 72 pt: zooming in keeps it, zooming out
        // comes back into the range, reset gives it back.
        assert_eq!(zoomed_size(72.0, 72.0, ZoomStep::In), 72.0);
        assert_eq!(zoomed_size(72.0, 72.0, ZoomStep::Out), 48.0);
        assert_eq!(zoomed_size(48.0, 72.0, ZoomStep::Reset), 72.0);
        assert_eq!(zoomed_size(5.0, 5.0, ZoomStep::Out), 5.0);
        assert_eq!(zoomed_size(5.0, 5.0, ZoomStep::In), 6.0);
    }

    #[test]
    fn wheel_notches_become_steps() {
        assert_eq!(wheel_steps(0, 120), (1, 0));
        assert_eq!(wheel_steps(0, -120), (-1, 0));
        assert_eq!(wheel_steps(0, 360), (3, 0));
        assert_eq!(wheel_steps(0, 0), (0, 0));
    }

    #[test]
    fn fine_wheel_deltas_add_up_to_a_notch() {
        let (s, rest) = wheel_steps(0, 40);
        assert_eq!((s, rest), (0, 40));
        let (s, rest) = wheel_steps(rest, 40);
        assert_eq!((s, rest), (0, 80));
        let (s, rest) = wheel_steps(rest, 50);
        assert_eq!((s, rest), (1, 10));
        assert_eq!(wheel_steps(-100, -30), (-1, -10));
    }

    #[test]
    fn a_turn_back_drops_the_carried_rotation() {
        assert_eq!(wheel_steps(100, -40), (0, -40));
        assert_eq!(wheel_steps(-100, 120), (1, 0));
        // A huge rotation saturates instead of overflowing.
        assert_eq!(wheel_steps(i32::MAX - 10, 120), (i32::MAX / WHEEL_DELTA, i32::MAX % WHEEL_DELTA));
    }

    #[test]
    fn state_zooms_and_follows_the_settings() {
        let s = EditorFontState::from_settings("Consolas", 11.0);
        assert_eq!(s.size_pt, 11.0);
        let z = s.zoomed(ZoomStep::In).zoomed(ZoomStep::In);
        assert_eq!((z.base_pt, z.size_pt), (11.0, 13.0));
        assert!(z.follows("Consolas", 11.0), "a zoom keeps following its settings");
        assert!(!z.follows("Consolas", 12.0));
        assert!(!z.follows("Courier New", 11.0));
        assert_eq!(z.zoomed(ZoomStep::Reset), s);
    }

    /// Records the calls in order; `fail` makes `apply_font` refuse.
    #[derive(Default)]
    struct Fake {
        calls: Vec<String>,
        stored: Option<EditorFontState>,
        fail: bool,
    }

    impl FontSteps for Fake {
        fn apply_font(&mut self, family: &str, size_pt: f64) -> Result<(), String> {
            self.calls.push(format!("apply {family} {size_pt}"));
            if self.fail { Err("CreateFontIndirectW failed".into()) } else { Ok(()) }
        }
        fn store(&mut self, state: EditorFontState) {
            self.calls.push(format!("store {}", state.size_pt));
            self.stored = Some(state);
        }
        fn refresh(&mut self) {
            self.calls.push("refresh".into());
        }
    }

    #[test]
    fn a_zoom_applies_then_stores_then_refreshes() {
        let s = EditorFontState::from_settings("Consolas", 11.0);
        let mut fake = Fake::default();
        assert_eq!(switch_font(&s, s.zoomed(ZoomStep::In), &mut fake), Ok(true));
        assert_eq!(fake.calls, ["apply Consolas 12", "store 12", "refresh"]);
        assert_eq!(fake.stored, Some(EditorFontState { family: "Consolas".into(), base_pt: 11.0, size_pt: 12.0 }));
    }

    #[test]
    fn a_zoom_at_its_bound_does_nothing() {
        let s = EditorFontState { family: "Consolas".into(), base_pt: 11.0, size_pt: ZOOM_MAX_PT };
        let mut fake = Fake::default();
        assert_eq!(switch_font(&s, s.zoomed(ZoomStep::In), &mut fake), Ok(false));
        assert!(fake.calls.is_empty());
    }

    #[test]
    fn new_settings_font_ends_the_zoom() {
        let zoomed = EditorFontState::from_settings("Consolas", 11.0).zoomed(ZoomStep::In).zoomed(ZoomStep::In);
        let mut fake = Fake::default();
        let target = EditorFontState::from_settings("Courier New", 14.0);
        assert_eq!(switch_font(&zoomed, target.clone(), &mut fake), Ok(true));
        assert_eq!(fake.calls, ["apply Courier New 14", "store 14", "refresh"]);
        assert_eq!(fake.stored, Some(target));
    }

    #[test]
    fn settings_size_equal_to_the_zoom_only_records_it() {
        let zoomed = EditorFontState::from_settings("Consolas", 11.0).zoomed(ZoomStep::In);
        let mut fake = Fake::default();
        let target = EditorFontState::from_settings("Consolas", 12.0);
        assert_eq!(switch_font(&zoomed, target.clone(), &mut fake), Ok(false));
        assert_eq!(fake.calls, ["store 12"]);
        assert_eq!(fake.stored, Some(target));
    }

    #[test]
    fn a_refused_font_stores_nothing() {
        let s = EditorFontState::from_settings("Consolas", 11.0);
        let mut fake = Fake { fail: true, ..Default::default() };
        let r = switch_font(&s, s.zoomed(ZoomStep::Out), &mut fake);
        assert_eq!(r, Err("CreateFontIndirectW failed".to_string()));
        assert_eq!(fake.calls, ["apply Consolas 10"]);
        assert_eq!(fake.stored, None, "the editor keeps the font it had");
    }

    #[test]
    fn zoom_in_out_reset_sequence() {
        let base = EditorFontState::from_settings("Consolas", 11.0);
        let mut state = base.clone();
        let mut fake = Fake::default();
        for step in [ZoomStep::In, ZoomStep::In, ZoomStep::Out, ZoomStep::Reset, ZoomStep::Reset] {
            switch_font(&state, state.zoomed(step), &mut fake).expect("applied");
            if let Some(s) = fake.stored.clone() {
                state = s;
            }
        }
        assert_eq!(state, base);
        assert_eq!(
            fake.calls,
            [
                "apply Consolas 12", "store 12", "refresh",
                "apply Consolas 13", "store 13", "refresh",
                "apply Consolas 12", "store 12", "refresh",
                "apply Consolas 11", "store 11", "refresh",
            ]
        );
    }

    // ── wiring, read from the code (crate::read_source) ──

    fn source(file: &str) -> String {
        crate::read_source(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ui/editor").join(file))
    }

    fn body_of(file: &str, fun: &str) -> String {
        let src = source(file);
        let start = src.find(fun).unwrap_or_else(|| panic!("{fun} in {file}"));
        let end = src[start..].find("
    }
").map_or(src.len(), |e| start + e);
        src[start..end].to_string()
    }

    #[test]
    fn ctrl_wheel_never_reaches_richedit() {
        let ev = source("events.rs");
        let start = ev.find("0x020A if w_param & 0x0008 != 0 =>").expect("Ctrl+wheel arm in the editor handler");
        let arm = &ev[start..start + ev[start..].find("return Some(0);").expect("the arm consumes the message")];
        assert!(arm.contains("f_ed.on_wheel_zoom(delta)"));
        assert!(arm.contains("if !f_ed.input_blocked()"), "no zoom while locked");
    }

    #[test]
    fn ctrl_wheel_over_the_gutter_zooms_too() {
        // The gutter passes its wheel to the main window (DefWindowProc).
        let ev = source("events.rs");
        let start = ev.find("if msg == 0x020A && w_param & 0x0008 != 0 {").expect("Ctrl+wheel in the main-window handler");
        let arm = &ev[start..start + ev[start..].find("return Some(0);").expect("the arm consumes the message")];
        assert!(arm.contains("f2.on_wheel_zoom(delta)"));
        assert!(arm.contains("if !f2.input_blocked()"), "no zoom while locked");
        let plain = ev.find("if msg == 0x020A {").expect("plain wheel repaint");
        assert!(start < plain, "Ctrl+wheel is handled before the plain wheel repaint");
    }

    #[test]
    fn preferences_report_a_font_failure_once_everything_is_applied() {
        let body = body_of("prefs_ops.rs", "pub(super) fn on_preferences");
        let theme = body.find("self.apply_theme_live(").expect("theme applied");
        let lang = body.find("t(\"settings.restart\")").expect("language message");
        let font = body.find("t(\"settings.font.failed\")").expect("font message");
        assert!(theme < font && lang < font, "no state change follows the font message");
    }

    #[test]
    fn an_imported_font_that_fails_is_reported() {
        // The import keeps the result of sync_view_menu (body_of panics otherwise).
        let body = body_of("backup_ops.rs", "let font = self.sync_view_menu();");
        assert!(body.contains("if let Err(e) = font"), "and reads it");
        assert!(body.contains("t(\"settings.font.failed\")"), "and tells the user");
    }

    #[test]
    fn zoom_keys_are_dispatched_after_the_lock_check() {
        let body = body_of("edit_ops.rs", "pub(super) fn handle_shortcut");
        let blocked = body.find("if self.input_blocked()").expect("lock check");
        let zoom = body.find("zoom::zoom_key(vk, zoom::key_char(").expect("zoom keys, by layout character");
        assert!(blocked < zoom);
    }

    #[test]
    fn the_zoom_is_never_saved() {
        for fun in ["pub(super) fn on_zoom", "pub(super) fn on_wheel_zoom"] {
            let body = body_of("edit_ops.rs", fun);
            assert!(!body.contains("settings"), "{fun} touches the settings");
            assert!(!body.contains("mark_modified"), "{fun} marks the note modified");
        }
    }

    #[test]
    fn startup_zoom_and_preferences_build_the_font_the_same_way() {
        assert!(source("build.rs").contains("build_editor_font(&settings.font_family, settings.font_size)"));
        assert!(body_of("edit_ops.rs", "fn apply_font(").contains("build_editor_font(family, size_pt)?"));
        assert!(body_of("edit_ops.rs", "fn apply_font(").contains("surface::set_font("));
        assert!(body_of("prefs_ops.rs", "pub(super) fn on_preferences").contains("self.apply_settings_font()"));
        assert!(body_of("edit_ops.rs", "pub(super) fn sync_view_menu").contains("self.apply_settings_font()"));
    }
}
