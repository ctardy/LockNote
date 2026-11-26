// Free helper functions: fonts, dates, placement, UTF-16 offsets, search, file sniffing.
// Split out of the former editor.rs.

#[allow(unused_imports)]
use super::*;

/// Convert a font size in points to NWG logical units.
/// NWG uses roughly: nwg_size = points * (96/72) ≈ points * 1.333
/// But NWG Font builder `size` is in tenths of a point internally,
/// and the actual mapping depends on DPI. A practical formula:
/// nwg_size ≈ (points * 96 / 72) as i32
/// For 9pt → 12, for 11pt → ~15, etc.
/// Actually, NWG's size parameter maps more simply: it's the height
/// in logical pixels. The default font at size=17 gives roughly 9pt.
pub(super) fn font_size_to_nwg(pt: f64) -> u32 {
    // NWG default: 17 for 9pt → ratio ~1.889
    // A more accurate mapping: size = pt * 96 / 72 * (4/3) ≈ pt * 1.78
    // Empirically from the spec: 9pt → 17, so ratio = 17/9 ≈ 1.889
    (pt * 17.0 / 9.0).round() as u32
}

/// Format a Unix timestamp as "YYYY-MM-DD HH:MM:SS" (UTC).
/// Simple implementation without chrono.
pub(in crate::ui) fn format_timestamp(secs: u64) -> String {
    // Days from epoch
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Date calculation (from Unix epoch 1970-01-01)
    let (year, month, day) = days_to_ymd(days);

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        year, month, day, hours, minutes, seconds
    )
}

/// Convert days since Unix epoch to (year, month, day).
pub(super) fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    // Civil calendar algorithm (from Howard Hinnant)
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Work areas of every monitor (screen coordinates) and the primary
/// monitor's work-area offset (taskbar at the top/left), for
/// `window_geometry::to_workspace`.
pub(super) fn monitor_work_areas() -> (Vec<crate::ui::window_geometry::Rect>, (i32, i32)) {
    use windows::Win32::Foundation::{LPARAM, RECT};
    use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO};
    use windows::Win32::UI::WindowsAndMessaging::MONITORINFOF_PRIMARY;

    struct Acc {
        areas: Vec<crate::ui::window_geometry::Rect>,
        primary_offset: (i32, i32),
    }
    unsafe extern "system" fn collect(m: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> windows::core::BOOL {
        let acc = unsafe { &mut *(data.0 as *mut Acc) };
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if unsafe { GetMonitorInfoW(m, &mut mi) }.as_bool() {
            let w = mi.rcWork;
            acc.areas.push(crate::ui::window_geometry::Rect { left: w.left, top: w.top, right: w.right, bottom: w.bottom });
            if mi.dwFlags & MONITORINFOF_PRIMARY != 0 {
                acc.primary_offset = (w.left - mi.rcMonitor.left, w.top - mi.rcMonitor.top);
            }
        }
        true.into()
    }
    let mut acc = Acc { areas: Vec::new(), primary_offset: (0, 0) };
    let ok = unsafe { EnumDisplayMonitors(None, None, Some(collect), LPARAM(&mut acc as *mut Acc as isize)) };
    if !ok.as_bool() {
        eprintln!("window: EnumDisplayMonitors failed");
    }
    (acc.areas, acc.primary_offset)
}

