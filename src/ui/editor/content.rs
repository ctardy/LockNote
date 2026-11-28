// Content, status bar and line-number gutter.
// Part of the EditorForm implementation, split out of the former editor.rs.

#[allow(unused_imports)]
use super::*;

// ---------------------------------------------------------------------
// Pure layout helpers (no Win32 calls, unit-tested in tests.rs)
// ---------------------------------------------------------------------

/// Height of the status bar in pixels (1 px top line included).
pub(super) const STATUS_BAR_HEIGHT: i32 = 28;
/// Left inner margin of the editor text, in pixels (EM_SETMARGINS).
pub(super) const EDITOR_LEFT_MARGIN: i32 = 20;
/// Gap between the gutter numbers and the right edge of the gutter.
const GUTTER_RIGHT_PADDING: i32 = 14;
/// Horizontal margin at both ends of the status bar.
const STATUS_MARGIN: i32 = 12;
/// Gap between two status bar items.
const STATUS_ITEM_GAP: i32 = 20;
/// Gap between a status icon (dot, padlock) and its text.
const STATUS_ICON_GAP: i32 = 6;
/// Diameter of the "Saved" dot.
pub(super) const STATUS_DOT_SIZE: i32 = 7;
/// Width of the padlock icon.
pub(super) const STATUS_LOCK_WIDTH: i32 = 10;

/// Digit width assumed for a gutter font of `font_pt` when it cannot be
/// measured (`gutter_digit_px`): 0.6 em, the widest of the usual
/// monospaced families (Courier New, Lucida Console).
pub(super) fn estimated_gutter_digit_px(font_pt: f64) -> i32 {
    (f64::from(font_size_to_nwg(font_pt)) * 0.6).ceil() as i32
}

/// Gutter width for a document of `total_paragraphs` lines whose digits
/// are `digit_px` wide (`gutter_digit_px`): the digits plus the right
/// padding, never narrower than 40 px.
pub(super) fn gutter_width(total_paragraphs: usize, digit_px: i32) -> i32 {
    let digit_count = total_paragraphs.max(1).to_string().len() as i32;
    (digit_count * digit_px + GUTTER_RIGHT_PADDING + 4).max(40)
}

/// Width of the line-number gutter as laid out: none when the user hid the
/// line numbers (View > Line Numbers, setting `show_line_numbers`, issue #26).
pub(super) fn shown_gutter_width(show_line_numbers: bool, total_paragraphs: usize, digit_px: i32) -> i32 {
    if show_line_numbers {
        gutter_width(total_paragraphs, digit_px)
    } else {
        0
    }
}

/// The font the gutter draws its numbers with: the editor's family and
/// size (the zoom included, A2). The caller deletes it.
pub(super) fn gutter_font(family: &str, font_pt: f64) -> windows::Win32::Graphics::Gdi::HFONT {
    use windows::Win32::Graphics::Gdi::*;
    let face: Vec<u16> = family.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        CreateFontW(
            -(font_size_to_nwg(font_pt) as i32),
            0, 0, 0,
            FW_NORMAL.0 as i32,
            0, 0, 0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            (FF_DONTCARE.0 | FIXED_PITCH.0) as u32,
            windows::core::PCWSTR(face.as_ptr()),
        )
    }
}

static WARNED_GUTTER_DIGIT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Width in px of the widest digit of the gutter font, measured so the
/// numbers are never cut on the left whatever the family and the zoom. If
/// the font cannot be measured, the estimate is used and reported once.
pub(super) fn gutter_digit_px(family: &str, font_pt: f64) -> i32 {
    thread_local! {
        static MEMO: DigitWidthMemo = DigitWidthMemo::default();
    }
    match MEMO.with(|memo| memo.get(family, font_pt, || measure_gutter_digit(family, font_pt))) {
        Ok(px) => px,
        Err(e) => {
            let estimate = estimated_gutter_digit_px(font_pt);
            if !WARNED_GUTTER_DIGIT.swap(true, std::sync::atomic::Ordering::Relaxed) {
                eprintln!("WARN gutter: digit width of {family} {font_pt} pt not measured ({e}); {estimate} px assumed");
            }
            estimate
        }
    }
}

/// The last measured digit width and the font it is for: the gutter asks on
/// every repaint (scroll, caret move) and the layout on every resize, while
/// it only changes with the family or the size. A failure is not kept.
#[derive(Default)]
pub(super) struct DigitWidthMemo {
    last: std::cell::RefCell<Option<(String, f64, i32)>>,
}

impl DigitWidthMemo {
    pub(super) fn get(
        &self,
        family: &str,
        font_pt: f64,
        measure: impl FnOnce() -> Result<i32, String>,
    ) -> Result<i32, String> {
        if let Some((f, pt, px)) = self.last.borrow().as_ref() {
            if f == family && *pt == font_pt {
                return Ok(*px);
            }
        }
        let px = measure()?;
        *self.last.borrow_mut() = Some((family.to_string(), font_pt, px));
        Ok(px)
    }
}

