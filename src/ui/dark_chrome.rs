// Window chrome: title bar, menu bar and popup menus, for both themes.
//
// Win32 has no documented way to theme the title bar and the classic menu
// bar, so this module uses three undocumented/semi-documented mechanisms:
//   - DwmSetWindowAttribute(DWMWA_USE_IMMERSIVE_DARK_MODE) for the title bar
//     (attribute 20; 19 on early Windows 10 builds); in the Light theme also
//     DWMWA_CAPTION_COLOR / TEXT_COLOR / BORDER_COLOR (34-36, Windows 11),
//     given back to the system (DWMWA_COLOR_DEFAULT) when going Dark,
//   - the WM_UAHDRAWMENU / WM_UAHDRAWMENUITEM messages to owner-paint the
//     menu bar and its items, plus a repaint of the 1 px line under the bar,
//   - uxtheme.dll ordinals 135 (SetPreferredAppMode) and 136
//     (FlushMenuThemes) for dark popup menus (Default mode again in Light).
//
// The theme can change while the window is open (`apply_title_bar`,
// `apply_popup_menu_theme` and `redraw_menu_bar` re-apply it). When an OS
// feature is unavailable the native look is kept and a single
// `WARN dark_chrome: ...` line is logged to stderr.
//
// The decision logic (colors per state, struct layouts, ordinal lookup) is
// pure and unit-tested; the Win32 calls are thin wrappers around it.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::core::{PCSTR, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, POINT, RECT};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWINDOWATTRIBUTE};
use windows::Win32::Graphics::Gdi::{
    CreateRoundRectRgn, CreateSolidBrush, DeleteObject, DrawTextW, FillRect, FillRgn, GetWindowDC,
    MapWindowPoints, RedrawWindow, ReleaseDC, SelectObject, SetBkMode, SetTextColor, DT_CENTER,
    DT_HIDEPREFIX, DT_SINGLELINE, DT_VCENTER, HDC, HGDIOBJ, RDW_FRAME, RDW_INVALIDATE, RDW_UPDATENOW,
    TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows::Win32::UI::Controls::DRAWITEMSTRUCT;
use windows::Win32::UI::Shell::DefSubclassProc;
use windows::Win32::UI::WindowsAndMessaging::{
    DrawMenuBar, GetClientRect, GetMenuBarInfo, GetMenuItemInfoW, GetWindowRect, SystemParametersInfoW, HMENU,
    MENUBARINFO, MENUITEMINFOW, MIIM_STRING, NONCLIENTMETRICSW, OBJID_MENU,
    SPI_GETNONCLIENTMETRICS, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
};

use crate::theme::{self, Color, ThemeMode, ThemePalette};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Undocumented: the system asks the window to paint the whole menu bar.
const WM_UAHDRAWMENU: u32 = 0x0091;
/// Undocumented: the system asks the window to paint one menu bar item.
const WM_UAHDRAWMENUITEM: u32 = 0x0092;
const WM_NCPAINT: u32 = 0x0085;
const WM_NCACTIVATE: u32 = 0x0086;

/// DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR (Windows 11 build 22000+).
const DWMWA_BORDER_COLOR: u32 = 34;
const DWMWA_CAPTION_COLOR: u32 = 35;
const DWMWA_TEXT_COLOR: u32 = 36;
/// DWMWA_COLOR_DEFAULT: give the attribute back to the system.
const DWMWA_COLOR_DEFAULT: u32 = 0xFFFF_FFFF;

/// DWMWA_USE_IMMERSIVE_DARK_MODE on Windows 10 20H1+ and Windows 11.
const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
/// Same attribute on Windows 10 1809 / 1903 (before it was documented).
const DWMWA_USE_IMMERSIVE_DARK_MODE_OLD: u32 = 19;

/// uxtheme.dll ordinal: SetPreferredAppMode (AllowDarkModeForApp on 1809).
const UXTHEME_ORD_SET_PREFERRED_APP_MODE: u16 = 135;
/// uxtheme.dll ordinal: FlushMenuThemes.
const UXTHEME_ORD_FLUSH_MENU_THEMES: u16 = 136;
/// PreferredAppMode::ForceDark. The app theme is independent of the OS
/// setting, so AllowDark (1) would leave popups light on a light-mode OS.
/// On 1809 (BOOL parameter) any non-zero value means "allow".
const PREFERRED_APP_MODE_FORCE_DARK: i32 = 2;
/// PreferredAppMode::Default (FALSE on 1809): the app is not dark-aware, so
/// popup menus are light. This is what the Light theme switches back to.
const PREFERRED_APP_MODE_DEFAULT: i32 = 0;

// DRAWITEMSTRUCT.itemState bits (ODS_*)
const ODS_SELECTED: u32 = 0x0001;
const ODS_GRAYED: u32 = 0x0002;
const ODS_DISABLED: u32 = 0x0004;
const ODS_HOTLIGHT: u32 = 0x0040;
const ODS_NOACCEL: u32 = 0x0100;

// ---------------------------------------------------------------------------
// Undocumented structures (same layout as the widely used samples)
// ---------------------------------------------------------------------------

/// UAHMENU: payload of WM_UAHDRAWMENU.
#[repr(C)]
struct UahMenu {
    hmenu: HMENU,
    hdc: HDC,
    flags: u32,
}

/// UAHMENUITEMMETRICS: a union of 2 bar sizes and 4 popup sizes; only its
/// size matters here (4 x 2 DWORD = 32 bytes).
#[repr(C)]
struct UahMenuItemMetrics {
    rgsize: [[u32; 2]; 4],
}

/// UAHMENUPOPUPMETRICS: 4 widths and a 2-bit flag field.
#[repr(C)]
struct UahMenuPopupMetrics {
    rgcx: [u32; 4],
    flags: u32,
}

/// UAHMENUITEM.
#[repr(C)]
struct UahMenuItem {
    position: i32,
    umim: UahMenuItemMetrics,
    umpm: UahMenuPopupMetrics,
}

/// UAHDRAWMENUITEM: payload of WM_UAHDRAWMENUITEM.
#[repr(C)]
struct UahDrawMenuItem {
    dis: DRAWITEMSTRUCT,
    um: UahMenu,
    umi: UahMenuItem,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq)]
