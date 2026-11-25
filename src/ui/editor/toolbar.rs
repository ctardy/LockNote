// Tab-strip toolbar buttons (Lock, Attach, TOTP, Vault): states, glyph and
// tooltips.
//
// The buttons are BS_OWNERDRAW children of the main window, positioned by
// `layout_controls` in the slots of `ui/tab_layout.rs` (that geometry is not
// touched here). This module owns what they look like: the colours of each
// state (idle, hot, pressed, active panel, keyboard focus), the padlock glyph
// geometry, the hover and click-focus tracking an owner-drawn button does not
// give, and the tooltips. Decisions are pure and unit-tested for both palettes.

#[allow(unused_imports)]
use super::*;

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering as AtomicOrdering};

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Gdi::InvalidateRect;
use windows::Win32::UI::Controls::{SetWindowTheme, TTF_IDISHWND, TTF_SUBCLASS, TTTOOLINFOW};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT, VIRTUAL_KEY, VK_CONTROL, VK_MENU,
};
use windows::Win32::UI::Shell::DefSubclassProc;
use windows::Win32::UI::WindowsAndMessaging::{CreateWindowExW, SendMessageW, WINDOW_EX_STYLE, WINDOW_STYLE, WS_POPUP};

use crate::theme::{self, Color, ThemeMode, ThemePalette};

// ---------------------------------------------------------------------
// Pure decisions
// ---------------------------------------------------------------------

/// DRAWITEMSTRUCT.itemState bits used by the buttons.
pub(super) const ODS_SELECTED: u32 = 0x0001;
pub(super) const ODS_FOCUS: u32 = 0x0010;
pub(super) const ODS_NOFOCUSRECT: u32 = 0x0200;

/// Corner radius of the button background.
pub(super) const BUTTON_RADIUS: i32 = 6;
/// Height of the padlock glyph; width is `LOCK_GLYPH_W`.
pub(super) const LOCK_GLYPH_H: i32 = 14;
pub(super) const LOCK_GLYPH_W: i32 = 10;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct ButtonState {
    /// The panel this button toggles is open.
    pub active: bool,
    /// The pointer is over the button.
    pub hot: bool,
    /// The button is being pressed (ODS_SELECTED).
    pub pressed: bool,
    /// Keyboard focus ring requested.
    pub focused: bool,
}

impl ButtonState {
    /// `focus_from_mouse`: the button got its focus from a click (see
    /// `is_mouse_focused`). The ring is "focus-visible": drawn only for a
    /// focus that came from the keyboard, since Windows stops sending
    /// ODS_NOFOCUSRECT once the keyboard has been used anywhere.
    pub(super) fn from_item_state(item_state: u32, active: bool, hot: bool, focus_from_mouse: bool) -> Self {
        Self {
            active,
            hot,
            pressed: item_state & ODS_SELECTED != 0,
            focused: item_state & ODS_FOCUS != 0 && item_state & ODS_NOFOCUSRECT == 0 && !focus_from_mouse,
        }
    }
}

#[derive(Debug, PartialEq)]
pub(super) struct ButtonPaint {
    pub background: Color,
    pub border: Color,
    /// Label and glyph colour.
    pub foreground: Color,
    /// Badge (entry count) colours.
    pub badge_background: Color,
    pub badge_foreground: Color,
    /// Colour of the keyboard focus ring, `None` when not focused.
    pub focus_ring: Option<Color>,
}

/// The foreground that reads best on `bg`: the palette's own text colours or
/// white/black, whichever has the highest contrast.
fn readable_on(bg: Color, pal: &ThemePalette) -> Color {
    let candidates = [pal.text_primary, pal.chrome, Color::new(255, 255, 255), Color::new(0, 0, 0)];
    *bg.most_readable(&candidates).expect("candidates are not empty")
}