/// Apply the saved placement to the main window while it is still hidden
/// (issue 044). Returns whether it must be shown maximized.
pub(super) fn apply_initial_placement(window: &nwg::Window, settings: &crate::settings::Settings) -> bool {
    use crate::ui::window_geometry as wg;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SetWindowPlacement, SM_CYCAPTION, SW_HIDE, WINDOWPLACEMENT,
    };
    let nwg::ControlHandle::Hwnd(h) = window.handle else {
        eprintln!("window: no HWND, placement not restored");
        return false;
    };
    let (areas, primary_offset) = monitor_work_areas();
    let caption_h = unsafe { GetSystemMetrics(SM_CYCAPTION) };
    match wg::initial_window_placement(settings, &wg::to_workspace(&areas, primary_offset), caption_h) {
        wg::InitialPlacement::Default { reason, maximized } => {
            if reason != wg::DefaultReason::NeverSaved {
                eprintln!("window: saved placement ignored reason={:?}", reason);
            }
            maximized
        }
        wg::InitialPlacement::Restore(g) => {
            let mut wp = WINDOWPLACEMENT {
                length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
                showCmd: SW_HIDE.0 as u32,
                ..Default::default()
            };
            wp.rcNormalPosition = windows::Win32::Foundation::RECT {
                left: g.x,
                top: g.y,
                right: g.x + g.w as i32,
                bottom: g.y + g.h as i32,
            };
            let hwnd = windows::Win32::Foundation::HWND(h as *mut _);
            if let Err(e) = unsafe { SetWindowPlacement(hwnd, &wp) } {
                eprintln!("window: SetWindowPlacement failed err={:?}", e);
            }
            g.maximized
        }
    }
}

/// Precondition for writing the encrypted payload. The password is only
/// empty after a lock zeroized it; encrypting with "" would overwrite the
/// real notes with a blob anyone can open, so saving must fail fast.
pub(super) fn save_precondition(password: &str) -> Result<(), String> {
    if password.is_empty() {
        return Err("refusing to save: password is empty (editor locked)".to_string());
    }
    Ok(())
}

/// Convert a UTF-16 code-unit range to a UTF-8 byte range within `text`.
/// Returns `(byte_start, byte_end)`. Clamps to string length if out of range.
pub(super) fn utf16_range_to_byte_range(text: &str, utf16_start: usize, utf16_end: usize) -> (usize, usize) {
    let mut byte_start = text.len();
    let mut byte_end = text.len();
    let mut u16_pos: usize = 0;
    let bytes = text.as_bytes();

    for (byte_idx, ch) in text.char_indices() {
        // RichEdit counts \r\n as a single character — skip \n after \r
        // before matching, so a position on the line start maps to the first
        // char of the line and not to the \n it shares its offset with.
        if ch == '\n' && byte_idx > 0 && bytes[byte_idx - 1] == b'\r' {
            continue;
        }
        if u16_pos == utf16_start {
            byte_start = byte_idx;
        }
        if u16_pos == utf16_end {
            byte_end = byte_idx;
            return (byte_start, byte_end);
        }
        u16_pos += ch.len_utf16();
    }

    if u16_pos == utf16_start {
        byte_start = text.len();
    }
    if u16_pos == utf16_end {
        byte_end = text.len();
    }

    (byte_start, byte_end)
}

/// Length of `text` in RichEdit character positions (UTF-16 code units,
/// `\r\n` = 1). nwg's `len()` counts `char`s, which under-counts characters
/// outside the BMP.
pub(super) fn utf16_len(text: &str) -> usize {
    build_utf16_offset_map(text)[text.len()]
}

/// The range a double-click should select, in RichEdit positions: the
/// native word-select `(u16_start, u16_end)` widened by `expand_word_selection`.
/// `None` when the native selection already is the target or cannot be widened.
pub(super) fn dblclick_target(text: &str, u16_start: u32, u16_end: u32) -> Option<(u32, u32)> {
    if u16_start > u16_end {
        return None;
    }
    let (b0, b1) = utf16_range_to_byte_range(text, u16_start as usize, u16_end as usize);
    let (n0, n1) = expand_word_selection(text, b0, b1)?;
    let offsets = build_utf16_offset_map(text);
    let target = (offsets[n0] as u32, offsets[n1] as u32);
    (target != (u16_start, u16_end)).then_some(target)
}

/// Notification structs of richedit.h. The header wraps them in
/// `pshpack4.h`, so 8-byte fields sit on 4-byte boundaries (x64): reading
/// them as plain `repr(C)` shifts every field after `msg`.
#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct NmHdr {
    pub hwnd_from: isize,
    pub id_from: usize,
    pub code: u32,
}

#[repr(C, packed(4))]
#[derive(Clone, Copy)]
pub(super) struct MsgFilter {
    pub nmhdr: NmHdr,
    pub msg: u32,
    pub wparam: usize,
    pub lparam: isize,
}

