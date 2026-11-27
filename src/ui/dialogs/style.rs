// Dialog styling: applies the shared dialog look to a built nwg dialog.
//
// `apply_dialog_theme` (mod.rs) registers the window here right after it is
// built. The decisions (metrics, layout, colors) live in `metrics.rs`; this
// file measures the real controls, applies the results and paints:
//
//   - one pass, run from the first message of the dialog's loop (a posted
//     message, so it runs before the first paint and after every control
//     exists): per-monitor DPI scaling of the window and its controls, the
//     font hierarchy, reflow of text that does not fit, uniform right-aligned
//     footer buttons, dark native controls (Dark theme only) and centering on
//     the application's main window;
//   - owner-drawn push buttons (accent fill for the default button, the
//     palette's button colors for the others in Dark; only the default button
//     in Light, the rest keeps the native look);
//   - WM_CTLCOLOR* for labels, edit boxes and lists, Esc = close.
//
// Failures never hide: an OS call that fails logs one `WARN dialog_style: ...`
// line to stderr and the control keeps its native look.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::rc::Rc;

use native_windows_gui as nwg;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateFontIndirectW, CreateSolidBrush, DeleteObject, DrawTextW, FillRect, GetDC, GetMonitorInfoW,
    GetObjectW, InvalidateRect, MapWindowPoints, ReleaseDC, RoundRect, SelectObject,
    SetBkColor, SetBkMode, SetTextColor, DT_CALCRECT, DT_CENTER, DT_HIDEPREFIX, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER,
    DT_WORDBREAK, HBRUSH, HDC, HFONT, HGDIOBJ, LOGFONTW, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    TRANSPARENT,
};
use windows::Win32::UI::Controls::{DRAWITEMSTRUCT, GetComboBoxInfo, COMBOBOXINFO, SetWindowTheme};
use windows::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow};
use windows::Win32::UI::Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumThreadWindows, GetClassNameW, GetClientRect, GetParent, GetWindowLongPtrW,
    GetWindowRect, GetWindowTextW, IsIconic, IsWindowVisible, PostMessageW, SendMessageW, SetWindowLongPtrW,
    SetWindowPos, GWL_EXSTYLE, GWL_STYLE, HWND_TOP, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOCOPYBITS, SWP_NOMOVE, SWP_NOSIZE,
    SWP_NOZORDER, WM_CLOSE, WM_GETFONT, WM_SETFONT,
};

use super::metrics::{
    self, button_colors, center_in, layout, role_font, role_for, role_text_color, scale, ButtonKind,
    ButtonState, ItemKind, LayoutItem, Rect, Role, BASE_DPI, CORNER_DP,
};
use crate::theme::{self, Color, ThemeMode, ThemePalette};

// ---------------------------------------------------------------------------
// Messages and styles
// ---------------------------------------------------------------------------

/// Posted by `register`: runs the one-time pass.
const WM_STYLE_PASS: u32 = 0x8000 + 0x4C4E;
const WM_COMMAND: u32 = 0x0111;
const WM_DRAWITEM: u32 = 0x002B;
const WM_NCDESTROY: u32 = 0x0082;
const WM_MOUSEMOVE: u32 = 0x0200;
const WM_MOUSELEAVE: u32 = 0x02A3;
const WM_ERASEBKGND: u32 = 0x0014;
const WM_CTLCOLORLISTBOX: u32 = 0x0134;
const WM_CTLCOLOREDIT: u32 = 0x0133;
const WM_CTLCOLORBTN: u32 = 0x0135;
const WM_CTLCOLORSTATIC: u32 = 0x0138;
const IDCANCEL: usize = 2;

const BS_TYPEMASK: u32 = 0x0F;
const BS_PUSHBUTTON: u32 = 0x00;
const BS_DEFPUSHBUTTON: u32 = 0x01;
const BS_OWNERDRAW: u32 = 0x0B;
const BS_MULTILINE: u32 = 0x2000;
const SS_TYPEMASK: u32 = 0x1F;
const SS_NOPREFIX: u32 = 0x80;
const ES_MULTILINE: u32 = 0x0004;
const ODS_SELECTED: u32 = 0x0001;
const ODS_DISABLED: u32 = 0x0004;
const ODS_FOCUS: u32 = 0x0010;

const SUBCLASS_BUTTON: usize = 0x4C4E_0001;
const SUBCLASS_FRAME: usize = 0x4C4E_0002;
const SUBCLASS_CHECK: usize = 0x4C4E_0003;
const SUBCLASS_NC: usize = 0x4C4E_0004;

fn warn(msg: &str) {
    eprintln!("WARN dialog_style: {msg}");
}

fn hwnd_of(h: nwg::ControlHandle) -> Option<HWND> {
    match h {
        nwg::ControlHandle::Hwnd(p) => Some(HWND(p as *mut c_void)),
        _ => None,
    }
}

fn key(h: HWND) -> isize {
    h.0 as isize
}

fn colorref(c: Color) -> COLORREF {
    COLORREF(c.to_colorref())
}

// ---------------------------------------------------------------------------
// Per-thread state
// ---------------------------------------------------------------------------

#[derive(Default)]
struct DialogState {
    /// Default (Enter) button: drawn as the primary button.
    primary: Option<isize>,
    /// Labels drawn in the error color.
    errors: Vec<isize>,
    /// Role of each styled control (text color of labels).
    roles: HashMap<isize, Role>,
    /// Owner-drawn push buttons.
    owner_drawn: Vec<isize>,
    /// Fonts created by the pass; deleted with the window.
    fonts: Vec<HFONT>,
    done: bool,
}

#[derive(Clone, Copy)]
struct Brushes {
    background: HBRUSH,
    input: HBRUSH,
}

thread_local! {
    static STATES: RefCell<HashMap<isize, Rc<RefCell<DialogState>>>> = RefCell::new(HashMap::new());
    /// Button under the mouse (owner-drawn buttons only).
    static HOT: RefCell<Option<isize>> = const { RefCell::new(None) };
    /// Fill of hairline / bar frames.
    static FRAME_FILL: RefCell<HashMap<isize, u32>> = RefCell::new(HashMap::new());
}

