// Editor surface: line spacing, current-line band, dark scrollbars.
//
// Three things the RichEdit cannot do on its own are done here, all with
// documented messages and without touching the saved text:
//
//   * Line spacing. Every paragraph gets PARAFORMAT2 `dyLineSpacing` = 1.5
//     lines with `bLineSpacingRule` = 5 ("multiple of a line", in 20ths).
//     Measured on RICHEDIT50W (see `live_*` tests): with rule 5 the extra
//     space is added BELOW the glyphs, so the first pixel row of a line, and
//     therefore the line numbers the gutter draws at `EM_POSFROMCHAR`, stay
//     where they were; only the pitch between two lines grows, from the font
//     height to `line_pitch(font height)`. (Rules 3 and 4 push the glyphs
//     down instead and would misalign the gutter, so they are not used.)
//     `WM_SETTEXT` resets the paragraph format, hence the subclass below
//     re-applies it after every text replacement.
//
//   * Current-line band. RichEdit has no such feature and paints its own
//     background, so after each `WM_PAINT` the band is alpha-blended over the
//     freshly painted update rectangle, in the editor text colour at 6 %.
//     Text pixels already have that colour, so they do not change; the
//     background moves 6 % towards it; a selection (the band is not drawn
//     while the selection is non-empty) and the occurrence highlights
//     (character background) are only tinted by the same 6 %. Nothing is
//     written to the document: no character or paragraph attribute changes
//     when the caret moves.
//
//   * Dark scrollbars: `SetWindowTheme(DarkMode_Explorer)` in the Dark theme.
//
// The decisions (pitch, band rectangle, colours, struct bytes) are pure and
// unit-tested; the Win32 calls are thin wrappers around them.

#[allow(unused_imports)]
use super::*;

use std::cell::Cell;
use std::sync::atomic::AtomicBool;

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    AlphaBlend, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, DeleteDC, DeleteObject,
    GetDC, GetDeviceCaps, GetTextMetricsW, GetUpdateRect, InvalidateRect, RedrawWindow, ReleaseDC,
    SelectObject, SetPixel, AC_SRC_OVER, BLENDFUNCTION, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS,
    FONT_CHARSET, HGDIOBJ, LOGPIXELSY, OUT_DEFAULT_PRECIS, RDW_ERASE, RDW_FRAME, RDW_INVALIDATE,
    TEXTMETRICW,
};
use windows::core::Interface;
use windows::Win32::UI::Controls::RichEdit::{ITextDocument, EM_GETOLEINTERFACE, tomResume, tomSuspend};
use windows::Win32::UI::Controls::SetWindowTheme;
use windows::Win32::UI::Shell::DefSubclassProc;
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, GetParent, HideCaret, PostMessageW, SendMessageW, ShowCaret,
};

use crate::theme::{self, Color, ThemeMode, ThemePalette};

// ---------------------------------------------------------------------
// Pure helpers (no Win32 calls, unit-tested below)
// ---------------------------------------------------------------------

/// `dyLineSpacing` in 20ths of a line for `bLineSpacingRule` 5: 30/20 = 1.5.
pub(super) const LINE_SPACING_20THS: i32 = 30;
/// PARAFORMAT2 `bLineSpacingRule`: `dyLineSpacing` is a multiple of the single
/// line height, in 20ths.
const LINE_SPACING_RULE_MULTIPLE: u8 = 5;
/// PFM_LINESPACING.
const PFM_LINESPACING: u32 = 0x0000_0100;
/// sizeof(PARAFORMAT2) with the 32-entry tab array.
const PARAFORMAT2_SIZE: usize = 188;
const PF_OFFSET_MASK: usize = 4;
const PF_OFFSET_LINE_SPACING: usize = 164;
const PF_OFFSET_LINE_SPACING_RULE: usize = 170;

/// Alpha (out of 255) of the current-line band: 15/255 = 5.9 %.
pub(super) const CURRENT_LINE_ALPHA: u8 = 15;

/// Distance in pixels between the tops of two consecutive lines once the
/// spacing is applied, for a font whose single line is `natural` pixels
/// (`tmHeight + tmExternalLeading`). RichEdit rounds half up, which the live
/// tests confirm for 18, 24, 33 and 41 px fonts.
pub(super) fn line_pitch(natural: i32) -> i32 {
    (natural.max(1) * LINE_SPACING_20THS + 10) / 20
}

/// The PARAFORMAT2 bytes that set the line spacing and nothing else.
pub(super) fn line_spacing_paraformat() -> [u8; PARAFORMAT2_SIZE] {
    let mut pf = [0u8; PARAFORMAT2_SIZE];
    pf[0..4].copy_from_slice(&(PARAFORMAT2_SIZE as u32).to_le_bytes());
    pf[PF_OFFSET_MASK..PF_OFFSET_MASK + 4].copy_from_slice(&PFM_LINESPACING.to_le_bytes());
    pf[PF_OFFSET_LINE_SPACING..PF_OFFSET_LINE_SPACING + 4]
        .copy_from_slice(&LINE_SPACING_20THS.to_le_bytes());
    pf[PF_OFFSET_LINE_SPACING_RULE] = LINE_SPACING_RULE_MULTIPLE;
    pf
}

/// Colour the band tints with: the editor text colour, so text pixels are
/// unchanged by the blend.
pub(super) fn current_line_overlay(pal: &ThemePalette) -> Color {
    pal.editor_text
}

/// What `base` (the editor or the gutter background) looks like under the band.
pub(super) fn current_line_background(base: Color, pal: &ThemePalette) -> Color {
    base.blend_alpha(&current_line_overlay(pal), CURRENT_LINE_ALPHA)
}

/// The band of the caret line as `(top, bottom)` client pixels, `None` when
/// a selection exists (the band would compete with the selection colour) or
/// the line is entirely outside the `client_h` pixels shown.
pub(super) fn current_line_band(
    selection_empty: bool,
    line_top: i32,
    pitch: i32,
    client_h: i32,
) -> Option<(i32, i32)> {
    if !selection_empty || pitch <= 0 {
        return None;
    }
    let bottom = line_top + pitch;
    if bottom <= 0 || line_top >= client_h {
        return None;
    }
    Some((line_top, bottom))
}

/// Single value comparing "where the caret is": line number and whether a
/// selection exists. Changing it changes what the band and the gutter show.
pub(super) fn caret_key(line: usize, selection_empty: bool) -> usize {
    (line << 1) | usize::from(!selection_empty)
}

/// The caret end of the selection `min..max`: RichEdit reports the selection
/// ordered, without saying which end the caret is on. `char_at_caret` is the
/// character under the system caret (None when the editor does not own it);
/// the caret is on `min` only when that character is `min`, otherwise on
/// `max` (also an empty selection, and a selection made by the code).
pub(super) fn active_end(min: usize, max: usize, char_at_caret: Option<usize>) -> usize {
    if min != max && char_at_caret == Some(min) {
        min
    } else {
        max
    }
}

// ---------------------------------------------------------------------
// Win32 plumbing
// ---------------------------------------------------------------------