#[repr(C, packed(4))]
#[derive(Clone, Copy)]
pub(super) struct EnLink {
    pub nmhdr: NmHdr,
    pub msg: u32,
    pub wparam: usize,
    pub lparam: isize,
    pub chrg_min: i32,
    pub chrg_max: i32,
}

/// Why a RichEdit character range could not be mapped onto the text.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Utf16RangeError {
    Negative,
    Empty,
    OutOfBounds { max: usize, len_u16: usize },
    MidSurrogate { pos: usize },
}

/// Resolve the text of a RichEdit `CHARRANGE` (EN_LINK). Positions are
/// RichEdit character positions (UTF-16 code units, `\r\n` = 1).
pub(super) fn link_text_range(text: &str, chrg_min: i32, chrg_max: i32) -> Result<&str, Utf16RangeError> {
    if chrg_min < 0 || chrg_max < 0 {
        return Err(Utf16RangeError::Negative);
    }
    if chrg_min >= chrg_max {
        return Err(Utf16RangeError::Empty);
    }
    let start = utf16_to_byte_exact(text, chrg_min as usize)?;
    let end = utf16_to_byte_exact(text, chrg_max as usize)?;
    Ok(&text[start..end])
}

/// Map one RichEdit character position to a UTF-8 byte offset, without the
/// silent clamping of `utf16_range_to_byte_range`: a position past the end
/// or inside a surrogate pair is an error. `\r\n` counts as 1, a lone `\r`
/// as 1 (RichEdit's own counting).
pub(super) fn utf16_to_byte_exact(text: &str, pos: usize) -> Result<usize, Utf16RangeError> {
    let bytes = text.as_bytes();
    let mut u16_pos: usize = 0;
    for (byte_idx, ch) in text.char_indices() {
        if ch == '\n' && byte_idx > 0 && bytes[byte_idx - 1] == b'\r' {
            continue;
        }
        if u16_pos == pos {
            return Ok(byte_idx);
        }
        let width = ch.len_utf16();
        if pos < u16_pos + width {
            return Err(Utf16RangeError::MidSurrogate { pos });
        }
        u16_pos += width;
    }
    if u16_pos == pos {
        return Ok(text.len());
    }
    Err(Utf16RangeError::OutOfBounds { max: pos, len_u16: u16_pos })
}

/// Build a lookup table mapping each UTF-8 byte offset to a UTF-16 code-unit offset.
/// The returned Vec has length `text.len() + 1` so the end-of-string position is mapped.
pub(in crate::ui) fn build_utf16_offset_map(text: &str) -> Vec<usize> {
    let mut map = vec![0usize; text.len() + 1];
    let mut u16_pos: usize = 0;
    let bytes = text.as_bytes();

    for (byte_idx, ch) in text.char_indices() {
        map[byte_idx] = u16_pos;
        // RichEdit counts \r\n as a single character — skip \n after \r
        if ch == '\n' && byte_idx > 0 && bytes[byte_idx - 1] == b'\r' {
            continue;
        }
        u16_pos += ch.len_utf16();
    }
    map[text.len()] = u16_pos;

    map
}

/// Longest selection whose other occurrences get highlighted.
pub(super) const MAX_HIGHLIGHT_CHARS: usize = 256;

/// The term to highlight for the current selection, if any: at least 2
/// bytes, single line, no whitespace at the edges.
pub(super) fn highlight_term(selected: &str) -> Option<&str> {
    let ok = selected.len() >= 2
        && !selected.contains('\n')
        && !selected.contains('\r')
        && selected == selected.trim()
        && selected.chars().count() <= MAX_HIGHLIGHT_CHARS;
    ok.then_some(selected)
}