fn measure_gutter_digit(family: &str, font_pt: f64) -> Result<i32, String> {
    use windows::Win32::Graphics::Gdi::*;
    let font = gutter_font(family, font_pt);
    if font.is_invalid() {
        return Err("CreateFontW failed".to_string());
    }
    let result = unsafe {
        let dc = GetDC(None);
        if dc.is_invalid() {
            let _ = DeleteObject(font.into());
            return Err("GetDC failed".to_string());
        }
        let old = SelectObject(dc, font.into());
        let mut widest = Ok(0);
        for digit in "0123456789".encode_utf16() {
            let mut size = windows::Win32::Foundation::SIZE::default();
            if !GetTextExtentPoint32W(dc, &[digit], &mut size).as_bool() {
                widest = Err("GetTextExtentPoint32W failed".to_string());
                break;
            }
            widest = widest.map(|w: i32| w.max(size.cx));
        }
        SelectObject(dc, old);
        ReleaseDC(None, dc);
        widest
    };
    unsafe {
        let _ = DeleteObject(font.into());
    }
    match result {
        Ok(px) if px > 0 => Ok(px),
        Ok(_) => Err("zero-width digits".to_string()),
        Err(e) => Err(e),
    }
}

/// X and width of the editor right of the gutter, left of the side panel.
pub(super) fn editor_columns(client_w: u32, right_panel_w: u32, gutter_w: u32) -> (i32, u32) {
    (gutter_w as i32, client_w.saturating_sub(right_panel_w).saturating_sub(gutter_w))
}

/// X coordinate of the right edge of the line numbers inside the gutter.
pub(super) fn gutter_number_right(gutter_w: i32) -> i32 {
    gutter_w - GUTTER_RIGHT_PADDING
}

/// i18n key of the save indicator text.
pub(super) fn save_indicator_text(modified: bool) -> &'static str {
    if modified {
        "status.modified"
    } else {
        "status.saved"
    }
}

/// X positions (left edges) of the status bar items, computed from the
/// measured text widths.
#[derive(Debug, PartialEq)]
pub(super) struct StatusLayout {
    /// Left edge of each left-hand text (words, characters, lines).
    pub left: Vec<i32>,
    /// Left edge of the "Saved" dot.
    pub dot_x: i32,
    pub saved_text_x: i32,
    /// Left edge of the padlock icon.
    pub lock_x: i32,
    pub encrypted_text_x: i32,
    /// Left edge of the right-most info (encoding), when there is one.
    pub info_x: Option<i32>,
    /// Left edge of the caret position ("Ln 3, Col 5"); None when it is not
    /// drawn (no room after the counters).
    pub caret_x: Option<i32>,
    /// Where the path of the LockNote file goes; None when it is not drawn.
    pub path: Option<PathSlot>,
}

/// Narrowest room in which the path of the LockNote file is still drawn.
pub(super) const STATUS_PATH_MIN_WIDTH: i32 = 60;
/// Timer (on the status bar window) that brings the path back after the
/// "Path copied" feedback, and how long that feedback stays.
const STATUS_PATH_FEEDBACK_TIMER: usize = 0x0570;
const STATUS_PATH_FEEDBACK_MS: u32 = 2000;

/// The rectangle (x, width; full bar height) of the path in the status bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PathSlot {
    pub x: i32,
    pub w: i32,
}

impl PathSlot {
    /// Whether the client x coordinate `x` falls on the path.
    pub fn contains(&self, x: i32) -> bool {
        x >= self.x && x < self.x + self.w
    }
}

/// Lays the status bar out: the counters flow from the left; from the right
/// edge come, right to left, the optional info, "Encrypted" (padlock + text),
/// the save indicator (dot + text) and the caret position. `saved_w` must be
/// the widest of the two save texts so the group does not jump when the
/// state changes. The caret position is drawn only when it fits after the
/// counters; the path takes what is left between the two.
pub(super) fn status_layout(
    client_w: i32,
    left_widths: &[i32],
    saved_w: i32,
    encrypted_w: i32,
    info_w: Option<i32>,
    path_w: Option<i32>,
    caret_w: Option<i32>,
) -> StatusLayout {
    let mut left = Vec::with_capacity(left_widths.len());
    let mut x = STATUS_MARGIN;
    for w in left_widths {
        left.push(x);
        x += w + STATUS_ITEM_GAP;
    }
    let path_x = x;

    let mut x = client_w - STATUS_MARGIN;
    let info_x = info_w.map(|w| {
        x -= w;
        let ix = x;
        x -= STATUS_ITEM_GAP;
        ix
    });
    x -= encrypted_w;
    let encrypted_text_x = x;
    x -= STATUS_ICON_GAP + STATUS_LOCK_WIDTH;
    let lock_x = x;
    x -= STATUS_ITEM_GAP + saved_w;
    let saved_text_x = x;
    x -= STATUS_ICON_GAP + STATUS_DOT_SIZE;
    let dot_x = x;
    // The caret position ends one gap before the save dot.
    let caret_x = caret_w.map(|w| dot_x - STATUS_ITEM_GAP - w).filter(|&cx| cx >= path_x);
    // The path takes what is left between the counters and the caret
    // position (or the save dot).
    let room = caret_x.unwrap_or(dot_x) - STATUS_ITEM_GAP - path_x;
    let path = path_w
        .filter(|_| room >= STATUS_PATH_MIN_WIDTH)
        .map(|w| PathSlot { x: path_x, w: w.min(room) });
    StatusLayout { left, dot_x, saved_text_x, lock_x, encrypted_text_x, info_x, caret_x, path }
}

