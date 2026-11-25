// Tab bar painting.
// Part of the EditorForm implementation, split out of the former editor.rs.

#[allow(unused_imports)]
use super::*;

impl EditorForm {
    // =====================================================================
    // Tab bar painting
    // =====================================================================

    /// Paint the tab strip: chrome background, tabs sitting on the bottom
    /// edge, the active tab joined to the editor (rounded top corners, accent
    /// line). Everything is drawn in an off-screen bitmap and blitted once.
    pub(super) fn paint_tab_bar(&self) {
        let tb_hwnd_val = *self.tab_bar_hwnd.borrow();
        if tb_hwnd_val == 0 {
            return;
        }
        let tb_hwnd = windows::Win32::Foundation::HWND(tb_hwnd_val as *mut _);

        unsafe {
            use windows::Win32::Foundation::{COLORREF, RECT};
            use windows::Win32::Graphics::Gdi::*;
            use windows::Win32::UI::WindowsAndMessaging::*;
            use crate::ui::tab_layout as tl;

            let screen_dc = GetDC(Some(tb_hwnd));
            if screen_dc.is_invalid() {
                return;
            }

            let mut rect = RECT::default();
            let _ = GetClientRect(tb_hwnd, &mut rect);
            let w = rect.right;
            let h = rect.bottom;

            // Double buffering: no flicker between background and tabs.
            let hdc = CreateCompatibleDC(Some(screen_dc));
            let bitmap = CreateCompatibleBitmap(screen_dc, w.max(1), h.max(1));
            let old_bitmap = SelectObject(hdc, bitmap.into());

            let palette = crate::theme::current();
            let fill = |r: &RECT, c: crate::theme::Color| {
                let brush = CreateSolidBrush(COLORREF(c.to_colorref()));
                FillRect(hdc, r, brush);
                let _ = DeleteObject(brush.into());
            };

            // Strip background
            fill(&rect, palette.chrome);

            // Create UI font (Segoe UI 9pt)
            let font_family: Vec<u16> = "Segoe UI".encode_utf16().chain(std::iter::once(0)).collect();
            let hfont = CreateFontW(
                -15, 0, 0, 0,
                FW_NORMAL.0 as i32,
                0, 0, 0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                CLEARTYPE_QUALITY,
                0,
                windows::core::PCWSTR(font_family.as_ptr()),
            );
            let old_font = SelectObject(hdc, hfont.into());
            SetBkMode(hdc, TRANSPARENT);
            // Bold variant for the active tab
            let hfont_bold = CreateFontW(
                -15, 0, 0, 0,
                FW_BOLD.0 as i32,
                0, 0, 0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                CLEARTYPE_QUALITY,
                0,
                windows::core::PCWSTR(font_family.as_ptr()),
            );

            // Line between the chrome and the editor; the active tab is drawn
            // over it so it joins the editor.
            fill(&RECT { left: 0, top: h - 1, right: w, bottom: h }, palette.chrome_border);

            // Single layout source shared with the click handlers.
            let (tabs, active_idx, layout) = match self.current_tab_layout(w) {
                Some(v) => v,
                None => (Vec::new(), 0, tl::TabLayout {
                    slots: Vec::new(),
                    hidden: Vec::new(),
                    overflow_x: None,
                    plus_x: 0,
                    truncated_active: false,
                }),
            };
            let overflow_w = tl::OVERFLOW_BTN_W;
            let to_rect = |r: &tl::PxRect| RECT { left: r.left, top: r.top, right: r.right, bottom: r.bottom };

            for &(i, slot) in layout.slots.iter() {
                let tab = &tabs[i];
                let text = tab.display_text();
                let is_active = i == active_idx;
                let tr = tl::tab_rect(slot, h);

                // Active tab: editor background, rounded top corners (square
                // bottom), 2 px accent line along the top edge. The rounded
                // rectangle is taller than the tab; the part below the strip
                // is clipped by the bitmap.
                if is_active {
                    let rgn = CreateRoundRectRgn(
                        tr.left, tr.top,
                        tr.right + 1, tr.bottom + 2 * tl::TAB_RADIUS,
                        2 * tl::TAB_RADIUS, 2 * tl::TAB_RADIUS,
                    );
                    SelectClipRgn(hdc, Some(rgn));
                    fill(&to_rect(&tr), palette.editor_background);
                    fill(
                        &RECT { left: tr.left, top: tr.top, right: tr.right, bottom: tr.top + tl::ACCENT_LINE_H },
                        palette.accent,
                    );
                    SelectClipRgn(hdc, None);
                    let _ = DeleteObject(rgn.into());
                }

                // Tab color dot (8 px filled circle) before the name
                let dot_color = tab.color.as_deref().and_then(crate::theme::get_tab_color);
                if let Some(color) = dot_color {
                    let d = tl::dot_rect(&tr);
                    let brush = CreateSolidBrush(COLORREF(color.to_colorref()));
                    let old_brush = SelectObject(hdc, brush.into());
                    // Light chrome: the pale dot colours (yellow, orange) vanish
                    // on it, so they get a 1 px outline (>= 3:1). Dark keeps the
                    // NULL pen, which leaves the right/bottom edge out: +1.
                    let outline = crate::theme::current_mode() == crate::theme::ThemeMode::Light;
                    let pen = if outline {
                        Some(CreatePen(PS_SOLID, 1, COLORREF(palette.text_secondary.to_colorref())))
                    } else {
                        None
                    };
                    let old_pen = SelectObject(hdc, pen.map(|p| p.into()).unwrap_or_else(|| GetStockObject(NULL_PEN)));
                    let _ = Ellipse(hdc, d.left, d.top, d.right + 1, d.bottom + 1);
                    SelectObject(hdc, old_pen);
                    if let Some(p) = pen {
                        let _ = DeleteObject(p.into());
                    }
                    SelectObject(hdc, old_brush);
                    let _ = DeleteObject(brush.into());
                }

                // Tab text
                let text_color = if is_active { palette.text_primary } else { palette.text_secondary };
                SetTextColor(hdc, COLORREF(text_color.to_colorref()));

                let show_close = tabs.len() > 1;
                let mut wide: Vec<u16> = text.encode_utf16().collect();
                let mut text_rect = to_rect(&tl::text_rect(&tr, dot_color.is_some(), show_close));
                if is_active {
                    SelectObject(hdc, hfont_bold.into());
                }
                DrawTextW(hdc, &mut wide, &mut text_rect, DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | DT_END_ELLIPSIS);
                SelectObject(hdc, hfont.into());

                // Close button (×) — only when more than one tab
                if show_close {
                    SetTextColor(hdc, COLORREF(palette.text_secondary.to_colorref()));
                    let mut close_text: Vec<u16> = "\u{00D7}".encode_utf16().collect();
                    let mut close_rect = to_rect(&tl::close_rect(&tr));
                    DrawTextW(hdc, &mut close_text, &mut close_rect, DT_SINGLELINE | DT_VCENTER | DT_CENTER | DT_NOPREFIX);
                }
            }

            // Drag feedback: accent frame on the dragged tab and an
            // insertion mark on the edge where it would land.
            if let (Some((from, _)), Some(to)) = (self.tab_drag.get(), self.tab_drag_over.get()) {
                if let Some((_, s)) = layout.slots.iter().find(|(i, _)| *i == from) {
                    let t = tl::tab_rect(*s, h);
                    fill(&RECT { left: t.left, top: t.top, right: t.right, bottom: t.top + 2 }, palette.accent);
                    fill(&RECT { left: t.left, top: t.top, right: t.left + 2, bottom: t.bottom }, palette.accent);
                    fill(&RECT { left: t.right - 2, top: t.top, right: t.right, bottom: t.bottom }, palette.accent);
                }
                if let Some((_, s)) = layout.slots.iter().find(|(i, _)| *i == to) {
                    let mx = tl::drop_edge_x(*s, from, to);
                    fill(&RECT { left: mx - 2, top: tl::TAB_TOP_INSET, right: mx + 2, bottom: h }, palette.accent);
                }
            }

            // Drop-down button (tabs do not all fit): a down-pointing triangle
            // at the right end of the strip.
            if let Some(x) = layout.overflow_x {
                let cx = x + overflow_w / 2;
                let cy = (tl::TAB_TOP_INSET + h) / 2;
                let pts = [
                    windows::Win32::Foundation::POINT { x: cx - 5, y: cy - 2 },
                    windows::Win32::Foundation::POINT { x: cx + 5, y: cy - 2 },
                    windows::Win32::Foundation::POINT { x: cx, y: cy + 3 },
                ];
                let brush = CreateSolidBrush(COLORREF(palette.text_secondary.to_colorref()));
                let old_brush = SelectObject(hdc, brush.into());
                let old_pen = SelectObject(hdc, GetStockObject(NULL_PEN));
                let _ = Polygon(hdc, &pts);
                SelectObject(hdc, old_pen);
                SelectObject(hdc, old_brush);
                let _ = DeleteObject(brush.into());
            }

            // "+" button
            let plus_x = layout.plus_x;
            let plus_w = tl::PLUS_BTN_W;
            SetTextColor(hdc, COLORREF(palette.text_secondary.to_colorref()));
            let mut plus_text: Vec<u16> = "+".encode_utf16().collect();
            let mut plus_rect = RECT { left: plus_x, top: tl::TAB_TOP_INSET, right: plus_x + plus_w, bottom: h };
            DrawTextW(hdc, &mut plus_text, &mut plus_rect, DT_SINGLELINE | DT_VCENTER | DT_CENTER | DT_NOPREFIX);

            // Single blit to the window
            let _ = BitBlt(screen_dc, 0, 0, w, h, Some(hdc), 0, 0, SRCCOPY);

            // Cleanup
            SelectObject(hdc, old_font);
            SelectObject(hdc, old_bitmap);
            let _ = DeleteObject(hfont.into());
            let _ = DeleteObject(hfont_bold.into());
            let _ = DeleteObject(bitmap.into());
            let _ = DeleteDC(hdc);
            let _ = ReleaseDC(Some(tb_hwnd), screen_dc);
        }
    }
}
