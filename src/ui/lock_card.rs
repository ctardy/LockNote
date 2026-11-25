// Shared look of the unlock popup (dialogs/unlock.rs) and of the lock screen
// (editor/lock_overlay.rs, lock_ops.rs): one centred "card" with a padlock,
// a title, a prompt, a password field with a show/hide (eye) button, a status
// line (wrong password, retry delay, Caps Lock warning) and the buttons.
//
// Everything that can be decided without a window is a pure function and is
// unit-tested: DPI scaling, geometry, popup placement, colour contrast and the
// status-line decision. The Win32 part below only turns those decisions into
// GDI calls and control styles.
//
// Secrets: nothing in this module ever reads, stores or logs the password
// text; it only toggles the edit control's password character.

use crate::theme::{self, Color, ThemeMode, ThemePalette};
use crate::ui::dialogs::style::{install_nc_bands, NcFill};
use native_windows_gui as nwg;
use std::cell::RefCell;
use std::rc::Rc;

// ===========================================================================
// DPI scaling (pure)
// ===========================================================================

/// Reference DPI: the base sizes below are expressed at 96 DPI (100 %).
pub const BASE_DPI: u32 = 96;

/// Scale a 96-DPI length to `dpi`, rounded to the nearest pixel.
pub fn scale(px: i32, dpi: u32) -> i32 {
    ((i64::from(px) * i64::from(dpi) + i64::from(BASE_DPI) / 2).div_euclid(i64::from(BASE_DPI))) as i32
}

// ===========================================================================
// Geometry (pure)
// ===========================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }
    pub fn right(&self) -> i32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }
    pub fn inflate(&self, d: i32) -> Rect {
        Rect::new(self.x - d, self.y - d, self.w + 2 * d, self.h + 2 * d)
    }
}

/// Where the card lives: the popup window is the card; on the lock screen the
/// card floats on a backdrop that covers the whole client area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardMode {
    Popup,
    Overlay,
}

/// Positions of every element, in client coordinates of the window that owns
/// the controls (the popup window or the lock frame).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CardLayout {
    pub card: Rect,
    pub icon: Rect,
    pub title: Rect,
    pub subtitle: Rect,
    /// The rounded box painted behind the input and the eye button.
    pub field: Rect,
    pub input: Rect,
    pub eye: Rect,
    pub status: Rect,
    pub primary: Rect,
    /// Popup only: the lock screen has nothing to cancel.
    pub secondary: Option<Rect>,
}

// Sizes at 96 DPI.
const CARD_W: i32 = 440;
const CARD_MARGIN: i32 = 16; // minimum gap between the card and the window edge
const CARD_MIN_W: i32 = 280;
const PAD_X: i32 = 32;
const PAD_TOP: i32 = 28;
const PAD_BOTTOM: i32 = 28;
const ICON: i32 = 44;
const GAP_ICON_TITLE: i32 = 10;
const TITLE_H: i32 = 38;
const GAP_TITLE_SUB: i32 = 2;
const SUB_H: i32 = 24;
const GAP_SUB_FIELD: i32 = 22;
const FIELD_H: i32 = 52;
const GAP_FIELD_STATUS: i32 = 8;
const STATUS_H: i32 = 24;
const GAP_STATUS_BTN: i32 = 14;
const BTN_H: i32 = 44;
const BTN_PRIMARY_W: i32 = 132;
const BTN_SECONDARY_W: i32 = 112;
const BTN_GAP: i32 = 12;
const INPUT_PAD: i32 = 16;
const INPUT_H: i32 = 30;
const EYE: i32 = 36;
const EYE_PAD: i32 = 8;
const EYE_INPUT_GAP: i32 = 6;

impl CardLayout {
    /// Layout for a window whose client area is `client_w` x `client_h`.
    /// In `Popup` mode the card is the whole client area (use
    /// `popup_client_size` to size the window); in `Overlay` mode it is
    /// centred, and narrowed (never below a minimum) when the window is small.
    pub fn compute(mode: CardMode, client_w: i32, client_h: i32, dpi: u32) -> CardLayout {
        let s = |v: i32| scale(v, dpi);
        let card_w = match mode {
            CardMode::Popup => client_w,
            CardMode::Overlay => s(CARD_W).min(client_w - 2 * s(CARD_MARGIN)).max(s(CARD_MIN_W)),
        };
        let content_h = Self::content_height(dpi);
        let (card_x, card_y) = match mode {
            CardMode::Popup => (0, 0),
            CardMode::Overlay => ((client_w - card_w) / 2, ((client_h - content_h) / 2).max(0)),
        };
        let inner_x = card_x + s(PAD_X);
        let inner_w = card_w - 2 * s(PAD_X);

        let mut y = card_y + s(PAD_TOP);
        let icon_s = s(ICON);
        let icon = Rect::new(card_x + (card_w - icon_s) / 2, y, icon_s, icon_s);
        y += icon_s + s(GAP_ICON_TITLE);
        let title = Rect::new(inner_x, y, inner_w, s(TITLE_H));
        y += title.h + s(GAP_TITLE_SUB);
        let subtitle = Rect::new(inner_x, y, inner_w, s(SUB_H));
        y += subtitle.h + s(GAP_SUB_FIELD);
        let field = Rect::new(inner_x, y, inner_w, s(FIELD_H));
        let eye_s = s(EYE);
        let eye = Rect::new(field.right() - s(EYE_PAD) - eye_s, field.y + (field.h - eye_s) / 2, eye_s, eye_s);
        let input_h = s(INPUT_H);
        let input_x = field.x + s(INPUT_PAD);
        let input = Rect::new(input_x, field.y + (field.h - input_h) / 2, eye.x - s(EYE_INPUT_GAP) - input_x, input_h);
        y += field.h + s(GAP_FIELD_STATUS);
        let status = Rect::new(inner_x, y, inner_w, s(STATUS_H));
        y += status.h + s(GAP_STATUS_BTN);

        let btn_h = s(BTN_H);
        let (primary, secondary) = match mode {
            CardMode::Overlay => (Rect::new(inner_x, y, inner_w, btn_h), None),
            CardMode::Popup => {
                let pw = s(BTN_PRIMARY_W);
                let sw = s(BTN_SECONDARY_W);
                let primary = Rect::new(inner_x + inner_w - pw, y, pw, btn_h);
                let secondary = Rect::new(primary.x - s(BTN_GAP) - sw, y, sw, btn_h);
                (primary, Some(secondary))
            }
        };
        let card = Rect::new(card_x, card_y, card_w, content_h);
        CardLayout { card, icon, title, subtitle, field, input, eye, status, primary, secondary }
    }

    /// Height of the card at `dpi` (it does not depend on the width).
    fn content_height(dpi: u32) -> i32 {
        let s = |v: i32| scale(v, dpi);
        s(PAD_TOP)
            + s(ICON)
            + s(GAP_ICON_TITLE)
            + s(TITLE_H)
            + s(GAP_TITLE_SUB)
            + s(SUB_H)
            + s(GAP_SUB_FIELD)
            + s(FIELD_H)
            + s(GAP_FIELD_STATUS)
            + s(STATUS_H)
            + s(GAP_STATUS_BTN)
            + s(BTN_H)
            + s(PAD_BOTTOM)
    }

    /// Client size of the popup window at `dpi`.
    pub fn popup_client_size(dpi: u32) -> (i32, i32) {
        (scale(CARD_W, dpi), Self::content_height(dpi))
    }
}

/// Top-left corner that centres a `w` x `h` window on `anchor`.
pub fn center_in(anchor: Rect, w: i32, h: i32) -> (i32, i32) {
    (anchor.x + (anchor.w - w) / 2, anchor.y + (anchor.h - h) / 2)
}

/// Move `r` (without resizing it) so that it lies inside `area`; when it is
/// larger than `area` the top-left corner of `area` wins so the title bar
/// stays reachable.
pub fn clamp_into(r: Rect, area: Rect) -> Rect {
    let x = r.x.min(area.right() - r.w).max(area.x);
    let y = r.y.min(area.bottom() - r.h).max(area.y);
    Rect::new(x, y, r.w, r.h)
}

// ===========================================================================
// Colours and contrast (pure)
// ===========================================================================

const WHITE: Color = Color::new(255, 255, 255);
const BLACK: Color = Color::new(0, 0, 0);
/// Near-black text for light buttons (the dark theme's accent is light).
const DARK_TEXT: Color = Color::new(16, 18, 22);
/// Base of the Caps Lock warning colour, pushed to 4.5:1 on the card.
const WARNING_BASE: Color = Color::new(224, 160, 32);