/// Status bar part (SB_SETTEXTW index) of the caret position; the counters
/// are parts 0 to 2 and the encoding is the last part.
pub(super) const STATUS_CARET_PART: usize = 3;
/// Number of status bar parts (build.rs, SB_SETPARTS).
pub(super) const STATUS_PART_COUNT: usize = 5;

/// Where the caret is, as the status bar shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CaretStatus {
    /// 1-based line (hard line breaks, as the gutter numbers them).
    pub line: usize,
    /// 1-based column, in characters as displayed: a tab and a surrogate
    /// pair each count as 1.
    pub col: usize,
    /// Selected characters, a line break counting as 1 (as RichEdit).
    pub selected: usize,
}

/// The caret status from the editor text (`get_content`, \r\n line breaks)
/// and the RichEdit UTF-16 offsets of the selection and of the caret.
pub(super) fn caret_status(text: &str, sel_min: usize, sel_max: usize, caret: usize) -> CaretStatus {
    // Byte offsets on char boundaries (an offset past the end maps to the end).
    let (caret_b, _) = utf16_range_to_byte_range(text, caret, caret);
    let before = &text[..caret_b];
    let line_start = before.rfind('\n').map_or(0, |p| p + 1);
    let (start_b, end_b) = utf16_range_to_byte_range(text, sel_min, sel_max);
    let selected = text
        .get(start_b..end_b)
        .map_or(0, |s| s.chars().count() - s.matches("\r\n").count());
    CaretStatus {
        line: before.matches('\n').count() + 1,
        col: before[line_start..].chars().count() + 1,
        selected,
    }
}

/// "Ln 3, Col 5", then " (12 selected)" when the selection is not empty.
pub(super) fn caret_status_text(s: CaretStatus) -> String {
    let position = t("status.caret")
        .replace("{line}", &s.line.to_string())
        .replace("{col}", &s.col.to_string());
    if s.selected == 0 {
        position
    } else {
        format!("{} ({})", position, count_text(s.selected, "status.selected.one", "status.selected.many"))
    }
}

/// The window side of the caret status refresh, behind a trait so the flow
/// is tested without a window.
pub(super) trait CaretStatusSteps {
    /// The selection `(min, max)` and the caret, RichEdit UTF-16 offsets.
    fn selection(&self) -> (usize, usize, usize);
    /// The editor text for display (`display_content`).
    fn text(&self) -> Result<String, EditorTextError>;
    /// The caret status the bar shows now (trimmed part text).
    fn shown(&self) -> String;
    /// Show `text` in the caret part and repaint the bar.
    fn show(&mut self, text: &str);
}

/// Refresh the caret status: called on every highlight timer tick (caret
/// moves, selection changes) and with the counters (typing, tab switch). The
/// bar is repainted only when the text changes. Returns whether it did.
pub(super) fn refresh_caret_status(steps: &mut impl CaretStatusSteps) -> bool {
    let (min, max, caret) = steps.selection();
    let whole = match steps.text() {
        Ok(whole) => whole,
        Err(e) => {
            // A status line is not worth a dialog (it runs on every tick).
            eprintln!("ERROR status: caret status not refreshed, editor text unreadable err={:?}", e);
            return false;
        }
    };
    let text = caret_status_text(caret_status(&whole, min, max, caret));
    if steps.shown() == text {
        return false;
    }
    steps.show(&text);
    true
}

/// `CaretStatusSteps` on the editor window.
struct WindowCaretStatus<'a> {
    form: &'a EditorForm,
    editor: windows::Win32::Foundation::HWND,
    status: windows::Win32::Foundation::HWND,
}

impl CaretStatusSteps for WindowCaretStatus<'_> {
    fn selection(&self) -> (usize, usize, usize) {
        surface::selection_and_caret(self.editor)
    }
    fn text(&self) -> Result<String, EditorTextError> {
        self.form.display_content()
    }
    fn shown(&self) -> String {
        EditorForm::status_part_text(self.status, STATUS_CARET_PART)
    }
    fn show(&mut self, text: &str) {
        self.form.status_bar.set_text(STATUS_CARET_PART as u8, &format!(" {}", text));
        self.form.invalidate_status_bar();
    }
}

impl EditorForm {
    // =====================================================================
    // Content management
    // =====================================================================

    /// Set the editor text content.
    pub(super) fn set_content(&self, text: &str) {
        self.editor.set_text(text);
        self.is_modified.store(false, Ordering::Relaxed);
        { *self.detected_encoding.borrow_mut() = "UTF-8".to_string(); }
        self.update_status_bar();
        self.layout_controls();
    }

    /// The editor text, for every path that stores, exports, encrypts or
    /// moves it, or writes part of it back into the editor. A text that is
    /// not valid UTF-16 (a lone surrogate, which RichEdit keeps) is an error:
    /// the caller refuses the action and tells the user (`tell_unreadable`).
    /// Never nwg's `RichTextBox::text`: it turns such a text into the literal
    /// "Decoding error" (see `crate::ui::editor_text`).
    pub(super) fn get_content(&self) -> Result<String, EditorTextError> {
        crate::ui::editor_text::read_strict(crate::ui::editor_text::hwnd_of(self.editor.handle))
    }

