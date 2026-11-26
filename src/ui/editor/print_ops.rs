// Printing (File > Print, Ctrl+P).
// Part of the EditorForm implementation, split out of the former editor.rs.
//
// The page is laid out by pure functions (`layout_pages` and its helpers)
// that take a text-width measure, so wrapping and pagination are tested
// without a printer; `print_to_dc` only measures and draws. `run_print`
// chains the dialogs and the printing (`PrintSteps`, tested with a fake).

#[allow(unused_imports)]
use super::*;

/// Why a print job stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PrintError {
    /// PrintDlgW failed (its CommDlgExtendedError code; 0 is a cancel).
    Dialog(u32),
    /// Not one body line fits between the margins with the editor font.
    PageTooSmall,
    /// A GDI call of the print job failed (its name).
    Gdi(&'static str),
}

/// The message shown when printing fails.
pub(super) fn print_error_text(e: &PrintError) -> String {
    match e {
        PrintError::PageTooSmall => t("print.error.too_small"),
        PrintError::Dialog(code) => {
            t("print.error.failed").replace("{error}", &format!("PrintDlgW 0x{:04X}", code))
        }
        PrintError::Gdi(call) => t("print.error.failed").replace("{error}", &format!("{} failed", call)),
    }
}

/// The text to print: the selection when there is one, else the whole tab.
pub(super) fn print_text<'a>(whole: &'a str, selection: &'a str) -> &'a str {
    if selection.is_empty() { whole } else { selection }
}

/// The page footer, "Page N / M" in the current language.
pub(super) fn footer_text(page: usize, pages: usize) -> String {
    t("print.footer").replace("{n}", &page.to_string()).replace("{m}", &pages.to_string())
}

/// How many body lines fit on a page of `page_height` device units: the
/// header row sits on the top margin and the footer row on the bottom one,
/// each separated from the body by one empty row.
pub(super) fn body_lines(page_height: i32, margin: i32, line_height: i32) -> usize {
    if line_height <= 0 {
        return 0;
    }
    let body = page_height - 2 * margin - 4 * line_height;
    if body <= 0 { 0 } else { (body / line_height) as usize }
}

/// The lines of `text`, split at CR LF, CR or LF (RichEdit uses CR). A final
/// line break adds no empty line, as with `str::lines`.
pub(super) fn split_print_lines(text: &str) -> Vec<&str> {
    // CR and LF are ASCII: a byte index next to them is a char boundary.
    let b = text.as_bytes();
    let mut lines = Vec::new();
    let (mut start, mut i) = (0, 0);
    while i < b.len() {
        match b[i] {
            b'\r' => {
                lines.push(&text[start..i]);
                if b.get(i + 1) == Some(&b'\n') {
                    i += 1;
                }
                start = i + 1;
            }
            b'\n' => {
                lines.push(&text[start..i]);
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    if start < text.len() {
        lines.push(&text[start..]);
    }
    lines
}

/// The tab-separated runs of `s`, each with the x offset it starts at: a tab
/// moves to the next multiple of `tab_stop` (RichEdit's default stops are
/// every half inch). `plain` measures text that has no tab.
pub(super) fn tab_runs<'a>(s: &'a str, tab_stop: i32, plain: &dyn Fn(&str) -> i32) -> Vec<(i32, &'a str)> {
    // print_to_dc refuses a stop under 1; max only keeps the division defined.
    let stop = tab_stop.max(1);
    let mut runs = Vec::new();
    let mut x = 0;
    for (i, run) in s.split('\t').enumerate() {
        if i > 0 {
            x = (x / stop + 1) * stop;
        }
        runs.push((x, run));
        x += plain(run);
    }
    runs
}

/// Width of `s` drawn with `tab_runs`.
pub(super) fn tabbed_width(s: &str, tab_stop: i32, plain: &dyn Fn(&str) -> i32) -> i32 {
    tab_runs(s, tab_stop, plain).last().map_or(0, |(x, run)| x + plain(run))
}

/// Where a too-long line may break.
fn is_break(c: char) -> bool {
    c == ' ' || c == '\t'
}

/// Cut one line (no line break inside) into rows no wider than `max_width`:
/// break after the last space or tab that fits, hard break inside a word
/// wider than the row (at least one character per row, so a character wider
/// than the page still prints). The spaces at a break are dropped; the
/// indentation that starts the line is kept. An empty line is one empty row.
pub(super) fn wrap_line<'a>(line: &'a str, max_width: i32, measure: &dyn Fn(&str) -> i32) -> Vec<&'a str> {
    let chars: Vec<char> = line.chars().collect();
    let bounds: Vec<usize> = line.char_indices().map(|(i, _)| i).chain(std::iter::once(line.len())).collect();
    let n = chars.len();
    if n == 0 {
        return vec![line];
    }
    let mut rows = Vec::new();
    let mut s = 0;
    while s < n {
        let fits = |e: usize| measure(&line[bounds[s]..bounds[e]]) <= max_width;
        // Largest end that fits: grow by doubling, then bisect, so a very
        // long line costs a few measures of about one row each.
        let (mut lo, mut hi, mut step) = (s, n, 64);
        loop {
            let probe = (s + step).min(n);
            if fits(probe) {
                lo = probe;
                if probe == n {
                    break;
                }
                step *= 2;
            } else {
                hi = probe;
                break;
            }
        }
        if lo == n {
            rows.push(&line[bounds[s]..]);
            break;
        }
        while hi - lo > 1 {
            let mid = (lo + hi) / 2;
            if fits(mid) { lo = mid } else { hi = mid }
        }
        let cut = if lo == s {
            // Not even one character fits: print it alone.
            s + 1
        } else if is_break(chars[lo]) && !line[bounds[s]..bounds[lo]].trim_end_matches(is_break).is_empty() {
            lo
        } else {
            (s + 1..lo)
                .rev()
                .find(|&w| is_break(chars[w]) && !line[bounds[s]..bounds[w]].trim_end_matches(is_break).is_empty())
                .unwrap_or(lo)
        };
        rows.push(line[bounds[s]..bounds[cut]].trim_end_matches(is_break));
        s = cut;
        while s < n && is_break(chars[s]) {
            s += 1;
        }
    }
    rows
}