fn channel(c: u8) -> f64 {
    let v = f64::from(c) / 255.0;
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// WCAG 2.x relative luminance.
pub fn relative_luminance(c: Color) -> f64 {
    0.2126 * channel(c.r) + 0.7152 * channel(c.g) + 0.0722 * channel(c.b)
}

/// WCAG 2.x contrast ratio, 1.0 (identical) to 21.0 (black on white).
pub fn contrast_ratio(a: Color, b: Color) -> f64 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// `fg` if it already reaches `min` on `bg`, else `fg` pulled toward the
/// extreme (black or white) that contrasts most with `bg`, in 5 % steps, until
/// it does. `min` must be at most 4.5 so the extreme always satisfies it.
pub fn ensure_contrast(fg: Color, bg: Color, min: f64) -> Color {
    if contrast_ratio(fg, bg) >= min {
        return fg;
    }
    let extreme = if contrast_ratio(BLACK, bg) >= contrast_ratio(WHITE, bg) { BLACK } else { WHITE };
    for p in (5..=100).step_by(5) {
        let candidate = fg.mix(&extreme, p);
        if contrast_ratio(candidate, bg) >= min {
            return candidate;
        }
    }
    extreme
}

/// White or near-black, whichever reads better on the worst of `backgrounds`.
pub fn best_text_on(backgrounds: &[Color]) -> Color {
    let worst = |text: Color| backgrounds.iter().map(|b| contrast_ratio(text, *b)).fold(f64::INFINITY, f64::min);
    if worst(WHITE) >= worst(DARK_TEXT) {
        WHITE
    } else {
        DARK_TEXT
    }
}

/// Every colour of the card, derived from the theme palette.
#[derive(Clone, Copy, Debug)]
pub struct CardColors {
    pub backdrop: Color,
    pub card_bg: Color,
    pub card_border: Color,
    pub field_bg: Color,
    /// Border of the password field at rest (>= 3:1 on the card, WCAG 1.4.11).
    pub field_border: Color,
    pub field_border_focus: Color,
    pub icon: Color,
    pub text_primary: Color,
    pub text_secondary: Color,
    pub error: Color,
    pub warning: Color,
    pub primary_bg: Color,
    pub primary_bg_pressed: Color,
    pub primary_text: Color,
    pub primary_bg_disabled: Color,
    pub primary_text_disabled: Color,
    pub secondary_bg: Color,
    pub secondary_bg_pressed: Color,
    pub secondary_border: Color,
    pub secondary_text: Color,
}

impl CardColors {
    pub fn for_mode(mode: ThemeMode) -> CardColors {
        Self::from_palette(theme::get_palette(mode), mode)
    }

    pub fn from_palette(pal: &ThemePalette, mode: ThemeMode) -> CardColors {
        let (backdrop, card_bg, field_bg) = match mode {
            ThemeMode::Dark => (pal.chrome, pal.surface, pal.background),
            ThemeMode::Light => (pal.chrome, WHITE, pal.surface),
        };
        // The border must stand out from both the card and the field fill.
        let field_border = ensure_contrast(ensure_contrast(pal.border, card_bg, 3.0), field_bg, 3.0);
        let primary_bg = pal.accent;
        let primary_bg_pressed = pal.accent_hover;
        let primary_bg_disabled = card_bg.mix(&pal.text_secondary, 18);
        let text_secondary = ensure_contrast(pal.text_secondary, card_bg, 4.5);
        CardColors {
            backdrop,
            card_bg,
            card_border: pal.border,
            field_bg,
            field_border,
            field_border_focus: ensure_contrast(pal.accent, card_bg, 3.0),
            icon: ensure_contrast(pal.accent, card_bg, 3.0),
            text_primary: pal.text_primary,
            text_secondary,
            error: ensure_contrast(pal.error_text, card_bg, 4.5),
            warning: ensure_contrast(WARNING_BASE, card_bg, 4.5),
            primary_bg,
            primary_bg_pressed,
            primary_text: best_text_on(&[primary_bg, primary_bg_pressed]),
            primary_bg_disabled,
            primary_text_disabled: ensure_contrast(pal.text_secondary, primary_bg_disabled, 3.0),
            secondary_bg: card_bg,
            secondary_bg_pressed: pal.surface_light,
            secondary_border: field_border,
            secondary_text: pal.text_primary,
        }
    }
}

// ===========================================================================
// Status line (pure decision + shared state)
// ===========================================================================

/// How the status line is coloured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Neutral,
    Error,
    Warning,
}

/// The Caps Lock hint is only useful while the user is typing in the field.
pub fn caps_hint_visible(caps_on: bool, field_focused: bool) -> bool {
    caps_on && field_focused
}

/// What the status line shows: an explicit message always wins (a wrong
/// password must not be hidden by the hint); otherwise the Caps Lock hint;
/// otherwise nothing.
pub fn status_line(msg_tone: Tone, msg: &str, caps_visible: bool, caps_text: &str) -> (Tone, String) {
    if !msg.is_empty() {
        (msg_tone, msg.to_string())
    } else if caps_visible {
        (Tone::Warning, caps_text.to_string())
    } else {
        (Tone::Neutral, String::new())
    }
}

struct StatusState {
    label: isize,
    msg_tone: Tone,
    msg: String,
    caps_text: String,
    field_focused: bool,
    shown: (Tone, String),
}

/// The text of the status label. Shared (cheap clones) between the code that
/// reports results and the input hooks that react to focus and Caps Lock.
#[derive(Clone)]
pub struct StatusMsg(Rc<RefCell<StatusState>>);

impl Default for StatusMsg {
    fn default() -> Self {
        Self::new()
    }
}

impl StatusMsg {
    pub fn new() -> Self {
        StatusMsg(Rc::new(RefCell::new(StatusState {
            label: 0,
            msg_tone: Tone::Neutral,
            msg: String::new(),
            caps_text: String::new(),
            field_focused: false,
            shown: (Tone::Neutral, String::new()),
        })))
    }

    /// Attach the label control this status drives.
    pub fn bind_label(&self, label: &nwg::ControlHandle) {
        self.0.borrow_mut().label = hwnd_of(label);
    }

    pub fn set_caps_text(&self, text: &str) {
        self.0.borrow_mut().caps_text = text.to_string();
        self.render();
    }

    /// Show `text` (empty clears it).
    pub fn set(&self, tone: Tone, text: &str) {
        {
            let mut st = self.0.borrow_mut();
            st.msg_tone = tone;
            st.msg = text.to_string();
        }
        self.render();
    }

    pub fn clear(&self) {
        self.set(Tone::Neutral, "");
    }

    /// The password field gained or lost the keyboard focus.
    pub fn set_field_focus(&self, focused: bool) {
        self.0.borrow_mut().field_focused = focused;
        self.render();
    }

    /// Re-evaluate the Caps Lock hint (the toggle key was pressed).
    pub fn refresh_caps(&self) {
        self.render();
    }

    fn render(&self) {
        let mut st = self.0.borrow_mut();
        let wanted = status_line(
            st.msg_tone,
            &st.msg,
            caps_hint_visible(caps_lock_on(), st.field_focused),
            &st.caps_text,
        );
        if wanted == st.shown {
            return;
        }
        st.shown = wanted.clone();
        if st.label != 0 {
            set_label_text_and_tone(st.label, &wanted.1, wanted.0);
        }
    }
}

// ===========================================================================
// Win32: handles, DPI, caps lock, fonts
// ===========================================================================

pub fn hwnd_of(h: &nwg::ControlHandle) -> isize {
    match h {
        nwg::ControlHandle::Hwnd(p) => *p as isize,
        // A control that is not a window cannot be part of the card: fail fast.
        other => panic!("lock_card: expected a window handle, got {other:?}"),
    }
}

fn to_hwnd(h: isize) -> windows::Win32::Foundation::HWND {
    windows::Win32::Foundation::HWND(h as *mut _)
}

#[link(name = "user32")]
extern "system" {
    fn GetDpiForWindow(hwnd: *mut core::ffi::c_void) -> u32;
    fn GetDpiForSystem() -> u32;
}

/// DPI of the monitor `hwnd` is on. A zero answer (invalid window) falls back
/// to the system DPI, loudly.
pub fn window_dpi(hwnd: isize) -> u32 {
    // SAFETY: plain user32 queries; an invalid handle yields 0.
    let d = unsafe { GetDpiForWindow(hwnd as *mut _) };
    if d != 0 {
        return d;
    }
    let sys = unsafe { GetDpiForSystem() };
    let used = if sys != 0 { sys } else { BASE_DPI };
    eprintln!("WARN lock_card: GetDpiForWindow returned 0 hwnd=0x{hwnd:x} fallback_dpi={used}");
    used
}

/// Caps Lock toggle state (low bit of GetKeyState(VK_CAPITAL)).
pub fn caps_lock_on() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CAPITAL};
    // SAFETY: reads the calling thread's keyboard state.
    (unsafe { GetKeyState(i32::from(VK_CAPITAL.0)) } & 1) != 0
}

/// Fonts of the card, built for one DPI. Keep it alive as long as the controls
/// that use it.
pub struct CardFonts {
    pub title: nwg::Font,
    pub subtitle: nwg::Font,
    pub status: nwg::Font,
    pub button: nwg::Font,
    pub input: nwg::Font,
}