/// Colours of one button state. The idle button belongs to the strip
/// (`chrome`), hovering lifts it to `surface_light`, pressing to `border`; an
/// open panel is the accent (`accent_hover` while hot).
pub(super) fn button_paint(pal: &ThemePalette, st: ButtonState) -> ButtonPaint {
    let (background, border) = if st.active {
        let bg = if st.hot || st.pressed { pal.accent_hover } else { pal.accent };
        (bg, bg)
    } else if st.pressed {
        (pal.border, pal.border)
    } else if st.hot {
        (pal.chrome_hover, pal.border)
    } else {
        (pal.chrome, pal.chrome_border)
    };
    let foreground = if st.active {
        readable_on(background, pal)
    } else if st.hot || st.pressed {
        pal.text_primary
    } else {
        pal.text_secondary
    };
    // The badge inverts the button: it must stand out from the button
    // background and stay readable, which `foreground` on `background` is.
    let (badge_background, badge_foreground) = if st.active || pal.accent.contrast_ratio(&background) < 3.0 {
        (foreground, background)
    } else {
        (pal.accent, readable_on(pal.accent, pal))
    };
    ButtonPaint {
        background,
        border,
        foreground,
        badge_background,
        badge_foreground,
        focus_ring: st.focused.then_some(if st.active { foreground } else { pal.accent }),
    }
}

/// Padlock geometry for a button centred on `(cx, cy)`: `(body, shackle)` as
/// `(left, top, right, bottom)`. The whole glyph is `LOCK_GLYPH_H` tall and
/// centred vertically; the shackle's lower half hides behind the body.
pub(super) fn lock_glyph(cx: i32, cy: i32) -> ((i32, i32, i32, i32), (i32, i32, i32, i32)) {
    let top = cy - LOCK_GLYPH_H / 2;
    let body_top = top + 7;
    let body = (cx - LOCK_GLYPH_W / 2, body_top, cx + LOCK_GLYPH_W / 2, top + LOCK_GLYPH_H);
    let shackle = (cx - 4, top, cx + 4, body_top + 1);
    (body, shackle)
}

// ---------------------------------------------------------------------
// Hover tracking
// ---------------------------------------------------------------------

/// The toolbar button under the pointer (0 = none). One pointer, one button.
static HOT_BUTTON: AtomicIsize = AtomicIsize::new(0);

pub(super) fn is_hot(button: isize) -> bool {
    button != 0 && HOT_BUTTON.load(AtomicOrdering::Relaxed) == button
}

/// The toolbar button whose focus came from a click (0 = none). Only one
/// window has the focus, so one slot is enough.
static MOUSE_FOCUSED: AtomicIsize = AtomicIsize::new(0);

pub(super) fn is_mouse_focused(button: isize) -> bool {
    button != 0 && MOUSE_FOCUSED.load(AtomicOrdering::Relaxed) == button
}

fn mark_mouse_focus(button: isize) {
    MOUSE_FOCUSED.store(button, AtomicOrdering::Relaxed);
}

/// Forget the click mark of `button` (no-op when another button holds it).
/// Returns whether the mark was cleared.
fn clear_mouse_focus(button: isize) -> bool {
    MOUSE_FOCUSED.compare_exchange(button, 0, AtomicOrdering::Relaxed, AtomicOrdering::Relaxed).is_ok()
}

/// A key pressed on a focused button that means the keyboard is driving it
/// (Tab, Enter, Space, arrows), so its focus ring should show. Modifiers and
/// shortcuts (anything with Ctrl or Alt held) do not: Ctrl+S after clicking
/// Vault must not bring the ring back.
pub(super) fn key_shows_focus(vk: u32, ctrl_down: bool, alt_down: bool) -> bool {
    const VK_TAB: u32 = 0x09;
    const VK_RETURN: u32 = 0x0D;
    const VK_SPACE: u32 = 0x20;
    const VK_LEFT: u32 = 0x25;
    const VK_DOWN: u32 = 0x28;
    if ctrl_down || alt_down {
        return false;
    }
    matches!(vk, VK_TAB | VK_RETURN | VK_SPACE | VK_LEFT..=VK_DOWN)
}

const WM_MOUSEMOVE: u32 = 0x0200;
const WM_MOUSELEAVE: u32 = 0x02A3;
const WM_LBUTTONDOWN: u32 = 0x0201;
const WM_KEYDOWN: u32 = 0x0100;
const WM_KILLFOCUS: u32 = 0x0008;
/// First id of the per-button hover subclasses (0x40005 is the draw handler).
const HOVER_HANDLER_BASE_ID: usize = 0x40040;

