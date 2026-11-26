// Printing.
// Part of the EditorForm implementation, split out of the former editor.rs.

#[allow(unused_imports)]
use super::*;

impl EditorForm {
    /// Print the current note using the Windows print dialog.
    pub(super) fn on_print(&self) {
        use crate::ui::dialogs::confirm_password::ConfirmPasswordDialog;

        let epoch = self.lock_epoch();
        // Require password confirmation before printing
        if !ConfirmPasswordDialog::show(&self.password_copy(), &t("confirmpwd.print")) {
            return;
        }
        // Answers returning after a curtain went up print nothing (#30).
        if !self.still_valid(epoch) {
            eprintln!("INFO print: dropped, the note locked meanwhile");
            return;
        }

        use windows::Win32::UI::Controls::Dialogs::*;
        use windows::Win32::Graphics::Gdi::*;

        unsafe {
            let mut pd: PRINTDLGW = std::mem::zeroed();
            pd.lStructSize = std::mem::size_of::<PRINTDLGW>() as u32;
            if let nwg::ControlHandle::Hwnd(hwnd) = self.window.handle {
                pd.hwndOwner = windows::Win32::Foundation::HWND(hwnd as *mut _);
            }
            pd.Flags = PD_RETURNDC | PD_NOPAGENUMS | PD_NOSELECTION;

            if PrintDlgW(&mut pd) == true {
                let hdc = pd.hDC;
                if !hdc.is_invalid() {
                    if self.still_valid(epoch) {
                        self.print_to_dc(hdc);
                    } else {
                        eprintln!("INFO print: dropped, the note locked meanwhile");
                    }
                    let _ = DeleteDC(hdc);
                }
            }
        }
    }

    /// Render the note content to a printer device context.
    pub(super) fn print_to_dc(&self, hdc: windows::Win32::Graphics::Gdi::HDC) {
        use windows::Win32::Graphics::Gdi::*;
        use windows::Win32::Storage::Xps::*;
        use windows::core::w;

        let content = self.get_content();
        let doc_name = w!("LockNote");

        unsafe {
            let di = DOCINFOW {
                cbSize: std::mem::size_of::<DOCINFOW>() as i32,
                lpszDocName: doc_name,
                ..std::mem::zeroed()
            };

            if StartDocW(hdc, &di) <= 0 {
                return;
            }
            if StartPage(hdc) <= 0 {
                EndDoc(hdc);
                return;
            }

            // Get printable area
            let page_width = GetDeviceCaps(Some(hdc), HORZRES);
            let page_height = GetDeviceCaps(Some(hdc), VERTRES);

            // Calculate margins: ~1 inch using printer DPI
            let dpi_x = GetDeviceCaps(Some(hdc), LOGPIXELSX);
            let dpi_y = GetDeviceCaps(Some(hdc), LOGPIXELSY);
            let margin_x = dpi_x; // 1 inch horizontal margin
            let margin_y = dpi_y; // 1 inch vertical margin

            // Create a proportional font scaled to the printer DPI
            // ~10pt font: height = -(10 * dpi_y / 72)
            let font_height = -(10 * dpi_y / 72);
            let font = CreateFontW(
                font_height, 0, 0, 0,
                FW_NORMAL.0 as i32,
                0, 0, 0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                DEFAULT_QUALITY,
                (FF_MODERN.0 | FIXED_PITCH.0) as u32,
                w!("Consolas"),
            );
            let old_font = SelectObject(hdc, font.into());

            // Get text metrics for line height
            let mut tm: TEXTMETRICW = std::mem::zeroed();
            let _ = GetTextMetricsW(hdc, &mut tm);
            let line_height = tm.tmHeight + tm.tmExternalLeading;
            let max_y = page_height - margin_y;
            let _max_x = page_width - margin_x;
            let mut y = margin_y;

            for line in content.lines() {
                if y + line_height > max_y {
                    EndPage(hdc);
                    if StartPage(hdc) <= 0 {
                        break;
                    }
                    y = margin_y;
                }
                let wide: Vec<u16> = line.encode_utf16().collect();
                let _ = TextOutW(hdc, margin_x, y, &wide);
                y += line_height;
            }

            EndPage(hdc);
            EndDoc(hdc);

            SelectObject(hdc, old_font);
            let _ = DeleteObject(font.into());
        }
    }
}
