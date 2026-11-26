// Resize grip of the right-side panels (Vault, TOTP, Attach).
//
// A thin raw Win32 child window between the editor and the open panel (only
// one panel is open at a time, see close_other_panels). Dragging it with the
// left button changes the width of that panel live; the width is written to
// the settings when the button is released, and saved with them. The
// geometry (bounds, drag arithmetic, stored value) is pure and unit-tested
// in `ui/side_panel.rs`; this module only wires the window and the mouse.

#[allow(unused_imports)]
use super::*;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetCapture, ReleaseCapture, SetCapture};
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

use crate::ui::side_panel::{self, SidePanel};

/// A drag of the grip in progress.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PanelDrag {
    pub panel: SidePanel,
    /// Screen x of the cursor when the button went down.
    pub start_x: i32,
    /// Width the panel was shown at when the button went down.
    pub start_w: u32,
    /// Width asked by the cursor now (bounded by `clamp_panel_width` at layout).
    pub width: u32,
}

const WM_SETCURSOR: u32 = 0x0020;
const WM_ERASEBKGND: u32 = 0x0014;
const WM_PAINT: u32 = 0x000F;
const WM_MOUSEMOVE: u32 = 0x0200;
const WM_LBUTTONDOWN: u32 = 0x0201;
const WM_LBUTTONUP: u32 = 0x0202;
const WM_CAPTURECHANGED: u32 = 0x0215;
/// Id of the mouse subclass on the grip window.
const GRIP_HANDLER_ID: usize = 0x40060;

/// Window procedure of the grip class: the horizontal-resize cursor and a
/// band in the strip colour. The mouse is handled by the subclass bound in
/// `bind_panel_grip`, which can reach the form.
unsafe extern "system" fn grip_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, FillRect, HBRUSH, PAINTSTRUCT};
    use windows::Win32::UI::WindowsAndMessaging::{DefWindowProcW, LoadCursorW, SetCursor, IDC_SIZEWE};
    match msg {
        WM_SETCURSOR => {
            match LoadCursorW(None, IDC_SIZEWE) {
                Ok(cursor) => {
                    SetCursor(Some(cursor));
                }
                Err(e) => eprintln!("WARN panel_grip: LoadCursorW(IDC_SIZEWE) failed ({e})"),
            }
            LRESULT(1)
        }
        WM_ERASEBKGND => LRESULT(1), // painted in WM_PAINT
        WM_PAINT => {
            let mut ps = std::mem::zeroed::<PAINTSTRUCT>();
            let hdc = BeginPaint(hwnd, &mut ps);
            let brush = HBRUSH(crate::ui::lock_card::cached_brush(crate::theme::current().chrome) as *mut _);
            FillRect(hdc, &ps.rcPaint, brush);
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Create the grip, hidden, as a child of the main window. Returns its handle,
/// 0 when it could not be created (the panels then keep their width).
pub(super) fn create_panel_grip(parent: &nwg::ControlHandle) -> isize {
    use windows::Win32::Graphics::Gdi::HBRUSH;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, RegisterClassW, WINDOW_EX_STYLE, WNDCLASSW, WS_CHILD, WS_CLIPSIBLINGS,
    };
    let nwg::ControlHandle::Hwnd(parent) = *parent else {
        eprintln!("WARN panel_grip: the main window has no handle; the side panels cannot be resized");
        return 0;
    };
    let class_name = windows::core::w!("LockNotePanelGrip");
    let instance = match unsafe { windows::Win32::System::LibraryLoader::GetModuleHandleW(None) } {
        Ok(h) => h,
        Err(e) => {
            eprintln!("WARN panel_grip: GetModuleHandleW failed ({e}); the side panels cannot be resized");
            return 0;
        }
    };
    let wc = WNDCLASSW {
        lpfnWndProc: Some(grip_wnd_proc),
        hInstance: instance.into(),
        lpszClassName: class_name,
        hbrBackground: HBRUSH::default(),
        ..Default::default()
    };
    if unsafe { RegisterClassW(&wc) } == 0 {
        eprintln!("WARN panel_grip: RegisterClassW failed ({})", windows::core::Error::from_thread());
    }
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class_name,
            None,
            WS_CHILD | WS_CLIPSIBLINGS,
            0, 0, side_panel::GRIP_W as i32, 0,
            Some(HWND(parent as *mut _)),
            None,
            Some(instance.into()),
            None,
        )
    };
    match hwnd {
        Ok(h) => h.0 as isize,
        Err(e) => {
            eprintln!("WARN panel_grip: grip window not created ({e}); the side panels cannot be resized");
            0
        }
    }
}