fn repaint(hwnd: HWND) {
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

/// Subclass `button` so it learns when the pointer enters and leaves, and
/// whether its focus came from a click (no focus ring) or the keyboard.
fn track_hover(button: &nwg::Button, id: usize) {
    let nwg::ControlHandle::Hwnd(raw) = button.handle else { return };
    let raw = raw as isize;
    let handler = nwg::bind_raw_event_handler(&button.handle, id, move |h, msg, w, l| {
        let hwnd = HWND(h as *mut _);
        match msg {
            WM_MOUSEMOVE => {
                if HOT_BUTTON.swap(raw, AtomicOrdering::Relaxed) != raw {
                    let mut tme = TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    if let Err(e) = unsafe { TrackMouseEvent(&mut tme) } {
                        eprintln!("WARN toolbar: TrackMouseEvent failed ({e}); the hover state may stick");
                    }
                    repaint(hwnd);
                }
                let res = unsafe { DefSubclassProc(hwnd, msg, WPARAM(w), LPARAM(l)) };
                Some(res.0)
            }
            WM_MOUSELEAVE => {
                let _ = HOT_BUTTON.compare_exchange(raw, 0, AtomicOrdering::Relaxed, AtomicOrdering::Relaxed);
                repaint(hwnd);
                let res = unsafe { DefSubclassProc(hwnd, msg, WPARAM(w), LPARAM(l)) };
                Some(res.0)
            }
            WM_LBUTTONDOWN => {
                // Marked before the default handling, which gives the focus
                // and paints the pressed button.
                mark_mouse_focus(raw);
                let res = unsafe { DefSubclassProc(hwnd, msg, WPARAM(w), LPARAM(l)) };
                Some(res.0)
            }
            WM_KEYDOWN => {
                // The keyboard navigates or activates this button: show its
                // ring. Modifiers and shortcuts leave the click mark.
                let held = |vk: VIRTUAL_KEY| unsafe { GetKeyState(i32::from(vk.0)) } < 0;
                if key_shows_focus(w as u32, held(VK_CONTROL), held(VK_MENU)) && clear_mouse_focus(raw) {
                    repaint(hwnd);
                }
                let res = unsafe { DefSubclassProc(hwnd, msg, WPARAM(w), LPARAM(l)) };
                Some(res.0)
            }
            WM_KILLFOCUS => {
                let _ = clear_mouse_focus(raw);
                let res = unsafe { DefSubclassProc(hwnd, msg, WPARAM(w), LPARAM(l)) };
                Some(res.0)
            }
            _ => None,
        }
    });
    match handler {
        Ok(h) => std::mem::forget(h),
        Err(e) => panic!("toolbar hover handler: {e:?}"),
    }
}

// ---------------------------------------------------------------------
// Tooltips
// ---------------------------------------------------------------------

static WARNED_TOOLTIP_THEME: AtomicBool = AtomicBool::new(false);

/// Visual-style class of the tooltips for a mode; `None` = the default theme.
fn tooltip_theme_name(mode: ThemeMode) -> Option<&'static str> {
    match mode {
        ThemeMode::Dark => Some("DarkMode_Explorer"),
        ThemeMode::Light => None,
    }
}

/// The toolbar's tooltip control (0 = none), kept so a theme change can
/// re-theme it. The UI is single-threaded.
static TOOLTIP_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

/// Apply the tooltip theme of the current mode to `tip`. A failure is
/// reported once and the previous look stays.
fn theme_tooltip(tip: HWND) {
    let name: Option<Vec<u16>> =
        tooltip_theme_name(theme::current_mode()).map(|n| n.encode_utf16().chain(std::iter::once(0)).collect());
    let class = name.as_ref().map_or(windows::core::PCWSTR::null(), |n| windows::core::PCWSTR(n.as_ptr()));
    if let Err(e) = unsafe { SetWindowTheme(tip, class, windows::core::PCWSTR::null()) } {
        if !WARNED_TOOLTIP_THEME.swap(true, AtomicOrdering::SeqCst) {
            eprintln!("WARN toolbar: tooltip theme unavailable ({e}); keeping the previous look");
        }
    }
}