/// Widen a word-selection (as produced by RichEdit's double-click
/// word-break) to the whole whitespace-delimited word (issue 045).
///
/// RichEdit's default word-break splits `CT6A61EN` at every letter↔digit
/// transition and `user@x.com` at the punctuation, and word-selects often
/// include the trailing space. This function:
///
/// 1. Tightens the range to its non-whitespace core (so the trailing space
///    doesn't let the expand step bridge onto the next word).
/// 2. Extends both ends until whitespace (`char::is_whitespace`: U+00A0
///    splits, U+200B does not; CJK text without spaces is one word).
///
/// Nothing is stripped: the editor mostly holds secrets, and any
/// punctuation rule would truncate some password (`Pass@word1!`, `abc)`).
///
/// Returns `Some((byte_start, byte_end))` with the widened range, or `None`
/// when the input range is invalid or is only whitespace.
pub(super) fn expand_word_selection(text: &str, start: usize, end: usize) -> Option<(usize, usize)> {
    if start > end || end > text.len() {
        return None;
    }

    let started_empty = start == end;
    let mut byte_start = start;
    let mut byte_end = end;

    // Trim whitespace from a non-empty range so the expand step doesn't
    // bridge across a trailing/leading space onto the next word. When the
    // range is empty (caret-between-chars from a double-click that landed
    // on a character boundary), there is nothing to trim — fall through to
    // expansion, which grows in both directions from the caret position.
    if !started_empty {
        while byte_start < byte_end {
            let ch = text[byte_start..].chars().next()?;
            if !ch.is_whitespace() {
                break;
            }
            byte_start += ch.len_utf8();
        }
        while byte_end > byte_start {
            let (i, ch) = text[..byte_end].char_indices().next_back()?;
            if !ch.is_whitespace() {
                break;
            }
            byte_end = i;
        }
        // A non-empty range that trimmed down to nothing was pure whitespace;
        // don't silently grab a neighbouring word the user didn't select.
        if byte_start >= byte_end {
            return None;
        }
    }

    while byte_start > 0 {
        let Some((i, ch)) = text[..byte_start].char_indices().next_back() else { break };
        if ch.is_whitespace() {
            break;
        }
        byte_start = i;
    }
    while byte_end < text.len() {
        let Some(ch) = text[byte_end..].chars().next() else { break };
        if ch.is_whitespace() {
            break;
        }
        byte_end += ch.len_utf8();
    }

    if byte_start >= byte_end {
        return None;
    }
    Some((byte_start, byte_end))
}

/// Find all case-insensitive occurrences of `needle` in `text`.
/// Returns byte-offset pairs `(start, end)` in the **original** `text`,
/// so they can safely index into a UTF-16 offset map built from `text`.
pub(in crate::ui) fn find_all_case_insensitive(text: &str, needle: &str) -> Vec<(usize, usize)> {
    case_insensitive_matches(text, needle).collect()
}

/// The occurrences of `find_all_case_insensitive`, found one at a time (a
/// caller that needs only a few stops early). Overlapping, in text order.
pub(in crate::ui) fn case_insensitive_matches<'a>(text: &'a str, needle: &str) -> impl Iterator<Item = (usize, usize)> + 'a {
    let needle_lower: Vec<char> = needle.chars().flat_map(|c| c.to_lowercase()).collect();
    // An empty needle finds nothing.
    let starts = if needle_lower.is_empty() { 0 } else { usize::MAX };
    text.char_indices().take(starts).filter_map(move |(start, _)| {
        let mut ni = 0; // index into needle_lower
        let mut end = start;
        for c in text[start..].chars() {
            if ni == needle_lower.len() {
                break;
            }
            for lc in c.to_lowercase() {
                if ni >= needle_lower.len() || lc != needle_lower[ni] {
                    return None;
                }
                ni += 1;
            }
            end += c.len_utf8();
        }
        (ni == needle_lower.len()).then_some((start, end))
    })
}