impl CardFonts {
    pub fn build(dpi: u32) -> Result<CardFonts, nwg::NwgError> {
        // NWG font sizes are cell heights in pixels, like the 17 used for the
        // 9 pt global default font at 96 DPI.
        let make = |size: i32, weight: u32| -> Result<nwg::Font, nwg::NwgError> {
            let mut f = nwg::Font::default();
            nwg::Font::builder()
                .family("Segoe UI")
                .size(scale(size, dpi) as u32)
                .weight(weight)
                .build(&mut f)?;
            Ok(f)
        };
        Ok(CardFonts {
            title: make(30, 600)?,
            subtitle: make(17, 400)?,
            status: make(16, 400)?,
            button: make(17, 600)?,
            input: make(22, 400)?,
        })
    }
}

/// Remove the border and the sunken edge of a RichEdit (the editor).
///
/// Not for a plain EDIT: one created with WS_BORDER draws its frame itself,
/// inside its client area, and clearing the styles changes nothing (see
/// `build_password_input`).
pub fn flatten_edit(h: &nwg::ControlHandle) {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, GWL_STYLE, SWP_FRAMECHANGED, SWP_NOACTIVATE,
        SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    };
    const WS_BORDER: isize = 0x0080_0000;
    const WS_EX_CLIENTEDGE: isize = 0x0000_0200;
    let hwnd = to_hwnd(hwnd_of(h));
    // SAFETY: style edits on a window we own.
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
        SetWindowLongPtrW(hwnd, GWL_STYLE, style & !WS_BORDER);
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex & !WS_EX_CLIENTEDGE);
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

/// The password field of the card: a single-line edit that paints nothing but
/// its text (the card paints the rounded field around it).
///
/// It is created here, not by `nwg::TextInput::builder()`: nwg always adds
/// WS_BORDER, and an EDIT created with it keeps drawing a 1 px frame inside
/// its client area whatever is done to its styles afterwards. The returned
/// `nwg::TextInput` only wraps the handle (text, focus, font, events and
/// destruction work as for any nwg input); the text is centred vertically by
/// `CardControls::apply_layout`, not by nwg's non-client bands.
pub fn build_password_input(parent: &nwg::ControlHandle) -> nwg::TextInput {
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, WINDOW_EX_STYLE, WINDOW_STYLE, WS_CHILD, WS_TABSTOP, WS_VISIBLE,
    };
    const ES_PASSWORD: u32 = 0x0020;
    const ES_AUTOHSCROLL: u32 = 0x0080;
    // SAFETY: creates a child window of a window of this thread.
    let created = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            windows::core::w!("EDIT"),
            None,
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(ES_PASSWORD | ES_AUTOHSCROLL),
            0,
            0,
            100,
            30,
            Some(to_hwnd(hwnd_of(parent))),
            None,
            None,
            None,
        )
    };
    let hwnd = match created {
        Ok(h) => h,
        // No password field, no way to unlock: fail fast.
        Err(e) => panic!("lock_card: cannot create the password input: {e}"),
    };
    send(hwnd.0 as isize, EM_SETPASSWORDCHAR, PASSWORD_CHAR as usize, 0);
    let mut input = nwg::TextInput::default();
    input.handle = nwg::ControlHandle::Hwnd(hwnd.0 as _);
    input
}

/// Height in pixels of one line of text in the font `h` currently uses.
fn text_line_height(h: &nwg::ControlHandle) -> Option<i32> {
    use windows::Win32::Graphics::Gdi::{GetDC, GetTextMetricsW, ReleaseDC, SelectObject, HGDIOBJ, TEXTMETRICW};
    const WM_GETFONT: u32 = 0x0031;
    let hwnd = hwnd_of(h);
    let font = send(hwnd, WM_GETFONT, 0, 0);
    // SAFETY: DC of a window of this thread, released below.
    unsafe {
        let hdc = GetDC(Some(to_hwnd(hwnd)));
        if hdc.is_invalid() {
            return None;
        }
        // A null font means the system font, which is the one the DC holds.
        let old = (font != 0).then(|| SelectObject(hdc, HGDIOBJ(font as *mut _)));
        let mut tm = TEXTMETRICW::default();
        let ok = GetTextMetricsW(hdc, &mut tm).as_bool();
        if let Some(old) = old {
            SelectObject(hdc, old);
        }
        ReleaseDC(Some(to_hwnd(hwnd)), hdc);
        ok.then_some(tm.tmHeight)
    }
}

/// Where a one-line edit goes inside `slot` so its text is vertically
/// centred: a window exactly one line (`line_h`) tall, in the middle. A line
/// taller than the slot keeps the whole slot (the text is clipped, not moved
/// out of the field).
pub fn fit_line(slot: Rect, line_h: i32) -> Rect {
    if line_h <= 0 || line_h >= slot.h {
        return slot;
    }
    Rect::new(slot.x, slot.y + (slot.h - line_h) / 2, slot.w, line_h)
}

/// Turn a push button into an owner-drawn one (painted by `draw_button`) and
/// switch off its visual theme.
pub fn make_owner_draw(h: &nwg::ControlHandle) {
    use windows::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrW, SetWindowLongPtrW, GWL_STYLE};
    const BS_TYPEMASK: u32 = 0x0F;
    const BS_OWNERDRAW: u32 = 0x0B;
    let hwnd = to_hwnd(hwnd_of(h));
    // SAFETY: style edit on a window we own.
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        SetWindowLongPtrW(hwnd, GWL_STYLE, ((style & !BS_TYPEMASK) | BS_OWNERDRAW) as isize);
    }
    disable_theme(h);
}

pub fn disable_theme(h: &nwg::ControlHandle) {
    let empty = [0u16; 1];
    // SAFETY: SetWindowTheme with empty strings strips the visual style.
    unsafe {
        let _ = windows::Win32::UI::Controls::SetWindowTheme(
            to_hwnd(hwnd_of(h)),
            windows::core::PCWSTR(empty.as_ptr()),
            windows::core::PCWSTR(empty.as_ptr()),
        );
    }
}

/// WS_CLIPCHILDREN: the parent never paints over its controls.
pub fn add_clip_children(h: &nwg::ControlHandle) {
    use windows::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrW, SetWindowLongPtrW, GWL_STYLE};
    const WS_CLIPCHILDREN: u32 = 0x0200_0000;
    let hwnd = to_hwnd(hwnd_of(h));
    // SAFETY: style edit on a window we own.
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        SetWindowLongPtrW(hwnd, GWL_STYLE, (style | WS_CLIPCHILDREN) as isize);
    }
}

/// Move/resize a child control (client coordinates of its parent).
pub fn place(h: &nwg::ControlHandle, r: Rect) {
    use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER};
    // SAFETY: repositions a window we own.
    unsafe {
        let _ = SetWindowPos(to_hwnd(hwnd_of(h)), None, r.x, r.y, r.w, r.h, SWP_NOZORDER | SWP_NOACTIVATE);
    }
}

/// The text controls of the card and the fill of their non-client bands.
fn card_nc_targets<'a>(
    title: &'a nwg::ControlHandle,
    subtitle: &'a nwg::ControlHandle,
    status: &'a nwg::ControlHandle,
) -> [(&'a nwg::ControlHandle, NcFill); 3] {
    [
        (title, NcFill::CardSurface),
        (subtitle, NcFill::CardSurface),
        (status, NcFill::CardSurface),
    ]
}

/// Place and size every control of the card.
pub struct CardControls<'a> {
    pub title: &'a nwg::Label,
    pub subtitle: &'a nwg::Label,
    pub input: &'a nwg::TextInput,
    pub eye: &'a nwg::Button,
    pub status: &'a nwg::Label,
    pub primary: &'a nwg::Button,
    pub secondary: Option<&'a nwg::Button>,
}

impl CardControls<'_> {
    /// Every label of the card with the fill its non-client bands must take
    /// (nwg centres their text vertically with a band above and below that
    /// nobody repaints: left alone it stays white). The password field has
    /// no such bands (`build_password_input`).
    pub fn nc_band_targets(&self) -> [(&nwg::ControlHandle, NcFill); 3] {
        card_nc_targets(&self.title.handle, &self.subtitle.handle, &self.status.handle)
    }

    /// Make those bands follow the palette (idempotent).
    pub fn install_nc_bands(&self) {
        for (h, fill) in self.nc_band_targets() {
            install_nc_bands(h, fill);
        }
    }

    pub fn apply_layout(&self, l: &CardLayout) {
        self.install_nc_bands();
        place(&self.title.handle, l.title);
        place(&self.subtitle.handle, l.subtitle);
        // One line tall and centred in its slot (fonts are applied first).
        let line_h = text_line_height(&self.input.handle).unwrap_or_else(|| {
            eprintln!("WARN lock_card: text metrics of the password field are unreadable; text not centred vertically");
            0
        });
        place(&self.input.handle, fit_line(l.input, line_h));
        place(&self.eye.handle, l.eye);
        place(&self.status.handle, l.status);
        place(&self.primary.handle, l.primary);
        if let (Some(b), Some(r)) = (self.secondary, l.secondary) {
            place(&b.handle, r);
        }
    }

    pub fn apply_fonts(&self, f: &CardFonts) {
        self.title.set_font(Some(&f.title));
        self.subtitle.set_font(Some(&f.subtitle));
        self.input.set_font(Some(&f.input));
        self.status.set_font(Some(&f.status));
        self.primary.set_font(Some(&f.button));
        self.eye.set_font(Some(&f.button));
        if let Some(b) = self.secondary {
            b.set_font(Some(&f.button));
        }
    }
}