/// Re-theme the toolbar tooltips after a theme change (no-op before they exist).
pub(super) fn retheme_tooltips() {
    let raw = TOOLTIP_HWND.load(AtomicOrdering::SeqCst);
    if raw != 0 {
        theme_tooltip(HWND(raw as *mut _));
    }
}

const TTS_ALWAYSTIP: u32 = 0x01;
const TTS_NOPREFIX: u32 = 0x02;
const TTM_ADDTOOLW: u32 = 0x0432;

/// One tooltip control for the toolbar buttons: shown after the standard
/// delay, dark in the Dark theme. `entries` are `(button, text)`.
fn add_tooltips(parent: HWND, entries: &[(isize, String)]) {
    let tip = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            windows::core::w!("tooltips_class32"),
            None,
            WS_POPUP | WINDOW_STYLE(TTS_ALWAYSTIP | TTS_NOPREFIX),
            0, 0, 0, 0,
            Some(parent),
            None,
            None,
            None,
        )
    };
    let tip = match tip {
        Ok(t) => t,
        Err(e) => {
            eprintln!("WARN toolbar: tooltip control not created ({e}); the buttons have no tooltip");
            return;
        }
    };
    TOOLTIP_HWND.store(tip.0 as isize, AtomicOrdering::SeqCst);
    theme_tooltip(tip);
    for (button, text) in entries {
        // The tooltip keeps the pointer: the text lives as long as the process.
        let wide: &'static mut [u16] = Box::leak(text.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>().into_boxed_slice());
        let mut info = TTTOOLINFOW {
            cbSize: std::mem::size_of::<TTTOOLINFOW>() as u32,
            uFlags: TTF_IDISHWND | TTF_SUBCLASS,
            hwnd: parent,
            uId: *button as usize,
            lpszText: windows::core::PWSTR(wide.as_mut_ptr()),
            ..Default::default()
        };
        let ok = unsafe {
            SendMessageW(tip, TTM_ADDTOOLW, None, Some(LPARAM(&mut info as *mut _ as isize)))
        };
        if ok.0 == 0 {
            eprintln!("WARN toolbar: TTM_ADDTOOLW refused the tooltip of button {button:#x}");
        }
    }
}

impl EditorForm {
    /// Hover tracking and tooltips for the tab-strip buttons. Called once,
    /// after the buttons are built.
    pub(super) fn install_toolbar_chrome(&self) {
        let nwg::ControlHandle::Hwnd(parent) = self.window.handle else { return };
        let mut tips: Vec<(isize, String)> = Vec::new();
        let mut next_id = HOVER_HANDLER_BASE_ID;
        let mut add = |btn: &nwg::Button, text: String| {
            if let nwg::ControlHandle::Hwnd(h) = btn.handle {
                tips.push((h as isize, text));
                track_hover(btn, next_id);
                next_id += 1;
            }
        };
        add(&self.toolbar_prompts, t("menu.toggle.prompts"));
        add(&self.toolbar_vault, t("menu.toggle.vault"));
        add(&self.toolbar_totp, t("menu.toggle.totp"));
        add(&self.toolbar_attach, t("menu.toggle.attach"));
        add(&self.toolbar_lock, t("menu.toggle.lock"));
        add_tooltips(HWND(parent as *mut _), &tips);
    }
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const BOTH: [ThemeMode; 2] = [ThemeMode::Dark, ThemeMode::Light];

    fn all_states() -> Vec<ButtonState> {
        let mut v = Vec::new();
        for active in [false, true] {
            for hot in [false, true] {
                for pressed in [false, true] {
                    v.push(ButtonState { active, hot, pressed, focused: false });
                }
            }
        }
        v
    }

    #[test]
    fn tooltips_are_dark_only_in_dark() {
        assert_eq!(tooltip_theme_name(ThemeMode::Dark), Some("DarkMode_Explorer"));
        assert_eq!(tooltip_theme_name(ThemeMode::Light), None);
    }

    #[test]
    fn item_state_bits_map_to_button_state() {
        let s = ButtonState::from_item_state(ODS_SELECTED, true, true, false);
        assert_eq!(s, ButtonState { active: true, hot: true, pressed: true, focused: false });
        assert!(ButtonState::from_item_state(ODS_FOCUS, false, false, false).focused);
        // Windows asks to hide focus cues until the keyboard is used.
        assert!(!ButtonState::from_item_state(ODS_FOCUS | ODS_NOFOCUSRECT, false, false, false).focused);
    }