/// Password dialog for steganography operations.
///
/// When `extract` is false (hide/embed), shows two fields (password + confirm).
/// When `extract` is true, shows a single password field.
/// Returns `None` if cancelled.
pub(super) fn stego_password_dialog(parent: &nwg::ControlHandle, extract: bool) -> Option<String> {
    use std::cell::RefCell;
    use std::rc::Rc;

    #[link(name = "user32")]
    extern "system" {
        fn GetSystemMetrics(index: i32) -> i32;
    }

    let win_h: i32 = if extract { 170 } else { 220 };
    let win_w: i32 = 380;
    let screen_w = unsafe { GetSystemMetrics(0) };
    let screen_h = unsafe { GetSystemMetrics(1) };
    let pos_x = (screen_w - win_w) / 2;
    let pos_y = (screen_h - win_h) / 2;

    let title = if extract {
        t("stego.pw.title_extract")
    } else {
        t("stego.pw.title_hide")
    };

    let mut window = Default::default();
    nwg::Window::builder()
        .size((win_w, win_h))
        .position((pos_x, pos_y))
        .title(&title)
        .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
        .parent(Some(parent))
        .build(&mut window)
        .expect("Failed to build stego password dialog");

    crate::ui::dialogs::apply_dialog_theme(&window);

    let mut font = Default::default();
    nwg::Font::builder()
        .family("Segoe UI")
        .size(16)
        .build(&mut font)
        .expect("Failed to build font");

    let margin: i32 = 20;
    let content_w: i32 = win_w - margin * 2;

    let prompt = if extract {
        t("stego.pw.prompt_extract")
    } else {
        t("stego.pw.prompt_hide")
    };

    let mut _lbl = Default::default();
    nwg::Label::builder()
        .text(&prompt)
        .parent(&window)
        .position((margin, 15))
        .size((content_w, 22))
        .font(Some(&font))
        .build(&mut _lbl)
        .expect("Failed to build label");

    let mut txt_pw = Default::default();
    nwg::TextInput::builder()
        .parent(&window)
        .position((margin, 42))
        .size((content_w, 28))
        .password(Some('*'))
        .font(Some(&font))
        .focus(true)
        .build(&mut txt_pw)
        .expect("Failed to build password input");

    // Confirm field (always built, hidden in extract mode)
    let mut txt_confirm = Default::default();
    nwg::TextInput::builder()
        .parent(&window)
        .position((margin, 105))
        .size((content_w, 28))
        .password(Some('*'))
        .font(Some(&font))
        .build(&mut txt_confirm)
        .expect("Failed to build confirm input");

    if !extract {
        let mut _lbl2 = Default::default();
        nwg::Label::builder()
            .text(&t("stego.pw.confirm"))
            .parent(&window)
            .position((margin, 78))
            .size((content_w, 22))
            .font(Some(&font))
            .build(&mut _lbl2)
            .expect("Failed to build confirm label");
        txt_confirm.set_visible(true);
    } else {
        txt_confirm.set_visible(false);
    }

    let err_y = if extract { 78 } else { 140 };
    let btn_y = if extract { 105 } else { 168 };

    let mut lbl_error = Default::default();
    nwg::Label::builder()
        .text("")
        .parent(&window)
        .position((margin, err_y))
        .size((content_w, 22))
        .font(Some(&font))
        .build(&mut lbl_error)
        .expect("Failed to build error label");

    let mut btn_ok = Default::default();
    nwg::Button::builder()
        .text(&t("settings.ok"))
        .parent(&window)
        .position((win_w - margin - 185, btn_y))
        .size((85, 32))
        .font(Some(&font))
        .build(&mut btn_ok)
        .expect("Failed to build OK button");

    let mut btn_cancel = Default::default();
    nwg::Button::builder()
        .text(&t("settings.cancel"))
        .parent(&window)
        .position((win_w - margin - 90, btn_y))
        .size((90, 32))
        .font(Some(&font))
        .build(&mut btn_cancel)
        .expect("Failed to build Cancel button");

    crate::ui::dialogs::bind_enter_to_button(window.handle, btn_ok.handle);

    let result: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let result_clone = result.clone();
    let window_handle = window.handle;
    let is_extract = extract;

    let handler = nwg::full_bind_event_handler(&window_handle, move |evt, _evt_data, handle| {
        match evt {
            nwg::Event::OnButtonClick => {
                if handle == btn_ok.handle {
                    let pw = txt_pw.text();
                    if pw.is_empty() {
                        lbl_error.set_text(&t("stego.pw.empty"));
                        return;
                    }
                    if !is_extract {
                        let confirm = txt_confirm.text();
                        if pw != confirm {
                            lbl_error.set_text(&t("stego.pw.mismatch"));
                            txt_confirm.set_text("");
                            return;
                        }
                    }
                    *result_clone.borrow_mut() = Some(pw);
                    txt_pw.set_text("");
                    txt_confirm.set_text("");
                    nwg::stop_thread_dispatch();
                } else if handle == btn_cancel.handle {
                    txt_pw.set_text("");
                    txt_confirm.set_text("");
                    nwg::stop_thread_dispatch();
                }
            }
            nwg::Event::OnWindowClose => {
                txt_pw.set_text("");
                txt_confirm.set_text("");
                nwg::stop_thread_dispatch();
            }
            _ => {}
        }
    });

    nwg::dispatch_thread_events();
    nwg::unbind_event_handler(&handler);

    let val = result.borrow_mut().take();
    val
}