    /// The editor text for display and positions only (counts, gutter,
    /// highlights, caret moves, search results, print): each lone surrogate
    /// is one U+FFFD, so positions match the RichEdit's. Never stored, never
    /// written back into the note.
    pub(super) fn display_content(&self) -> Result<String, EditorTextError> {
        crate::ui::editor_text::read_display(crate::ui::editor_text::hwnd_of(self.editor.handle))
    }

    /// `action` was refused because the editor text could not be read
    /// strictly: logged (no content), then told to the user. Only for
    /// actions the user started, never from EN_CHANGE / typing handlers.
    ///
    /// After the message, the first invalid character is selected: it may
    /// show as nothing, and Delete then removes it. Not done for a refused
    /// save, which autosave also runs: selecting the first half of an emoji
    /// being typed would let its second half replace it.
    pub(super) fn tell_unreadable(&self, action: &str, e: &EditorTextError) {
        eprintln!("WARN editor_text: refused action={} err={:?}", action, e);
        let epoch = self.lock_epoch();
        self.inform(&EditorTextError::title(), &e.user_message());
        if !matches!(e, EditorTextError::InvalidUtf16 { .. }) || !self.still_valid(epoch) {
            return;
        }
        let hwnd = crate::ui::editor_text::hwnd_of(self.editor.handle);
        match crate::ui::editor_text::select_first_lone_surrogate(hwnd) {
            Ok(Some(pos)) => eprintln!("INFO editor_text: selected action={} position={}", action, pos),
            Ok(None) => eprintln!("INFO editor_text: nothing to select action={}", action),
            Err(err) => eprintln!("WARN editor_text: select failed action={} err={:?}", action, err),
        }
    }

    /// Mark the document as modified and update the title.
    pub(super) fn mark_modified(&self) {
        self.is_modified.store(true, Ordering::Relaxed);
        self.update_title();
    }

    /// Mark the document as clean and update the title.
    pub(super) fn mark_clean(&self) {
        self.is_modified.store(false, Ordering::Relaxed);
        self.update_title();
    }

    /// Update window title: "LockNote" or "LockNote *" if modified.
    pub(super) fn update_title(&self) {
        let title = if self.is_modified.load(Ordering::Relaxed) {
            t("app.title.modified")
        } else {
            t("app.title")
        };
        self.window.set_text(&title);
        self.invalidate_status_bar();
    }

    // =====================================================================
    // Status bar
    // =====================================================================

    /// Update the status bar with word/character/line counts.
    pub(super) fn update_status_bar(&self) {
        match self.display_content() {
            Ok(text) => {
                let char_count = text.chars().count();
                let line_count = if text.is_empty() { 1 } else { text.lines().count().max(1) };
                let word_count = text.split_whitespace().count();

                self.status_bar.set_text(0, &format!(" {}", count_text(word_count, "status.word", "status.words")));
                self.status_bar.set_text(1, &format!(" {}", count_text(char_count, "status.char", "status.chars")));
                self.status_bar.set_text(2, &format!(" {}", count_text(line_count, "status.line", "status.lines")));
            }
            // Runs on every keystroke: logged, never a dialog.
            Err(e) => eprintln!("ERROR status: counts not refreshed, editor text unreadable err={:?}", e),
        }
        self.refresh_caret_status();

        {
            let enc = self.detected_encoding.borrow();
            self.status_bar.set_text((STATUS_PART_COUNT - 1) as u8, &format!(" {}", &*enc));
        }
        self.invalidate_status_bar();
    }

    /// Update the caret position / selection of the status bar if it changed.
    pub(super) fn refresh_caret_status(&self) {
        let (nwg::ControlHandle::Hwnd(editor), nwg::ControlHandle::Hwnd(status)) =
            (self.editor.handle, self.status_bar.handle)
        else {
            return;
        };
        refresh_caret_status(&mut WindowCaretStatus {
            form: self,
            editor: windows::Win32::Foundation::HWND(editor as *mut _),
            status: windows::Win32::Foundation::HWND(status as *mut _),
        });
    }

    /// Ask the status bar to repaint (it is custom-painted, see
    /// `bind_status_bar_paint`): the save indicator depends on `is_modified`.
    pub(super) fn invalidate_status_bar(&self) {
        if let nwg::ControlHandle::Hwnd(h) = self.status_bar.handle {
            unsafe {
                let _ = windows::Win32::Graphics::Gdi::InvalidateRect(
                    Some(windows::Win32::Foundation::HWND(h as *mut _)),
                    None,
                    false,
                );
            }
        }
    }