const WM_SETTEXT: u32 = 0x000C;
const WM_PAINT: u32 = 0x000F;
const WM_SETREDRAW: u32 = 0x000B;
const WM_KEYDOWN: u32 = 0x0100;
const WM_KEYUP: u32 = 0x0101;
const WM_CHAR: u32 = 0x0102;
const WM_MOUSEMOVE: u32 = 0x0200;
const WM_LBUTTONDOWN: u32 = 0x0201;
const WM_LBUTTONUP: u32 = 0x0202;
const WM_CUT: u32 = 0x0300;
const WM_PASTE: u32 = 0x0302;
const WM_CLEAR: u32 = 0x0303;
const WM_UNDO: u32 = 0x0304;
const MK_LBUTTON: usize = 0x0001;
const EM_GETMODIFY: u32 = 0x00B8;
const EM_SETMODIFY: u32 = 0x00B9;
const EM_SETSEL: u32 = 0x00B1;
const EM_GETLINECOUNT: u32 = 0x00BA;
const EM_LINEINDEX: u32 = 0x00BB;
const EM_CANUNDO: u32 = 0x00C6;
const EM_UNDO: u32 = 0x00C7;
const EM_REPLACESEL: u32 = 0x00C2;
const EM_EMPTYUNDOBUFFER: u32 = 0x00CD;
const EM_LINEFROMCHAR: u32 = 0x00C9;
const EM_POSFROMCHAR: u32 = 0x00D6;
const EM_CHARFROMPOS: u32 = 0x00D7;
const EM_EXGETSEL: u32 = 0x0434;
const EM_EXSETSEL: u32 = 0x0437;
const EM_GETEVENTMASK: u32 = 0x043B;
const EM_SETEVENTMASK: u32 = 0x0445;
const EM_SETPARAFORMAT: u32 = 0x0447;
const EM_REDO: u32 = 0x0454;
const ENM_CHANGE: isize = 0x0000_0001;
/// WM_APP+3 on the main window: deferred gutter repaint (see events.rs).
const WM_APP_GUTTER_REPAINT: u32 = 0x8003;
/// Id of the editor subclass (the key handler uses 0x10001).
const EDITOR_SURFACE_HANDLER_ID: usize = 0x10002;

/// RichEdit `CHARRANGE`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct CharRange {
    min: i32,
    max: i32,
}

static WARNED_SPACING: AtomicBool = AtomicBool::new(false);
static WARNED_GEOMETRY: AtomicBool = AtomicBool::new(false);
static WARNED_CARET: AtomicBool = AtomicBool::new(false);
static WARNED_SCROLLBAR_THEME: AtomicBool = AtomicBool::new(false);

fn warn_once(flag: &AtomicBool, detail: &str) {
    if !flag.swap(true, std::sync::atomic::Ordering::SeqCst) {
        eprintln!("WARN editor_surface: {detail}");
    }
}

fn send(hwnd: HWND, msg: u32, wparam: usize, lparam: isize) -> isize {
    unsafe { SendMessageW(hwnd, msg, Some(WPARAM(wparam)), Some(LPARAM(lparam))).0 }
}

/// Apply the line spacing to every paragraph of the editor. The selection,
/// the undo state, the modified flag and the EN_CHANGE notification are left
/// as they were: this is presentation, not an edit.
pub(super) fn apply_line_spacing(hwnd: HWND) -> Result<(), String> {
    let could_undo = send(hwnd, EM_CANUNDO, 0, 0) != 0;
    let was_modified = send(hwnd, EM_GETMODIFY, 0, 0) != 0;
    let event_mask = send(hwnd, EM_GETEVENTMASK, 0, 0);
    let mut selection = CharRange::default();
    send(hwnd, EM_EXGETSEL, 0, &mut selection as *mut _ as isize);

    send(hwnd, EM_SETEVENTMASK, 0, event_mask & !ENM_CHANGE);
    send(hwnd, WM_SETREDRAW, 0, 0);
    send(hwnd, EM_SETSEL, 0, -1);
    let mut pf = line_spacing_paraformat();
    let applied = send(hwnd, EM_SETPARAFORMAT, 0, pf.as_mut_ptr() as isize);
    send(hwnd, EM_EXSETSEL, 0, &selection as *const _ as isize);
    send(hwnd, WM_SETREDRAW, 1, 0);
    // Turning the redraw back on repaints nothing, and whatever was invalid
    // before it was turned off (the text WM_SETTEXT just replaced) is no
    // longer pending: without this the previous text stays on screen.
    let redrawn = unsafe { RedrawWindow(Some(hwnd), None, None, RDW_INVALIDATE | RDW_ERASE | RDW_FRAME) };
    if !redrawn.as_bool() {
        eprintln!("ERROR editor_surface: RedrawWindow failed after the line spacing; the editor may show stale text");
    }

    if !could_undo {
        send(hwnd, EM_EMPTYUNDOBUFFER, 0, 0);
    }
    send(hwnd, EM_SETMODIFY, usize::from(was_modified), 0);
    send(hwnd, EM_SETEVENTMASK, 0, event_mask);

    if applied == 0 {
        return Err("EM_SETPARAFORMAT refused the line spacing".to_string());
    }
    Ok(())
}

const WM_SETFONT: u32 = 0x0030;

/// Give the editor a new font (`hfont`, a GDI font the caller keeps alive),
/// the zoom and the preferences (A2). Like the startup font, it goes in with
/// `WM_SETFONT`; the text colour and the line spacing it may reset are put
/// back. The text, the selection, the modified flag, the undo history and
/// the EN_CHANGE notification are left as they were: this is presentation,
/// not an edit. `Err` means the editor did NOT take the font (it keeps the
/// one it had); once it took it, a later failure is reported here and the
/// font counts as applied.
pub(super) fn set_font(hwnd: HWND, hfont: isize) -> Result<(), String> {
    // WM_SETFONT turns the text colour back to automatic, and putting it
    // back (EM_SETCHARFORMAT) or the spacing (EM_SETPARAFORMAT) would each
    // be an undo step: Ctrl+Z would then revert the colour instead of the
    // typing (`live_set_font_*`). The undo history is suspended meanwhile.
    let doc = text_document(hwnd)?;
    unsafe { doc.Undo(tomSuspend.0) }.map_err(|e| format!("the undo history could not be suspended ({e})"))?;
    let was_modified = send(hwnd, EM_GETMODIFY, 0, 0) != 0;
    let event_mask = send(hwnd, EM_GETEVENTMASK, 0, 0);
    send(hwnd, EM_SETEVENTMASK, 0, event_mask & !ENM_CHANGE);
    // No redraw here: apply_line_spacing repaints the whole control.
    send(hwnd, WM_SETFONT, hfont as usize, 0);
    apply_editor_colors(hwnd);
    if let Err(e) = apply_line_spacing(hwnd) {
        warn_once(&WARNED_SPACING, &format!("{e}; the text keeps single spacing"));
    }
    // EM_SETCHARFORMAT raises the modified flag: a font is not an edit.
    send(hwnd, EM_SETMODIFY, usize::from(was_modified), 0);
    send(hwnd, EM_SETEVENTMASK, 0, event_mask);
    if let Err(e) = unsafe { doc.Undo(tomResume.0) } {
        eprintln!("ERROR editor_surface: the undo history could not be resumed after a font change ({e}); edits may not be undoable");
    }
    Ok(())
}

/// The TOM document of the editor (EM_GETOLEINTERFACE; no OleInitialize
/// needed, see build.rs).
fn text_document(hwnd: HWND) -> Result<ITextDocument, String> {
    let mut raw: *mut core::ffi::c_void = core::ptr::null_mut();
    let ok = send(hwnd, EM_GETOLEINTERFACE, 0, &mut raw as *mut _ as isize);
    if ok == 0 || raw.is_null() {
        return Err("EM_GETOLEINTERFACE gave no interface".to_string());
    }
    // The message hands over a reference (AddRef'd): `from_raw` owns it.
    let ole = unsafe { windows::core::IUnknown::from_raw(raw) };
    ole.cast::<ITextDocument>().map_err(|e| format!("ITextDocument unavailable ({e})"))
}