pub enum DarkChromeError {
    /// The DLL could not be loaded.
    ModuleLoad(String),
    /// The DLL does not export this ordinal.
    MissingOrdinal { module: String, ordinal: u16 },
}

impl std::fmt::Display for DarkChromeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ModuleLoad(m) => write!(f, "cannot load {m}"),
            Self::MissingOrdinal { module, ordinal } => {
                write!(f, "{module} does not export ordinal {ordinal}")
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Pure decision logic
// ---------------------------------------------------------------------------

/// Value of DWMWA_USE_IMMERSIVE_DARK_MODE for a theme mode.
fn immersive_dark_value(mode: ThemeMode) -> i32 {
    match mode {
        ThemeMode::Dark => 1,
        ThemeMode::Light => 0,
    }
}

/// Attribute ids to try, in order (current id first, early Windows 10 id second).
fn dwm_attribute_candidates() -> [u32; 2] {
    [DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_USE_IMMERSIVE_DARK_MODE_OLD]
}

/// Whether the custom menu bar painting is active for a mode.
fn paints_menu_bar(mode: ThemeMode) -> bool {
    match mode {
        ThemeMode::Dark | ThemeMode::Light => true,
    }
}

/// `PreferredAppMode` handed to uxtheme ordinal 135 for a mode.
fn preferred_app_mode(mode: ThemeMode) -> i32 {
    match mode {
        ThemeMode::Dark => PREFERRED_APP_MODE_FORCE_DARK,
        ThemeMode::Light => PREFERRED_APP_MODE_DEFAULT,
    }
}

/// Title bar colours of a mode; `None` = the system default (`DWMWA_COLOR_DEFAULT`).
#[derive(Debug, PartialEq)]
struct TitleBarColors {
    caption: Option<Color>,
    text: Option<Color>,
    border: Option<Color>,
}

/// Dark: the immersive dark title bar needs no colour. Light: the chrome
/// tint with the palette's text colour (>= 4.5:1, tested) and its border.
fn title_bar_colors(mode: ThemeMode, pal: &ThemePalette) -> TitleBarColors {
    match mode {
        ThemeMode::Dark => TitleBarColors { caption: None, text: None, border: None },
        ThemeMode::Light => TitleBarColors {
            caption: Some(pal.chrome),
            text: Some(pal.text_primary),
            border: Some(pal.chrome_border),
        },
    }
}

/// COLORREF value of a DWM colour attribute.
fn dwm_color_value(c: Option<Color>) -> u32 {
    match c {
        Some(c) => c.to_colorref(),
        None => DWMWA_COLOR_DEFAULT,
    }
}

/// Menu bar background.
fn bar_background(pal: &ThemePalette) -> Color {
    pal.chrome
}

/// Color of the 1 px line under the menu bar.
fn bar_line_color(pal: &ThemePalette) -> Color {
    pal.chrome_border
}

/// How one menu bar item is painted.
#[derive(Debug, PartialEq)]
struct ItemPaint {
    /// Rounded highlight behind the label, `None` when the item is idle.
    pill: Option<Color>,
    text: Color,
    /// Hide the `&` mnemonic underline (keyboard cues not requested).
    hide_prefix: bool,
}

/// Decide the look of a menu bar item from its `ODS_*` state bits.
fn item_paint(pal: &ThemePalette, state: u32) -> ItemPaint {
    let disabled = state & (ODS_GRAYED | ODS_DISABLED) != 0;
    let active = state & (ODS_HOTLIGHT | ODS_SELECTED) != 0;
    ItemPaint {
        pill: if active && !disabled { Some(pal.chrome_hover) } else { None },
        text: if disabled { pal.text_muted } else { pal.menu_text },
        hide_prefix: state & ODS_NOACCEL != 0,
    }
}

fn colorref(c: Color) -> COLORREF {
    COLORREF(c.to_colorref())
}

/// Resolve an export of `module` by ordinal. Explicit error, never a null.
fn lookup_ordinal(module: &str, ordinal: u16) -> Result<usize, DarkChromeError> {
    let wide: Vec<u16> = module.encode_utf16().chain(std::iter::once(0)).collect();
    let handle = unsafe { LoadLibraryW(PCWSTR(wide.as_ptr())) }
        .map_err(|_| DarkChromeError::ModuleLoad(module.to_string()))?;
    // MAKEINTRESOURCE: an ordinal is passed as a pointer with only the low word set.
    let proc = unsafe { GetProcAddress(handle, PCSTR(ordinal as usize as *const u8)) };
    match proc {
        Some(p) => Ok(p as usize),
        None => Err(DarkChromeError::MissingOrdinal { module: module.to_string(), ordinal }),
    }
}

// ---------------------------------------------------------------------------
// Explicit degradation
// ---------------------------------------------------------------------------

static WARNED_TITLE_BAR: AtomicBool = AtomicBool::new(false);
static WARNED_POPUP_MENUS: AtomicBool = AtomicBool::new(false);
static WARNED_TITLE_COLORS: AtomicBool = AtomicBool::new(false);
static WARNED_REDRAW: AtomicBool = AtomicBool::new(false);
/// The caption / text / border colours were set (so Dark must give them back).

fn warn_once(flag: &AtomicBool, detail: &str) {
    if !flag.swap(true, Ordering::SeqCst) {
        eprintln!("WARN dark_chrome: {detail}; keeping the native look");
    }
}

// ---------------------------------------------------------------------------
// Title bar and popup menus
// ---------------------------------------------------------------------------

/// Apply the title bar of `theme::current_mode()` to a top-level window:
/// the immersive dark flag, and in Light the chrome caption colours. Safe to
/// call again when the theme changes.
pub fn apply_title_bar(handle: &nwg::ControlHandle) {
    let hwnd = match handle {
        nwg::ControlHandle::Hwnd(h) => HWND(*h as *mut c_void),
        _ => return,
    };
    apply_title_bar_colors(hwnd);
    let value = immersive_dark_value(theme::current_mode());
    let mut last_err = None;
    for attr in dwm_attribute_candidates() {
        let res = unsafe {
            DwmSetWindowAttribute(
                hwnd,
                DWMWINDOWATTRIBUTE(attr as i32),
                &value as *const i32 as *const c_void,
                std::mem::size_of::<i32>() as u32,
            )
        };
        match res {
            Ok(()) => return,
            Err(e) => last_err = Some((attr, e)),
        }
    }
    if let Some((attr, e)) = last_err {
        warn_once(&WARNED_TITLE_BAR, &format!("DwmSetWindowAttribute({attr}) failed: {e}"));
    }
}

/// Set the caption / text / border colours of the current mode. Dark gives
/// them back to the system on every call (state is per window, so no
/// process-wide "was set" flag); Windows 10 has no such attribute and only
/// gets the single warning.
fn apply_title_bar_colors(hwnd: HWND) {
    let mode = theme::current_mode();
    let colors = title_bar_colors(mode, theme::current());
    for (attr, color) in [
        (DWMWA_CAPTION_COLOR, colors.caption),
        (DWMWA_TEXT_COLOR, colors.text),
        (DWMWA_BORDER_COLOR, colors.border),
    ] {
        let value = dwm_color_value(color);
        let res = unsafe {
            DwmSetWindowAttribute(
                hwnd,
                DWMWINDOWATTRIBUTE(attr as i32),
                &value as *const u32 as *const c_void,
                std::mem::size_of::<u32>() as u32,
            )
        };
        if let Err(e) = res {
            warn_once(
                &WARNED_TITLE_COLORS,
                &format!("title bar colour attribute {attr} failed: {e} (needs Windows 11 build 22000+)"),
            );
        }
    }
}

/// Set the popup menus' preferred app mode for `theme::current_mode()` and
/// flush uxtheme's menu cache. Call before the menus are shown, and again
/// when the theme changes.
pub fn apply_popup_menu_theme() {
    let mode = preferred_app_mode(theme::current_mode());
    let result = lookup_ordinal("uxtheme.dll", UXTHEME_ORD_SET_PREFERRED_APP_MODE).and_then(|set| {
        let flush = lookup_ordinal("uxtheme.dll", UXTHEME_ORD_FLUSH_MENU_THEMES)?;
        // SAFETY: both ordinals have these signatures on every Windows 10 1809+ build.
        unsafe {
            let set: extern "system" fn(i32) -> i32 = std::mem::transmute(set);
            let flush: extern "system" fn() = std::mem::transmute(flush);
            set(mode);
            flush();
        }
        Ok(())
    });
    if let Err(e) = result {
        warn_once(&WARNED_POPUP_MENUS, &format!("popup menu theme unavailable ({e})"));
    }
}

/// Repaint the menu bar and the non-client frame after a theme change
/// (`DrawMenuBar`, then a frame redraw so the title bar attributes show).
pub fn redraw_menu_bar(handle: &nwg::ControlHandle) {
    let hwnd = match handle {
        nwg::ControlHandle::Hwnd(h) => HWND(*h as *mut c_void),
        _ => return,
    };
    if let Err(e) = unsafe { DrawMenuBar(hwnd) } {
        warn_once(&WARNED_REDRAW, &format!("DrawMenuBar failed: {e}"));
    }
    let ok = unsafe { RedrawWindow(Some(hwnd), None, None, RDW_FRAME | RDW_INVALIDATE | RDW_UPDATENOW) };
    if !ok.as_bool() {
        warn_once(&WARNED_REDRAW, "RedrawWindow(RDW_FRAME) failed");
    }
}

/// Everything the main window needs once it is built.
pub fn apply_main_window(handle: &nwg::ControlHandle) {
    apply_title_bar(handle);
    apply_popup_menu_theme();
}

// ---------------------------------------------------------------------------
// Menu bar painting
// ---------------------------------------------------------------------------

/// Hook for the main window's raw event handler. Returns `None` for every
/// message it does not take over.
pub fn handle_message(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> Option<isize> {
    if !paints_menu_bar(theme::current_mode()) {
        return None;
    }
    let hwnd_w = HWND(hwnd as *mut c_void);
    let pal = theme::current();
    match msg {
        WM_UAHDRAWMENU => {
            if lparam == 0 {
                return None;
            }
            let um = unsafe { &*(lparam as *const UahMenu) };
            draw_bar_background(hwnd_w, um.hdc, pal);
            Some(1)
        }
        WM_UAHDRAWMENUITEM => {
            if lparam == 0 {
                return None;
            }
            let item = unsafe { &*(lparam as *const UahDrawMenuItem) };
            draw_bar_item(item, pal);
            Some(1)
        }
        WM_NCPAINT | WM_NCACTIVATE => {
            // Let the system paint first, then cover its 1 px line under the bar.
            let res = unsafe { DefSubclassProc(hwnd_w, msg, windows::Win32::Foundation::WPARAM(wparam), windows::Win32::Foundation::LPARAM(lparam)) };
            draw_bar_line(hwnd_w, pal);
            Some(res.0)
        }
        _ => None,
    }
}

/// Menu bar rectangle in window coordinates, `None` when the window has no bar.
fn bar_rect(hwnd: HWND) -> Option<RECT> {
    let mut mbi = MENUBARINFO { cbSize: std::mem::size_of::<MENUBARINFO>() as u32, ..Default::default() };
    unsafe { GetMenuBarInfo(hwnd, OBJID_MENU, 0, &mut mbi) }.ok()?;
    let mut win = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut win) }.ok()?;
    let mut rc = mbi.rcBar;
    rc.left -= win.left;
    rc.right -= win.left;
    rc.top -= win.top;
    rc.bottom -= win.top;
    Some(rc)
}