/// Lay `text` out on pages of `lines_per_page` rows no wider than
/// `max_width`. Always at least one page (an empty note prints one page
/// with its header and footer).
pub(super) fn layout_pages<'a>(
    text: &'a str,
    max_width: i32,
    lines_per_page: usize,
    measure: &dyn Fn(&str) -> i32,
) -> Result<Vec<Vec<&'a str>>, PrintError> {
    if max_width <= 0 || lines_per_page == 0 {
        return Err(PrintError::PageTooSmall);
    }
    let mut pages = Vec::new();
    let mut page = Vec::with_capacity(lines_per_page);
    for line in split_print_lines(text) {
        for row in wrap_line(line, max_width, measure) {
            if page.len() == lines_per_page {
                pages.push(std::mem::take(&mut page));
            }
            page.push(row);
        }
    }
    pages.push(page);
    Ok(pages)
}

/// What is printed, copied out of the window before the job starts.
pub(super) struct PrintJob {
    /// Name of the active tab, printed at the top of each page.
    pub(super) header: String,
    pub(super) text: Zeroizing<String>,
}

/// The steps of File > Print (tested without a window).
pub(super) trait PrintSteps {
    type Dc;
    /// Password confirmation; true when confirmed.
    fn confirm_password(&mut self) -> bool;
    /// No lock happened since the command started (#30).
    fn still_valid(&mut self) -> bool;
    /// The Windows print dialog; `None` when cancelled.
    fn choose_printer(&mut self) -> Result<Option<Self::Dc>, PrintError>;
    /// The selection, or the whole active tab, and the tab name.
    fn job(&mut self) -> PrintJob;
    /// Print `job`; the number of pages printed.
    fn render(&mut self, dc: &Self::Dc, job: &PrintJob) -> Result<usize, PrintError>;
    /// DeleteDC.
    fn release(&mut self, dc: Self::Dc);
    /// Tell the user why printing failed.
    fn report(&mut self, error: &PrintError);
}