    /// Pin the status bar to the bottom edge of the client area. The bar is
    /// created with CCS_NORESIZE | CCS_NOPARENTALIGN (build.rs) so its height
    /// is `STATUS_BAR_HEIGHT` and nothing else moves it.
    pub(super) fn position_status_bar(&self, client_w: i32, client_h: i32) {
        if let nwg::ControlHandle::Hwnd(h) = self.status_bar.handle {
            use windows::Win32::UI::WindowsAndMessaging::*;
            unsafe {
                let _ = SetWindowPos(
                    windows::Win32::Foundation::HWND(h as *mut _),
                    None,
                    0, client_h - STATUS_BAR_HEIGHT, client_w, STATUS_BAR_HEIGHT,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
        }
    }

    /// Install the status bar painter: a raw (subclass) handler that owns
    /// WM_ERASEBKGND and WM_PAINT. The system status bar cannot show a drawn
    /// dot or padlock, nor a custom text colour, so the whole bar is drawn
    /// here; the part texts set by `update_status_bar` stay the single source
    /// of the counters (read back with SB_GETTEXTW).
    pub(super) fn bind_status_bar_paint(form: std::rc::Rc<EditorForm>) {
        let f = form.clone();
        let handler = nwg::bind_raw_event_handler(
            &form.status_bar.handle,
            0x40020,
            move |hwnd, msg, wparam, lparam| match msg {
                0x0014 => Some(1), // WM_ERASEBKGND: painted in WM_PAINT
                0x000F => {
                    f.paint_status_bar(windows::Win32::Foundation::HWND(hwnd as *mut _));
                    Some(0)
                }
                // WM_LBUTTONDOWN on the path, then WM_LBUTTONUP on it: a
                // click copies it (a drag that only ends there does not).
                0x0201 | 0x0202 => {
                    let x = (lparam & 0xFFFF) as u16 as i16 as i32;
                    let on_path = f.status_path_slot.get().is_some_and(|s| s.contains(x));
                    let h = windows::Win32::Foundation::HWND(hwnd as *mut _);
                    if msg == 0x0201 {
                        if !on_path {
                            return None;
                        }
                        f.status_path_pressed.set(true);
                        unsafe { windows::Win32::UI::Input::KeyboardAndMouse::SetCapture(h) };
                        return Some(0);
                    }
                    if !f.status_path_pressed.replace(false) {
                        return None;
                    }
                    if let Err(e) = unsafe { windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture() } {
                        eprintln!("WARN status_bar: ReleaseCapture failed: {}", e);
                    }
                    if on_path {
                        f.copy_exe_path(h);
                    }
                    Some(0)
                }
                // WM_CAPTURECHANGED: the press is lost (another window took
                // the mouse), no click.
                0x0215 => {
                    f.status_path_pressed.set(false);
                    None
                }
                // WM_SETCURSOR over the client area: a hand over the path.
                0x0020 if (lparam & 0xFFFF) == 1 => {
                    use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, LoadCursorW, SetCursor, IDC_HAND};
                    let mut pt = windows::Win32::Foundation::POINT::default();
                    let over = unsafe {
                        GetCursorPos(&mut pt).is_ok()
                            && windows::Win32::Graphics::Gdi::ScreenToClient(
                                windows::Win32::Foundation::HWND(hwnd as *mut _),
                                &mut pt,
                            )
                            .as_bool()
                    } && f.status_path_slot.get().is_some_and(|s| s.contains(pt.x));
                    if over {
                        if let Ok(hand) = unsafe { LoadCursorW(None, IDC_HAND) } {
                            unsafe { SetCursor(Some(hand)) };
                            return Some(1);
                        }
                    }
                    None
                }
                // WM_TIMER of the click feedback: the path comes back.
                0x0113 if wparam == STATUS_PATH_FEEDBACK_TIMER => {
                    let killed = unsafe {
                        windows::Win32::UI::WindowsAndMessaging::KillTimer(
                            Some(windows::Win32::Foundation::HWND(hwnd as *mut _)),
                            STATUS_PATH_FEEDBACK_TIMER,
                        )
                    };
                    if let Err(e) = killed {
                        // It fires again in 2 s and only clears the feedback again.
                        eprintln!("WARN status_bar: KillTimer failed: {}", e);
                    }
                    *f.status_path_feedback.borrow_mut() = None;
                    f.invalidate_status_bar();
                    Some(0)
                }
                _ => None,
            },
        );
        match handler {
            Ok(h) => std::mem::forget(h),
            Err(e) => panic!("status bar paint handler: {e:?}"),
        }
    }

    /// Copy the full path of the LockNote file to the clipboard and show,
    /// for `STATUS_PATH_FEEDBACK_MS`, whether it worked in place of the path.
    /// A path is not a secret: the clipboard guard is not armed.
    fn copy_exe_path(&self, status_hwnd: windows::Win32::Foundation::HWND) {
        use clipboard_win::{formats, set_clipboard};
        let path = self.exe_path.display().to_string();
        let feedback = match set_clipboard(formats::Unicode, &path) {
            Ok(()) => t("status.path_copied"),
            Err(e) => {
                eprintln!("WARN status_bar: clipboard: {} path={}", e, path);
                t("status.path_copy_failed")
            }
        };
        let timer = unsafe {
            windows::Win32::UI::WindowsAndMessaging::SetTimer(
                Some(status_hwnd),
                STATUS_PATH_FEEDBACK_TIMER,
                STATUS_PATH_FEEDBACK_MS,
                None,
            )
        };
        if timer == 0 {
            // Without the timer the feedback would never go away.
            eprintln!("WARN status_bar: SetTimer failed: {}", windows::core::Error::from_thread());
        } else {
            *self.status_path_feedback.borrow_mut() = Some(feedback);
        }
        self.invalidate_status_bar();
    }

    /// Text of status bar part `index` (SB_GETTEXTLENGTHW / SB_GETTEXTW), trimmed.
    fn status_part_text(hwnd: windows::Win32::Foundation::HWND, index: usize) -> String {
        use windows::Win32::UI::WindowsAndMessaging::SendMessageW;
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        unsafe {
            let len = (SendMessageW(hwnd, 0x040C, Some(WPARAM(index)), None).0 & 0xFFFF) as usize;
            if len == 0 {
                return String::new();
            }
            let mut buf = vec![0u16; len + 1];
            SendMessageW(hwnd, 0x040D, Some(WPARAM(index)), Some(LPARAM(buf.as_mut_ptr() as isize)));
            String::from_utf16_lossy(&buf[..len]).trim().to_string()
        }
    }

    /// Paint the whole status bar into an off-screen bitmap, then blit it.
    fn paint_status_bar(&self, hwnd: windows::Win32::Foundation::HWND) {
        use windows::Win32::Foundation::{COLORREF, RECT, SIZE};
        use windows::Win32::Graphics::Gdi::*;

        let pal = crate::theme::current();
        #[allow(unused_mut)]
        let mut texts: Vec<String> = (0..STATUS_PART_COUNT).map(|i| Self::status_part_text(hwnd, i)).collect();
        // The last part is the detected encoding, drawn on the right; the
        // caret position goes before the save indicator.
        let info = texts.pop();
        let caret = texts.remove(STATUS_CARET_PART);

        let modified = self.is_modified.load(Ordering::Relaxed);
        let saved_text = t(save_indicator_text(modified));
        let saved_texts = [false, true].map(|m| t(save_indicator_text(m)));
        let encrypted_text = t("status.encrypted");
        // The slot is sized on the path; the click feedback is drawn in it.
        let path = self.exe_path.display().to_string();
        let path_text = self.status_path_feedback.borrow().clone().unwrap_or_else(|| path.clone());

        unsafe {
            let mut ps = std::mem::zeroed::<PAINTSTRUCT>();
            let screen_dc = BeginPaint(hwnd, &mut ps);
            let mut rc = RECT::default();
            let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rc);
            let (w, h) = (rc.right - rc.left, rc.bottom - rc.top);

            let hdc = CreateCompatibleDC(Some(screen_dc));
            let bmp = CreateCompatibleBitmap(screen_dc, w, h);
            let old_bmp = SelectObject(hdc, bmp.into());
            // The font the control was created with (WM_GETFONT).
            let font = windows::Win32::UI::WindowsAndMessaging::SendMessageW(hwnd, 0x0031, None, None);
            let old_font = SelectObject(hdc, HGDIOBJ(font.0 as *mut _));

            let fill = |r: &RECT, c: u32| {
                let b = CreateSolidBrush(COLORREF(c));
                FillRect(hdc, r, b);
                let _ = DeleteObject(b.into());
            };
            // Background and 1 px top line.
            fill(&rc, pal.chrome.to_colorref());
            fill(&RECT { left: rc.left, top: rc.top, right: rc.right, bottom: rc.top + 1 }, pal.chrome_border.to_colorref());

            let measure = |s: &str| -> i32 {
                let wide: Vec<u16> = s.encode_utf16().collect();
                let mut size = SIZE::default();
                let _ = GetTextExtentPoint32W(hdc, &wide, &mut size);
                size.cx
            };
            let draw_text = |s: &str, x: i32, color: u32| {
                let mut wide: Vec<u16> = s.encode_utf16().collect();
                let mut r = RECT { left: x, top: rc.top + 1, right: x + measure(s) + 2, bottom: rc.bottom };
                SetBkMode(hdc, TRANSPARENT);
                SetTextColor(hdc, COLORREF(color));
                DrawTextW(hdc, &mut wide, &mut r, DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
            };

            let left_widths: Vec<i32> = texts.iter().map(|s| measure(s)).collect();
            let saved_w = saved_texts.iter().map(|s| measure(s)).max().unwrap_or(0);
            let info_w = info.as_ref().map(|s| measure(s));
            let caret_w = (!caret.is_empty()).then(|| measure(&caret));
            let lay = status_layout(w, &left_widths, saved_w, measure(&encrypted_text), info_w, Some(measure(&path)), caret_w);

            let secondary = pal.text_secondary.to_colorref();
            for (s, x) in texts.iter().zip(&lay.left) {
                draw_text(s, *x, secondary);
            }
            // Path of the LockNote file: shortened in the middle by Windows
            // (the file name stays) when the slot is narrower than the text.
            self.status_path_slot.set(lay.path);
            if let Some(slot) = lay.path {
                let mut wide: Vec<u16> = path_text.encode_utf16().collect();
                let mut r = RECT { left: slot.x, top: rc.top + 1, right: slot.x + slot.w, bottom: rc.bottom };
                SetBkMode(hdc, TRANSPARENT);
                SetTextColor(hdc, COLORREF(secondary));
                DrawTextW(hdc, &mut wide, &mut r, DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX | DT_PATH_ELLIPSIS);
            }
            if let (Some(s), Some(x)) = (info.as_ref(), lay.info_x) {
                draw_text(s, x, secondary);
            }
            if let Some(x) = lay.caret_x {
                draw_text(&caret, x, secondary);
            }

            let cy = rc.top + 1 + (rc.bottom - rc.top - 1) / 2;

            // Save indicator: filled dot (green = saved, accent = modified) + text.
            let dot_color = if modified {
                pal.accent
            } else {
                crate::theme::status_saved_color(pal)
            }
            .to_colorref();
            let dot_brush = CreateSolidBrush(COLORREF(dot_color));
            let dot_pen = CreatePen(PS_SOLID, 1, COLORREF(dot_color));
            let (ob, op) = (SelectObject(hdc, dot_brush.into()), SelectObject(hdc, dot_pen.into()));
            let top = cy - STATUS_DOT_SIZE / 2;
            let _ = Ellipse(hdc, lay.dot_x, top, lay.dot_x + STATUS_DOT_SIZE, top + STATUS_DOT_SIZE);
            SelectObject(hdc, ob);
            SelectObject(hdc, op);
            let _ = DeleteObject(dot_brush.into());
            let _ = DeleteObject(dot_pen.into());
            draw_text(&saved_text, lay.saved_text_x, secondary);

            // Padlock in the accent colour: arched shackle behind a filled body.
            let accent = pal.accent.to_colorref();
            let shackle_pen = CreatePen(PS_SOLID, 2, COLORREF(accent));
            let body_pen = CreatePen(PS_SOLID, 1, COLORREF(accent));
            let body_brush = CreateSolidBrush(COLORREF(accent));
            let x0 = lay.lock_x;
            let body_top = cy - 1;
            let op = SelectObject(hdc, shackle_pen.into());
            let ob = SelectObject(hdc, GetStockObject(NULL_BRUSH));
            let _ = RoundRect(hdc, x0 + 2, body_top - 6, x0 + STATUS_LOCK_WIDTH - 1, body_top + 3, 6, 6);
            SelectObject(hdc, body_pen.into());
            SelectObject(hdc, body_brush.into());
            let _ = RoundRect(hdc, x0, body_top, x0 + STATUS_LOCK_WIDTH, body_top + 8, 2, 2);
            SelectObject(hdc, op);
            SelectObject(hdc, ob);
            let _ = DeleteObject(shackle_pen.into());
            let _ = DeleteObject(body_pen.into());
            let _ = DeleteObject(body_brush.into());
            draw_text(&encrypted_text, lay.encrypted_text_x, secondary);

            let _ = BitBlt(screen_dc, 0, 0, w, h, Some(hdc), 0, 0, SRCCOPY);
            SelectObject(hdc, old_font);
            SelectObject(hdc, old_bmp);
            let _ = DeleteObject(bmp.into());
            let _ = DeleteDC(hdc);
            let _ = EndPaint(hwnd, &ps);
        }
    }

    // =====================================================================
    // Line number gutter
    // =====================================================================

    /// Repaint the gutter when the caret moved to another line since the last
    /// paint, so the current line number follows the caret. Called from the
    /// highlight timer tick (selection changes have no other hook).
    pub(super) fn repaint_gutter_if_caret_moved(&self) {
        let editor_hwnd = if let nwg::ControlHandle::Hwnd(h) = self.editor.handle {
            windows::Win32::Foundation::HWND(h as *mut _)
        } else {
            return;
        };
        let (line, selection_empty) = surface::caret_position(editor_hwnd);
        if surface::caret_key(line, selection_empty) != *self.gutter_caret_line.borrow() {
            self.paint_gutter();
        }
    }

    /// Paint line numbers in the gutter panel, synchronized with the editor scroll.
    ///
    /// Line numbers correspond to paragraphs (text separated by hard newlines),
    /// NOT visual lines produced by word-wrap.  The function reads the editor
    /// text once, builds a list of paragraph UTF-16 start offsets, then uses
    /// EM_POSFROMCHAR to obtain the Y position of each paragraph's first
    /// character.  Only paragraphs whose Y falls within the visible gutter
    /// area are drawn.
    pub(super) fn paint_gutter(&self) {
        let gutter = *self.gutter_hwnd.borrow();
        if gutter == 0 {
            return;
        }
        // Hidden by View > Line Numbers: layout_controls hid the window.
        if !self.settings.borrow().show_line_numbers {
            return;
        }

        let gutter_hwnd = windows::Win32::Foundation::HWND(gutter as *mut _);

        // Get editor HWND
        let editor_hwnd = if let nwg::ControlHandle::Hwnd(h) = self.editor.handle {
            windows::Win32::Foundation::HWND(h as *mut _)
        } else {
            return;
        };

        // -----------------------------------------------------------------
        // Use RichEdit's native line API to get paragraph count and offsets.
        // EM_GETLINECOUNT returns the number of lines; EM_LINEINDEX returns
        // the character index of the first character in a given line.
        // This handles all edge cases (trailing newlines, empty last line)
        // correctly, unlike manual offset calculation from WM_GETTEXT text.
        // -----------------------------------------------------------------
        let total_paragraphs = unsafe {
            let result = windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                editor_hwnd,
                0x00BA, // EM_GETLINECOUNT
                None,
                None,
            );
            (result.0 as usize).max(1)
        };

        // The font the editor shows (the zoom included, A2), copied out.
        let (font_family, font_size) = {
            let font = self.editor_font_state.borrow();
            (font.family.clone(), font.size_pt)
        };
        let gutter_width = gutter_width(total_paragraphs, gutter_digit_px(&font_family, font_size));

        // Caret line (same line domain as EM_LINEINDEX below) and whether a
        // selection exists; the band follows the editor's current-line band
        // (same function, so both always show the same line).
        let (caret_line, selection_empty) = surface::caret_position(editor_hwnd);
        *self.gutter_caret_line.borrow_mut() = surface::caret_key(caret_line, selection_empty);
        let band = surface::current_line_rect(editor_hwnd);
        // Distance between two line tops once the line spacing is applied.
        let Some(pitch) = surface::editor_line_pitch(editor_hwnd) else {
            return;
        };

        // Get theme colors
        let palette = crate::theme::current();
        let bg_color = palette.gutter_background.to_colorref();
        let text_color = palette.gutter_text.to_colorref();
        let caret_color = palette.text_primary.to_colorref();
        let band_color = surface::current_line_background(palette.gutter_background, palette).to_colorref();

        unsafe {
            use windows::Win32::Graphics::Gdi::*;

            let hdc = GetDC(Some(gutter_hwnd));
            if hdc.is_invalid() {
                return;
            }

            // Get gutter client rect
            let mut rect = windows::Win32::Foundation::RECT::default();
            let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(gutter_hwnd, &mut rect);

            // Fill background
            let bg_brush = CreateSolidBrush(windows::Win32::Foundation::COLORREF(bg_color));
            let _ = FillRect(hdc, &rect, bg_brush);
            let _ = DeleteObject(bg_brush.into());

            // Current-line band, under the numbers.
            if let Some((band_top, band_bottom)) = band {
                let band_rect = windows::Win32::Foundation::RECT {
                    left: rect.left,
                    top: band_top.max(rect.top),
                    right: rect.right,
                    bottom: band_bottom.min(rect.bottom),
                };
                if band_rect.top < band_rect.bottom {
                    let band_brush = CreateSolidBrush(windows::Win32::Foundation::COLORREF(band_color));
                    let _ = FillRect(hdc, &band_rect, band_brush);
                    let _ = DeleteObject(band_brush.into());
                }
            }

            // Create and select a font matching the editor font
            let hfont = gutter_font(&font_family, font_size);
            let old_font = SelectObject(hdc, hfont.into());

            // Set text properties
            let _ = SetBkMode(hdc, TRANSPARENT);
            let _ = SetTextColor(hdc, windows::Win32::Foundation::COLORREF(text_color));

            let gutter_height = rect.bottom - rect.top;

            // Iterate lines using EM_LINEINDEX + EM_POSFROMCHAR
            let mut prev_y: Option<i32> = None;
            for line_idx in 0..total_paragraphs {
                // EM_LINEINDEX: get character index of first char in this line
                let char_index = windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                    editor_hwnd,
                    0x00BB, // EM_LINEINDEX
                    Some(windows::Win32::Foundation::WPARAM(line_idx)),
                    None,
                );
                if char_index.0 == -1 {
                    continue; // invalid line
                }

                // EM_POSFROMCHAR: get Y position of that character
                let mut point = windows::Win32::Foundation::POINT { x: 0, y: 0 };
                let _ = windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                    editor_hwnd,
                    0x00D6, // EM_POSFROMCHAR
                    Some(windows::Win32::Foundation::WPARAM(&mut point as *mut _ as usize)),
                    Some(windows::Win32::Foundation::LPARAM(char_index.0 as isize)),
                );
                let y = point.y;

                // EM_POSFROMCHAR returns (-1,-1) for chars beyond text end;
                // for the last empty line, estimate y from previous line.
                let y = if point.x == -1 && y == -1 {
                    if let Some(py) = prev_y {
                        py + pitch
                    } else {
                        0
                    }
                } else {
                    y
                };

                prev_y = Some(y);

                // Skip paragraphs above visible area
                if y < -pitch {
                    continue;
                }
                // Stop once past the bottom of the visible area
                if y >= gutter_height {
                    break;
                }

                let num_str = format!("{}", line_idx + 1);
                let mut wide: Vec<u16> = num_str.encode_utf16().collect();
                let mut line_rect = windows::Win32::Foundation::RECT {
                    left: 0,
                    top: y,
                    right: gutter_number_right(gutter_width),
                    bottom: y + pitch,
                };
                let _ = SetTextColor(hdc, windows::Win32::Foundation::COLORREF(
                    if line_idx == caret_line { caret_color } else { text_color },
                ));
                let _ = DrawTextW(hdc, &mut wide, &mut line_rect, DT_RIGHT | DT_SINGLELINE | DT_NOPREFIX);
            }

            // Restore old font and clean up
            SelectObject(hdc, old_font);
            let _ = DeleteObject(hfont.into());
            let _ = ReleaseDC(Some(gutter_hwnd), hdc);
        }
    }
}