fn fill(hdc: HDC, rc: &RECT, color: Color) {
    let brush = unsafe { CreateSolidBrush(colorref(color)) };
    unsafe {
        FillRect(hdc, rc, brush);
        let _ = DeleteObject(HGDIOBJ(brush.0));
    }
}

fn draw_bar_background(hwnd: HWND, hdc: HDC, pal: &ThemePalette) {
    if let Some(rc) = bar_rect(hwnd) {
        fill(hdc, &rc, bar_background(pal));
    }
}

/// Paint the 1 px line just above the client area (the bottom of the bar).
fn draw_bar_line(hwnd: HWND, pal: &ThemePalette) {
    if bar_rect(hwnd).is_none() {
        return;
    }
    let mut client = RECT::default();
    let mut win = RECT::default();
    unsafe {
        if GetClientRect(hwnd, &mut client).is_err() || GetWindowRect(hwnd, &mut win).is_err() {
            return;
        }
        // Client rect (0,0 based) -> screen -> window coordinates.
        let mut pts = [POINT { x: client.left, y: client.top }, POINT { x: client.right, y: client.bottom }];
        MapWindowPoints(Some(hwnd), None, &mut pts);
        let top = pts[0].y - win.top;
        let line = RECT { left: pts[0].x - win.left, top: top - 1, right: pts[1].x - win.left, bottom: top };
        let hdc = GetWindowDC(Some(hwnd));
        if hdc.is_invalid() {
            return;
        }
        fill(hdc, &line, bar_line_color(pal));
        ReleaseDC(Some(hwnd), hdc);
    }
}