/// File > Print: password, print dialog, then print what `job` returns. Each
/// answer returning after a lock prints nothing; the printer DC is always
/// released, before any message box.
pub(super) fn run_print<S: PrintSteps>(steps: &mut S) {
    if !steps.confirm_password() {
        return;
    }
    if !steps.still_valid() {
        eprintln!("INFO print: dropped, the note locked meanwhile");
        return;
    }
    let dc = match steps.choose_printer() {
        Ok(Some(dc)) => dc,
        Ok(None) => {
            eprintln!("INFO print: cancelled");
            return;
        }
        Err(e) => {
            eprintln!("ERROR print: dialog failed {:?}", e);
            steps.report(&e);
            return;
        }
    };
    let result = if steps.still_valid() {
        let job = steps.job();
        Some(steps.render(&dc, &job))
    } else {
        eprintln!("INFO print: dropped, the note locked meanwhile");
        None
    };
    steps.release(dc);
    match result {
        Some(Ok(pages)) => eprintln!("INFO print: done pages={}", pages),
        Some(Err(e)) => {
            eprintln!("ERROR print: {:?}", e);
            // No message box over the lock curtain.
            if steps.still_valid() {
                steps.report(&e);
            }
        }
        None => {}
    }
}

/// `PrintSteps` on the real window.
struct FormPrintSteps<'a> {
    form: &'a EditorForm,
    still_valid: Box<dyn Fn() -> bool + 'a>,
}

impl PrintSteps for FormPrintSteps<'_> {
    type Dc = windows::Win32::Graphics::Gdi::HDC;

    fn confirm_password(&mut self) -> bool {
        use crate::ui::dialogs::confirm_password::ConfirmPasswordDialog;
        ConfirmPasswordDialog::show(&self.form.password_copy(), &t("confirmpwd.print"))
    }

    fn still_valid(&mut self) -> bool {
        (self.still_valid)()
    }

    fn choose_printer(&mut self) -> Result<Option<Self::Dc>, PrintError> {
        use windows::Win32::UI::Controls::Dialogs::*;
        unsafe {
            let mut pd: PRINTDLGW = std::mem::zeroed();
            pd.lStructSize = std::mem::size_of::<PRINTDLGW>() as u32;
            if let nwg::ControlHandle::Hwnd(hwnd) = self.form.window.handle {
                pd.hwndOwner = windows::Win32::Foundation::HWND(hwnd as *mut _);
            }
            // The selection is printed when there is one (no dialog choice).
            pd.Flags = PD_RETURNDC | PD_NOPAGENUMS | PD_NOSELECTION;
            if PrintDlgW(&mut pd).as_bool() {
                if pd.hDC.is_invalid() {
                    return Err(PrintError::Gdi("PrintDlgW (no DC)"));
                }
                Ok(Some(pd.hDC))
            } else {
                match CommDlgExtendedError().0 {
                    0 => Ok(None),
                    code => Err(PrintError::Dialog(code)),
                }
            }
        }
    }

    fn job(&mut self) -> PrintJob {
        let header = self.form.tab_bar.borrow().active_tab().name;
        let whole = Zeroizing::new(self.form.get_content());
        let selection = Zeroizing::new(self.form.current_selection_text());
        PrintJob { header, text: Zeroizing::new(print_text(&whole, &selection).to_string()) }
    }

    fn render(&mut self, dc: &Self::Dc, job: &PrintJob) -> Result<usize, PrintError> {
        self.form.print_to_dc(*dc, job)
    }

    fn release(&mut self, dc: Self::Dc) {
        if !unsafe { windows::Win32::Graphics::Gdi::DeleteDC(dc) }.as_bool() {
            eprintln!("WARN print: DeleteDC failed");
        }
    }

    fn report(&mut self, error: &PrintError) {
        self.form.inform(&t("print.error.title"), &print_error_text(error));
    }
}

/// Page geometry of a print job, in printer device units.
struct PageFrame {
    width: i32,
    height: i32,
    margin_x: i32,
    margin_y: i32,
    line_height: i32,
    tab_stop: i32,
}

impl EditorForm {
    /// Print the selection, or the whole active tab, using the Windows print
    /// dialog.
    pub(super) fn on_print(&self) {
        let epoch = self.lock_epoch();
        // Answers returning after a curtain went up print nothing (#30).
        let mut steps = FormPrintSteps { form: self, still_valid: Box::new(move || self.still_valid(epoch)) };
        run_print(&mut steps);
    }