/// Detect the encoding of raw bytes and decode to a UTF-8 String.
/// Heuristic: is this path likely a text file?
///
/// 1. Known-binary extensions → false (fast path).
/// 2. Otherwise sniff first 8 KiB:
///    - BOM (UTF-8 / UTF-16 LE/BE) → text
///    - Any nul byte → binary
///    - Valid UTF-8 on the prefix → text (with handling for split multi-byte sequence at the tail)
///    - Else → binary
pub(super) fn is_probably_text_file(path: &std::path::Path) -> bool {
    use std::io::Read;

    const BINARY_EXTS: &[&str] = &[
        "exe", "dll", "bin", "obj", "o", "lib", "so", "dylib",
        "zip", "rar", "7z", "gz", "tar", "bz2", "xz", "zst",
        "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx",
        "png", "jpg", "jpeg", "gif", "bmp", "ico", "webp", "svg", "tiff",
        "mp3", "mp4", "avi", "mkv", "wav", "flac", "ogg", "wmv",
        "msi", "iso", "cab", "wim",
    ];

    let ext = path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    if BINARY_EXTS.contains(&ext.as_str()) {
        return false;
    }

    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let mut buf = [0u8; 8192];
    let n = match file.read(&mut buf) {
        Ok(n) => n,
        Err(_) => return false,
    };
    if n == 0 {
        return true;
    }
    let data = &buf[..n];

    if data.starts_with(&[0xEF, 0xBB, 0xBF])
        || data.starts_with(&[0xFF, 0xFE])
        || data.starts_with(&[0xFE, 0xFF])
    {
        return true;
    }

    if data.contains(&0) {
        return false;
    }

    match std::str::from_utf8(data) {
        Ok(_) => true,
        Err(e) => {
            if e.error_len().is_none() {
                e.valid_up_to() > 0 || n < 4
            } else {
                false
            }
        }
    }
}

/// Returns `(decoded_text, encoding_name)`.
pub(super) fn detect_and_decode(bytes: &[u8]) -> (String, String) {
    // 1. Check for BOM
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        // UTF-8 BOM — strip the 3 BOM bytes
        let text = String::from_utf8_lossy(&bytes[3..]).into_owned();
        return (text, "UTF-8 BOM".to_string());
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        // UTF-16 LE BOM
        let (cow, _, _) = encoding_rs::UTF_16LE.decode(&bytes[2..]);
        return (cow.into_owned(), "UTF-16 LE".to_string());
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        // UTF-16 BE BOM
        let (cow, _, _) = encoding_rs::UTF_16BE.decode(&bytes[2..]);
        return (cow.into_owned(), "UTF-16 BE".to_string());
    }

    // 2. Try UTF-8
    if let Ok(text) = std::str::from_utf8(bytes) {
        return (text.to_string(), "UTF-8".to_string());
    }

    // 3. Fallback to Windows-1252 (most common non-UTF-8 encoding on Windows)
    let (cow, _, _) = encoding_rs::WINDOWS_1252.decode(bytes);
    (cow.into_owned(), "Windows-1252".to_string())
}