/// Label of the item at `position` (with its `&` mnemonic), UTF-16 without NUL.
fn item_text(hmenu: HMENU, position: i32) -> Option<Vec<u16>> {
    if position < 0 {
        return None;
    }
    let mut info = MENUITEMINFOW {
        cbSize: std::mem::size_of::<MENUITEMINFOW>() as u32,
        fMask: MIIM_STRING,
        ..Default::default()
    };
    // First call: length. Second call: text (cch includes room for the NUL).
    unsafe { GetMenuItemInfoW(hmenu, position as u32, true, &mut info) }.ok()?;
    let mut buf = vec![0u16; info.cch as usize + 1];
    info.cch += 1;
    info.dwTypeData = windows::core::PWSTR(buf.as_mut_ptr());
    unsafe { GetMenuItemInfoW(hmenu, position as u32, true, &mut info) }.ok()?;
    buf.truncate(info.cch as usize);
    Some(buf)
}

fn draw_bar_item(item: &UahDrawMenuItem, pal: &ThemePalette) {
    let hdc = item.um.hdc;
    let mut rc = item.dis.rcItem;
    let paint = item_paint(pal, item.dis.itemState.0);
    fill(hdc, &rc, bar_background(pal));
    if let Some(pill) = paint.pill {
        unsafe {
            let brush = CreateSolidBrush(colorref(pill));
            let rgn = CreateRoundRectRgn(rc.left + 2, rc.top + 3, rc.right - 2, rc.bottom - 3, 8, 8);
            let _ = FillRgn(hdc, rgn, brush);
            let _ = DeleteObject(HGDIOBJ(rgn.0));
            let _ = DeleteObject(HGDIOBJ(brush.0));
        }
    }
    let Some(mut text) = item_text(item.um.hmenu, item.umi.position) else {
        return;
    };
    // System menu font, so the label matches the native metrics.
    let mut ncm = NONCLIENTMETRICSW { cbSize: std::mem::size_of::<NONCLIENTMETRICSW>() as u32, ..Default::default() };
    let have_font = unsafe {
        SystemParametersInfoW(
            SPI_GETNONCLIENTMETRICS,
            ncm.cbSize,
            Some(&mut ncm as *mut _ as *mut c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    }
    .is_ok();
    let font = if have_font {
        Some(unsafe { windows::Win32::Graphics::Gdi::CreateFontIndirectW(&ncm.lfMenuFont) })
    } else {
        None
    };
    unsafe {
        let old = font.map(|f| SelectObject(hdc, HGDIOBJ(f.0)));
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, colorref(paint.text));
        let mut flags = DT_CENTER | DT_SINGLELINE | DT_VCENTER;
        if paint.hide_prefix {
            flags |= DT_HIDEPREFIX;
        }
        DrawTextW(hdc, &mut text, &mut rc, flags);
        if let (Some(old), Some(f)) = (old, font) {
            SelectObject(hdc, old);
            let _ = DeleteObject(HGDIOBJ(f.0));
        }
    }
}

use native_windows_gui as nwg;

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{offset_of, size_of};

    fn dark() -> &'static ThemePalette {
        theme::get_palette(ThemeMode::Dark)
    }

    #[test]
    fn immersive_value_follows_mode() {
        assert_eq!(immersive_dark_value(ThemeMode::Dark), 1);
        assert_eq!(immersive_dark_value(ThemeMode::Light), 0);
    }

    #[test]
    fn dwm_attribute_tries_current_id_before_old_one() {
        assert_eq!(dwm_attribute_candidates(), [20, 19]);
    }

    fn light() -> &'static ThemePalette {
        theme::get_palette(ThemeMode::Light)
    }

    #[test]
    fn menu_bar_is_painted_in_both_themes() {
        assert!(paints_menu_bar(ThemeMode::Dark));
        assert!(paints_menu_bar(ThemeMode::Light));
    }

    #[test]
    fn popup_menus_go_back_to_the_default_mode_in_light() {
        assert_eq!(preferred_app_mode(ThemeMode::Dark), 2);
        assert_eq!(preferred_app_mode(ThemeMode::Light), 0);
    }

    #[test]
    fn dark_title_bar_keeps_the_system_colours() {
        let c = title_bar_colors(ThemeMode::Dark, dark());
        assert_eq!(c, TitleBarColors { caption: None, text: None, border: None });
        assert_eq!(dwm_color_value(c.caption), 0xFFFF_FFFF);
    }

    #[test]
    fn light_title_bar_takes_the_chrome_tint_with_readable_text() {
        let p = light();
        let c = title_bar_colors(ThemeMode::Light, p);
        assert_eq!(c.caption, Some(p.chrome));
        assert_eq!(c.border, Some(p.chrome_border));
        let text = c.text.expect("light title text colour");
        assert!(text.contrast_ratio(&p.chrome) >= 4.5, "title text on caption");
        assert_eq!(dwm_color_value(c.caption), Color::new(219, 232, 252).to_colorref());
    }

    #[test]
    fn dwm_attribute_ids_match_the_win11_sdk() {
        assert_eq!((DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR), (34, 35, 36));
    }

    #[test]
    fn light_bar_uses_the_tinted_chrome_and_its_hover() {
        let p = light();
        assert_eq!(bar_background(p), p.chrome);
        assert_eq!(bar_line_color(p), p.chrome_border);
        let hot = item_paint(p, ODS_HOTLIGHT);
        assert_eq!(hot.pill, Some(p.chrome_hover));
        assert_eq!(hot.text, p.menu_text);
        assert!(hot.text.contrast_ratio(&p.chrome_hover) >= 4.5);
        let idle = item_paint(p, 0);
        assert_eq!(idle.pill, None);
        assert!(idle.text.contrast_ratio(&p.chrome) >= 4.5);
        let off = item_paint(p, ODS_GRAYED);
        assert!(off.text.contrast_ratio(&p.chrome) >= 4.5);
    }

    #[test]
    fn dark_bar_items_are_unchanged() {
        let p = dark();
        assert_eq!(item_paint(p, ODS_HOTLIGHT).pill, Some(Color::new(53, 57, 66)));
        assert_eq!(item_paint(p, 0).text, Color::new(212, 216, 224));
    }

    #[test]
    fn bar_uses_chrome_colors() {
        let p = dark();
        assert_eq!(bar_background(p).to_colorref(), Color::new(27, 29, 35).to_colorref());
        assert_eq!(bar_line_color(p).to_colorref(), Color::new(47, 51, 61).to_colorref());
    }

    #[test]
    fn idle_item_has_no_pill_and_primary_text() {
        let p = item_paint(dark(), 0);
        assert_eq!(p.pill, None);
        assert_eq!(p.text, dark().text_primary);
        assert!(!p.hide_prefix);
    }

    #[test]
    fn hot_and_selected_items_get_surface_light_pill() {
        for state in [ODS_HOTLIGHT, ODS_SELECTED, ODS_HOTLIGHT | ODS_SELECTED] {
            assert_eq!(item_paint(dark(), state).pill, Some(Color::new(53, 57, 66)));
        }
    }

    #[test]
    fn disabled_item_is_muted_without_pill() {
        for state in [ODS_GRAYED, ODS_DISABLED, ODS_GRAYED | ODS_HOTLIGHT] {
            let p = item_paint(dark(), state);
            assert_eq!(p.pill, None);
            assert_eq!(p.text, dark().text_muted);
        }
    }

    #[test]
    fn noaccel_hides_mnemonic_underline() {
        assert!(item_paint(dark(), ODS_NOACCEL).hide_prefix);
    }

    #[test]
    fn uah_struct_layouts_match_the_undocumented_abi() {
        assert_eq!(size_of::<UahMenuItemMetrics>(), 32);
        assert_eq!(size_of::<UahMenuPopupMetrics>(), 20);
        assert_eq!(size_of::<UahMenuItem>(), 56);
        assert_eq!(offset_of!(UahMenu, hdc), size_of::<usize>());
        #[cfg(target_pointer_width = "64")]
        {
            assert_eq!(size_of::<DRAWITEMSTRUCT>(), 64);
            assert_eq!(size_of::<UahMenu>(), 24);
            assert_eq!(size_of::<UahDrawMenuItem>(), 144);
            assert_eq!(offset_of!(UahDrawMenuItem, um), 64);
            assert_eq!(offset_of!(UahDrawMenuItem, umi), 88);
        }
        #[cfg(target_pointer_width = "32")]
        {
            assert_eq!(size_of::<DRAWITEMSTRUCT>(), 48);
            assert_eq!(size_of::<UahMenu>(), 12);
            assert_eq!(size_of::<UahDrawMenuItem>(), 116);
        }
    }

    #[test]
    fn lookup_ordinal_reports_missing_module() {
        let err = lookup_ordinal("locknote_no_such_module.dll", 135).unwrap_err();
        assert_eq!(err, DarkChromeError::ModuleLoad("locknote_no_such_module.dll".into()));
    }

    #[test]
    fn lookup_ordinal_reports_missing_ordinal() {
        let err = lookup_ordinal("kernel32.dll", 0xFFF0).unwrap_err();
        assert_eq!(
            err,
            DarkChromeError::MissingOrdinal { module: "kernel32.dll".into(), ordinal: 0xFFF0 }
        );
    }

    #[test]
    fn lookup_ordinal_resolves_an_existing_export() {
        // uxtheme exports ordinals 135/136 on every supported Windows 10+ build.
        assert!(lookup_ordinal("uxtheme.dll", UXTHEME_ORD_SET_PREFERRED_APP_MODE).is_ok());
        assert!(lookup_ordinal("uxtheme.dll", UXTHEME_ORD_FLUSH_MENU_THEMES).is_ok());
    }
}