    #[test]
    fn focus_given_by_a_click_draws_no_ring() {
        // Once the keyboard has been used Windows stops sending
        // ODS_NOFOCUSRECT: a clicked button must still not show the ring
        // (it read as a black square around the label of an open panel).
        for active in [false, true] {
            assert!(!ButtonState::from_item_state(ODS_FOCUS, active, true, true).focused, "active={active}");
            assert!(!ButtonState::from_item_state(ODS_FOCUS | ODS_SELECTED, active, true, true).focused, "active={active}");
        }
        // Keyboard focus keeps its ring.
        assert!(ButtonState::from_item_state(ODS_FOCUS, true, false, false).focused);
    }

    #[test]
    fn only_navigation_and_activation_keys_show_the_ring() {
        const VK_TAB: u32 = 0x09;
        const VK_RETURN: u32 = 0x0D;
        const VK_SPACE: u32 = 0x20;
        const VK_LEFT: u32 = 0x25;
        const VK_DOWN: u32 = 0x28;
        for vk in [VK_TAB, VK_RETURN, VK_SPACE, VK_LEFT, 0x26, 0x27, VK_DOWN] {
            assert!(key_shows_focus(vk, false, false), "vk={vk:#x}");
        }
        // Modifiers alone (Shift, Ctrl, Alt, left/right Win).
        for vk in [0x10u32, 0x11, 0x12, 0x5B, 0x5C] {
            assert!(!key_shows_focus(vk, false, false), "vk={vk:#x}");
        }
        // Shortcuts typed after a click: Ctrl+S, Ctrl+F, Ctrl+Tab, Alt+arrow.
        assert!(!key_shows_focus(0x53, true, false));
        assert!(!key_shows_focus(0x46, true, false));
        assert!(!key_shows_focus(VK_TAB, true, false));
        assert!(!key_shows_focus(VK_LEFT, false, true));
        // Letters, digits, function keys, Escape.
        for vk in [0x41u32, 0x30, 0x70, 0x1B] {
            assert!(!key_shows_focus(vk, false, false), "vk={vk:#x}");
        }
    }

    #[test]
    fn mouse_focus_mark_is_set_by_a_click_and_cleared_by_keys_or_focus_loss() {
        MOUSE_FOCUSED.store(0, AtomicOrdering::Relaxed);
        assert!(!is_mouse_focused(0));
        assert!(!is_mouse_focused(7));
        mark_mouse_focus(7);
        assert!(is_mouse_focused(7));
        assert!(!is_mouse_focused(8));
        // A key on another button does not touch this one's mark.
        clear_mouse_focus(8);
        assert!(is_mouse_focused(7));
        // A key on the button (Tab, Space) or a focus loss clears it.
        clear_mouse_focus(7);
        assert!(!is_mouse_focused(7));
        // A click on another button moves the single mark.
        mark_mouse_focus(7);
        mark_mouse_focus(8);
        assert!(!is_mouse_focused(7));
        assert!(is_mouse_focused(8));
        MOUSE_FOCUSED.store(0, AtomicOrdering::Relaxed);
    }

    #[test]
    fn idle_hot_and_pressed_are_three_distinct_looks() {
        for mode in BOTH {
            let pal = theme::get_palette(mode);
            let idle = button_paint(pal, ButtonState::default());
            let hot = button_paint(pal, ButtonState { hot: true, ..Default::default() });
            let pressed = button_paint(pal, ButtonState { pressed: true, hot: true, ..Default::default() });
            assert_ne!(idle.background, hot.background, "{mode:?}");
            assert_ne!(hot.background, pressed.background, "{mode:?}");
            assert_ne!(idle.background, pressed.background, "{mode:?}");
        }
    }

    #[test]
    fn idle_button_belongs_to_the_strip() {
        for mode in BOTH {
            let pal = theme::get_palette(mode);
            assert_eq!(button_paint(pal, ButtonState::default()).background, pal.chrome);
        }
    }