/// Place a popup window: client area `client_w` x `client_h`, centred on the
/// parent window (or on the work area of the primary monitor when there is no
/// usable parent), then kept inside the work area of that monitor.
pub fn place_popup(hwnd: isize, parent: Option<isize>, client_w: i32, client_h: i32) {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetClientRect, GetWindowRect, IsIconic, SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER,
    };
    let win = to_hwnd(hwnd);
    let rect_of = |r: RECT| Rect::new(r.left, r.top, r.right - r.left, r.bottom - r.top);
    // SAFETY: geometry queries and one SetWindowPos on windows we own / read.
    unsafe {
        let mut outer: RECT = std::mem::zeroed();
        let mut client: RECT = std::mem::zeroed();
        if GetWindowRect(win, &mut outer).is_err() || GetClientRect(win, &mut client).is_err() {
            eprintln!("WARN lock_card: place_popup cannot measure the popup hwnd=0x{hwnd:x}");
            return;
        }
        // Non-client size (title bar, frame) is whatever the window has now.
        let outer_w = (outer.right - outer.left) - (client.right - client.left) + client_w;
        let outer_h = (outer.bottom - outer.top) - (client.bottom - client.top) + client_h;

        let usable_parent = parent.map(to_hwnd).filter(|p| !IsIconic(*p).as_bool());
        let (anchor_rect, monitor) = match usable_parent {
            Some(p) => {
                let mut pr: RECT = std::mem::zeroed();
                let ok = GetWindowRect(p, &mut pr).is_ok();
                if ok {
                    (Some(rect_of(pr)), MonitorFromWindow(p, MONITOR_DEFAULTTONEAREST))
                } else {
                    (None, MonitorFromWindow(win, MONITOR_DEFAULTTOPRIMARY))
                }
            }
            None => (None, MonitorFromWindow(win, MONITOR_DEFAULTTOPRIMARY)),
        };
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if !GetMonitorInfoW(monitor, &mut mi).as_bool() {
            eprintln!("WARN lock_card: GetMonitorInfoW failed hwnd=0x{hwnd:x}; popup not repositioned");
            return;
        }
        let work = rect_of(mi.rcWork);
        let (x, y) = center_in(anchor_rect.unwrap_or(work), outer_w, outer_h);
        let placed = clamp_into(Rect::new(x, y, outer_w, outer_h), work);
        let _ = SetWindowPos(win, None, placed.x, placed.y, placed.w, placed.h, SWP_NOZORDER | SWP_NOACTIVATE);
    }
}

// ===========================================================================
// Win32: password reveal and status label
// ===========================================================================

/// Character shown instead of the typed ones.
pub const PASSWORD_CHAR: char = '\u{25CF}';

const EM_SETPASSWORDCHAR: u32 = 0x00CC;
const EM_GETPASSWORDCHAR: u32 = 0x00D2;

fn send(h: isize, msg: u32, w: usize, l: isize) -> isize {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    // SAFETY: SendMessageW to a window of this thread.
    unsafe { windows::Win32::UI::WindowsAndMessaging::SendMessageW(to_hwnd(h), msg, Some(WPARAM(w)), Some(LPARAM(l))).0 }
}

pub fn is_revealed(edit: isize) -> bool {
    send(edit, EM_GETPASSWORDCHAR, 0, 0) == 0
}

fn set_window_text(h: isize, text: &str) {
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: `wide` is NUL-terminated and outlives the call.
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowTextW(to_hwnd(h), windows::core::PCWSTR(wide.as_ptr()));
    }
}

fn invalidate(h: isize) {
    // SAFETY: invalidates a window of this thread.
    unsafe {
        let _ = windows::Win32::Graphics::Gdi::InvalidateRect(Some(to_hwnd(h)), None, true);
    }
}

/// Show (`true`) or hide the typed text, and give the eye button the matching
/// accessible name (`action_label` = what a click will do next).
pub fn set_revealed(edit: isize, eye: isize, revealed: bool, action_label: &str) {
    send(edit, EM_SETPASSWORDCHAR, if revealed { 0 } else { PASSWORD_CHAR as usize }, 0);
    set_window_text(eye, action_label);
    invalidate(edit);
    invalidate(eye);
}

const PROP_ROLE: windows::core::PCWSTR = windows::core::w!("LockNoteCardRole");
const PROP_TONE: windows::core::PCWSTR = windows::core::w!("LockNoteCardTone");

/// What a label is, so one WM_CTLCOLORSTATIC handler can colour it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Title = 1,
    Subtitle = 2,
    Status = 3,
}

pub fn set_role(h: &nwg::ControlHandle, role: Role) {
    use windows::Win32::Foundation::HANDLE;
    // SAFETY: stores a small integer as a window property.
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::SetPropW(
            to_hwnd(hwnd_of(h)),
            PROP_ROLE,
            Some(HANDLE(role as usize as *mut _)),
        );
    }
}

fn prop(h: isize, name: windows::core::PCWSTR) -> usize {
    // SAFETY: reads a window property.
    unsafe { windows::Win32::UI::WindowsAndMessaging::GetPropW(to_hwnd(h), name).0 as usize }
}

fn set_label_text_and_tone(label: isize, text: &str, tone: Tone) {
    use windows::Win32::Foundation::HANDLE;
    // SAFETY: stores a small integer as a window property.
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::SetPropW(
            to_hwnd(label),
            PROP_TONE,
            Some(HANDLE(tone as usize as *mut _)),
        );
    }
    set_window_text(label, text);
    invalidate(label);
    // Paint now: Argon2 blocks the UI thread right after "Decrypting...".
    // SAFETY: repaints a window of this thread.
    unsafe {
        let _ = windows::Win32::Graphics::Gdi::UpdateWindow(to_hwnd(label));
    }
}

/// The window handles the card's message handler needs.
#[derive(Clone, Copy, Debug, Default)]
pub struct CardHwnds {
    pub input: isize,
    pub eye: isize,
    pub primary: isize,
    pub secondary: isize,
}

/// Keep the caret in `field` whenever `window` gets the focus.
///
/// WM_ACTIVATE (window activated, from another app or at creation) moves the
/// focus to the field; but DefWindowProc then gives the focus to the window
/// itself (WM_SETFOCUS), which undoes it, so the field takes it again once
/// the window really has the focus. Shared by the unlock popup and every
/// password dialog. The handler must be unbound by the caller after the
/// dialog's message loop.
pub fn keep_focus_on(window: &nwg::ControlHandle, field: &nwg::ControlHandle) -> Result<nwg::RawEventHandler, nwg::NwgError> {
    use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
    const WM_ACTIVATE: u32 = 0x0006;
    const WM_SETFOCUS: u32 = 0x0007;
    let field_hwnd = hwnd_of(field);
    nwg::bind_raw_event_handler(window, FOCUS_HANDLER_ID, move |_hwnd, msg, wparam, _lparam| {
        // LOWORD(wparam) != 0: activated (WA_ACTIVE = 1 or WA_CLICKACTIVE = 2).
        if (msg == WM_ACTIVATE && (wparam & 0xFFFF) != 0) || msg == WM_SETFOCUS {
            // SAFETY: focus change within this thread's windows.
            unsafe {
                let _ = SetFocus(Some(to_hwnd(field_hwnd)));
            }
        }
        None
    })
}

/// Raw-handler id used by `keep_focus_on` (one per dialog window).
const FOCUS_HANDLER_ID: usize = 0x20001;

/// Raw hook on the password edit: tracks focus and Caps Lock for the status
/// line. The returned handler must be unbound by the caller (or leaked).
pub fn hook_password_input(
    input: &nwg::ControlHandle,
    handler_id: usize,
    status: StatusMsg,
) -> Result<nwg::RawEventHandler, nwg::NwgError> {
    const WM_SETFOCUS: u32 = 0x0007;
    const WM_KILLFOCUS: u32 = 0x0008;
    const WM_KEYUP: u32 = 0x0101;
    const VK_CAPITAL: usize = 0x14;
    nwg::bind_raw_event_handler(input, handler_id, move |_hwnd, msg, wparam, _lparam| {
        match msg {
            WM_SETFOCUS => status.set_field_focus(true),
            WM_KILLFOCUS => status.set_field_focus(false),
            WM_KEYUP if wparam == VK_CAPITAL => status.refresh_caps(),
            _ => {}
        }
        None
    })
}

// ===========================================================================
// Win32: painting
// ===========================================================================