/// What a key pressed without Ctrl or Shift does in `handle_shortcut`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum PlainKey {
    /// F5: insert the date and time.
    InsertTimestamp,
    /// Escape.
    Escape,
    /// F2: rename the active tab (Tabs > Rename Tab...).
    RenameTab,
}

/// The action of a key pressed without Ctrl or Shift, `None` when it is not
/// a shortcut (the key goes on to the control).
pub(super) fn plain_key_action(vk: u32) -> Option<PlainKey> {
    match vk {
        0x74 => Some(PlainKey::InsertTimestamp), // F5
        0x1B => Some(PlainKey::Escape),          // Escape
        // Shown next to Tabs > Rename Tab... (issue #24). The vault tree
        // gets its own keys (handle_shortcut is not called there) and has
        // no label editing (build.rs), so F2 stays inert in it.
        0x71 => Some(PlainKey::RenameTab),       // F2
        _ => None,
    }
}

/// The menu label of a tab colour: `color.<name>` in the current language.
/// The stored name (`theme::TAB_COLORS`, "blue"...) is not translated.
pub(super) fn tab_color_label(name: &str) -> String {
    crate::features::i18n::get(&format!("color.{}", name)).to_string()
}

/// Wipe the text in `cell` without ever panicking: a `RefCell` borrowed
/// further up the stack (a dialog's nested loop under which WM_ENDSESSION or
/// the auto-lock runs, pass 3) makes `borrow_mut` panic, and the release
/// build aborts. Returns false and sets `pending` when it could not wipe now:
/// the caller retries (`retry_pending_wipe`).
pub(super) fn wipe_or_defer<T: zeroize::Zeroize>(cell: &RefCell<T>, pending: &Cell<bool>, what: &str) -> bool {
    match cell.try_borrow_mut() {
        Ok(mut s) => {
            s.zeroize();
            pending.set(false);
            true
        }
        Err(_) => {
            eprintln!("WARN wipe: {} borrowed further up, wipe deferred to the next tick", what);
            pending.set(true);
            false
        }
    }
}

/// Whether a WM_MOUSEMOVE is a real move: its packed position (lParam)
/// differs from the last one (pass 6: Windows sends one after a dialog
/// closes, cursor unchanged, which reset the auto-lock).
pub(super) fn mouse_really_moved(last: Option<isize>, now: isize) -> bool {
    last != Some(now)
}

/// Whether input and display behave as locked: locked, or a curtain up
/// (pass 6: one predicate for every input entry point and the layout).
pub(super) fn input_blocked_by(locked: bool, curtain: super::modal_guard::Curtain) -> bool {
    locked || curtain != super::modal_guard::Curtain::None
}

/// The curtain subtitle while a lock waits: which window it waits for
/// (pass 6), when its title is known.
pub(super) fn curtain_waiting_text(window: &str) -> String {
    if window.trim().is_empty() {
        crate::features::i18n::get("lock.curtain").to_string()
    } else {
        crate::features::i18n::get("lock.curtain.waiting_for").replace("{window}", window.trim())
    }
}

/// Which subtitle the curtain shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum CurtainSubtitle {
    /// A window keeps the lock waiting (named, `lock.curtain.waiting_for`).
    WaitingFor,
    /// The save before the lock failed (`lock.curtain.save_failed`).
    SaveFailed,
    /// Just "enter your password" (`lock.curtain`).
    Password,
}

/// A window that keeps the lock waiting comes first (pass 9): a failed save
/// is retried only once it is gone.
pub(super) fn curtain_subtitle(modal_open: bool, save_failed: bool) -> CurtainSubtitle {
    if modal_open {
        CurtainSubtitle::WaitingFor
    } else if save_failed {
        CurtainSubtitle::SaveFailed
    } else {
        CurtainSubtitle::Password
    }
}

/// Whether the window's activation gives the focus to the password field:
/// locked, or the curtain (it has one, pass 8).
pub(super) fn focus_password_field(locked: bool, curtain: super::modal_guard::Curtain) -> bool {
    locked || curtain != super::modal_guard::Curtain::None
}

/// What `layout_controls` lays out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum LayoutMode {
    /// Locked or under the curtain: only the lock screen; no panel, editor
    /// or gutter is shown again behind it.
    OverlayOnly,
    Full,
}