/// Brushes of the current palette, from the colour-keyed cache: a theme
/// switch changes the colours asked for, never a stale brush.
fn brushes(pal: &ThemePalette) -> Brushes {
    Brushes {
        background: HBRUSH(crate::ui::lock_card::cached_brush(pal.background) as *mut c_void),
        input: HBRUSH(crate::ui::lock_card::cached_brush(pal.input_background) as *mut c_void),
    }
}

#[cfg(test)]
mod brush_tests {
    use super::*;
    use crate::theme::{get_palette, ThemeMode};

    #[test]
    fn brushes_follow_the_palette_not_the_first_dialog() {
        let dark = brushes(get_palette(ThemeMode::Dark));
        let light = brushes(get_palette(ThemeMode::Light));
        assert_ne!(dark.background.0, light.background.0);
        assert_ne!(dark.input.0, light.input.0);
        // Same colour, same brush (bounded cache).
        assert_eq!(dark.background.0, brushes(get_palette(ThemeMode::Dark)).background.0);
    }
}

fn state_of(window: isize) -> Option<Rc<RefCell<DialogState>>> {
    STATES.with(|s| s.borrow().get(&window).cloned())
}

/// Number of LockNote dialogs open on this thread: registered by
/// `apply_dialog_theme`, forgotten at WM_NCDESTROY (issue #30: the main
/// window's timers wait while one is open).
pub fn open_dialog_count() -> usize {
    STATES.with(|s| s.borrow().len())
}

/// The windows of the LockNote dialogs open on this thread.
pub fn registered_dialogs() -> Vec<isize> {
    STATES.with(|s| s.borrow().keys().copied().collect())
}

/// Whether `window` is a LockNote dialog open on this thread (issue #32:
/// the input hook asks for every key press or click, no list built).
pub fn is_registered_dialog(window: isize) -> bool {
    STATES.with(|s| s.borrow().contains_key(&window))
}

/// Register a freshly built dialog and queue its styling pass.
pub fn register(window: &nwg::Window) {
    let Some(hwnd) = hwnd_of(window.handle) else {
        warn("register: window has no HWND");
        return;
    };
    STATES.with(|s| {
        s.borrow_mut().insert(key(hwnd), Rc::new(RefCell::new(DialogState::default())));
    });
    unsafe {
        if let Err(e) = PostMessageW(Some(hwnd), WM_STYLE_PASS, WPARAM(0), LPARAM(0)) {
            warn(&format!("register: PostMessageW failed: {e}"));
        }
    }
}

/// Declare the default (Enter) button of a dialog: it becomes the primary button.
pub fn set_primary(window: nwg::ControlHandle, button: nwg::ControlHandle) {
    if let (Some(w), Some(b)) = (hwnd_of(window), hwnd_of(button)) {
        match state_of(key(w)) {
            Some(st) => st.borrow_mut().primary = Some(key(b)),
            None => warn("set_primary: dialog not registered (call apply_dialog_theme first)"),
        }
    }
}

/// Draw `label` in the error color.
pub fn mark_error(window: nwg::ControlHandle, label: nwg::ControlHandle) {
    if let (Some(w), Some(l)) = (hwnd_of(window), hwnd_of(label)) {
        match state_of(key(w)) {
            Some(st) => st.borrow_mut().errors.push(key(l)),
            None => warn("mark_error: dialog not registered (call apply_dialog_theme first)"),
        }
    }
}

/// Paint a frame (bar, separator) with `color`. Unlike a window-class brush,
/// this is per control: all nwg frames share one class.
pub fn set_frame_color(frame: nwg::ControlHandle, color: Color) {
    let Some(h) = hwnd_of(frame) else { return };
    let first = FRAME_FILL.with(|f| f.borrow_mut().insert(key(h), color.to_colorref()).is_none());
    unsafe {
        if first && !SetWindowSubclass(h, Some(frame_proc), SUBCLASS_FRAME, 0).as_bool() {
            warn("set_frame_color: SetWindowSubclass failed");
        }
        let _ = InvalidateRect(Some(h), None, true);
    }
}

// ---------------------------------------------------------------------------
// Messages from the dialog's raw handler
// ---------------------------------------------------------------------------