/// CHARFORMAT2W layout (pack(4), 116 bytes), see highlight.rs.
const CF_SIZE: usize = 116;
const CF_OFFSET_HEIGHT: usize = 12;
const CF_OFFSET_CHARSET: usize = 24;
const CF_OFFSET_FACE: usize = 26;
const CF_FACE_LEN: usize = 32;
const CF_OFFSET_WEIGHT: usize = 90;
const CFM_SIZE: u32 = 0x8000_0000;
const CFM_FACE: u32 = 0x2000_0000;
const CFM_CHARSET: u32 = 0x0800_0000;
const CFM_WEIGHT: u32 = 0x0040_0000;
const EM_GETCHARFORMAT: u32 = 0x043A;
const SCF_DEFAULT: usize = 0;

/// Pixel height of the control's default font from its CHARFORMAT2 height in
/// twips (1/1440 inch) at `dpi`, rounded to the nearest pixel.
pub(super) fn twips_to_px(twips: i32, dpi: i32) -> i32 {
    (twips * dpi + 720) / 1440
}

/// The editor's default font as RichEdit holds it, built as a GDI font.
/// (`WM_GETFONT` is not answered by RichEdit with the font it was given.)
fn editor_default_font(hwnd: HWND, dpi: i32) -> Option<HGDIOBJ> {
    let mut cf = [0u8; CF_SIZE];
    cf[0..4].copy_from_slice(&(CF_SIZE as u32).to_le_bytes());
    cf[4..8].copy_from_slice(&(CFM_SIZE | CFM_FACE | CFM_CHARSET | CFM_WEIGHT).to_le_bytes());
    send(hwnd, EM_GETCHARFORMAT, SCF_DEFAULT, cf.as_mut_ptr() as isize);
    let mask = u32::from_le_bytes(cf[4..8].try_into().ok()?);
    if mask & (CFM_SIZE | CFM_FACE) != (CFM_SIZE | CFM_FACE) {
        return None;
    }
    let twips = i32::from_le_bytes(cf[CF_OFFSET_HEIGHT..CF_OFFSET_HEIGHT + 4].try_into().ok()?);
    let mut face = [0u16; CF_FACE_LEN];
    for (i, c) in face.iter_mut().enumerate() {
        let o = CF_OFFSET_FACE + i * 2;
        *c = u16::from_le_bytes([cf[o], cf[o + 1]]);
    }
    let weight = if mask & CFM_WEIGHT != 0 {
        u16::from_le_bytes([cf[CF_OFFSET_WEIGHT], cf[CF_OFFSET_WEIGHT + 1]]) as i32
    } else {
        400
    };
    let charset = if mask & CFM_CHARSET != 0 { cf[CF_OFFSET_CHARSET] } else { 1 };
    let font = unsafe {
        CreateFontW(
            -twips_to_px(twips, dpi), 0, 0, 0, weight, 0, 0, 0,
            FONT_CHARSET(charset), OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY, 0,
            windows::core::PCWSTR(face.as_ptr()),
        )
    };
    (!font.is_invalid()).then_some(font.into())
}

/// Height in pixels of one unspaced line of the editor font.
fn natural_line_height(hwnd: HWND) -> Option<i32> {
    unsafe {
        let hdc = GetDC(Some(hwnd));
        if hdc.is_invalid() {
            return None;
        }
        let dpi = GetDeviceCaps(Some(hdc), LOGPIXELSY);
        let font = editor_default_font(hwnd, dpi);
        let old = font.map(|f| SelectObject(hdc, f));
        let mut tm = TEXTMETRICW::default();
        let ok = GetTextMetricsW(hdc, &mut tm).as_bool();
        if let Some(old) = old {
            SelectObject(hdc, old);
        }
        if let Some(f) = font {
            let _ = DeleteObject(f);
        }
        ReleaseDC(Some(hwnd), hdc);
        (ok && font.is_some()).then_some(tm.tmHeight + tm.tmExternalLeading)
    }
}

/// Distance between two line tops of the editor right now (font height times
/// the spacing). `None`, reported once, when the font metrics are unreadable.
pub(super) fn editor_line_pitch(hwnd: HWND) -> Option<i32> {
    let pitch = natural_line_height(hwnd).map(line_pitch);
    if pitch.is_none() {
        warn_once(&WARNED_GEOMETRY, "text metrics of the editor font are unreadable; the gutter is not painted");
    }
    pitch
}

/// Client Y of the first pixel row of `line`, `None` when RichEdit has no
/// position for it (EM_POSFROMCHAR answers -1/-1 past the text).
fn line_top(hwnd: HWND, line: usize) -> Option<i32> {
    let index = send(hwnd, EM_LINEINDEX, line, 0);
    if index < 0 {
        return None;
    }
    let mut p = windows::Win32::Foundation::POINT::default();
    send(hwnd, EM_POSFROMCHAR, &mut p as *mut _ as usize, index);
    (!(p.x == -1 && p.y == -1)).then_some(p.y)
}

/// Where the caret is: `(line, selection_empty)`.
pub(super) fn caret_position(hwnd: HWND) -> (usize, bool) {
    let mut sel = CharRange::default();
    send(hwnd, EM_EXGETSEL, 0, &mut sel as *mut _ as isize);
    let line = send(hwnd, EM_LINEFROMCHAR, sel.min.max(0) as usize, 0).max(0) as usize;
    (line, sel.min == sel.max)
}

/// The selection `(min, max)` and the caret offset (its active end, see
/// `active_end`), in UTF-16 code units as RichEdit counts them.
pub(super) fn selection_and_caret(hwnd: HWND) -> (usize, usize, usize) {
    let mut sel = CharRange::default();
    send(hwnd, EM_EXGETSEL, 0, &mut sel as *mut _ as isize);
    let (min, max) = (sel.min.max(0) as usize, sel.max.max(0) as usize);
    let at_caret = if min != max { char_at_caret(hwnd) } else { None };
    (min, max, active_end(min, max, at_caret))
}

/// The character under the system caret, when the editor has the focus
/// (otherwise the thread's caret, if any, belongs to another control).
/// RichEdit keeps its caret on the active end of a selection, even while it
/// hides it (`live_caret_is_on_the_active_end_of_the_selection`).
fn char_at_caret(hwnd: HWND) -> Option<usize> {
    if unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetFocus() } != hwnd {
        return None;
    }
    let mut p = windows::Win32::Foundation::POINT::default();
    if let Err(e) = unsafe { windows::Win32::UI::WindowsAndMessaging::GetCaretPos(&mut p) } {
        warn_once(&WARNED_CARET, &format!("GetCaretPos failed ({e}); the status bar shows the end of the selection"));
        return None;
    }
    // RichEdit takes a POINTL in lParam (not the EDIT control's packed x, y).
    let pt = windows::Win32::Foundation::POINTL { x: p.x, y: p.y };
    let index = send(hwnd, EM_CHARFROMPOS, 0, &pt as *const _ as isize);
    usize::try_from(index).ok()
}

/// Pitch of `line`: the measured distance to the next line when there is
/// one, otherwise the computed `line_pitch` of the editor font.
fn measured_pitch(hwnd: HWND, line: usize, top: i32, line_count: usize) -> Option<i32> {
    if line + 1 < line_count {
        if let Some(next) = line_top(hwnd, line + 1) {
            if next > top {
                return Some(next - top);
            }
        }
    }
    natural_line_height(hwnd).map(line_pitch)
}