pub(super) fn layout_mode(blocked: bool) -> LayoutMode {
    if blocked {
        LayoutMode::OverlayOnly
    } else {
        LayoutMode::Full
    }
}

/// Wipe the TOTP secrets of a `Settings` (a copy given to a dialog, or the
/// live one at lock / end of session).
pub(super) fn zeroize_settings_secrets(s: &mut crate::settings::Settings) {
    use zeroize::Zeroize;
    for e in s.totp_entries.iter_mut() {
        e.secret.zeroize();
    }
    s.totp_entries.clear();
    s.totp_2fa_secret.zeroize();
}

/// The wait after the `attempts`-th wrong password on the curtain: the lock
/// screen's backoff (pass 7), never an exit.
pub(super) fn curtain_wrong_password_delay(attempts: u32) -> u32 {
    super::lock_overlay::unlock_backoff_seconds(attempts)
}

/// Whether the password typed on the curtain is the one in memory (constant time; an empty one never matches).
pub(super) fn curtain_password_ok(typed: &str, password: &str) -> bool {
    !password.is_empty() && crate::ui::dialogs::confirm_password::constant_time_eq(typed.as_bytes(), password.as_bytes())
}

/// Whether a save may run: never while locked or once the session ended.
pub(super) fn save_allowed(locked: bool, exiting: bool) -> Result<(), &'static str> {
    if exiting {
        Err("exiting")
    } else if locked {
        Err("locked")
    } else {
        Ok(())
    }
}

/// The text of a failed save at lock: the reason, and that the note stays
/// open and unsaved.
pub(super) fn lock_save_failed_text(reason: &str) -> String {
    crate::features::i18n::get("lock.save_failed").replace("{error}", reason)
}

/// Whether a deferred wipe runs now: only while locked or exiting.
pub(super) fn wipe_retry_due(pending: bool, locked: bool, exiting: bool) -> bool {
    pending && (locked || exiting)
}

/// Remove `list[idx]` if it still exists; false (and a WARN) otherwise,
/// never a panic (pass 4).
pub(super) fn remove_checked<T>(list: &mut Vec<T>, idx: usize, what: &str) -> bool {
    if idx < list.len() {
        list.remove(idx);
        true
    } else {
        eprintln!("WARN {}: index {} gone (len {}), nothing removed", what, idx, list.len());
        false
    }
}

/// The check marks of View > Word Wrap and View > Line Numbers for these
/// settings, as (word wrap, line numbers).
pub(super) fn view_menu_checks(settings: &crate::settings::Settings) -> (bool, bool) {
    (settings.word_wrap, settings.show_line_numbers)
}

/// Whether `n` takes the singular in `lang`: French uses it for 0 and 1
/// ("0 mot"), English, Spanish and German for 1 only.
pub(super) fn is_singular(lang: crate::features::i18n::Language, n: usize) -> bool {
    match lang {
        crate::features::i18n::Language::French => n <= 1,
        _ => n == 1,
    }
}

/// "{n} word" / "{n} words" from the i18n keys `one` / `many` (status bar).
pub(super) fn count_text(n: usize, one: &str, many: &str) -> String {
    let key = if is_singular(crate::features::i18n::current_language(), n) { one } else { many };
    crate::features::i18n::get(key).replace("{n}", &n.to_string())
}

/// The translated text of an attachment size limit (the `Display` of
/// `AttachmentError` is English).
pub(super) fn attach_limit_message(e: &crate::features::attachments::AttachmentError) -> String {
    use crate::features::attachments::{format_size, AttachmentError};
    match e {
        AttachmentError::FileTooLarge { size, max } => crate::features::i18n::get("attach.error.toolarge")
            .replace("{size}", &format_size(*size))
            .replace("{max}", &format_size(*max)),
        AttachmentError::TotalTooLarge { total, max } => crate::features::i18n::get("attach.error.totaltoolarge")
            .replace("{size}", &format_size(*total))
            .replace("{max}", &format_size(*max)),
    }
}