/// Screen x of the cursor.
fn cursor_x() -> Option<i32> {
    let mut pt = windows::Win32::Foundation::POINT::default();
    match unsafe { GetCursorPos(&mut pt) } {
        Ok(()) => Some(pt.x),
        Err(e) => {
            eprintln!("WARN panel_grip: GetCursorPos failed ({e}); drag step ignored");
            None
        }
    }
}

impl EditorForm {
    /// The open side panel (the left-most one if several were open).
    pub(super) fn open_side_panel(&self) -> Option<SidePanel> {
        if self.prompts_panel.is_visible() {
            return Some(SidePanel::Prompts);
        }
        if self.attach_panel.is_visible() {
            return Some(SidePanel::Attach);
        }
        if self.totp_panel.is_visible() {
            return Some(SidePanel::Totp);
        }
        if self.vault_panel.is_visible() {
            return Some(SidePanel::Vault);
        }
        None
    }

    /// The frame that holds the controls of `p`.
    fn side_panel_frame(&self, p: SidePanel) -> &nwg::Frame {
        match p {
            SidePanel::Vault => &self.vault_frame,
            SidePanel::Totp => &self.totp_frame,
            SidePanel::Attach => &self.attach_frame,
            SidePanel::Prompts => &self.prompts_frame,
        }
    }

    /// Width `p` is laid out at: the width of a drag in progress on it, or
    /// its setting, bounded for this client area.
    pub(super) fn side_panel_width(&self, p: SidePanel, client_w: u32, gutter_w: u32) -> u32 {
        let requested = match self.panel_drag.get() {
            Some(d) if d.panel == p => d.width,
            _ => side_panel::width_setting(&self.settings.borrow(), p),
        };
        side_panel::clamp_panel_width(requested, client_w, gutter_w)
    }

    /// Place the grip on the left edge of the open panel (`x`, `y`, `h` in
    /// client coordinates), or hide it (`None`). Hidden while locked: the
    /// lock overlay must stay on top of everything.
    pub(super) fn place_panel_grip(&self, at: Option<(i32, i32, i32)>) {
        use windows::Win32::UI::WindowsAndMessaging::{
            SetWindowPos, ShowWindow, SWP_NOACTIVATE, SWP_NOZORDER, SWP_SHOWWINDOW, SW_HIDE,
        };
        if self.panel_grip_hwnd == 0 {
            return;
        }
        let grip = HWND(self.panel_grip_hwnd as *mut _);
        match at {
            Some((x, y, h)) if !self.note_is_locked() => {
                let placed = unsafe {
                    SetWindowPos(
                        grip,
                        None,
                        x, y, side_panel::GRIP_W as i32, h,
                        SWP_NOZORDER | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                    )
                };
                if let Err(e) = placed {
                    eprintln!("WARN panel_grip: SetWindowPos failed ({e}); the grip may be misplaced");
                }
            }
            _ => unsafe {
                // The return value is the previous visibility, not an error.
                let _ = ShowWindow(grip, SW_HIDE);
            },
        }
    }

    fn note_is_locked(&self) -> bool {
        self.input_blocked()
    }

    /// Left button down on the grip: start a drag of the open panel.
    fn panel_drag_start(&self) {
        if self.note_is_locked() {
            eprintln!("INFO panel_grip: drag ignored, the note is locked");
            return;
        }
        let Some(panel) = self.open_side_panel() else {
            eprintln!("WARN panel_grip: button down on the grip with no panel open; ignored");
            return;
        };
        let Some(start_x) = cursor_x() else { return };
        let start_w = self.side_panel_frame(panel).size().0;
        self.panel_drag.set(Some(PanelDrag { panel, start_x, start_w, width: start_w }));
        unsafe {
            // The return value is the previous capture window.
            let _ = SetCapture(HWND(self.panel_grip_hwnd as *mut _));
        }
    }