fn colorref(c: Color) -> windows::Win32::Foundation::COLORREF {
    windows::Win32::Foundation::COLORREF(c.to_colorref())
}

thread_local! {
    /// Brushes handed back to WM_CTLCOLOR*: Windows does not free them, so a
    /// small cache (a handful of colours per theme) replaces one leak per message.
    static BRUSHES: RefCell<Vec<(u32, isize)>> = const { RefCell::new(Vec::new()) };
}

pub(crate) fn cached_brush(c: Color) -> isize {
    let key = c.to_colorref();
    BRUSHES.with(|b| {
        let mut b = b.borrow_mut();
        if let Some((_, h)) = b.iter().find(|(k, _)| *k == key) {
            return *h;
        }
        // SAFETY: the brush lives for the rest of the process (cached).
        let h = unsafe { windows::Win32::Graphics::Gdi::CreateSolidBrush(colorref(c)) }.0 as isize;
        b.push((key, h));
        h
    })
}

fn fill_rect(hdc: windows::Win32::Graphics::Gdi::HDC, r: &windows::Win32::Foundation::RECT, c: Color) {
    use windows::Win32::Graphics::Gdi::{CreateSolidBrush, DeleteObject, FillRect};
    // SAFETY: temporary brush, deleted right away.
    unsafe {
        let brush = CreateSolidBrush(colorref(c));
        FillRect(hdc, r, brush);
        let _ = DeleteObject(brush.into());
    }
}

/// Rounded rectangle with a fill (or none) and a border of `border_w` pixels.
fn round_rect(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    r: Rect,
    radius: i32,
    fill: Option<Color>,
    border: Color,
    border_w: i32,
) {
    use windows::Win32::Graphics::Gdi::{
        CreatePen, CreateSolidBrush, DeleteObject, GetStockObject, RoundRect, SelectObject, NULL_BRUSH, PS_SOLID,
    };
    // SAFETY: GDI objects created, selected, restored and deleted in order.
    unsafe {
        let pen = CreatePen(PS_SOLID, border_w.max(1), colorref(border));
        let (brush_obj, owned_brush) = match fill {
            Some(f) => {
                let b = CreateSolidBrush(colorref(f));
                (b.into(), Some(b))
            }
            None => (GetStockObject(NULL_BRUSH), None),
        };
        let old_pen = SelectObject(hdc, pen.into());
        let old_brush = SelectObject(hdc, brush_obj);
        let _ = RoundRect(hdc, r.x, r.y, r.right(), r.bottom(), radius * 2, radius * 2);
        SelectObject(hdc, old_pen);
        SelectObject(hdc, old_brush);
        let _ = DeleteObject(pen.into());
        if let Some(b) = owned_brush {
            let _ = DeleteObject(b.into());
        }
    }
}

fn ellipse(hdc: windows::Win32::Graphics::Gdi::HDC, r: Rect, fill: Option<Color>, border: Color, border_w: i32) {
    use windows::Win32::Graphics::Gdi::{
        CreatePen, CreateSolidBrush, DeleteObject, Ellipse, GetStockObject, SelectObject, NULL_BRUSH, PS_SOLID,
    };
    // SAFETY: GDI objects created, selected, restored and deleted in order.
    unsafe {
        let pen = CreatePen(PS_SOLID, border_w.max(1), colorref(border));
        let (brush_obj, owned_brush) = match fill {
            Some(f) => {
                let b = CreateSolidBrush(colorref(f));
                (b.into(), Some(b))
            }
            None => (GetStockObject(NULL_BRUSH), None),
        };
        let old_pen = SelectObject(hdc, pen.into());
        let old_brush = SelectObject(hdc, brush_obj);
        let _ = Ellipse(hdc, r.x, r.y, r.right(), r.bottom());
        SelectObject(hdc, old_pen);
        SelectObject(hdc, old_brush);
        let _ = DeleteObject(pen.into());
        if let Some(b) = owned_brush {
            let _ = DeleteObject(b.into());
        }
    }
}

fn line(hdc: windows::Win32::Graphics::Gdi::HDC, a: (i32, i32), b: (i32, i32), color: Color, w: i32) {
    use windows::Win32::Graphics::Gdi::{CreatePen, DeleteObject, LineTo, MoveToEx, SelectObject, PS_SOLID};
    // SAFETY: pen created, selected, restored and deleted in order.
    unsafe {
        let pen = CreatePen(PS_SOLID, w.max(1), colorref(color));
        let old = SelectObject(hdc, pen.into());
        let _ = MoveToEx(hdc, a.0, a.1, None);
        let _ = LineTo(hdc, b.0, b.1);
        SelectObject(hdc, old);
        let _ = DeleteObject(pen.into());
    }
}

/// A padlock drawn with GDI primitives (no dependency on an icon font).
fn draw_padlock(hdc: windows::Win32::Graphics::Gdi::HDC, r: Rect, c: &CardColors) {
    let s = r.w.min(r.h);
    let pen_w = (s / 9).max(2);
    // Shackle: an outlined rounded box whose lower half the body covers.
    let shackle = Rect::new(r.x + s * 26 / 100, r.y + pen_w / 2, s * 48 / 100, s * 62 / 100);
    round_rect(hdc, shackle, s * 12 / 100, None, c.icon, pen_w);
    // Body.
    let body = Rect::new(r.x + s * 12 / 100, r.y + s * 42 / 100, s * 76 / 100, s * 58 / 100 - 1);
    round_rect(hdc, body, s * 10 / 100, Some(c.icon), c.icon, 1);
    // Keyhole.
    let kd = (s * 14 / 100).max(3);
    let kx = body.x + (body.w - kd) / 2;
    let ky = body.y + body.h * 30 / 100;
    ellipse(hdc, Rect::new(kx, ky, kd, kd), Some(c.card_bg), c.card_bg, 1);
    line(
        hdc,
        (kx + kd / 2, ky + kd),
        (kx + kd / 2, ky + kd + body.h * 24 / 100),
        c.card_bg,
        (kd / 3).max(2),
    );
}

/// Paint the whole background of the card window: backdrop (lock screen),
/// card, padlock and the password field box.
fn paint_card(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    client: windows::Win32::Foundation::RECT,
    layout: &CardLayout,
    c: &CardColors,
    mode: CardMode,
    field_focused: bool,
    dpi: u32,
) {
    match mode {
        CardMode::Popup => fill_rect(hdc, &client, c.card_bg),
        CardMode::Overlay => {
            fill_rect(hdc, &client, c.backdrop);
            round_rect(hdc, layout.card, scale(14, dpi), Some(c.card_bg), c.card_border, scale(1, dpi));
        }
    }
    draw_padlock(hdc, layout.icon, c);
    let (border, w) = if field_focused {
        (c.field_border_focus, scale(2, dpi))
    } else {
        (c.field_border, scale(1, dpi))
    };
    round_rect(hdc, layout.field, scale(10, dpi), Some(c.field_bg), border, w);
}

/// Which button an owner-drawn control is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ButtonKind {
    Primary,
    Secondary,
    Eye,
}