/// The band to draw for the caret line, in editor client pixels. Shared by
/// the editor overlay and the gutter so both always show the same line.
pub(super) fn current_line_rect(hwnd: HWND) -> Option<(i32, i32)> {
    let (line, empty) = caret_position(hwnd);
    if !empty {
        return None;
    }
    let top = match line_top(hwnd, line) {
        Some(t) => t,
        None => {
            warn_once(&WARNED_GEOMETRY, &format!("no position for caret line {line}; no current-line band"));
            return None;
        }
    };
    let line_count = send(hwnd, EM_GETLINECOUNT, 0, 0).max(1) as usize;
    let pitch = measured_pitch(hwnd, line, top, line_count)?;
    let mut client = RECT::default();
    unsafe { GetClientRect(hwnd, &mut client).ok()? };
    current_line_band(true, top, pitch, client.bottom - client.top)
}

/// Blend the band into the already painted `update` rectangle.
fn overlay_band(hwnd: HWND, update: &RECT) {
    let Some((top, bottom)) = current_line_rect(hwnd) else { return };
    let mut client = RECT::default();
    if unsafe { GetClientRect(hwnd, &mut client) }.is_err() {
        return;
    }
    let (y0, y1) = (top.max(update.top), bottom.min(update.bottom));
    let (x0, x1) = (update.left.max(client.left), update.right.min(client.right));
    if y0 >= y1 || x0 >= x1 {
        return;
    }
    let pal = theme::current();
    unsafe {
        let hdc = GetDC(Some(hwnd));
        if hdc.is_invalid() {
            return;
        }
        let src = CreateCompatibleDC(Some(hdc));
        let bmp = CreateCompatibleBitmap(hdc, 1, 1);
        let old = SelectObject(src, bmp.into());
        SetPixel(src, 0, 0, COLORREF(current_line_overlay(pal).to_colorref()));
        // The system caret is XOR-drawn: take it off the screen while the
        // pixels under it change, then put it back.
        let caret_hidden = HideCaret(Some(hwnd)).is_ok();
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: CURRENT_LINE_ALPHA,
            AlphaFormat: 0,
        };
        let _ = AlphaBlend(hdc, x0, y0, x1 - x0, y1 - y0, src, 0, 0, 1, 1, blend);
        if caret_hidden {
            let _ = ShowCaret(Some(hwnd));
        }
        SelectObject(src, old);
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(src);
        ReleaseDC(Some(hwnd), hdc);
    }
}

/// Last band and caret key seen, to repaint exactly what changed.
struct BandState {
    key: Cell<usize>,
    band: Cell<Option<(i32, i32)>>,
}

fn invalidate_band(hwnd: HWND, band: Option<(i32, i32)>) {
    let Some((top, bottom)) = band else { return };
    let mut client = RECT::default();
    if unsafe { GetClientRect(hwnd, &mut client) }.is_err() {
        return;
    }
    let rc = RECT { left: client.left, top, right: client.right, bottom };
    unsafe {
        let _ = InvalidateRect(Some(hwnd), Some(&rc), false);
    }
}

/// After a message that may have moved the caret: repaint the band that went
/// away and the one that appeared. `user_driven` also asks the main window
/// for an immediate gutter repaint (programmatic selections, such as the
/// occurrence highlighting, rely on the gutter's own timer instead).
fn track_caret(hwnd: HWND, state: &BandState, user_driven: bool) {
    let (line, empty) = caret_position(hwnd);
    let key = caret_key(line, empty);
    if key == state.key.get() {
        return;
    }
    state.key.set(key);
    invalidate_band(hwnd, state.band.get());
    let band = current_line_rect(hwnd);
    state.band.set(band);
    invalidate_band(hwnd, band);
    if user_driven {
        if let Ok(parent) = unsafe { GetParent(hwnd) } {
            unsafe {
                let _ = PostMessageW(Some(parent), WM_APP_GUTTER_REPAINT, WPARAM(0), LPARAM(0));
            }
        }
    }
}

fn is_user_caret_message(msg: u32, wparam: usize) -> bool {
    match msg {
        WM_KEYDOWN | WM_KEYUP | WM_CHAR | WM_LBUTTONDOWN | WM_LBUTTONUP | WM_CUT | WM_PASTE
        | WM_CLEAR | WM_UNDO | EM_UNDO | EM_REDO | EM_REPLACESEL => true,
        WM_MOUSEMOVE => wparam & MK_LBUTTON != 0,
        _ => false,
    }
}

fn is_programmatic_caret_message(msg: u32) -> bool {
    matches!(msg, EM_SETSEL | EM_EXSETSEL)
}

/// Visual-style class of the scrollbars for a mode; `None` = the control's
/// default theme (what `SetWindowTheme(NULL, NULL)` restores).
pub(super) fn scrollbar_theme_name(mode: ThemeMode) -> Option<&'static str> {
    match mode {
        ThemeMode::Dark => Some("DarkMode_Explorer"),
        ThemeMode::Light => None,
    }
}

const CFM_COLOR: u32 = 0x4000_0000;
const CF_OFFSET_TEXT_COLOR: usize = 20;
const EM_SETBKGNDCOLOR: u32 = 0x0443;
const EM_SETCHARFORMAT: u32 = 0x0444;

/// CHARFORMAT2W that sets only the text colour (no CFE_AUTOCOLOR), applied
/// to the control's default format: text that carries no own colour follows.
pub(super) fn text_color_charformat(color: Color) -> [u8; CF_SIZE] {
    let mut cf = [0u8; CF_SIZE];
    cf[0..4].copy_from_slice(&(CF_SIZE as u32).to_le_bytes());
    cf[4..8].copy_from_slice(&CFM_COLOR.to_le_bytes());
    cf[CF_OFFSET_TEXT_COLOR..CF_OFFSET_TEXT_COLOR + 4].copy_from_slice(&color.to_colorref().to_le_bytes());
    cf
}

/// Background and default text colour of the editor for the current theme.
/// Only control state changes: the text, its modified flag, the undo stack
/// and the selection are untouched (no `EN_CHANGE` is raised by these two
/// messages). A refusal is reported, never ignored.
pub(super) fn apply_editor_colors(hwnd: HWND) {
    let pal = theme::current();
    send(hwnd, EM_SETBKGNDCOLOR, 0, pal.editor_background.to_colorref() as isize);
    let mut cf = text_color_charformat(pal.editor_text);
    let applied = send(hwnd, EM_SETCHARFORMAT, SCF_DEFAULT, cf.as_mut_ptr() as isize);
    if applied == 0 {
        eprintln!("WARN theme: EM_SETCHARFORMAT refused the editor text colour; the text keeps its previous colour");
    }
}

/// Scrollbar theme of the current mode: dark class in Dark, the control's
/// default theme in Light (`SetWindowTheme(NULL, NULL)`). A failure keeps the
/// previous bars and is reported once.
pub(super) fn theme_scrollbars(hwnd: HWND) {
    let name: Option<Vec<u16>> =
        scrollbar_theme_name(theme::current_mode()).map(|n| n.encode_utf16().chain(std::iter::once(0)).collect());
    let class = name.as_ref().map_or(windows::core::PCWSTR::null(), |n| windows::core::PCWSTR(n.as_ptr()));
    if let Err(e) = unsafe { SetWindowTheme(hwnd, class, windows::core::PCWSTR::null()) } {
        warn_once(&WARNED_SCROLLBAR_THEME, &format!("scrollbar theme unavailable ({e}); keeping the previous look"));
    }
}