    /// Print `job` on a printer device context with the editor's font
    /// family and size; the number of pages printed.
    pub(super) fn print_to_dc(
        &self,
        hdc: windows::Win32::Graphics::Gdi::HDC,
        job: &PrintJob,
    ) -> Result<usize, PrintError> {
        use windows::Win32::Graphics::Gdi::*;

        let (family, size_pt) = {
            let settings = self.settings.borrow();
            (settings.font_family.clone(), settings.font_size)
        };
        unsafe {
            let dpi_x = GetDeviceCaps(Some(hdc), LOGPIXELSX);
            let dpi_y = GetDeviceCaps(Some(hdc), LOGPIXELSY);
            if dpi_x < 2 || dpi_y < 1 {
                return Err(PrintError::Gdi("GetDeviceCaps"));
            }
            // The editor font at its point size on the printer.
            let height = -((size_pt * dpi_y as f64 / 72.0).round() as i32);
            let face: Vec<u16> = family.encode_utf16().chain(std::iter::once(0)).collect();
            let font = CreateFontW(
                height, 0, 0, 0,
                FW_NORMAL.0 as i32,
                0, 0, 0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                DEFAULT_QUALITY,
                (FF_DONTCARE.0 | DEFAULT_PITCH.0) as u32,
                windows::core::PCWSTR(face.as_ptr()),
            );
            if font.is_invalid() {
                return Err(PrintError::Gdi("CreateFontW"));
            }
            let old_font = SelectObject(hdc, font.into());

            let mut tm: TEXTMETRICW = std::mem::zeroed();
            let result = if GetTextMetricsW(hdc, &mut tm).as_bool() {
                let frame = PageFrame {
                    width: GetDeviceCaps(Some(hdc), HORZRES),
                    height: GetDeviceCaps(Some(hdc), VERTRES),
                    // ~1 inch margins, tab stops every half inch (RichEdit's default).
                    margin_x: dpi_x,
                    margin_y: dpi_y,
                    line_height: tm.tmHeight + tm.tmExternalLeading,
                    tab_stop: dpi_x / 2,
                };
                print_job(hdc, job, &frame)
            } else {
                Err(PrintError::Gdi("GetTextMetricsW"))
            };

            SelectObject(hdc, old_font);
            if !DeleteObject(font.into()).as_bool() {
                eprintln!("WARN print: DeleteObject(font) failed");
            }
            result
        }
    }
}

/// Lay out and print `job` with the font selected in `hdc`.
unsafe fn print_job(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    job: &PrintJob,
    frame: &PageFrame,
) -> Result<usize, PrintError> {
    use windows::Win32::Graphics::Gdi::*;
    use windows::Win32::Storage::Xps::*;

    let measure_failed = std::cell::Cell::new(false);
    let plain = |s: &str| -> i32 {
        if s.is_empty() {
            return 0;
        }
        let wide: Vec<u16> = s.encode_utf16().collect();
        let mut size = windows::Win32::Foundation::SIZE::default();
        if !unsafe { GetTextExtentPoint32W(hdc, &wide, &mut size) }.as_bool() {
            measure_failed.set(true);
        }
        size.cx
    };
    let measure = |s: &str| tabbed_width(s, frame.tab_stop, &plain);
    let pages = layout_pages(
        &job.text,
        frame.width - 2 * frame.margin_x,
        body_lines(frame.height, frame.margin_y, frame.line_height),
        &measure,
    )?;
    if measure_failed.get() {
        return Err(PrintError::Gdi("GetTextExtentPoint32W"));
    }

    let di = DOCINFOW {
        cbSize: std::mem::size_of::<DOCINFOW>() as i32,
        lpszDocName: windows::core::w!("LockNote"),
        ..unsafe { std::mem::zeroed() }
    };
    if unsafe { StartDocW(hdc, &di) } <= 0 {
        return Err(PrintError::Gdi("StartDocW"));
    }
    for (i, rows) in pages.iter().enumerate() {
        let drawn = unsafe { print_page(hdc, job, frame, rows, &footer_text(i + 1, pages.len()), &plain) };
        if let Err(e) = drawn.and_then(|()| {
            if measure_failed.get() { Err(PrintError::Gdi("GetTextExtentPoint32W")) } else { Ok(()) }
        }) {
            unsafe { AbortDoc(hdc) };
            return Err(e);
        }
    }
    if unsafe { EndDoc(hdc) } <= 0 {
        return Err(PrintError::Gdi("EndDoc"));
    }
    Ok(pages.len())
}