fn draw_button(dis: &windows::Win32::UI::Controls::DRAWITEMSTRUCT, kind: ButtonKind, c: &CardColors, dpi: u32) {
    use windows::Win32::Graphics::Gdi::{
        DrawTextW, SelectObject, SetBkMode, SetTextColor, DT_CENTER, DT_SINGLELINE, DT_VCENTER, TRANSPARENT,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetWindowTextW;
    const ODS_SELECTED: u32 = 0x0001;
    const ODS_DISABLED: u32 = 0x0004;
    const ODS_FOCUS: u32 = 0x0010;
    let state = dis.itemState.0;
    let disabled = state & ODS_DISABLED != 0;
    let pressed = state & ODS_SELECTED != 0;
    let focused = state & ODS_FOCUS != 0;
    let hdc = dis.hDC;
    let rc = dis.rcItem;
    let item = Rect::new(rc.left, rc.top, rc.right - rc.left, rc.bottom - rc.top);
    let radius = scale(8, dpi);
    let ring = scale(2, dpi);

    let field_bg = c.field_bg;
    let (text_color, label): (Color, bool) = match kind {
        ButtonKind::Primary => {
            fill_rect(hdc, &rc, c.card_bg);
            let (bg, fg) = if disabled {
                (c.primary_bg_disabled, c.primary_text_disabled)
            } else if pressed {
                (c.primary_bg_pressed, c.primary_text)
            } else {
                (c.primary_bg, c.primary_text)
            };
            round_rect(hdc, item, radius, Some(bg), bg, 1);
            (fg, true)
        }
        ButtonKind::Secondary => {
            fill_rect(hdc, &rc, c.card_bg);
            let bg = if pressed { c.secondary_bg_pressed } else { c.secondary_bg };
            let fg = if disabled { c.text_secondary } else { c.secondary_text };
            round_rect(hdc, item, radius, Some(bg), c.secondary_border, scale(1, dpi));
            (fg, true)
        }
        ButtonKind::Eye => {
            fill_rect(hdc, &rc, field_bg);
            if pressed {
                round_rect(hdc, item, radius, Some(c.secondary_bg_pressed), c.secondary_bg_pressed, 1);
            }
            (if disabled { c.text_secondary } else { c.text_primary }, false)
        }
    };

    if focused && !disabled {
        round_rect(hdc, item.inflate(-ring / 2), radius, None, c.field_border_focus, ring);
    }

    if label {
        let mut buf = [0u16; 128];
        // SAFETY: reads the button caption into a local buffer; the control's
        // own font is selected so the caption uses the DPI-scaled font.
        unsafe {
            let len = GetWindowTextW(dis.hwndItem, &mut buf).max(0) as usize;
            if len > 0 {
                let ctrl_font = windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                    dis.hwndItem,
                    0x0031, // WM_GETFONT
                    None,
                    None,
                )
                .0;
                let old = if ctrl_font != 0 {
                    Some(SelectObject(hdc, windows::Win32::Graphics::Gdi::HGDIOBJ(ctrl_font as *mut _)))
                } else {
                    None
                };
                SetBkMode(hdc, TRANSPARENT);
                SetTextColor(hdc, colorref(text_color));
                let mut text: Vec<u16> = buf[..len].to_vec();
                let mut r = rc;
                DrawTextW(hdc, text.as_mut_slice(), &mut r, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
                if let Some(o) = old {
                    SelectObject(hdc, o);
                }
            }
        }
    }
}

/// Paint the eye glyph: an open eye while the text is hidden, the same eye
/// struck through while it is shown (click = hide).
fn draw_eye(dis: &windows::Win32::UI::Controls::DRAWITEMSTRUCT, revealed: bool, c: &CardColors, dpi: u32) {
    let rc = dis.rcItem;
    let (w, h) = (rc.right - rc.left, rc.bottom - rc.top);
    let cx = rc.left + w / 2;
    let cy = rc.top + h / 2;
    let e = w.min(h) / 2; // half of the glyph's width
    let pen = scale(2, dpi);
    let color = if dis.itemState.0 & 0x0004 != 0 { c.text_secondary } else { c.text_primary };
    let outline = Rect::new(cx - e * 85 / 100, cy - e * 45 / 100, e * 170 / 100, e * 90 / 100);
    ellipse(dis.hDC, outline, None, color, pen);
    let pr = (e * 24 / 100).max(2);
    ellipse(dis.hDC, Rect::new(cx - pr, cy - pr, 2 * pr, 2 * pr), Some(color), color, 1);
    if revealed {
        // Slash with a card-coloured halo so it reads over the outline.
        line(dis.hDC, (cx - e * 80 / 100, cy + e * 62 / 100), (cx + e * 80 / 100, cy - e * 62 / 100), c.field_bg, pen + scale(3, dpi));
        line(dis.hDC, (cx - e * 80 / 100, cy + e * 62 / 100), (cx + e * 80 / 100, cy - e * 62 / 100), color, pen);
    }
}

/// Handle the messages that give the card its look and behaviour; shared by
/// the popup (raw handler) and the lock screen (window procedure). `None`
/// means "not handled, continue with the default processing".
pub fn handle_message(
    hwnd: isize,
    msg: u32,
    wparam: usize,
    lparam: isize,
    mode: CardMode,
    h: &CardHwnds,
) -> Option<isize> {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, SetBkColor, SetBkMode, SetTextColor, HDC, PAINTSTRUCT, TRANSPARENT};
    use windows::Win32::UI::Controls::DRAWITEMSTRUCT;
    use windows::Win32::UI::Input::KeyboardAndMouse::GetFocus;
    use windows::Win32::UI::WindowsAndMessaging::GetClientRect;

    const WM_ERASEBKGND: u32 = 0x0014;
    const WM_PAINT: u32 = 0x000F;
    const WM_CTLCOLOREDIT: u32 = 0x0133;
    const WM_CTLCOLORBTN: u32 = 0x0135;
    const WM_CTLCOLORSTATIC: u32 = 0x0138;
    const WM_DRAWITEM: u32 = 0x002B;
    const WM_COMMAND: u32 = 0x0111;

    let colors = CardColors::for_mode(theme::current_mode());
    let win = to_hwnd(hwnd);

    let paint = |hdc: HDC| {
        let mut client: RECT = unsafe { std::mem::zeroed() };
        // SAFETY: client rect of the window being painted.
        if unsafe { GetClientRect(win, &mut client) }.is_err() {
            eprintln!("WARN lock_card: GetClientRect failed during paint hwnd=0x{hwnd:x}");
            return;
        }
        let dpi = window_dpi(hwnd);
        let layout = CardLayout::compute(mode, client.right - client.left, client.bottom - client.top, dpi);
        // SAFETY: GetFocus has no preconditions.
        let focused = h.input != 0 && unsafe { GetFocus() }.0 as isize == h.input;
        paint_card(hdc, client, &layout, &colors, mode, focused, dpi);
    };

    match msg {
        WM_ERASEBKGND => {
            paint(HDC(wparam as *mut _));
            Some(1)
        }
        WM_PAINT => {
            // SAFETY: standard BeginPaint/EndPaint pair.
            unsafe {
                let mut ps: PAINTSTRUCT = std::mem::zeroed();
                let hdc = BeginPaint(win, &mut ps);
                paint(hdc);
                let _ = EndPaint(win, &ps);
            }
            Some(0)
        }
        WM_CTLCOLORSTATIC => {
            let ctl = lparam;
            let text = match prop(ctl, PROP_ROLE) {
                r if r == Role::Title as usize => colors.text_primary,
                r if r == Role::Subtitle as usize => colors.text_secondary,
                r if r == Role::Status as usize => match prop(ctl, PROP_TONE) {
                    t if t == Tone::Error as usize => colors.error,
                    t if t == Tone::Warning as usize => colors.warning,
                    _ => colors.text_primary,
                },
                _ => colors.text_primary,
            };
            let hdc = HDC(wparam as *mut _);
            // SAFETY: the DC belongs to the message being processed.
            unsafe {
                SetTextColor(hdc, colorref(text));
                SetBkColor(hdc, colorref(colors.card_bg));
                SetBkMode(hdc, TRANSPARENT);
            }
            Some(cached_brush(colors.card_bg))
        }
        WM_CTLCOLOREDIT => {
            let hdc = HDC(wparam as *mut _);
            // SAFETY: the DC belongs to the message being processed.
            unsafe {
                SetTextColor(hdc, colorref(colors.text_primary));
                SetBkColor(hdc, colorref(colors.field_bg));
            }
            Some(cached_brush(colors.field_bg))
        }
        WM_CTLCOLORBTN => Some(cached_brush(colors.card_bg)),
        WM_DRAWITEM if lparam != 0 => {
            // SAFETY: for WM_DRAWITEM lparam points to a DRAWITEMSTRUCT.
            let dis = unsafe { &*(lparam as *const DRAWITEMSTRUCT) };
            let item = dis.hwndItem.0 as isize;
            let dpi = window_dpi(hwnd);
            let kind = if item == h.primary {
                ButtonKind::Primary
            } else if h.secondary != 0 && item == h.secondary {
                ButtonKind::Secondary
            } else if item == h.eye {
                ButtonKind::Eye
            } else {
                return None;
            };
            draw_button(dis, kind, &colors, dpi);
            if kind == ButtonKind::Eye {
                draw_eye(dis, is_revealed(h.input), &colors, dpi);
            }
            Some(1)
        }
        WM_COMMAND => {
            const EN_SETFOCUS: usize = 0x0100;
            const EN_KILLFOCUS: usize = 0x0200;
            const BN_CLICKED: usize = 0;
            let code = (wparam >> 16) & 0xFFFF;
            let src = lparam;
            if src != 0 && src == h.input && (code == EN_SETFOCUS || code == EN_KILLFOCUS) {
                // Redraw the field box (border thickens and turns accent on focus).
                let mut client: RECT = unsafe { std::mem::zeroed() };
                // SAFETY: client rect, then invalidation of the field box.
                unsafe {
                    if GetClientRect(win, &mut client).is_ok() {
                        let dpi = window_dpi(hwnd);
                        let l = CardLayout::compute(mode, client.right - client.left, client.bottom - client.top, dpi);
                        let f = l.field.inflate(scale(3, dpi));
                        let r = RECT { left: f.x, top: f.y, right: f.right(), bottom: f.bottom() };
                        let _ = windows::Win32::Graphics::Gdi::InvalidateRect(Some(win), Some(&r), true);
                    }
                }
            } else if src != 0 && src == h.eye && code == BN_CLICKED {
                toggle_reveal(h);
            }
            None
        }
        _ => None,
    }
}

thread_local! {
    /// Accessible names of the eye button: (show, hide). Set by the owner.
    static EYE_NAMES: RefCell<(String, String)> = const { RefCell::new((String::new(), String::new())) };
}