/// Re-apply the theme-dependent state of the editor (colours, scrollbars)
/// and repaint it. Used by the live theme switch.
pub(super) fn retheme(editor: &nwg::RichTextBox) {
    let nwg::ControlHandle::Hwnd(raw) = editor.handle else {
        eprintln!("WARN theme: the editor has no window handle; its colours were not updated");
        return;
    };
    let hwnd = HWND(raw as *mut _);
    apply_editor_colors(hwnd);
    theme_scrollbars(hwnd);
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, true);
    }
}

/// Install everything on the editor: no frame, colours, scrollbars, line
/// spacing on the current (empty) text, and the subclass that keeps both
/// alive across `WM_SETTEXT` and paints the current-line band.
pub(super) fn install(editor: &nwg::RichTextBox) {
    let nwg::ControlHandle::Hwnd(raw) = editor.handle else {
        panic!("editor surface: the editor has no window handle");
    };
    let hwnd = HWND(raw as *mut _);
    // nwg gives the RichEdit WS_BORDER and RichEdit adds a sunken edge: both
    // are drawn in fixed system greys and white, a pale frame around the text
    // in the Dark theme. The tab strip and the status bar already edge the
    // editor, so it carries no frame of its own.
    crate::ui::lock_card::flatten_edit(&editor.handle);
    apply_editor_colors(hwnd);
    theme_scrollbars(hwnd);
    if let Err(e) = apply_line_spacing(hwnd) {
        panic!("editor surface: {e}");
    }

    let state = BandState { key: Cell::new(usize::MAX), band: Cell::new(None) };
    let handler = nwg::bind_raw_event_handler(&editor.handle, EDITOR_SURFACE_HANDLER_ID, move |h, msg, w, l| {
        let hwnd = HWND(h as *mut _);
        match msg {
            WM_PAINT if w == 0 => {
                // The update rectangle is gone once the default paint ran.
                let mut update = RECT::default();
                let has_update = unsafe { GetUpdateRect(hwnd, Some(&mut update), false) }.as_bool();
                let res = unsafe { DefSubclassProc(hwnd, msg, WPARAM(w), LPARAM(l)) };
                if has_update {
                    overlay_band(hwnd, &update);
                    state.band.set(current_line_rect(hwnd));
                }
                Some(res.0)
            }
            WM_SETTEXT => {
                let res = unsafe { DefSubclassProc(hwnd, msg, WPARAM(w), LPARAM(l)) };
                if let Err(e) = apply_line_spacing(hwnd) {
                    warn_once(&WARNED_SPACING, &format!("{e}; the text keeps single spacing"));
                }
                track_caret(hwnd, &state, false);
                Some(res.0)
            }
            _ if is_user_caret_message(msg, w) || is_programmatic_caret_message(msg) => {
                let res = unsafe { DefSubclassProc(hwnd, msg, WPARAM(w), LPARAM(l)) };
                track_caret(hwnd, &state, is_user_caret_message(msg, w));
                Some(res.0)
            }
            _ => None,
        }
    });
    match handler {
        Ok(h) => std::mem::forget(h),
        Err(e) => panic!("editor surface handler: {e:?}"),
    }
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const BOTH: [ThemeMode; 2] = [ThemeMode::Dark, ThemeMode::Light];

    #[test]
    fn scrollbars_use_the_dark_class_only_in_dark() {
        assert_eq!(scrollbar_theme_name(ThemeMode::Dark), Some("DarkMode_Explorer"));
        assert_eq!(scrollbar_theme_name(ThemeMode::Light), None);
    }

    #[test]
    fn text_colour_charformat_sets_only_the_colour() {
        let cf = text_color_charformat(Color::new(0x11, 0x22, 0x33));
        assert_eq!(u32::from_le_bytes(cf[0..4].try_into().unwrap()), CF_SIZE as u32);
        assert_eq!(u32::from_le_bytes(cf[4..8].try_into().unwrap()), CFM_COLOR, "mask");
        assert_eq!(u32::from_le_bytes(cf[8..12].try_into().unwrap()), 0, "effects: no CFE_AUTOCOLOR");
        assert_eq!(
            u32::from_le_bytes(cf[CF_OFFSET_TEXT_COLOR..CF_OFFSET_TEXT_COLOR + 4].try_into().unwrap()),
            0x0033_2211,
            "COLORREF 0x00BBGGRR"
        );
        assert!(cf[12..CF_OFFSET_TEXT_COLOR].iter().all(|b| *b == 0));
        assert!(cf[CF_OFFSET_TEXT_COLOR + 4..].iter().all(|b| *b == 0));
    }

    #[test]
    fn pitch_is_one_and_a_half_lines_rounded_half_up() {
        // Values measured on RICHEDIT50W with Consolas (live tests below).
        assert_eq!(line_pitch(18), 27);
        assert_eq!(line_pitch(24), 36);
        assert_eq!(line_pitch(33), 50);
        assert_eq!(line_pitch(41), 62);
        assert_eq!(line_pitch(0), 2); // never zero or negative
    }

    #[test]
    fn twips_convert_to_pixels_at_common_dpi() {
        assert_eq!(twips_to_px(315, 96), 21); // the 11 pt editor font
        assert_eq!(twips_to_px(225, 96), 15);
        assert_eq!(twips_to_px(315, 120), 26); // 125 %
        assert_eq!(twips_to_px(315, 144), 32); // 150 %
    }

    #[test]
    fn spacing_factor_is_comfortable() {
        let factor = LINE_SPACING_20THS as f64 / 20.0;
        assert!((1.4..=1.65).contains(&factor), "factor {factor}");
    }

    #[test]
    fn paraformat_bytes_set_only_the_line_spacing() {
        let pf = line_spacing_paraformat();
        assert_eq!(pf.len(), 188);
        assert_eq!(u32::from_le_bytes(pf[0..4].try_into().unwrap()), 188);
        assert_eq!(u32::from_le_bytes(pf[4..8].try_into().unwrap()), 0x100);
        assert_eq!(i32::from_le_bytes(pf[164..168].try_into().unwrap()), 30);
        assert_eq!(pf[170], 5);
        let others: Vec<usize> = (8..188).filter(|i| ![164, 165, 166, 167, 170].contains(i) && pf[*i] != 0).collect();
        assert!(others.is_empty(), "unexpected non-zero bytes at {others:?}");
    }

    #[test]
    fn band_is_the_caret_line_only_without_a_selection() {
        assert_eq!(current_line_band(true, 36, 36, 500), Some((36, 72)));
        assert_eq!(current_line_band(false, 36, 36, 500), None);
    }

    #[test]
    fn band_outside_the_view_is_not_drawn() {
        assert_eq!(current_line_band(true, -36, 36, 500), None); // ends at 0
        assert_eq!(current_line_band(true, -20, 36, 500), Some((-20, 16)));
        assert_eq!(current_line_band(true, 500, 36, 500), None);
        assert_eq!(current_line_band(true, 499, 36, 500), Some((499, 535)));
        assert_eq!(current_line_band(true, 0, 0, 500), None);
    }

    #[test]
    fn active_end_is_max_unless_the_caret_is_on_min() {
        // Empty selection: the caret is there, whatever the caret says.
        assert_eq!(active_end(5, 5, None), 5);
        assert_eq!(active_end(5, 5, Some(2)), 5);
        // Selection made forwards (Shift+Right): caret on max.
        assert_eq!(active_end(2, 8, Some(8)), 8);
        // Made backwards (Shift+Left): caret on min.
        assert_eq!(active_end(2, 8, Some(2)), 2);
        // Editor without the caret (focus elsewhere), or a character that is
        // neither end: max.
        assert_eq!(active_end(2, 8, None), 8);
        assert_eq!(active_end(2, 8, Some(5)), 8);
    }

    #[test]
    fn caret_key_distinguishes_line_and_selection() {
        assert_ne!(caret_key(3, true), caret_key(3, false));
        assert_ne!(caret_key(3, true), caret_key(4, true));
        assert_eq!(caret_key(3, true), caret_key(3, true));
    }

    #[test]
    fn band_is_a_subtle_tint_in_both_themes() {
        for mode in BOTH {
            let pal = theme::get_palette(mode);
            for base in [pal.editor_background, pal.gutter_background] {
                let band = current_line_background(base, pal);
                assert_ne!(band, base, "{mode:?}: the band must be visible");
                let ratio = band.contrast_ratio(&base);
                assert!(ratio < 1.3, "{mode:?}: band too strong ({ratio:.2}:1 against its background)");
            }
        }
    }

    #[test]
    fn text_stays_readable_on_the_band_in_both_themes() {
        for mode in BOTH {
            let pal = theme::get_palette(mode);
            let band = current_line_background(pal.editor_background, pal);
            let ratio = pal.editor_text.contrast_ratio(&band);
            assert!(ratio >= 4.5, "{mode:?}: editor text on the band {ratio:.2}:1");
            // The band only ever sits behind the caret line's number, which is
            // drawn in `text_primary` (the other numbers are never on it).
            let gutter_band = current_line_background(pal.gutter_background, pal);
            let ratio = pal.text_primary.contrast_ratio(&gutter_band);
            assert!(ratio >= 4.5, "{mode:?}: emphasised line number on the band {ratio:.2}:1");
        }
    }

    #[test]
    fn text_pixels_are_not_changed_by_the_overlay() {
        for mode in BOTH {
            let pal = theme::get_palette(mode);
            assert_eq!(pal.editor_text.blend_alpha(&current_line_overlay(pal), CURRENT_LINE_ALPHA), pal.editor_text);
        }
    }

    #[test]
    fn base_text_contrast_meets_aa_in_both_themes() {
        for mode in BOTH {
            let pal = theme::get_palette(mode);
            assert!(pal.editor_text.contrast_ratio(&pal.editor_background) >= 4.5, "{mode:?} editor text");
            assert!(pal.gutter_text.contrast_ratio(&pal.gutter_background) >= 4.5, "{mode:?} gutter text");
            assert!(pal.text_primary.contrast_ratio(&pal.gutter_background) >= 4.5, "{mode:?} caret line number");
        }
    }

    #[test]
    fn occurrence_highlight_keeps_the_text_readable_with_the_band() {
        for mode in BOTH {
            let pal = theme::get_palette(mode);
            let under_band = pal.match_highlight.blend_alpha(&current_line_overlay(pal), CURRENT_LINE_ALPHA);
            let ratio = pal.editor_text.contrast_ratio(&under_band);
            assert!(ratio >= 4.5, "{mode:?}: text on a highlighted match under the band {ratio:.2}:1");
        }
    }

    // -----------------------------------------------------------------
    // Live RichEdit tests: the gutter alignment guarantee. They create a
    // hidden-in-the-test-process RICHEDIT50W (never the application).
    // -----------------------------------------------------------------

    mod live {
        use super::*;
        use windows::core::w;
        use windows::Win32::Graphics::Gdi::{GetPixel, GetSysColor, COLOR_WINDOW, DEFAULT_CHARSET};
        use windows::Win32::System::LibraryLoader::LoadLibraryW;
        use windows::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WINDOW_STYLE, WS_CHILD, WS_POPUP, WS_VISIBLE,
        };

        const TEXT: &str = "HHHH\r\nHHHH\r\nHHHH";

        struct Rich {
            top: HWND,
            ed: HWND,
        }

        impl Drop for Rich {
            fn drop(&mut self) {
                unsafe {
                    let _ = DestroyWindow(self.top);
                }
            }
        }

        fn make(font_height: i32) -> Rich {
            unsafe {
                LoadLibraryW(w!("msftedit.dll")).expect("msftedit.dll");
                let top = CreateWindowExW(
                    WINDOW_EX_STYLE(0), w!("STATIC"), None, WS_POPUP, 0, 0, 400, 300, None, None, None, None,
                )
                .expect("host window");
                // ES_MULTILINE | ES_AUTOVSCROLL | ES_NOHIDESEL, as the editor.
                let ed = CreateWindowExW(
                    WINDOW_EX_STYLE(0), w!("RICHEDIT50W"), None,
                    WS_CHILD | WS_VISIBLE | WINDOW_STYLE(0x0004 | 0x0040 | 0x0100), 0, 0, 380, 280, Some(top), None, None, None,
                )
                .expect("RICHEDIT50W");
                let font = CreateFontW(
                    font_height, 0, 0, 0, 400, 0, 0, 0, DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS,
                    CLEARTYPE_QUALITY, 0, w!("Consolas"),
                );
                send(ed, WM_SETFONT, font.0 as usize, 1);
                send(ed, 0x00D3, 1, 20); // EM_SETMARGINS left, as the editor
                set_text(ed, TEXT);
                Rich { top, ed }
            }
        }

        const WM_SETFONT: u32 = 0x0030;

        fn set_text(ed: HWND, text: &str) {
            let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
            send(ed, WM_SETTEXT, 0, wide.as_ptr() as isize);
        }

        fn tops(ed: HWND) -> Vec<i32> {
            (0..3).map(|i| line_top(ed, i).expect("line position")).collect()
        }

        /// First and last pixel row (client coordinates) holding ink, per text line.
        fn ink_rows(ed: HWND) -> Vec<(i32, i32)> {
            unsafe {
                let (w, h) = (380, 280);
                let screen = GetDC(None);
                let dc = CreateCompatibleDC(Some(screen));
                let bmp = CreateCompatibleBitmap(screen, w, h);
                let old = SelectObject(dc, bmp.into());
                // PRF_CLIENT | PRF_ERASEBKGND
                SendMessageW(ed, 0x0317, Some(WPARAM(dc.0 as usize)), Some(LPARAM(0x4 | 0x8)));
                let bg = GetSysColor(COLOR_WINDOW);
                let mut rows = vec![];
                let mut start = None;
                for y in 0..h {
                    let any = (0..w).any(|x| GetPixel(dc, x, y).0 != bg);
                    match (any, start) {
                        (true, None) => start = Some(y),
                        (false, Some(s)) => {
                            rows.push((s, y - 1));
                            start = None;
                        }
                        _ => {}
                    }
                }
                SelectObject(dc, old);
                let _ = DeleteObject(bmp.into());
                let _ = DeleteDC(dc);
                ReleaseDC(None, screen);
                rows
            }
        }

        /// `selection_and_caret` reads which end of the selection the caret
        /// is on from the system caret: RichEdit must keep it there.
        #[test]
        fn live_caret_is_on_the_active_end_of_the_selection() {
            const EM_SETSEL_: u32 = 0x00B1;
            let r = make(-21);
            unsafe {
                let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(Some(r.ed));
            }
            assert_eq!(
                unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetFocus() },
                r.ed,
                "the test editor must own the focus"
            );
            // EM_SETSEL(anchor, caret) in TEXT, where a line break is one unit.
            for (anchor, caret) in [(2usize, 8usize), (8, 2), (1, 12), (12, 1), (0, 4), (4, 0)] {
                send(r.ed, EM_SETSEL_, anchor, caret as isize);
                let (min, max, at) = selection_and_caret(r.ed);
                assert_eq!((min, max), (anchor.min(caret), anchor.max(caret)));
                assert_eq!(at, caret, "selection {anchor}->{caret}");
            }
            send(r.ed, EM_SETSEL_, 6, 6);
            assert_eq!(selection_and_caret(r.ed), (6, 6, 6));
        }

        #[test]
        fn live_pitch_matches_the_computed_pitch_for_several_font_sizes() {
            for font_height in [-15, -21, -28, -35] {
                let r = make(font_height);
                let natural = natural_line_height(r.ed).expect("text metrics");
                let single = tops(r.ed);
                assert_eq!(single[1] - single[0], natural, "font {font_height}: unspaced pitch is the text height");
                apply_line_spacing(r.ed).expect("spacing");
                let spaced = tops(r.ed);
                assert_eq!(spaced[1] - spaced[0], line_pitch(natural), "font {font_height}: spaced pitch");
                assert_eq!(spaced[2] - spaced[1], line_pitch(natural), "font {font_height}: uniform pitch");
                assert_eq!(spaced[0], single[0], "font {font_height}: first line does not move");
            }
        }

        fn text_of(ed: HWND) -> String {
            let mut buf = [0u16; 256];
            let n = send(ed, 0x000D, buf.len(), buf.as_mut_ptr() as isize); // WM_GETTEXT
            String::from_utf16_lossy(&buf[..n.max(0) as usize])
        }

        fn consolas(height: i32) -> isize {
            let font = unsafe {
                CreateFontW(
                    height, 0, 0, 0, 400, 0, 0, 0, DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS,
                    CLEARTYPE_QUALITY, 0, w!("Consolas"),
                )
            };
            assert!(!font.is_invalid(), "CreateFontW");
            font.0 as isize
        }

        /// The zoom and a new font from the preferences (A2): every line
        /// takes the new size with the 1.5 spacing, the edit state is the
        /// one of before (not modified, the undo history unchanged).
        #[test]
        fn live_set_font_resizes_every_line_and_keeps_the_edit_state() {
            let r = make(-15);
            apply_line_spacing(r.ed).expect("spacing");
            send(r.ed, EM_SETSEL, 0, 0);
            let x: Vec<u16> = "X".encode_utf16().chain(std::iter::once(0)).collect();
            send(r.ed, EM_REPLACESEL, 1, x.as_ptr() as isize);
            send(r.ed, EM_SETMODIFY, 0, 0);
            let small = natural_line_height(r.ed).expect("text metrics");

            set_font(r.ed, consolas(-35)).expect("font");

            let natural = natural_line_height(r.ed).expect("text metrics");
            assert!(natural > small, "the default font grew: {small} -> {natural}");
            let t = tops(r.ed);
            assert_eq!(t[1] - t[0], line_pitch(natural), "the first line took the new size and the spacing");
            assert_eq!(t[2] - t[1], line_pitch(natural), "so did the last one");
            assert_eq!(send(r.ed, EM_GETMODIFY, 0, 0), 0, "a font is not an edit");
            assert_eq!(text_of(r.ed), format!("X{TEXT}"), "WM_GETTEXT answers CRLF");
            // Undo still undoes the typing, not the font.
            assert_ne!(send(r.ed, EM_CANUNDO, 0, 0), 0);
            send(r.ed, EM_UNDO, 0, 0);
            assert_eq!(text_of(r.ed), TEXT);
            assert_eq!(natural_line_height(r.ed), Some(natural), "the undo kept the font");

            // No history before: none after.
            send(r.ed, EM_EMPTYUNDOBUFFER, 0, 0);
            set_font(r.ed, consolas(-21)).expect("font");
            assert_eq!(send(r.ed, EM_CANUNDO, 0, 0), 0);
            let natural = natural_line_height(r.ed).expect("text metrics");
            let t = tops(r.ed);
            assert_eq!(t[1] - t[0], line_pitch(natural), "smaller again, still spaced");
        }

        #[test]
        fn live_glyphs_keep_their_place_inside_the_line_box() {
            // The gutter draws each number at the line's EM_POSFROMCHAR top,
            // as before the spacing: that is exact only if the glyphs sit at
            // the same offset from that top with and without spacing.
            let r = make(-21);
            let single_tops = tops(r.ed);
            let single_ink = ink_rows(r.ed);
            apply_line_spacing(r.ed).expect("spacing");
            let spaced_tops = tops(r.ed);
            let spaced_ink = ink_rows(r.ed);
            assert_eq!(single_ink.len(), 3);
            assert_eq!(spaced_ink.len(), 3);
            for i in 0..3 {
                assert_eq!(
                    spaced_ink[i].0 - spaced_tops[i],
                    single_ink[i].0 - single_tops[i],
                    "line {i}: glyph offset from the line top"
                );
                assert_eq!(spaced_ink[i].1 - spaced_ink[i].0, single_ink[i].1 - single_ink[i].0, "line {i}: glyph height");
            }
        }

        #[test]
        fn live_gutter_numbers_land_on_the_rows_of_their_text_line() {
            // Draw a glyph the way paint_gutter does (same font, DrawText at
            // the line's EM_POSFROMCHAR top, DT_TOP) and compare its pixel
            // rows with the editor's own rows for the same line.
            use windows::Win32::Foundation::RECT;
            use windows::Win32::Graphics::Gdi::{
                CreateFontW, CreateSolidBrush, DrawTextW, FillRect, SetBkMode, SetTextColor, CLEARTYPE_QUALITY,
                CLIP_DEFAULT_PRECIS, DT_NOPREFIX, DT_RIGHT, DT_SINGLELINE, OUT_DEFAULT_PRECIS, TRANSPARENT,
            };
            let r = make(-21);
            apply_line_spacing(r.ed).expect("spacing");
            let line_tops = tops(r.ed);
            let editor_rows = ink_rows(r.ed);
            let pitch = editor_line_pitch(r.ed).expect("pitch");
            let (w, h) = (100, 280);
            let gutter_rows = unsafe {
                let screen = GetDC(None);
                let dc = CreateCompatibleDC(Some(screen));
                let bmp = CreateCompatibleBitmap(screen, w, h);
                let old = SelectObject(dc, bmp.into());
                let white = CreateSolidBrush(COLORREF(0x00FF_FFFF));
                FillRect(dc, &RECT { left: 0, top: 0, right: w, bottom: h }, white);
                let _ = DeleteObject(white.into());
                let font = CreateFontW(
                    -21, 0, 0, 0, 400, 0, 0, 0, DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS,
                    CLEARTYPE_QUALITY, 0, w!("Consolas"),
                );
                let old_font = SelectObject(dc, font.into());
                SetBkMode(dc, TRANSPARENT);
                SetTextColor(dc, COLORREF(0));
                for top in &line_tops {
                    let mut text: Vec<u16> = "H".encode_utf16().collect();
                    let mut rc = RECT { left: 0, top: *top, right: w - 10, bottom: top + pitch };
                    DrawTextW(dc, &mut text, &mut rc, DT_RIGHT | DT_SINGLELINE | DT_NOPREFIX);
                }
                let mut rows = vec![];
                let mut start = None;
                for y in 0..h {
                    let any = (0..w).any(|x| GetPixel(dc, x, y).0 != 0x00FF_FFFF);
                    match (any, start) {
                        (true, None) => start = Some(y),
                        (false, Some(s)) => {
                            rows.push((s, y - 1));
                            start = None;
                        }
                        _ => {}
                    }
                }
                SelectObject(dc, old_font);
                let _ = DeleteObject(font.into());
                SelectObject(dc, old);
                let _ = DeleteObject(bmp.into());
                let _ = DeleteDC(dc);
                ReleaseDC(None, screen);
                rows
            };
            assert_eq!(editor_rows.len(), 3);
            assert_eq!(gutter_rows, editor_rows, "gutter glyph rows == editor glyph rows, line by line");
        }

        #[test]
        fn live_spacing_survives_text_replacement_through_the_surface_subclass() {
            // Without the subclass WM_SETTEXT resets the format (measured);
            // apply_line_spacing is what the subclass calls afterwards.
            let r = make(-21);
            apply_line_spacing(r.ed).expect("spacing");
            set_text(r.ed, TEXT);
            let reset = tops(r.ed);
            assert_eq!(reset[1] - reset[0], natural_line_height(r.ed).unwrap(), "WM_SETTEXT resets the format");
            apply_line_spacing(r.ed).expect("spacing");
            let again = tops(r.ed);
            assert_eq!(again[1] - again[0], line_pitch(natural_line_height(r.ed).unwrap()));
        }

        /// Like `make`, but shown on screen: only a visible window has an
        /// update region.
        fn make_visible() -> Rich {
            use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_SHOWNOACTIVATE};
            let r = make(-21);
            unsafe {
                let _ = ShowWindow(r.top, SW_SHOWNOACTIVATE);
            }
            r
        }

        /// Part of the client area waiting for a repaint, if any.
        fn pending_repaint(ed: HWND) -> Option<RECT> {
            let mut update = RECT::default();
            unsafe { GetUpdateRect(ed, Some(&mut update), false) }.as_bool().then_some(update)
        }

        fn paint_now(ed: HWND) {
            use windows::Win32::Graphics::Gdi::UpdateWindow;
            unsafe {
                let _ = UpdateWindow(ed);
            }
            assert_eq!(pending_repaint(ed).map(|r| (r.top, r.bottom)), None, "painted: nothing pending");
        }

        #[test]
        fn live_text_replacement_is_repainted_after_the_spacing() {
            // A tab switch is WM_SETTEXT followed (in the subclass) by
            // apply_line_spacing: the new text must reach the screen.
            let r = make_visible();
            apply_line_spacing(r.ed).expect("spacing");
            paint_now(r.ed);

            // Control: the text replacement alone asks for a repaint.
            set_text(r.ed, "AAAA\r\nAAAA\r\nAAAA");
            assert!(pending_repaint(r.ed).is_some(), "WM_SETTEXT invalidates the editor");
            paint_now(r.ed);

            set_text(r.ed, "BBBB\r\nBBBB\r\nBBBB");
            apply_line_spacing(r.ed).expect("spacing");
            let update = pending_repaint(r.ed).expect("the replaced text is waiting for a repaint");
            let last_line_bottom = tops(r.ed)[2] + editor_line_pitch(r.ed).expect("pitch");
            assert!(
                update.top <= tops(r.ed)[0] && update.bottom >= last_line_bottom,
                "every line is repainted: update {}..{}, text {}..{}",
                update.top, update.bottom, tops(r.ed)[0], last_line_bottom
            );
        }

        #[test]
        fn live_installed_editor_has_no_frame() {
            // nwg creates the RichEdit with WS_BORDER and RichEdit adds a
            // sunken edge: three rows of system greys (and white on the
            // right / bottom) around the text, whatever the theme.
            use windows::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrW, GWL_EXSTYLE, GWL_STYLE};
            let _ = nwg::init();
            let mut window = nwg::Window::default();
            nwg::Window::builder().size((400, 300)).flags(nwg::WindowFlags::WINDOW).build(&mut window).expect("window");
            let mut editor = nwg::RichTextBox::default();
            nwg::RichTextBox::builder().parent(&window).size((200, 100)).build(&mut editor).expect("editor");
            install(&editor);
            let nwg::ControlHandle::Hwnd(raw) = editor.handle else { panic!("no handle") };
            let ed = HWND(raw as *mut _);

            let (style, ex) = unsafe { (GetWindowLongPtrW(ed, GWL_STYLE) as u32, GetWindowLongPtrW(ed, GWL_EXSTYLE) as u32) };
            assert_eq!(style & 0x0080_0000, 0, "WS_BORDER");
            assert_eq!(ex & 0x0000_0200, 0, "WS_EX_CLIENTEDGE");

            // Painted with its non-client area: the edge pixels are the
            // editor background, like the inside.
            let (w, h) = (200, 100);
            unsafe {
                let screen = GetDC(None);
                let dc = CreateCompatibleDC(Some(screen));
                let bmp = CreateCompatibleBitmap(screen, w, h);
                let old = SelectObject(dc, bmp.into());
                // PRF_NONCLIENT | PRF_CLIENT | PRF_ERASEBKGND
                SendMessageW(ed, 0x0317, Some(WPARAM(dc.0 as usize)), Some(LPARAM(0x2 | 0x4 | 0x8)));
                let inside = GetPixel(dc, w / 2, h / 2).0;
                for (x, y) in [(0, h / 2), (1, h / 2), (2, h / 2), (w / 2, 0), (w / 2, 1), (w / 2, 2), (w - 1, h / 2), (w / 2, h - 1)] {
                    assert_eq!(GetPixel(dc, x, y).0, inside, "pixel ({x},{y}) is not the editor background");
                }
                SelectObject(dc, old);
                let _ = DeleteObject(bmp.into());
                let _ = DeleteDC(dc);
                ReleaseDC(None, screen);
            }
        }

        #[test]
        fn live_spacing_is_presentation_only() {
            let r = make(-21);
            send(r.ed, EM_SETMODIFY, 0, 0);
            send(r.ed, EM_EMPTYUNDOBUFFER, 0, 0);
            send(r.ed, EM_SETSEL, 2, 5);
            apply_line_spacing(r.ed).expect("spacing");
            assert_eq!(send(r.ed, EM_GETMODIFY, 0, 0), 0, "modified flag untouched");
            assert_eq!(send(r.ed, EM_CANUNDO, 0, 0), 0, "no undo entry created");
            let mut sel = CharRange::default();
            send(r.ed, EM_EXGETSEL, 0, &mut sel as *mut _ as isize);
            assert_eq!((sel.min, sel.max), (2, 5), "selection restored");
            // The text read back is byte-identical (the saved data).
            let mut buf = vec![0u16; 64];
            let n = unsafe { SendMessageW(r.ed, 0x000D, Some(WPARAM(buf.len())), Some(LPARAM(buf.as_mut_ptr() as isize))).0 } as usize;
            assert_eq!(String::from_utf16_lossy(&buf[..n]), TEXT);
        }

        #[test]
        fn live_band_covers_the_whole_spaced_line_and_none_with_a_selection() {
            let r = make(-21);
            apply_line_spacing(r.ed).expect("spacing");
            send(r.ed, EM_SETSEL, 5, 5); // line 1
            let (top, bottom) = current_line_rect(r.ed).expect("band");
            let t = tops(r.ed);
            assert_eq!(top, t[1]);
            assert_eq!(bottom, t[2], "the band ends where the next line starts");
            send(r.ed, EM_SETSEL, 5, 8);
            assert_eq!(current_line_rect(r.ed), None, "no band while text is selected");
        }
    }
}