    /// Mouse move while the grip holds the capture: relayout at the new width.
    fn panel_drag_move(&self) {
        let Some(mut drag) = self.panel_drag.get() else { return };
        if self.note_is_locked() {
            self.panel_drag_cancel("locked");
            return;
        }
        if self.open_side_panel() != Some(drag.panel) {
            self.panel_drag_cancel("panel changed");
            return;
        }
        let Some(x) = cursor_x() else { return };
        let width = side_panel::drag_width(drag.start_w, drag.start_x, x);
        if width != drag.width {
            drag.width = width;
            self.panel_drag.set(Some(drag));
            self.layout_controls();
        }
    }

    /// Left button up: keep the width the panel is shown at, if it changed.
    fn panel_drag_end(&self) {
        // Clear first: ReleaseCapture sends WM_CAPTURECHANGED synchronously.
        let Some(drag) = self.panel_drag.take() else { return };
        if let Err(e) = unsafe { ReleaseCapture() } {
            eprintln!("WARN panel_grip: ReleaseCapture failed ({e})");
        }
        if self.note_is_locked() {
            eprintln!("INFO panel_grip: drag dropped, the note is locked");
            return;
        }
        if self.open_side_panel() != Some(drag.panel) {
            // Another panel was opened by a shortcut during the drag.
            eprintln!("INFO panel_grip: drag dropped, panel={:?} is no longer open", drag.panel);
            self.layout_controls();
            return;
        }
        let shown = self.side_panel_frame(drag.panel).size().0;
        let Some(stored) = side_panel::width_to_store_after_drag(drag.start_w, shown) else {
            // A click, or a window too narrow to let the panel move: the
            // setting stays (it may be wider than what this window shows).
            return;
        };
        side_panel::set_width_setting(&mut self.settings.borrow_mut(), drag.panel, stored);
        eprintln!("INFO panel_grip: resized panel={:?} width={stored}", drag.panel);
        self.layout_controls();
    }

    /// Drop a drag without storing it (capture lost, note locked): the panel
    /// goes back to its stored width.
    fn panel_drag_cancel(&self, reason: &str) {
        if self.panel_drag.take().is_none() {
            return;
        }
        eprintln!("INFO panel_grip: drag cancelled reason={reason}");
        if unsafe { GetCapture() } == HWND(self.panel_grip_hwnd as *mut _) {
            if let Err(e) = unsafe { ReleaseCapture() } {
                eprintln!("WARN panel_grip: ReleaseCapture failed ({e})");
            }
        }
        self.layout_controls();
    }

    /// Mouse handling of the grip. Called once, from `bind_events`.
    pub(super) fn bind_panel_grip(form: std::rc::Rc<EditorForm>) {
        if form.panel_grip_hwnd == 0 {
            return;
        }
        let f = form.clone();
        let handle = nwg::ControlHandle::Hwnd(form.panel_grip_hwnd as *mut _);
        let handler = nwg::bind_raw_event_handler(&handle, GRIP_HANDLER_ID, move |_h, msg, _w, _l| {
            match msg {
                WM_LBUTTONDOWN => {
                    f.panel_drag_start();
                    Some(0)
                }
                WM_MOUSEMOVE => {
                    f.panel_drag_move();
                    Some(0)
                }
                WM_LBUTTONUP => {
                    f.panel_drag_end();
                    Some(0)
                }
                WM_CAPTURECHANGED => {
                    // Another window took the mouse (Alt+Tab, a dialog, the
                    // lock): the drag is dropped.
                    f.panel_drag_cancel("capture lost");
                    None
                }
                _ => None,
            }
        });
        // A raw handler has no Drop: it stays bound for the life of the grip.
        match handler {
            Ok(_) => {}
            Err(e) => eprintln!("WARN panel_grip: mouse handler not bound ({e:?}); the side panels cannot be resized"),
        }
    }
}