/// Handle the messages the shared look cares about. `None` = not handled.
pub fn handle_message(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> Option<isize> {
    let pal = theme::current();
    match msg {
        WM_STYLE_PASS => {
            if let Some(st) = state_of(hwnd) {
                if !st.borrow().done {
                    st.borrow_mut().done = true;
                    run_pass(HWND(hwnd as *mut c_void), &st, pal);
                }
            }
            Some(0)
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLORBTN | WM_CTLCOLORLISTBOX | WM_CTLCOLOREDIT => {
            let hdc = HDC(wparam as *mut c_void);
            let child = lparam;
            let b = brushes(pal);
            let class = class_name(HWND(child as *mut c_void));
            // Read-only edits report as static, list boxes as list: both are inputs.
            let input = msg == WM_CTLCOLOREDIT
                || msg == WM_CTLCOLORLISTBOX
                || (msg == WM_CTLCOLORSTATIC && class.eq_ignore_ascii_case("Edit"));
            let (text, back, brush) = if input {
                (pal.text_primary, pal.input_background, b.input)
            } else {
                let st = state_of(hwnd);
                let (is_err, role) = st
                    .map(|s| {
                        let s = s.borrow();
                        (s.errors.contains(&child), s.roles.get(&child).copied().unwrap_or(Role::Body))
                    })
                    .unwrap_or((false, Role::Body));
                let text = if is_err { pal.error_text } else { role_text_color(pal, role) };
                (text, pal.background, b.background)
            };
            unsafe {
                SetTextColor(hdc, colorref(text));
                SetBkColor(hdc, colorref(back));
                if !input {
                    SetBkMode(hdc, TRANSPARENT);
                }
            }
            Some(brush.0 as isize)
        }
        WM_DRAWITEM => {
            let dis = unsafe { (lparam as *const DRAWITEMSTRUCT).as_ref()? };
            // ODT_BUTTON = 4
            if dis.CtlType.0 != 4 {
                return None;
            }
            let st = state_of(hwnd)?;
            let primary = st.borrow().primary == Some(dis.hwndItem.0 as isize);
            draw_button(dis, primary, pal);
            Some(1)
        }
        WM_COMMAND => {
            // IsDialogMessage turns Esc into WM_COMMAND(IDCANCEL) with no control:
            // same as the title-bar close button, which every dialog handles.
            if lparam == 0 && (wparam & 0xFFFF) == IDCANCEL && state_of(hwnd).is_some() {
                unsafe {
                    if let Err(e) = PostMessageW(Some(HWND(hwnd as *mut c_void)), WM_CLOSE, WPARAM(0), LPARAM(0)) {
                        warn(&format!("Esc: PostMessageW(WM_CLOSE) failed: {e}"));
                    }
                }
                return Some(0);
            }
            None
        }
        WM_NCDESTROY => {
            if let Some(st) = STATES.with(|s| s.borrow_mut().remove(&hwnd)) {
                let st = st.borrow();
                for f in &st.fonts {
                    unsafe {
                        let _ = DeleteObject(HGDIOBJ(f.0));
                    }
                }
                HOT.with(|h| {
                    let mut h = h.borrow_mut();
                    if h.map_or(false, |k| st.owner_drawn.contains(&k)) {
                        *h = None;
                    }
                });
            }
            None
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Win32 helpers
// ---------------------------------------------------------------------------

fn class_name(h: HWND) -> String {
    let mut buf = [0u16; 64];
    let n = unsafe { GetClassNameW(h, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn window_text(h: HWND) -> String {
    let mut buf = vec![0u16; 1024];
    let n = unsafe { GetWindowTextW(h, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn style_of(h: HWND) -> u32 {
    unsafe { GetWindowLongPtrW(h, GWL_STYLE) as u32 }
}

fn rect_in_parent(h: HWND, parent: HWND) -> Option<Rect> {
    let mut r = RECT::default();
    unsafe {
        GetWindowRect(h, &mut r).ok()?;
        let mut pts = [
            windows::Win32::Foundation::POINT { x: r.left, y: r.top },
            windows::Win32::Foundation::POINT { x: r.right, y: r.bottom },
        ];
        MapWindowPoints(None, Some(parent), &mut pts);
        Some(Rect::new(pts[0].x, pts[0].y, pts[1].x, pts[1].y))
    }
}

fn font_of(h: HWND) -> Option<HFONT> {
    let r = unsafe { SendMessageW(h, WM_GETFONT, None, None) };
    if r.0 == 0 { None } else { Some(HFONT(r.0 as *mut c_void)) }
}

fn logfont_of(f: HFONT) -> Option<LOGFONTW> {
    let mut lf = LOGFONTW::default();
    let n = unsafe {
        GetObjectW(HGDIOBJ(f.0), std::mem::size_of::<LOGFONTW>() as i32, Some(&mut lf as *mut _ as *mut c_void))
    };
    if n == 0 { None } else { Some(lf) }
}

fn face_of(lf: &LOGFONTW) -> String {
    let end = lf.lfFaceName.iter().position(|&c| c == 0).unwrap_or(lf.lfFaceName.len());
    String::from_utf16_lossy(&lf.lfFaceName[..end])
}

/// Pixel size of `text` in `font`; `width` > 0 wraps at that width.
fn measure(h: HWND, font: Option<HFONT>, text: &str, width: i32, no_prefix: bool) -> (i32, i32) {
    let mut wide: Vec<u16> = text.encode_utf16().collect();
    if wide.is_empty() {
        return (0, 0);
    }
    unsafe {
        let hdc = GetDC(Some(h));
        if hdc.is_invalid() {
            return (0, 0);
        }
        let old = font.map(|f| SelectObject(hdc, HGDIOBJ(f.0)));
        let mut rc = RECT { left: 0, top: 0, right: width.max(0), bottom: 0 };
        let mut flags = DT_CALCRECT;
        if width > 0 {
            flags |= DT_WORDBREAK;
        } else {
            flags |= DT_SINGLELINE;
        }
        if no_prefix {
            flags |= DT_NOPREFIX;
        }
        DrawTextW(hdc, &mut wide, &mut rc, flags);
        if let Some(o) = old {
            SelectObject(hdc, o);
        }
        ReleaseDC(Some(h), hdc);
        (rc.right - rc.left, rc.bottom - rc.top)
    }
}

/// DPI of the monitor the window is on (per-monitor aware process).
fn dpi_of(hwnd: HWND) -> u32 {
    #[cfg(test)]
    if let Some(d) = TEST_DPI.with(|d| d.get()) {
        return d;
    }
    match unsafe { GetDpiForWindow(hwnd) } {
        0 => {
            warn("GetDpiForWindow returned 0, assuming 96");
            BASE_DPI
        }
        d => d,
    }
}

#[cfg(test)]
thread_local! {
    static TEST_DPI: std::cell::Cell<Option<u32>> = const { std::cell::Cell::new(None) };
}

/// Test seam for the visual check (`snapshot.rs`): pretend the monitor is `dpi`.
#[cfg(test)]
pub fn set_test_dpi(dpi: Option<u32>) {
    TEST_DPI.with(|d| d.set(dpi));
}

fn set_theme(h: HWND, name: PCWSTR) {
    unsafe {
        if let Err(e) = SetWindowTheme(h, name, PCWSTR::null()) {
            warn(&format!("SetWindowTheme failed on {}: {e}", class_name(h)));
        }
    }
}

// ---------------------------------------------------------------------------
// The pass
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Class {
    Label,
    Push,
    Check,
    Edit { multiline: bool },
    Combo,
    List,
    Frame,
    Other,
}

struct Child {
    hwnd: HWND,
    class: Class,
    rect: Rect,
    text: String,
    role: Role,
    segoe: bool,
    prefix_off: bool,
    font: Option<HFONT>,
}

fn classify(h: HWND) -> Class {
    let cn = class_name(h);
    let style = style_of(h);
    match cn.to_ascii_lowercase().as_str() {
        "static" => match style & SS_TYPEMASK {
            0 | 1 | 2 => Class::Label,
            _ => Class::Other,
        },
        "button" => match style & BS_TYPEMASK {
            BS_PUSHBUTTON | BS_DEFPUSHBUTTON | BS_OWNERDRAW => Class::Push,
            2..=6 | 9 => Class::Check,
            _ => Class::Other,
        },
        "edit" => Class::Edit { multiline: style & ES_MULTILINE != 0 },
        "combobox" => Class::Combo,
        "listbox" => Class::List,
        _ if cn.to_ascii_lowercase().contains("frame") => Class::Frame,
        _ => Class::Other,
    }
}

fn collect_children(parent: HWND) -> Vec<HWND> {
    use windows::Win32::UI::WindowsAndMessaging::{GetWindow, GW_CHILD, GW_HWNDNEXT};
    let mut out = Vec::new();
    unsafe {
        let mut c = GetWindow(parent, GW_CHILD).ok();
        while let Some(h) = c {
            out.push(h);
            c = GetWindow(h, GW_HWNDNEXT).ok();
        }
    }
    out
}

/// The main application window to center over: the largest visible,
/// non-minimized top-level window of this thread other than `dialog`.
fn owner_rect(dialog: HWND) -> Option<RECT> {
    struct Acc {
        dialog: HWND,
        best: Option<(i64, RECT)>,
    }
    unsafe extern "system" fn cb(h: HWND, lp: LPARAM) -> windows::core::BOOL {
        let acc = &mut *(lp.0 as *mut Acc);
        if h == acc.dialog || !IsWindowVisible(h).as_bool() || IsIconic(h).as_bool() {
            return true.into();
        }
        // Only top-level windows (no parent)
        if GetParent(h).is_ok() {
            return true.into();
        }
        let mut r = RECT::default();
        if GetWindowRect(h, &mut r).is_ok() {
            let area = (r.right - r.left) as i64 * (r.bottom - r.top) as i64;
            if acc.best.map_or(true, |(a, _)| area > a) {
                acc.best = Some((area, r));
            }
        }
        true.into()
    }
    let mut acc = Acc { dialog, best: None };
    unsafe {
        let tid = windows::Win32::System::Threading::GetCurrentThreadId();
        let _ = EnumThreadWindows(tid, Some(cb), LPARAM(&mut acc as *mut Acc as isize));
    }
    acc.best.map(|(_, r)| r)
}

fn work_area_of(r: &RECT) -> Option<RECT> {
    unsafe {
        let mon = MonitorFromWindow_rect(r);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if GetMonitorInfoW(mon, &mut mi).as_bool() { Some(mi.rcWork) } else { None }
    }
}

#[allow(non_snake_case)]
unsafe fn MonitorFromWindow_rect(r: &RECT) -> windows::Win32::Graphics::Gdi::HMONITOR {
    windows::Win32::Graphics::Gdi::MonitorFromRect(r, MONITOR_DEFAULTTONEAREST)
}

fn make_font(face: &str, height: i32, weight: i32, base: Option<&LOGFONTW>) -> Option<HFONT> {
    let mut lf = base.copied().unwrap_or_default();
    lf.lfHeight = height;
    lf.lfWeight = weight;
    if base.is_none() || !face.is_empty() {
        lf.lfFaceName = [0; 32];
        for (i, c) in face.encode_utf16().take(31).enumerate() {
            lf.lfFaceName[i] = c;
        }
    }
    lf.lfCharSet = windows::Win32::Graphics::Gdi::DEFAULT_CHARSET;
    lf.lfQuality = windows::Win32::Graphics::Gdi::CLEARTYPE_QUALITY;
    let f = unsafe { CreateFontIndirectW(&lf) };
    if f.is_invalid() { None } else { Some(f) }
}

fn run_pass(hwnd: HWND, st: &Rc<RefCell<DialogState>>, pal: &'static ThemePalette) {
    let dark = theme::current_mode() == ThemeMode::Dark;

    // 1. Center over the main window first, so GetDpiForWindow reports the
    //    DPI of the monitor the dialog will really appear on.
    let owner = owner_rect(hwnd);
    let mut own = RECT::default();
    unsafe {
        if GetWindowRect(hwnd, &mut own).is_err() {
            warn("pass: GetWindowRect failed");
            return;
        }
    }
    let (w0, h0) = (own.right - own.left, own.bottom - own.top);
    let probe = owner.unwrap_or(own);
    let work = work_area_of(&probe);
    if let (Some(o), Some(wk)) = (owner, work) {
        let (x, y) = center_in(
            Rect::new(o.left, o.top, o.right, o.bottom),
            w0,
            h0,
            Rect::new(wk.left, wk.top, wk.right, wk.bottom),
        );
        unsafe {
            let _ = SetWindowPos(hwnd, None, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
        }
    }
    let dpi = dpi_of(hwnd);

    // 2. Inventory.
    let mut client = RECT::default();
    unsafe {
        if GetClientRect(hwnd, &mut client).is_err() {
            warn("pass: GetClientRect failed");
            return;
        }
    }
    let kids = collect_children(hwnd);
    let mut children: Vec<Child> = Vec::with_capacity(kids.len());
    let mut fonts: HashMap<(String, i32, i32), HFONT> = HashMap::new();
    for h in kids {
        let Some(rect) = rect_in_parent(h, hwnd) else { continue };
        let class = classify(h);
        let orig = font_of(h);
        let lf = orig.and_then(logfont_of);
        let (face, role, segoe) = match &lf {
            Some(lf) => {
                let face = face_of(lf);
                let segoe = face.eq_ignore_ascii_case("Segoe UI");
                (face, role_for(lf.lfHeight, lf.lfWeight), segoe)
            }
            None => ("Segoe UI".to_string(), Role::Body, true),
        };
        // New font: Segoe UI by role; any other face (Consolas) keeps its look,
        // scaled.
        let new_font = if segoe {
            let (pt, weight) = role_font(role);
            let hgt = metrics::font_height_px(pt, dpi);
            let k = (face.clone(), hgt, weight);
            if let Some(f) = fonts.get(&k) {
                Some(*f)
            } else {
                let f = make_font("Segoe UI", hgt, weight, lf.as_ref());
                if let Some(f) = f {
                    fonts.insert(k, f);
                }
                f
            }
        } else if let Some(lf) = &lf {
            let hgt = if lf.lfHeight < 0 { -scale(-lf.lfHeight, dpi) } else { scale(lf.lfHeight, dpi) };
            let k = (face.clone(), hgt, lf.lfWeight);
            if let Some(f) = fonts.get(&k) {
                Some(*f)
            } else {
                let f = make_font(&face, hgt, lf.lfWeight, Some(lf));
                if let Some(f) = f {
                    fonts.insert(k, f);
                }
                f
            }
        } else {
            None
        };
        if class != Class::Frame {
            if let Some(f) = new_font {
                unsafe {
                    SendMessageW(h, WM_SETFONT, Some(WPARAM(f.0 as usize)), Some(LPARAM(1)));
                }
            }
        }
        children.push(Child {
            hwnd: h,
            class,
            rect,
            text: window_text(h),
            role,
            segoe,
            prefix_off: style_of(h) & SS_NOPREFIX != 0,
            font: new_font.or(orig),
        });
    }
    {
        let mut s = st.borrow_mut();
        s.fonts.extend(fonts.values().copied());
        for c in &children {
            s.roles.insert(key(c.hwnd), c.role);
        }
    }

    // 3. Layout at the target DPI.
    let items: Vec<LayoutItem> = children
        .iter()
        .map(|c| {
            let r = c.rect.scaled(dpi);
            let (kind, text_w) = match c.class {
                Class::Label => (ItemKind::Label, measure(c.hwnd, c.font, &c.text, 0, c.prefix_off).0),
                Class::Check => (ItemKind::Check, measure(c.hwnd, c.font, &c.text, 0, false).0),
                Class::Push => (ItemKind::PushButton, measure(c.hwnd, c.font, &c.text, 0, false).0),
                Class::Edit { .. } | Class::Combo | Class::List => (ItemKind::Input, 0),
                Class::Frame if c.rect.h() <= 2 || c.rect.w() <= 2 => (ItemKind::Separator, 0),
                _ => (ItemKind::Other, 0),
            };
            let r = if kind == ItemKind::Separator {
                // Hairline: thickness follows the DPI, position stays on its row.
                Rect::new(r.l, r.t, r.r, r.t + scale(1, dpi).max(1))
            } else {
                r
            };
            LayoutItem { kind, rect: r, text_w }
        })
        .collect();
    let cw = scale(client.right - client.left, dpi);
    let ch = scale(client.bottom - client.top, dpi);
    let result = {
        let kids = &children;
        let mut wrap = |i: usize, width: i32| {
            let c = &kids[i];
            measure(c.hwnd, c.font, &c.text, width.max(1), c.prefix_off).1
        };
        layout(&items, cw, ch, dpi, &mut wrap)
    };

    // 4. Apply.
    for (i, c) in children.iter().enumerate() {
        let r = result.rects[i];
        // nwg centers the text of labels / inputs with a non-client band above
        // and below it and never repaints that band when the control is
        // resized: paint it from the palette.
        let nc_fill = match c.class {
            Class::Label => Some(NcFill::Window),
            Class::Edit { multiline: false } | Class::Combo => Some(NcFill::Input),
            _ => None,
        };
        if let Some(kind) = nc_fill {
            install_nc_bands(&nwg::ControlHandle::Hwnd(c.hwnd.0 as _), kind);
        }
        let mut style = style_of(c.hwnd);
        if result.wrapped.contains(&i) && c.class == Class::Check {
            style |= BS_MULTILINE;
            unsafe {
                SetWindowLongPtrW(c.hwnd, GWL_STYLE, style as isize);
            }
        }
        unsafe {
            if let Err(e) = SetWindowPos(
                c.hwnd,
                None,
                r.l,
                r.t,
                r.w(),
                r.h(),
                SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOCOPYBITS | SWP_FRAMECHANGED,
            ) {
                warn(&format!("pass: SetWindowPos(child) failed: {e}"));
            }
        }
        if items[i].kind == ItemKind::Separator {
            set_frame_color_hwnd(c.hwnd, pal.border);
        }
    }
    // Window: client size -> outer size at this DPI, keeping the center.
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        let mut rc = RECT { left: 0, top: 0, right: result.client_w, bottom: result.client_h };
        let adj = AdjustWindowRectExForDpi(
            &mut rc,
            windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(style),
            false,
            windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE(ex),
            dpi,
        );
        if let Err(e) = adj {
            warn(&format!("pass: AdjustWindowRectExForDpi failed: {e}"));
        } else {
            let (nw, nh) = (rc.right - rc.left, rc.bottom - rc.top);
            let mut cur = RECT::default();
            let _ = GetWindowRect(hwnd, &mut cur);
            let (cx, cy) = ((cur.left + cur.right) / 2, (cur.top + cur.bottom) / 2);
            let mut x = cx - nw / 2;
            let mut y = cy - nh / 2;
            if let Some(wk) = work_area_of(&cur) {
                let (px, py) = center_in(
                    Rect::new(x, y, x + nw, y + nh),
                    nw,
                    nh,
                    Rect::new(wk.left, wk.top, wk.right, wk.bottom),
                );
                x = px;
                y = py;
            }
            if let Err(e) = SetWindowPos(hwnd, Some(HWND_TOP), x, y, nw, nh, SWP_NOACTIVATE) {
                warn(&format!("pass: SetWindowPos(window) failed: {e}"));
            }
        }
    }

    // 5. Native control themes and owner-drawn buttons.
    let primary = st.borrow().primary;
    for c in &children {
        match c.class {
            Class::Edit { multiline } if dark => {
                set_theme(c.hwnd, if multiline { w!("DarkMode_Explorer") } else { w!("DarkMode_CFD") });
            }
            Class::Combo if dark => {
                set_theme(c.hwnd, w!("DarkMode_CFD"));
                let mut info = COMBOBOXINFO { cbSize: std::mem::size_of::<COMBOBOXINFO>() as u32, ..Default::default() };
                unsafe {
                    if GetComboBoxInfo(c.hwnd, &mut info).is_ok() && !info.hwndList.is_invalid() {
                        set_theme(info.hwndList, w!("DarkMode_Explorer"));
                    }
                }
            }
            Class::List if dark => set_theme(c.hwnd, w!("DarkMode_Explorer")),
            Class::Check if dark => {
                // Visual styles have no dark check box: paint it from the palette.
                unsafe {
                    if SetWindowSubclass(c.hwnd, Some(check_proc), SUBCLASS_CHECK, 0).as_bool() {
                        let _ = InvalidateRect(Some(c.hwnd), None, true);
                    } else {
                        warn("SetWindowSubclass(check box) failed: native look kept");
                    }
                }
            }
            Class::Push => {
                let is_primary = primary == Some(key(c.hwnd));
                // Light: native look, except the default button.
                if dark || is_primary {
                    make_owner_drawn(c.hwnd);
                    st.borrow_mut().owner_drawn.push(key(c.hwnd));
                }
            }
            _ => {}
        }
        let _ = c.segoe;
    }
    unsafe {
        use windows::Win32::Graphics::Gdi::{RedrawWindow, RDW_ALLCHILDREN, RDW_ERASE, RDW_FRAME, RDW_INVALIDATE};
        let _ = RedrawWindow(Some(hwnd), None, None, RDW_INVALIDATE | RDW_ERASE | RDW_FRAME | RDW_ALLCHILDREN);
    }
}

fn make_owner_drawn(h: HWND) {
    unsafe {
        let style = (style_of(h) & !BS_TYPEMASK) | BS_OWNERDRAW;
        SetWindowLongPtrW(h, GWL_STYLE, style as isize);
        let _ = SetWindowPos(
            h,
            None,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
        if !SetWindowSubclass(h, Some(button_proc), SUBCLASS_BUTTON, 0).as_bool() {
            warn("SetWindowSubclass(button) failed: no hover feedback");
        }
        let _ = InvalidateRect(Some(h), None, true);
    }
}

fn set_frame_color_hwnd(h: HWND, color: Color) {
    set_frame_color(nwg::ControlHandle::Hwnd(h.0 as _), color);
}

// ---------------------------------------------------------------------------
// Subclasses
// ---------------------------------------------------------------------------

unsafe extern "system" fn button_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> windows::Win32::Foundation::LRESULT {
    match msg {
        WM_MOUSEMOVE | WM_MOUSELEAVE => track_hot(hwnd, msg),
        WM_NCDESTROY => {
            let _ = RemoveWindowSubclass(hwnd, Some(button_proc), SUBCLASS_BUTTON);
        }
        _ => {}
    }
    DefSubclassProc(hwnd, msg, wparam, lparam)
}

unsafe extern "system" fn check_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> windows::Win32::Foundation::LRESULT {
    const WM_PAINT: u32 = 0x000F;
    const WM_PRINTCLIENT: u32 = 0x0318;
    match msg {
        WM_ERASEBKGND => return windows::Win32::Foundation::LRESULT(1),
        WM_PAINT => {
            let mut ps = windows::Win32::Graphics::Gdi::PAINTSTRUCT::default();
            let hdc = windows::Win32::Graphics::Gdi::BeginPaint(hwnd, &mut ps);
            paint_check(hwnd, hdc);
            let _ = windows::Win32::Graphics::Gdi::EndPaint(hwnd, &ps);
            return windows::Win32::Foundation::LRESULT(0);
        }
        WM_PRINTCLIENT => {
            paint_check(hwnd, HDC(wparam.0 as *mut c_void));
            return windows::Win32::Foundation::LRESULT(0);
        }
        WM_MOUSEMOVE | WM_MOUSELEAVE => {
            track_hot(hwnd, msg);
        }
        WM_NCDESTROY => {
            let _ = RemoveWindowSubclass(hwnd, Some(check_proc), SUBCLASS_CHECK);
        }
        _ => {}
    }
    DefSubclassProc(hwnd, msg, wparam, lparam)
}

fn paint_check(hwnd: HWND, hdc: HDC) {
    use windows::Win32::Graphics::Gdi::{CreatePen, GetStockObject, LineTo, MoveToEx, NULL_BRUSH, PS_SOLID};
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetFocus, IsWindowEnabled};
    let pal = theme::current();
    let dpi = dpi_of(hwnd);
    let mut rc = RECT::default();
    unsafe {
        if GetClientRect(hwnd, &mut rc).is_err() {
            return;
        }
        let checked = SendMessageW(hwnd, 0x00F0, None, None).0 == 1; // BM_GETCHECK == BST_CHECKED
        let state = if !IsWindowEnabled(hwnd).as_bool() {
            ButtonState::Disabled
        } else if HOT.with(|h| *h.borrow()) == Some(key(hwnd)) {
            ButtonState::Hot
        } else {
            ButtonState::Normal
        };
        let c = metrics::check_colors(pal, state, checked);

        let bg = CreateSolidBrush(colorref(pal.background));
        FillRect(hdc, &rc, bg);
        let _ = DeleteObject(HGDIOBJ(bg.0));

        let font = font_of(hwnd);
        let old_f = font.map(|f| SelectObject(hdc, HGDIOBJ(f.0)));
        let text = window_text(hwnd);
        let box_sz = scale(metrics::CHECK_CHROME_DP - 4, dpi);
        let gap = scale(6, dpi);
        let text_l = rc.left + box_sz + gap;
        let multiline = style_of(hwnd) & BS_MULTILINE != 0;
        let mut wide: Vec<u16> = text.encode_utf16().collect();
        let (line_h, text_h) = {
            let mut one: Vec<u16> = "Ag".encode_utf16().collect();
            let mut r1 = RECT::default();
            DrawTextW(hdc, &mut one, &mut r1, DT_CALCRECT | DT_SINGLELINE);
            let mut r2 = RECT { left: 0, top: 0, right: (rc.right - text_l).max(1), bottom: 0 };
            if !wide.is_empty() {
                let f = if multiline { DT_CALCRECT | DT_WORDBREAK } else { DT_CALCRECT | DT_SINGLELINE };
                DrawTextW(hdc, &mut wide, &mut r2, f);
            }
            (r1.bottom - r1.top, (r2.bottom - r2.top).max(r1.bottom - r1.top))
        };
        let top = rc.top + ((rc.bottom - rc.top) - text_h) / 2;
        let box_top = top + (line_h - box_sz) / 2;
        let (bl, bt) = (rc.left + 1, box_top);

        // Box.
        let fill = CreateSolidBrush(colorref(c.fill));
        let pen = CreatePen(PS_SOLID, 1, colorref(c.border));
        let (ob, op) = (SelectObject(hdc, HGDIOBJ(fill.0)), SelectObject(hdc, HGDIOBJ(pen.0)));
        let r = scale(CORNER_DP - 1, dpi) * 2;
        let _ = RoundRect(hdc, bl, bt, bl + box_sz, bt + box_sz, r, r);
        SelectObject(hdc, ob);
        SelectObject(hdc, op);
        let _ = DeleteObject(HGDIOBJ(fill.0));
        let _ = DeleteObject(HGDIOBJ(pen.0));

        // Check mark.
        if checked {
            let w = scale(2, dpi).max(1);
            let pen = CreatePen(PS_SOLID, w, colorref(c.mark));
            let op = SelectObject(hdc, HGDIOBJ(pen.0));
            let p = |fx: i32, fy: i32| (bl + box_sz * fx / 100, bt + box_sz * fy / 100);
            let (x1, y1) = p(24, 52);
            let (x2, y2) = p(43, 72);
            let (x3, y3) = p(77, 30);
            let _ = MoveToEx(hdc, x1, y1, None);
            let _ = LineTo(hdc, x2, y2);
            let _ = LineTo(hdc, x3, y3);
            SelectObject(hdc, op);
            let _ = DeleteObject(HGDIOBJ(pen.0));
        }

        // Caption.
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, colorref(c.text));
        let mut tr = RECT { left: text_l, top, right: rc.right, bottom: top + text_h };
        if !wide.is_empty() {
            let f = if multiline { DT_WORDBREAK | DT_HIDEPREFIX } else { DT_SINGLELINE | DT_VCENTER | DT_HIDEPREFIX };
            DrawTextW(hdc, &mut wide, &mut tr, f);
        }
        // Keyboard focus: outline around the caption.
        if GetFocus() == hwnd {
            let pen = CreatePen(PS_SOLID, 1, colorref(pal.text_secondary));
            let op = SelectObject(hdc, HGDIOBJ(pen.0));
            let ob = SelectObject(hdc, GetStockObject(NULL_BRUSH));
            let right = (text_l + measure_w(hdc, &text) + 3).min(rc.right - 1);
            let _ = RoundRect(hdc, text_l - 3, top - 2, right, top + text_h + 2, 4, 4);
            SelectObject(hdc, ob);
            SelectObject(hdc, op);
            let _ = DeleteObject(HGDIOBJ(pen.0));
        }
        if let Some(o) = old_f {
            SelectObject(hdc, o);
        }
    }
}

fn measure_w(hdc: HDC, text: &str) -> i32 {
    let mut wide: Vec<u16> = text.encode_utf16().collect();
    if wide.is_empty() {
        return 0;
    }
    let mut r = RECT::default();
    unsafe {
        DrawTextW(hdc, &mut wide, &mut r, DT_CALCRECT | DT_SINGLELINE);
    }
    r.right - r.left
}

/// Hover tracking shared by owner-drawn buttons and painted check boxes.
unsafe fn track_hot(hwnd: HWND, msg: u32) {
    let k = key(hwnd);
    match msg {
        WM_MOUSEMOVE => {
            if HOT.with(|h| *h.borrow()) != Some(k) {
                HOT.with(|h| *h.borrow_mut() = Some(k));
                let mut tme = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                let _ = TrackMouseEvent(&mut tme);
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
        }
        _ => {
            if HOT.with(|h| *h.borrow()) == Some(k) {
                HOT.with(|h| *h.borrow_mut() = None);
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
        }
    }
}

/// Fill of the non-client bands that nwg leaves above and below the text of a
/// label or a single-line input (it centres the text vertically with them).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NcFill {
    /// Dialog window background (labels of the shared dialog pass).
    Window = 0,
    /// Input background with the 1 px input border redrawn (shared dialog pass).
    Input = 1,
    /// Surface of the unlock / lock-screen card (its labels).
    CardSurface = 2,
}

impl NcFill {
    fn from_data(data: usize) -> NcFill {
        match data {
            1 => NcFill::Input,
            2 => NcFill::CardSurface,
            _ => NcFill::Window,
        }
    }
}

/// Colour of the bands, and of the 1 px top/bottom border to redraw over them
/// (inputs only), for `kind` under the theme `mode`. Resolved at paint time so
/// a theme switch is followed.
pub(crate) fn nc_colors(kind: NcFill, mode: ThemeMode) -> (Color, Option<Color>) {
    let pal = theme::get_palette(mode);
    match kind {
        NcFill::Window => (pal.background, None),
        NcFill::Input => (pal.input_background, Some(pal.input_border)),
        NcFill::CardSurface => (crate::ui::lock_card::CardColors::for_mode(mode).card_bg, None),
    }
}

/// Make the bands of the label / single-line input `h` follow the palette.
/// Idempotent (same subclass id: only the fill kind is updated).
pub(crate) fn install_nc_bands(h: &nwg::ControlHandle, kind: NcFill) -> bool {
    let Some(hwnd) = hwnd_of(*h) else {
        warn("install_nc_bands: control has no HWND");
        return false;
    };
    // SAFETY: subclassing a window of this thread; the proc removes itself on WM_NCDESTROY.
    let ok = unsafe { SetWindowSubclass(hwnd, Some(nc_proc), SUBCLASS_NC, kind as usize).as_bool() };
    if !ok {
        warn("SetWindowSubclass(nc band) failed: stale band possible");
    } else {
        // SAFETY: repaint the frame so the bands are filled right away.
        unsafe {
            let _ = windows::Win32::Graphics::Gdi::RedrawWindow(
                Some(hwnd),
                None,
                None,
                windows::Win32::Graphics::Gdi::RDW_INVALIDATE | windows::Win32::Graphics::Gdi::RDW_FRAME,
            );
        }
    }
    ok
}

/// True when `h` carries the band-painting subclass.
#[cfg(test)]
pub(crate) fn has_nc_bands(h: &nwg::ControlHandle) -> Option<NcFill> {
    let hwnd = hwnd_of(*h)?;
    let mut data = 0usize;
    // SAFETY: reads the subclass reference data of a window of this thread.
    let found = unsafe {
        windows::Win32::UI::Shell::GetWindowSubclass(hwnd, Some(nc_proc), SUBCLASS_NC, Some(&mut data)).as_bool()
    };
    found.then(|| NcFill::from_data(data))
}

/// Paints the non-client bands above and below the text of nwg labels and
/// inputs (`data`: an `NcFill`).
unsafe extern "system" fn nc_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    data: usize,
) -> windows::Win32::Foundation::LRESULT {
    const WM_NCPAINT: u32 = 0x0085;
    if msg == WM_NCPAINT {
        let r = DefSubclassProc(hwnd, msg, wparam, lparam);
        let (color, border) = nc_colors(NcFill::from_data(data), theme::current_mode());
        let mut win = RECT::default();
        let mut cl = RECT::default();
        if GetWindowRect(hwnd, &mut win).is_ok() && GetClientRect(hwnd, &mut cl).is_ok() {
            let mut pts = [
                windows::Win32::Foundation::POINT { x: cl.left, y: cl.top },
                windows::Win32::Foundation::POINT { x: cl.right, y: cl.bottom },
            ];
            MapWindowPoints(Some(hwnd), None, &mut pts);
            let (ww, wh) = (win.right - win.left, win.bottom - win.top);
            let (ct, cb) = (pts[0].y - win.top, pts[1].y - win.top);
            let hdc = windows::Win32::Graphics::Gdi::GetWindowDC(Some(hwnd));
            if !hdc.is_invalid() {
                let brush = CreateSolidBrush(colorref(color));
                let top = RECT { left: 0, top: 0, right: ww, bottom: ct };
                let bottom = RECT { left: 0, top: cb, right: ww, bottom: wh };
                if top.bottom > top.top {
                    FillRect(hdc, &top, brush);
                }
                if bottom.bottom > bottom.top {
                    FillRect(hdc, &bottom, brush);
                }
                let _ = DeleteObject(HGDIOBJ(brush.0));
                if let Some(border) = border {
                    // The bands covered the top and bottom border: redraw them.
                    let line = CreateSolidBrush(colorref(border));
                    FillRect(hdc, &RECT { left: 0, top: 0, right: ww, bottom: 1 }, line);
                    FillRect(hdc, &RECT { left: 0, top: wh - 1, right: ww, bottom: wh }, line);
                    let _ = DeleteObject(HGDIOBJ(line.0));
                }
                ReleaseDC(Some(hwnd), hdc);
            }
        }
        return r;
    }
    if msg == WM_NCDESTROY {
        let _ = RemoveWindowSubclass(hwnd, Some(nc_proc), SUBCLASS_NC);
    }
    DefSubclassProc(hwnd, msg, wparam, lparam)
}

unsafe extern "system" fn frame_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> windows::Win32::Foundation::LRESULT {
    match msg {
        WM_ERASEBKGND => {
            if let Some(c) = FRAME_FILL.with(|f| f.borrow().get(&key(hwnd)).copied()) {
                let mut rc = RECT::default();
                if GetClientRect(hwnd, &mut rc).is_ok() {
                    let brush = CreateSolidBrush(COLORREF(c));
                    FillRect(HDC(wparam.0 as *mut c_void), &rc, brush);
                    let _ = DeleteObject(HGDIOBJ(brush.0));
                    return windows::Win32::Foundation::LRESULT(1);
                }
            }
        }
        WM_NCDESTROY => {
            FRAME_FILL.with(|f| f.borrow_mut().remove(&key(hwnd)));
            let _ = RemoveWindowSubclass(hwnd, Some(frame_proc), SUBCLASS_FRAME);
        }
        _ => {}
    }
    DefSubclassProc(hwnd, msg, wparam, lparam)
}

// ---------------------------------------------------------------------------
// Owner-drawn push button
// ---------------------------------------------------------------------------

fn draw_button(dis: &DRAWITEMSTRUCT, primary: bool, pal: &ThemePalette) {
    let hwnd = dis.hwndItem;
    let hdc = dis.hDC;
    let rc = dis.rcItem;
    let state = if dis.itemState.0 & ODS_DISABLED != 0 {
        ButtonState::Disabled
    } else if dis.itemState.0 & ODS_SELECTED != 0 {
        ButtonState::Pressed
    } else if HOT.with(|h| *h.borrow()) == Some(key(hwnd)) {
        ButtonState::Hot
    } else {
        ButtonState::Normal
    };
    let kind = if primary { ButtonKind::Primary } else { ButtonKind::Secondary };
    let c = button_colors(pal, kind, state);
    let dpi = dpi_of(hwnd);
    unsafe {
        // Window background first, so the rounded corners blend in.
        let bg = CreateSolidBrush(colorref(pal.background));
        FillRect(hdc, &rc, bg);
        let _ = DeleteObject(HGDIOBJ(bg.0));

        let fill = CreateSolidBrush(colorref(c.fill));
        let pen = windows::Win32::Graphics::Gdi::CreatePen(
            windows::Win32::Graphics::Gdi::PS_SOLID,
            1,
            colorref(c.border),
        );
        let (old_b, old_p) = (SelectObject(hdc, HGDIOBJ(fill.0)), SelectObject(hdc, HGDIOBJ(pen.0)));
        let radius = scale(CORNER_DP, dpi) * 2;
        let _ = RoundRect(hdc, rc.left, rc.top, rc.right, rc.bottom, radius, radius);
        SelectObject(hdc, old_b);
        SelectObject(hdc, old_p);
        let _ = DeleteObject(HGDIOBJ(fill.0));
        let _ = DeleteObject(HGDIOBJ(pen.0));

        // Caption.
        let mut text = {
            let mut t: Vec<u16> = window_text(hwnd).encode_utf16().collect();
            t.push(0);
            t.pop();
            t
        };
        let font = font_of(hwnd);
        let old_f = font.map(|f| SelectObject(hdc, HGDIOBJ(f.0)));
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, colorref(c.text));
        let mut tr = RECT { left: rc.left + 2, top: rc.top, right: rc.right - 2, bottom: rc.bottom };
        DrawTextW(hdc, &mut text, &mut tr, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_HIDEPREFIX);
        if let Some(o) = old_f {
            SelectObject(hdc, o);
        }

        // Keyboard focus: inner outline in the caption color.
        if dis.itemState.0 & ODS_FOCUS != 0 {
            let inset = scale(3, dpi);
            let pen = windows::Win32::Graphics::Gdi::CreatePen(
                windows::Win32::Graphics::Gdi::PS_SOLID,
                1,
                colorref(if primary { c.text } else { pal.text_primary }),
            );
            let old_p = SelectObject(hdc, HGDIOBJ(pen.0));
            let old_b = SelectObject(hdc, windows::Win32::Graphics::Gdi::GetStockObject(windows::Win32::Graphics::Gdi::NULL_BRUSH));
            let r2 = scale(CORNER_DP, dpi);
            let _ = RoundRect(hdc, rc.left + inset, rc.top + inset, rc.right - inset, rc.bottom - inset, r2, r2);
            SelectObject(hdc, old_b);
            SelectObject(hdc, old_p);
            let _ = DeleteObject(HGDIOBJ(pen.0));
        }
    }
}