/// One page: the tab name on the top margin, the rows, the footer centred
/// on the bottom margin.
unsafe fn print_page(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    job: &PrintJob,
    frame: &PageFrame,
    rows: &[&str],
    footer: &str,
    plain: &dyn Fn(&str) -> i32,
) -> Result<(), PrintError> {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Gdi::*;
    use windows::Win32::Storage::Xps::*;

    if unsafe { StartPage(hdc) } <= 0 {
        return Err(PrintError::Gdi("StartPage"));
    }
    let band = |s: &str, top: i32, align: DRAW_TEXT_FORMAT| -> Result<(), PrintError> {
        let mut wide: Vec<u16> = s.encode_utf16().collect();
        if wide.is_empty() {
            return Ok(());
        }
        let mut rect = RECT {
            left: frame.margin_x,
            top,
            right: frame.width - frame.margin_x,
            bottom: top + frame.line_height,
        };
        let flags = align | DT_SINGLELINE | DT_NOPREFIX | DT_END_ELLIPSIS;
        if unsafe { DrawTextW(hdc, &mut wide, &mut rect, flags) } == 0 {
            return Err(PrintError::Gdi("DrawTextW"));
        }
        Ok(())
    };
    band(&job.header, frame.margin_y, DT_LEFT)?;
    let mut y = frame.margin_y + 2 * frame.line_height;
    for row in rows {
        for (x, run) in tab_runs(row, frame.tab_stop, plain) {
            if run.is_empty() {
                continue;
            }
            let wide: Vec<u16> = run.encode_utf16().collect();
            if !unsafe { TextOutW(hdc, frame.margin_x + x, y, &wide) }.as_bool() {
                return Err(PrintError::Gdi("TextOutW"));
            }
        }
        y += frame.line_height;
    }
    band(footer, frame.height - frame.margin_y - frame.line_height, DT_CENTER)?;
    if unsafe { EndPage(hdc) } <= 0 {
        return Err(PrintError::Gdi("EndPage"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 10 units per character, as a fixed-pitch font.
    fn mono(s: &str) -> i32 {
        s.chars().count() as i32 * 10
    }

    #[test]
    fn empty_text_is_one_empty_page() {
        assert_eq!(layout_pages("", 100, 5, &mono).unwrap(), vec![Vec::<&str>::new()]);
    }

    #[test]
    fn short_lines_are_kept_as_they_are() {
        assert_eq!(layout_pages("ab\ncd", 100, 5, &mono).unwrap(), vec![vec!["ab", "cd"]]);
    }

    #[test]
    fn cr_lf_cr_and_lf_all_end_a_line() {
        assert_eq!(split_print_lines("a\r\nb\rc\nd"), ["a", "b", "c", "d"]);
        // Empty lines are kept, a final break adds no line.
        assert_eq!(split_print_lines("a\r\r\nb\r\n"), ["a", "", "b"]);
        assert_eq!(split_print_lines("\n"), [""]);
        assert!(split_print_lines("").is_empty());
        assert_eq!(layout_pages("a\r\n\r\nb", 100, 5, &mono).unwrap(), vec![vec!["a", "", "b"]]);
    }

    #[test]
    fn long_lines_break_at_the_last_space_that_fits() {
        assert_eq!(wrap_line("aaa bbb ccc", 70, &mono), ["aaa bbb", "ccc"]);
        // The break spaces are dropped, several of them too.
        assert_eq!(wrap_line("aaa    bbb", 50, &mono), ["aaa", "bbb"]);
        // A space right after the last character that fits.
        assert_eq!(wrap_line("abc def", 30, &mono), ["abc", "def"]);
    }

    #[test]
    fn a_word_wider_than_the_line_is_cut_hard() {
        assert_eq!(wrap_line("abcdefghij", 35, &mono), ["abc", "def", "ghi", "j"]);
        assert_eq!(wrap_line("ab cdefghijk", 40, &mono), ["ab", "cdef", "ghij", "k"]);
    }

    #[test]
    fn one_huge_word_is_cut_on_char_boundaries_and_nothing_is_lost() {
        let word: String = "\u{00e9}\u{1F600}x".repeat(5_000);
        let rows = wrap_line(&word, 470, &mono);
        assert!(rows.iter().all(|r| mono(r) <= 470 && !r.is_empty()));
        assert_eq!(rows.concat(), word);
        assert_eq!(rows.len(), 15_000usize.div_ceil(47));
    }

    #[test]
    fn a_character_wider_than_the_line_prints_alone() {
        assert_eq!(wrap_line("abc", 5, &mono), ["a", "b", "c"]);
    }

    #[test]
    fn leading_indentation_is_kept_and_trailing_spaces_dropped() {
        assert_eq!(wrap_line("  ab cd", 50, &mono), ["  ab", "cd"]);
        assert_eq!(wrap_line("ab      ", 30, &mono), ["ab"]);
        assert_eq!(wrap_line("", 30, &mono), [""]);
    }

    #[test]
    fn tabs_move_to_the_next_stop() {
        assert_eq!(tab_runs("a\tb", 40, &mono), [(0, "a"), (40, "b")]);
        assert_eq!(tab_runs("abcd\tb", 40, &mono), [(0, "abcd"), (80, "b")]);
        assert_eq!(tab_runs("\t\tx", 40, &mono), [(0, ""), (40, ""), (80, "x")]);
        assert_eq!(tabbed_width("a\tb", 40, &mono), 50);
        assert_eq!(tabbed_width("a\t", 40, &mono), 40);
        assert_eq!(tabbed_width("", 40, &mono), 0);
        // Tabs widen the line and are a break point.
        let measure = |s: &str| tabbed_width(s, 40, &mono);
        assert_eq!(wrap_line("ab\tcd\tef", 90, &measure), ["ab\tcd", "ef"]);
        assert_eq!(layout_pages("\tx", 100, 5, &measure).unwrap(), vec![vec!["\tx"]]);
    }

    #[test]
    fn an_exact_page_fill_makes_no_empty_page() {
        assert_eq!(layout_pages("a\nb\nc", 100, 3, &mono).unwrap().len(), 1);
        assert_eq!(layout_pages("a\nb\nc\n", 100, 3, &mono).unwrap().len(), 1);
        assert_eq!(layout_pages("a\nb\nc\nd", 100, 3, &mono).unwrap(), vec![vec!["a", "b", "c"], vec!["d"]]);
        // Wrapped rows count as lines.
        assert_eq!(layout_pages("aa bb cc", 20, 3, &mono).unwrap(), vec![vec!["aa", "bb", "cc"]]);
        assert_eq!(layout_pages("aa bb cc dd", 20, 3, &mono).unwrap(), vec![vec!["aa", "bb", "cc"], vec!["dd"]]);
    }

    #[test]
    fn a_page_with_no_room_is_an_error() {
        assert_eq!(layout_pages("a", 0, 3, &mono), Err(PrintError::PageTooSmall));
        assert_eq!(layout_pages("a", 100, 0, &mono), Err(PrintError::PageTooSmall));
    }

    #[test]
    fn body_lines_leave_room_for_header_and_footer() {
        // 1000 high, margins 100: 800 left, minus 4 rows of 50 = 600 -> 12 rows.
        assert_eq!(body_lines(1000, 100, 50), 12);
        assert_eq!(body_lines(1000, 100, 49), 12);
        assert_eq!(body_lines(400, 100, 50), 0);
        assert_eq!(body_lines(300, 200, 50), 0);
        assert_eq!(body_lines(1000, 100, 0), 0);
    }

    #[test]
    fn the_selection_is_printed_when_there_is_one() {
        assert_eq!(print_text("whole", ""), "whole");
        assert_eq!(print_text("whole", "ho"), "ho");
    }

    #[test]
    fn footer_and_errors_are_localized() {
        let _lang = crate::features::i18n::test_support::lang_guard();
        assert_eq!(footer_text(2, 7), "Page 2 / 7");
        assert!(print_error_text(&PrintError::Gdi("StartDocW")).contains("StartDocW failed"));
        assert!(print_error_text(&PrintError::Dialog(0x1007)).contains("PrintDlgW 0x1007"));
        crate::features::i18n::set_language(crate::features::i18n::Language::German);
        assert_eq!(footer_text(1, 3), "Seite 1 / 3");
        crate::features::i18n::set_language(crate::features::i18n::Language::Spanish);
        assert_eq!(footer_text(1, 3), "P\u{00e1}gina 1 / 3");
        assert_eq!(print_error_text(&PrintError::PageTooSmall), t("print.error.too_small"));
    }

    /// The window side reads the editor font and no longer prints a fixed
    /// Consolas 10 pt or a selection-less job.
    #[test]
    fn printing_uses_the_editor_font_and_the_layout() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("ui").join("editor");
        let src = crate::read_source(dir.join("print_ops.rs"));
        let code = &src[..src.find("#[cfg(test)]").unwrap()];
        assert!(!code.contains("w!(\"Consolas\")"));
        assert!(code.contains("settings.font_family.clone(), settings.font_size"));
        assert!(code.contains("layout_pages("));
        assert!(code.contains("print_text(&whole, &selection)"));
    }

    struct Fake {
        confirm: bool,
        /// Answers of `still_valid`, in order (true once exhausted).
        valid: Vec<bool>,
        printer: Result<Option<()>, PrintError>,
        render: Result<usize, PrintError>,
        log: Vec<String>,
    }

    impl Fake {
        fn new() -> Fake {
            Fake { confirm: true, valid: Vec::new(), printer: Ok(Some(())), render: Ok(2), log: Vec::new() }
        }
    }

    impl PrintSteps for Fake {
        type Dc = ();
        fn confirm_password(&mut self) -> bool {
            self.log.push("confirm".into());
            self.confirm
        }
        fn still_valid(&mut self) -> bool {
            self.log.push("valid?".into());
            if self.valid.is_empty() { true } else { self.valid.remove(0) }
        }
        fn choose_printer(&mut self) -> Result<Option<()>, PrintError> {
            self.log.push("dialog".into());
            self.printer.clone()
        }
        fn job(&mut self) -> PrintJob {
            self.log.push("job".into());
            PrintJob { header: "Tab".into(), text: Zeroizing::new("text".into()) }
        }
        fn render(&mut self, _dc: &(), job: &PrintJob) -> Result<usize, PrintError> {
            self.log.push(format!("render:{}", job.header));
            self.render.clone()
        }
        fn release(&mut self, _dc: ()) {
            self.log.push("release".into());
        }
        fn report(&mut self, error: &PrintError) {
            self.log.push(format!("report:{:?}", error));
        }
    }

    #[test]
    fn a_print_runs_password_dialog_job_render_then_release() {
        let mut f = Fake::new();
        run_print(&mut f);
        assert_eq!(f.log, ["confirm", "valid?", "dialog", "valid?", "job", "render:Tab", "release"]);
    }

    #[test]
    fn a_refused_password_prints_nothing() {
        let mut f = Fake::new();
        f.confirm = false;
        run_print(&mut f);
        assert_eq!(f.log, ["confirm"]);
    }

    #[test]
    fn a_lock_during_the_password_opens_no_print_dialog() {
        let mut f = Fake::new();
        f.valid = vec![false];
        run_print(&mut f);
        assert_eq!(f.log, ["confirm", "valid?"]);
    }

    #[test]
    fn a_cancelled_print_dialog_prints_nothing() {
        let mut f = Fake::new();
        f.printer = Ok(None);
        run_print(&mut f);
        assert_eq!(f.log, ["confirm", "valid?", "dialog"]);
    }

    #[test]
    fn a_failed_print_dialog_is_reported() {
        let mut f = Fake::new();
        f.printer = Err(PrintError::Dialog(0x1007));
        run_print(&mut f);
        assert_eq!(f.log, ["confirm", "valid?", "dialog", "report:Dialog(4103)"]);
    }

    #[test]
    fn a_lock_during_the_print_dialog_reads_nothing_and_releases_the_printer() {
        let mut f = Fake::new();
        f.valid = vec![true, false];
        run_print(&mut f);
        assert_eq!(f.log, ["confirm", "valid?", "dialog", "valid?", "release"]);
    }

    #[test]
    fn a_failed_print_releases_the_printer_before_the_message() {
        let mut f = Fake::new();
        f.render = Err(PrintError::Gdi("StartDocW"));
        run_print(&mut f);
        assert_eq!(
            f.log,
            ["confirm", "valid?", "dialog", "valid?", "job", "render:Tab", "release", "valid?", "report:Gdi(\"StartDocW\")"]
        );
    }

    #[test]
    fn a_failed_print_shows_no_message_over_the_lock_curtain() {
        let mut f = Fake::new();
        f.render = Err(PrintError::PageTooSmall);
        f.valid = vec![true, true, false];
        run_print(&mut f);
        assert_eq!(f.log, ["confirm", "valid?", "dialog", "valid?", "job", "render:Tab", "release", "valid?"]);
    }
}