/// Names announced for the eye button when the text is hidden / shown.
pub fn set_eye_names(show: &str, hide: &str) {
    EYE_NAMES.with(|n| *n.borrow_mut() = (show.to_string(), hide.to_string()));
}

fn toggle_reveal(h: &CardHwnds) {
    use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
    let now_revealed = !is_revealed(h.input);
    let name = EYE_NAMES.with(|n| {
        let n = n.borrow();
        if now_revealed { n.1.clone() } else { n.0.clone() }
    });
    set_revealed(h.input, h.eye, now_revealed, &name);
    // Keep typing where the user was.
    // SAFETY: focus change within this thread's windows.
    unsafe {
        let _ = SetFocus(Some(to_hwnd(h.input)));
    }
}

/// Hide the typed text again (after a lock, a successful unlock, ...).
pub fn hide_password(h: &CardHwnds) {
    let name = EYE_NAMES.with(|n| n.borrow().0.clone());
    set_revealed(h.input, h.eye, false, &name);
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    impl Rect {
        fn contains(&self, o: &Rect) -> bool {
            o.x >= self.x && o.y >= self.y && o.right() <= self.right() && o.bottom() <= self.bottom()
        }
    }

    const DPIS: [u32; 6] = [96, 120, 144, 168, 192, 288];

    #[test]
    fn scale_is_identity_at_96_and_proportional_above() {
        assert_eq!(scale(44, 96), 44);
        assert_eq!(scale(44, 144), 66);
        assert_eq!(scale(44, 192), 88);
        assert_eq!(scale(1, 144), 2); // 1.5 rounds up: hairlines never vanish
        assert_eq!(scale(0, 288), 0);
    }

    #[test]
    fn popup_layout_fits_the_client_area_at_every_dpi() {
        for dpi in DPIS {
            let (w, h) = CardLayout::popup_client_size(dpi);
            let l = CardLayout::compute(CardMode::Popup, w, h, dpi);
            let client = Rect::new(0, 0, w, h);
            assert_eq!(l.card, client, "dpi {dpi}");
            for (name, r) in elements(&l) {
                assert!(client.contains(&r), "{name} {r:?} outside {client:?} at dpi {dpi}");
            }
        }
    }

    fn elements(l: &CardLayout) -> Vec<(&'static str, Rect)> {
        let mut v = vec![
            ("icon", l.icon),
            ("title", l.title),
            ("subtitle", l.subtitle),
            ("field", l.field),
            ("input", l.input),
            ("eye", l.eye),
            ("status", l.status),
            ("primary", l.primary),
        ];
        if let Some(s) = l.secondary {
            v.push(("secondary", s));
        }
        v
    }

    #[test]
    fn rows_are_stacked_without_overlap_and_the_field_holds_input_and_eye() {
        for dpi in DPIS {
            for mode in [CardMode::Popup, CardMode::Overlay] {
                let (w, h) = CardLayout::popup_client_size(dpi);
                let l = CardLayout::compute(mode, w + 200, h + 200, dpi);
                let rows = [l.icon, l.title, l.subtitle, l.field, l.status, l.primary];
                for pair in rows.windows(2) {
                    assert!(pair[0].bottom() <= pair[1].y, "{pair:?} overlap at dpi {dpi}");
                }
                assert!(l.field.contains(&l.input) && l.field.contains(&l.eye));
                assert!(l.input.right() <= l.eye.x, "input runs under the eye at dpi {dpi}");
                assert!(l.card.contains(&l.primary));
                assert!(l.card.bottom() > l.primary.bottom(), "no bottom padding");
            }
        }
    }

    #[test]
    fn popup_has_two_buttons_side_by_side_and_overlay_only_a_wide_primary() {
        let dpi = 144;
        let (w, h) = CardLayout::popup_client_size(dpi);
        let p = CardLayout::compute(CardMode::Popup, w, h, dpi);
        let s = p.secondary.expect("popup has a secondary button");
        assert!(s.right() < p.primary.x, "buttons must not touch");
        assert_eq!(s.y, p.primary.y);
        assert_eq!(p.primary.right(), p.field.right(), "primary is right-aligned with the field");
        let o = CardLayout::compute(CardMode::Overlay, 1280, 800, dpi);
        assert!(o.secondary.is_none());
        assert_eq!(o.primary.w, o.field.w);
    }

    #[test]
    fn layout_grows_with_dpi() {
        let (w1, h1) = CardLayout::popup_client_size(96);
        let (w2, h2) = CardLayout::popup_client_size(192);
        assert!((w2 - 2 * w1).abs() <= 1);
        assert!((h2 - 2 * h1).abs() <= 12, "12 rounding steps at most: {h1} -> {h2}");
        // Eye and field are the touch targets: at least 32 px at 100 %.
        let l = CardLayout::compute(CardMode::Popup, w1, h1, 96);
        assert!(l.eye.w >= 32 && l.primary.h >= 40 && l.field.h >= 48);
    }

    #[test]
    fn overlay_card_is_centred_and_narrows_on_small_windows() {
        let dpi = 96;
        let l = CardLayout::compute(CardMode::Overlay, 1000, 700, dpi);
        assert_eq!(l.card.w, 440);
        assert_eq!(l.card.x, (1000 - 440) / 2);
        assert_eq!(l.card.y, (700 - l.card.h) / 2);
        let small = CardLayout::compute(CardMode::Overlay, 360, 300, dpi);
        assert_eq!(small.card.w, 360 - 32);
        assert!(small.card.x >= 0 && small.card.y == 0, "never above the window");
        let tiny = CardLayout::compute(CardMode::Overlay, 100, 100, dpi);
        assert_eq!(tiny.card.w, 280, "minimum width");
        // Same card whatever the DPI, relative to the scale.
        let big = CardLayout::compute(CardMode::Overlay, 3000, 2000, 192);
        assert_eq!(big.card.w, 880);
    }

    #[test]
    fn center_in_and_clamp_into() {
        let parent = Rect::new(100, 50, 800, 600);
        assert_eq!(center_in(parent, 400, 300), (300, 200));
        let work = Rect::new(0, 0, 1920, 1040);
        assert_eq!(clamp_into(Rect::new(300, 200, 400, 300), work), Rect::new(300, 200, 400, 300));
        assert_eq!(clamp_into(Rect::new(-50, -20, 400, 300), work), Rect::new(0, 0, 400, 300));
        assert_eq!(clamp_into(Rect::new(1800, 900, 400, 300), work), Rect::new(1520, 740, 400, 300));
        // Larger than the work area: the title bar (top-left) stays reachable.
        assert_eq!(clamp_into(Rect::new(10, 10, 3000, 2000), work), Rect::new(0, 0, 3000, 2000));
    }

    #[test]
    fn contrast_ratio_matches_wcag_reference_values() {
        assert!((contrast_ratio(BLACK, WHITE) - 21.0).abs() < 1e-9);
        assert!((contrast_ratio(WHITE, WHITE) - 1.0).abs() < 1e-9);
        // #767676 on white is the textbook 4.54:1 boundary of AA.
        let r = contrast_ratio(Color::new(0x76, 0x76, 0x76), WHITE);
        assert!((r - 4.54).abs() < 0.01, "got {r}");
    }

    #[test]
    fn ensure_contrast_leaves_good_colours_and_fixes_bad_ones() {
        let bg = Color::new(44, 47, 55);
        let ok = Color::new(230, 230, 230);
        assert_eq!(ensure_contrast(ok, bg, 4.5), ok);
        let weak = Color::new(62, 66, 76);
        let fixed = ensure_contrast(weak, bg, 3.0);
        assert!(contrast_ratio(fixed, bg) >= 3.0);
        // On a light background it darkens instead.
        let light_fixed = ensure_contrast(Color::new(230, 230, 230), WHITE, 3.0);
        assert!(contrast_ratio(light_fixed, WHITE) >= 3.0);
        assert!(light_fixed.r < 230);
    }

    #[test]
    fn best_text_picks_the_readable_side() {
        assert_eq!(best_text_on(&[Color::new(95, 160, 230)]), DARK_TEXT);
        assert_eq!(best_text_on(&[Color::new(0, 90, 180)]), WHITE);
    }

    fn both() -> [(ThemeMode, CardColors); 2] {
        [ThemeMode::Dark, ThemeMode::Light].map(|m| (m, CardColors::for_mode(m)))
    }

    #[test]
    fn every_text_reaches_wcag_aa_on_its_background_in_both_palettes() {
        for (mode, c) in both() {
            let checks = [
                ("title", c.text_primary, c.card_bg),
                ("subtitle", c.text_secondary, c.card_bg),
                ("error", c.error, c.card_bg),
                ("caps warning", c.warning, c.card_bg),
                ("input text", c.text_primary, c.field_bg),
                ("primary", c.primary_text, c.primary_bg),
                ("primary pressed", c.primary_text, c.primary_bg_pressed),
                ("secondary", c.secondary_text, c.secondary_bg),
                ("secondary pressed", c.secondary_text, c.secondary_bg_pressed),
            ];
            for (name, fg, bg) in checks {
                let r = contrast_ratio(fg, bg);
                assert!(r >= 4.5, "{name} in {mode:?}: {r:.2}:1 < 4.5:1");
            }
        }
    }

    #[test]
    fn primary_button_text_is_far_above_the_old_2_7_to_1() {
        // The old white-on-accent text of the dark theme.
        let dark = theme::get_palette(ThemeMode::Dark);
        assert!(contrast_ratio(WHITE, dark.accent) < 3.0, "premise: the old pairing was ~2.7:1");
        let c = CardColors::for_mode(ThemeMode::Dark);
        assert!(contrast_ratio(c.primary_text, c.primary_bg) >= 6.0);
    }

    #[test]
    fn graphical_objects_reach_3_to_1_in_both_palettes() {
        for (mode, c) in both() {
            let checks = [
                ("field border / card", c.field_border, c.card_bg),
                ("field border / field", c.field_border, c.field_bg),
                ("focus ring / card", c.field_border_focus, c.card_bg),
                ("padlock / card", c.icon, c.card_bg),
                ("secondary border / card", c.secondary_border, c.secondary_bg),
                ("disabled primary text", c.primary_text_disabled, c.primary_bg_disabled),
            ];
            for (name, fg, bg) in checks {
                let r = contrast_ratio(fg, bg);
                assert!(r >= 3.0, "{name} in {mode:?}: {r:.2}:1 < 3:1");
            }
        }
    }

    #[test]
    fn card_stands_out_from_the_backdrop_and_from_the_field() {
        for (mode, c) in both() {
            assert_ne!(c.backdrop, c.card_bg, "{mode:?}");
            assert_ne!(c.card_bg, c.field_bg, "{mode:?}: the field is an inset, not the same fill as the card");
        }
    }

    #[test]
    fn the_backdrop_is_the_chrome_of_each_theme() {
        for (mode, c) in both() {
            assert_eq!(c.backdrop, theme::get_palette(mode).chrome, "{mode:?}");
        }
    }

    #[test]
    fn caps_hint_needs_caps_lock_and_the_focus() {
        assert!(caps_hint_visible(true, true));
        assert!(!caps_hint_visible(true, false));
        assert!(!caps_hint_visible(false, true));
        assert!(!caps_hint_visible(false, false));
    }

    #[test]
    fn status_line_prefers_the_message_then_the_hint_then_nothing() {
        let caps = "Caps Lock is on";
        assert_eq!(status_line(Tone::Error, "Wrong password (1/5)", true, caps), (Tone::Error, "Wrong password (1/5)".into()));
        assert_eq!(status_line(Tone::Neutral, "", true, caps), (Tone::Warning, caps.into()));
        assert_eq!(status_line(Tone::Error, "", false, caps), (Tone::Neutral, String::new()));
    }

    #[test]
    fn tones_and_roles_have_distinct_nonzero_codes() {
        // They are stored as window properties where 0 means "absent".
        let codes = [Tone::Neutral as usize, Tone::Error as usize, Tone::Warning as usize];
        assert_eq!(codes, [0, 1, 2]);
        assert_eq!([Role::Title as usize, Role::Subtitle as usize, Role::Status as usize], [1, 2, 3]);
    }

    #[test]
    fn nc_bands_take_the_card_colours_not_the_default_white() {
        use crate::ui::dialogs::style::nc_colors;
        for (mode, c) in both() {
            assert_eq!(nc_colors(NcFill::CardSurface, mode), (c.card_bg, None), "{mode:?}");
        }
        // The dark card is not white: a band left to nwg (COLOR_WINDOW) would show.
        assert_ne!(nc_colors(NcFill::CardSurface, ThemeMode::Dark).0, WHITE);
    }

    /// Hidden top-level window of `class`, standing for an nwg control.
    fn raw_control(class: windows::core::PCWSTR) -> nwg::ControlHandle {
        use windows::Win32::UI::WindowsAndMessaging::{CreateWindowExW, WINDOW_EX_STYLE, WINDOW_STYLE};
        // SAFETY: creates a hidden window owned by the test thread.
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class,
                windows::core::w!(""),
                WINDOW_STYLE(0),
                0,
                0,
                100,
                30,
                None,
                None,
                None,
                None,
            )
        }
        .expect("create test window");
        nwg::ControlHandle::Hwnd(hwnd.0 as _)
    }

    #[test]
    fn password_input_paints_no_frame() {
        // An EDIT created with WS_BORDER keeps drawing a 1 px frame inside
        // its client area whatever is done to its styles afterwards: the
        // field must be born without it.
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        use windows::Win32::Graphics::Gdi::{
            CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetPixel, ReleaseDC, SelectObject,
        };
        use windows::Win32::UI::WindowsAndMessaging::SendMessageW;
        let _ = nwg::init();
        let mut window = nwg::Window::default();
        nwg::Window::builder()
            .size((400, 300))
            .flags(nwg::WindowFlags::WINDOW)
            .build(&mut window)
            .expect("window");
        let input = build_password_input(&window.handle);
        let (w, h) = (200, 30);
        place(&input.handle, Rect::new(20, 20, w, h));
        // SAFETY: paints the control into a memory DC of this thread.
        unsafe {
            let screen = GetDC(None);
            let dc = CreateCompatibleDC(Some(screen));
            let bmp = CreateCompatibleBitmap(screen, w, h);
            let old = SelectObject(dc, bmp.into());
            // PRF_NONCLIENT | PRF_CLIENT | PRF_ERASEBKGND
            SendMessageW(to_hwnd(hwnd_of(&input.handle)), 0x0317, Some(WPARAM(dc.0 as usize)), Some(LPARAM(0x2 | 0x4 | 0x8)));
            let inside = GetPixel(dc, w / 2, h / 2).0;
            for (x, y) in [(0, h / 2), (w - 1, h / 2), (w / 2, 0), (w / 2, h - 1), (w / 2, 3), (w / 2, h - 4)] {
                assert_eq!(GetPixel(dc, x, y).0, inside, "pixel ({x},{y}) differs from the field fill");
            }
            SelectObject(dc, old);
            let _ = DeleteObject(bmp.into());
            let _ = DeleteDC(dc);
            ReleaseDC(None, screen);
        }
    }

    #[test]
    fn password_input_is_a_masked_one_line_edit_wrapped_for_nwg() {
        let _ = nwg::init();
        let mut window = nwg::Window::default();
        nwg::Window::builder()
            .size((400, 300))
            .flags(nwg::WindowFlags::WINDOW)
            .build(&mut window)
            .expect("window");
        let input = build_password_input(&window.handle);
        let hwnd = hwnd_of(&input.handle);
        assert!(!is_revealed(hwnd), "typed text is masked from the start");
        assert_eq!(send(hwnd, EM_GETPASSWORDCHAR, 0, 0) as u32, PASSWORD_CHAR as u32);
        // The nwg wrapper reads and writes the same window.
        input.set_text("s3cret");
        assert_eq!(input.text(), "s3cret");
        input.set_text("");
        assert_eq!(input.text(), "");
        assert!(text_line_height(&input.handle).is_some_and(|h| h > 0));
    }

    #[test]
    fn one_line_edit_is_centred_in_its_slot() {
        let slot = Rect::new(40, 100, 200, 30);
        assert_eq!(fit_line(slot, 20), Rect::new(40, 105, 200, 20));
        assert_eq!(fit_line(slot, 21), Rect::new(40, 104, 200, 21), "odd leftover: the extra pixel goes below");
        // Never taller than the slot, never moved out of it.
        assert_eq!(fit_line(slot, 30), slot);
        assert_eq!(fit_line(slot, 45), slot);
        // Unreadable metrics (0): the slot is kept as is.
        assert_eq!(fit_line(slot, 0), slot);
        assert_eq!(fit_line(slot, -3), slot);
    }

    #[test]
    fn every_text_control_of_the_card_gets_its_band_painter() {
        use crate::ui::dialogs::style::has_nc_bands;
        use windows::core::w;
        let (title, subtitle, status) = (raw_control(w!("STATIC")), raw_control(w!("STATIC")), raw_control(w!("STATIC")));
        let targets = card_nc_targets(&title, &subtitle, &status);
        assert_eq!(targets.len(), 3, "title, subtitle and status (the password field has no bands)");
        for (h, _) in &targets {
            assert_eq!(has_nc_bands(h), None, "nothing is registered before the card installs it");
        }
        for (h, fill) in &targets {
            assert!(install_nc_bands(h, *fill));
        }
        for (h, fill) in &targets {
            assert_eq!(has_nc_bands(h), Some(*fill));
        }
        // Idempotent: the layout runs again on every resize of the lock screen.
        for (h, fill) in &targets {
            assert!(install_nc_bands(h, *fill));
            assert_eq!(has_nc_bands(h), Some(*fill));
        }
        for h in [title, subtitle, status] {
            // SAFETY: destroys the windows created above.
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(windows::Win32::Foundation::HWND(
                    hwnd_of(&h) as *mut _,
                ));
            }
        }
    }
}