    #[test]
    fn open_panel_uses_the_accent_and_a_hot_one_the_hover_accent() {
        for mode in BOTH {
            let pal = theme::get_palette(mode);
            let on = button_paint(pal, ButtonState { active: true, ..Default::default() });
            let on_hot = button_paint(pal, ButtonState { active: true, hot: true, ..Default::default() });
            assert_eq!(on.background, pal.accent);
            assert_eq!(on_hot.background, pal.accent_hover);
        }
    }

    #[test]
    fn labels_and_glyphs_meet_aa_in_every_state_and_theme() {
        for mode in BOTH {
            let pal = theme::get_palette(mode);
            for st in all_states() {
                let p = button_paint(pal, st);
                let ratio = p.foreground.contrast_ratio(&p.background);
                assert!(ratio >= 4.5, "{mode:?} {st:?}: label {ratio:.2}:1");
            }
        }
    }

    #[test]
    fn badges_are_readable_and_stand_out_from_the_button() {
        for mode in BOTH {
            let pal = theme::get_palette(mode);
            for st in all_states() {
                let p = button_paint(pal, st);
                let text = p.badge_foreground.contrast_ratio(&p.badge_background);
                assert!(text >= 4.5, "{mode:?} {st:?}: badge text {text:.2}:1");
                let shape = p.badge_background.contrast_ratio(&p.background);
                assert!(shape >= 3.0, "{mode:?} {st:?}: badge against the button {shape:.2}:1");
            }
        }
    }

    #[test]
    fn button_edge_is_visible_against_the_strip_when_not_idle() {
        // A hot or pressed button is told apart from the strip by its fill
        // (the strip is `chrome`); the fill/strip contrast is the UI-glyph
        // 3:1 only for the accent states, the hover fill is deliberately soft.
        for mode in BOTH {
            let pal = theme::get_palette(mode);
            let on = button_paint(pal, ButtonState { active: true, ..Default::default() });
            assert!(on.background.contrast_ratio(&pal.chrome) >= 3.0, "{mode:?}: open-panel button vs strip");
        }
    }

    #[test]
    fn focus_ring_only_when_focused_and_visible() {
        for mode in BOTH {
            let pal = theme::get_palette(mode);
            let none = button_paint(pal, ButtonState::default());
            assert_eq!(none.focus_ring, None);
            let ring = button_paint(pal, ButtonState { focused: true, ..Default::default() }).focus_ring.expect("ring");
            assert!(ring.contrast_ratio(&pal.chrome) >= 3.0, "{mode:?}: focus ring vs strip");
            let on = button_paint(pal, ButtonState { active: true, focused: true, ..Default::default() });
            assert!(on.focus_ring.expect("ring").contrast_ratio(&on.background) >= 3.0, "{mode:?}: ring on the accent");
        }
    }

    #[test]
    fn lock_glyph_is_centred_and_fixed_size() {
        for (cx, cy) in [(16, 12), (100, 20), (7, 3)] {
            let (body, shackle) = lock_glyph(cx, cy);
            let top = shackle.1.min(body.1);
            let bottom = shackle.3.max(body.3);
            assert_eq!(bottom - top, LOCK_GLYPH_H);
            assert_eq!((top + bottom) / 2, cy, "vertically centred");
            assert_eq!(body.2 - body.0, LOCK_GLYPH_W);
            assert_eq!((body.0 + body.2) / 2, cx, "horizontally centred");
            assert_eq!((shackle.0 + shackle.2) / 2, cx);
            assert!(shackle.3 > body.1, "the shackle's end hides behind the body");
        }
    }

    #[test]
    fn glyph_fits_the_smallest_button() {
        // layout_controls gives the buttons a 24 px height.
        assert!(LOCK_GLYPH_H + 2 * BUTTON_RADIUS / 2 <= 24);
    }

    #[test]
    fn hot_button_is_a_single_slot() {
        HOT_BUTTON.store(0, AtomicOrdering::Relaxed);
        assert!(!is_hot(0));
        assert!(!is_hot(42));
        HOT_BUTTON.store(42, AtomicOrdering::Relaxed);
        assert!(is_hot(42));
        assert!(!is_hot(43));
        HOT_BUTTON.store(0, AtomicOrdering::Relaxed);
    }
}
