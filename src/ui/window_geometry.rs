// Window geometry — pure decisions for saving and restoring the main window
// placement (issue 044).
//
// Saved values are `WINDOWPLACEMENT.rcNormalPosition`: the *window* rect
// (frame included) in *workspace* coordinates. They are restored with
// `SetWindowPlacement`, which uses the same rect and coordinate space, so a
// save/restore cycle has no drift. (nwg's builder `.size()` is a *client*
// size: feeding it the window rect would grow the window every session.)

use crate::settings::Settings;

/// Default window rect used when nothing valid was saved.
pub const DEFAULT_X: i32 = 300;
pub const DEFAULT_Y: i32 = 200;
pub const DEFAULT_W: i32 = 900;
pub const DEFAULT_H: i32 = 650;

/// Minimum saved size (same bounds as `Settings::parse`).
pub const MIN_W: u32 = 200;
pub const MIN_H: u32 = 150;

/// How much of the title bar must be on a monitor for the saved position
/// to be reused (so the window can still be grabbed and moved).
pub const MIN_TITLE_VISIBLE_PX: i32 = 64;

// Win32 values (WinUser.h), kept numeric so this module stays pure.
const SW_SHOWMINIMIZED: u32 = 2;
const SW_SHOWMAXIMIZED: u32 = 3;
const WPF_RESTORETOMAXIMIZED: u32 = 0x0002;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SavedGeometry {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub maximized: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum DefaultReason {
    /// First run: `window_x == i32::MIN`.
    NeverSaved,
    /// The title bar would not be reachable on any current monitor.
    OffScreen,
    /// x + w or y + h overflows i32 (corrupted settings).
    Overflow,
}

#[derive(Debug, PartialEq, Eq)]
pub enum InitialPlacement {
    /// Use the builder defaults; `maximized` is still honoured (decision p:
    /// an unplugged monitor reopens maximized on the primary screen).
    Default { reason: DefaultReason, maximized: bool },
    Restore(SavedGeometry),
}

/// What to persist from a `GetWindowPlacement` result.
/// Closing while minimized reports `SW_SHOWMINIMIZED`; the maximized state
/// then lives in `WPF_RESTORETOMAXIMIZED`.
pub fn geometry_from_placement(show_cmd: u32, flags: u32, rc: Rect) -> SavedGeometry {
    let maximized = show_cmd == SW_SHOWMAXIMIZED
        || (show_cmd == SW_SHOWMINIMIZED && flags & WPF_RESTORETOMAXIMIZED != 0);
    SavedGeometry {
        x: rc.left,
        y: rc.top,
        w: (rc.right - rc.left).max(MIN_W as i32) as u32,
        h: (rc.bottom - rc.top).max(MIN_H as i32) as u32,
        maximized,
    }
}

/// Convert monitor work areas (screen coordinates) to workspace coordinates.
/// `primary_offset` is the primary monitor's work-area origin relative to
/// its monitor origin (non-zero when the taskbar is at the top or left).
pub fn to_workspace(work_areas: &[Rect], primary_offset: (i32, i32)) -> Vec<Rect> {
    let (dx, dy) = primary_offset;
    work_areas
        .iter()
        .map(|r| Rect { left: r.left - dx, top: r.top - dy, right: r.right - dx, bottom: r.bottom - dy })
        .collect()
}

/// Decide where the window opens. `work_areas` are in workspace
/// coordinates (see `to_workspace`); `caption_h` is the title bar height.
pub fn initial_window_placement(s: &Settings, work_areas: &[Rect], caption_h: i32) -> InitialPlacement {
    let maximized = s.window_maximized;
    if s.window_x == i32::MIN || s.window_y == i32::MIN {
        return InitialPlacement::Default { reason: DefaultReason::NeverSaved, maximized };
    }
    let (x, y) = (s.window_x, s.window_y);
    let (w, h) = (s.window_width.max(MIN_W), s.window_height.max(MIN_H));
    if x.checked_add(w as i32).is_none() || y.checked_add(h as i32).is_none() {
        return InitialPlacement::Default { reason: DefaultReason::Overflow, maximized };
    }

    // Monitor holding the largest part of the title bar.
    let band = Rect { left: x, top: y, right: x + w as i32, bottom: y + caption_h.max(1) };
    let best = work_areas
        .iter()
        .filter_map(|a| {
            let dx = band.right.min(a.right) - band.left.max(a.left);
            let dy = band.bottom.min(a.bottom) - band.top.max(a.top);
            let need_x = MIN_TITLE_VISIBLE_PX.min(w as i32);
            let need_y = (caption_h / 2).max(1);
            (dx >= need_x && dy >= need_y).then_some((dx as i64 * dy as i64, *a))
        })
        .max_by_key(|(area, _)| *area)
        .map(|(_, a)| a);
    let Some(area) = best else {
        return InitialPlacement::Default { reason: DefaultReason::OffScreen, maximized };
    };

    // Clamp the size to that monitor (portable exe on a smaller screen),
    // then pull the rect back inside it.
    let area_w = (area.right - area.left).max(MIN_W as i32) as u32;
    let area_h = (area.bottom - area.top).max(MIN_H as i32) as u32;
    let (w, h) = (w.min(area_w), h.min(area_h));
    let x = x.clamp(area.left, area.right - w as i32);
    let y = y.clamp(area.top, area.bottom - h as i32);
    InitialPlacement::Restore(SavedGeometry { x, y, w, h, maximized })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SW_SHOWNORMAL: u32 = 1;
    const CAPTION: i32 = 31;

    fn r(left: i32, top: i32, right: i32, bottom: i32) -> Rect {
        Rect { left, top, right, bottom }
    }

    fn screen() -> Vec<Rect> {
        vec![r(0, 0, 1920, 1040)]
    }

    fn settings(x: i32, y: i32, w: u32, h: u32, maximized: bool) -> Settings {
        let mut s = Settings::default();
        s.window_x = x;
        s.window_y = y;
        s.window_width = w;
        s.window_height = h;
        s.window_maximized = maximized;
        s
    }

    /// The editor must restore the saved placement at startup (it used to
    /// hard-code the builder size/position and never read the settings).
    #[test]
    fn editor_restores_saved_geometry() {
        let editor = crate::ui::editor::all_source();
        assert!(editor.contains("::initial_window_placement("));
        assert!(editor.contains("::geometry_from_placement("));
        assert!(editor.contains("SetWindowPlacement("));
    }

    // ── geometry_from_placement ──

    #[test]
    fn minimized_from_maximized_keeps_maximized() {
        let g = geometry_from_placement(SW_SHOWMINIMIZED, WPF_RESTORETOMAXIMIZED, r(10, 20, 910, 670));
        assert!(g.maximized);
    }

    #[test]
    fn minimized_from_normal_is_not_maximized() {
        let g = geometry_from_placement(SW_SHOWMINIMIZED, 0, r(10, 20, 910, 670));
        assert!(!g.maximized);
    }

    #[test]
    fn maximized_detected() {
        assert!(geometry_from_placement(SW_SHOWMAXIMIZED, 0, r(10, 20, 910, 670)).maximized);
    }

    #[test]
    fn normal_rect_is_window_rect() {
        let g = geometry_from_placement(SW_SHOWNORMAL, 0, r(10, 20, 910, 670));
        assert_eq!(g, SavedGeometry { x: 10, y: 20, w: 900, h: 650, maximized: false });
    }

    #[test]
    fn min_size_enforced() {
        let g = geometry_from_placement(SW_SHOWNORMAL, 0, r(0, 0, 50, 50));
        assert_eq!((g.w, g.h), (MIN_W, MIN_H));
    }

    // ── initial_window_placement ──

    #[test]
    fn never_saved() {
        let s = Settings::default();
        assert_eq!(
            initial_window_placement(&s, &screen(), CAPTION),
            InitialPlacement::Default { reason: DefaultReason::NeverSaved, maximized: false }
        );
    }

    #[test]
    fn saved_rect_is_restored() {
        let s = settings(100, 80, 1000, 700, false);
        assert_eq!(
            initial_window_placement(&s, &screen(), CAPTION),
            InitialPlacement::Restore(SavedGeometry { x: 100, y: 80, w: 1000, h: 700, maximized: false })
        );
    }

    #[test]
    fn offscreen_title_bar() {
        // Only the bottom edge of the window is on screen.
        let s = settings(100, -640, 900, 650, false);
        assert_eq!(
            initial_window_placement(&s, &screen(), CAPTION),
            InitialPlacement::Default { reason: DefaultReason::OffScreen, maximized: false }
        );
    }

    #[test]
    fn unplugged_monitor_maximized() {
        // Window was maximized on a second monitor that is gone (decision p).
        let s = settings(2500, 100, 900, 650, true);
        assert_eq!(
            initial_window_placement(&s, &screen(), CAPTION),
            InitialPlacement::Default { reason: DefaultReason::OffScreen, maximized: true }
        );
    }

    #[test]
    fn second_monitor_is_kept() {
        let areas = vec![r(0, 0, 1920, 1040), r(1920, 0, 3840, 1040)];
        let s = settings(2500, 100, 900, 650, false);
        assert_eq!(
            initial_window_placement(&s, &areas, CAPTION),
            InitialPlacement::Restore(SavedGeometry { x: 2500, y: 100, w: 900, h: 650, maximized: false })
        );
    }

    #[test]
    fn oversized_is_clamped_to_work_area() {
        let s = settings(0, 0, 3000, 2000, false);
        assert_eq!(
            initial_window_placement(&s, &screen(), CAPTION),
            InitialPlacement::Restore(SavedGeometry { x: 0, y: 0, w: 1920, h: 1040, maximized: false })
        );
    }

    #[test]
    fn partially_off_right_edge_is_pulled_back() {
        // Title bar visible (x = 1500), but the window sticks out: keep size,
        // move it back inside.
        let s = settings(1500, 100, 900, 650, false);
        assert_eq!(
            initial_window_placement(&s, &screen(), CAPTION),
            InitialPlacement::Restore(SavedGeometry { x: 1020, y: 100, w: 900, h: 650, maximized: false })
        );
    }

    #[test]
    fn overflow_is_default() {
        let s = settings(i32::MAX - 10, 0, 900, 650, false);
        assert_eq!(
            initial_window_placement(&s, &screen(), CAPTION),
            InitialPlacement::Default { reason: DefaultReason::Overflow, maximized: false }
        );
    }

    #[test]
    fn workspace_offset_applied() {
        // Taskbar on the left (48 px): screen work area starts at x = 48,
        // which is x = 0 in workspace coordinates.
        let areas = to_workspace(&[r(48, 0, 1920, 1080)], (48, 0));
        assert_eq!(areas, vec![r(0, 0, 1872, 1080)]);
        let s = settings(0, 0, 900, 650, false);
        assert_eq!(
            initial_window_placement(&s, &areas, CAPTION),
            InitialPlacement::Restore(SavedGeometry { x: 0, y: 0, w: 900, h: 650, maximized: false })
        );
    }

    #[test]
    fn roundtrip_through_settings_has_no_drift() {
        for (rc, cmd) in [
            (r(123, 45, 1123, 745), SW_SHOWNORMAL),
            (r(10, 10, 810, 610), SW_SHOWMAXIMIZED),
        ] {
            let g = geometry_from_placement(cmd, 0, rc);
            let mut s = Settings::default();
            s.window_x = g.x;
            s.window_y = g.y;
            s.window_width = g.w;
            s.window_height = g.h;
            s.window_maximized = g.maximized;
            let (parsed, _) = Settings::parse(&s.serialize("note"));
            assert_eq!(
                initial_window_placement(&parsed, &[r(0, 0, 3840, 2160)], CAPTION),
                InitialPlacement::Restore(g)
            );
        }
    }

    #[test]
    fn settings_clamp_width_to_8000() {
        // Documented limit: Settings::parse clamps the width to 8000.
        let mut s = Settings::default();
        s.window_x = 0;
        s.window_y = 0;
        s.window_width = 8500;
        let (parsed, _) = Settings::parse(&s.serialize("note"));
        assert_eq!(parsed.window_width, 8000);
    }
}
