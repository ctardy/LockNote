// Tab layout — pure geometry of the tab strip (issue 047).
//
// Single source for the toolbar button positions (used by layout_controls)
// and for which tabs are drawn, which go into the "…" overflow menu, and
// where "…" and "+" sit (used by paint, left click and right click).

use super::tab_bar::Tab;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolbarBtn {
    Prompts,
    Vault,
    Totp,
    Attach,
    Lock,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub x: i32,
    pub w: i32,
}

pub const PLUS_BTN_W: i32 = 28;
pub const OVERFLOW_BTN_W: i32 = 32;
/// Gap between the drop-down button and the toolbar side of the strip.
pub const OVERFLOW_RIGHT_INSET: i32 = 8;
/// Narrowest tab worth drawing ("×" 16 px + margins + ellipsis).
pub const MIN_SLOT_W: i32 = 36;

/// Toolbar buttons, right to left: (step from the previous right edge, width).
fn toolbar_metrics(b: ToolbarBtn) -> (i32, i32) {
    match b {
        ToolbarBtn::Lock => (36, 32),
        ToolbarBtn::Attach | ToolbarBtn::Totp | ToolbarBtn::Vault => (55, 50),
        // "Prompts" is the longest label: 10 px more, so the count badge
        // does not cover it.
        ToolbarBtn::Prompts => (65, 60),
    }
}

/// Position of each toolbar button, laid out from the right edge in the
/// given order (`buttons[0]` is the right-most).
pub fn toolbar_slots(client_w: i32, buttons: &[ToolbarBtn]) -> Vec<(ToolbarBtn, Slot)> {
    let mut right = client_w;
    buttons
        .iter()
        .map(|&b| {
            let (step, w) = toolbar_metrics(b);
            right -= step;
            (b, Slot { x: right, w })
        })
        .collect()
}

/// Gap kept between the last tab control ("+") and the toolbar buttons.
pub const TOOLBAR_GAP: i32 = 4;

/// Toolbar buttons of this build, right to left (same order as the
/// historical layout_controls code).
pub fn toolbar_buttons() -> Vec<ToolbarBtn> {
    #[allow(unused_mut)]
    let mut b = Vec::new();
    b.push(ToolbarBtn::Lock);
    b.push(ToolbarBtn::Attach);
    b.push(ToolbarBtn::Totp);
    b.push(ToolbarBtn::Vault);
    b.push(ToolbarBtn::Prompts);
    b
}

/// Right edge available to the tabs, "…" and "+": left of the toolbar.
pub fn tabs_right_limit(client_w: i32, buttons: &[ToolbarBtn]) -> i32 {
    toolbar_slots(client_w, buttons)
        .iter()
        .map(|(_, s)| s.x - TOOLBAR_GAP)
        .min()
        .unwrap_or(client_w)
}

/// What a click at `x` in the tab strip lands on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabHit {
    Tab { index: usize, slot: Slot },
    Overflow,
    Plus,
    Nothing,
}

pub fn hit_test(layout: &TabLayout, x: i32) -> TabHit {
    if let Some((index, slot)) = layout.slots.iter().find(|(_, s)| x >= s.x && x < s.x + s.w) {
        return TabHit::Tab { index: *index, slot: *slot };
    }
    if let Some(ox) = layout.overflow_x {
        if x >= ox && x < ox + OVERFLOW_BTN_W {
            return TabHit::Overflow;
        }
    }
    if x >= layout.plus_x && x < layout.plus_x + PLUS_BTN_W {
        return TabHit::Plus;
    }
    TabHit::Nothing
}

/// Mouse travel (px) before a press on a tab becomes a drag.
pub const DRAG_THRESHOLD: i32 = 4;

/// Tab a drag started on `from` (pressed at `start_x`) drops onto when the
/// button is released at `x`. `None`: not a drag yet (below the threshold),
/// released on "…", "+" or nothing, or on the tab itself.
pub fn drag_target(layout: &TabLayout, start_x: i32, x: i32, from: usize) -> Option<usize> {
    if (x - start_x).abs() < DRAG_THRESHOLD {
        return None;
    }
    match hit_test(layout, x) {
        TabHit::Tab { index, .. } if index != from => Some(index),
        _ => None,
    }
}

/// x of the insertion mark while dragging tab `from` over tab `to` (slot
/// `slot`): the dragged tab lands after `to` when moving right, before it
/// when moving left, so the mark sits on that edge of the target.
pub fn drop_edge_x(slot: Slot, from: usize, to: usize) -> i32 {
    if to > from { slot.x + slot.w } else { slot.x }
}

/// Height of the tab strip in pixels (also used by `layout_controls`).
pub const TAB_STRIP_H: i32 = 36;
/// Space kept above the tabs: they sit on the bottom edge of the strip.
pub const TAB_TOP_INSET: i32 = 4;
/// Visual gap between two tabs; each slot gives half of it on each side.
pub const TAB_GAP: i32 = 4;
/// Horizontal padding inside a tab.
pub const TAB_PAD_X: i32 = 18;
/// Radius of the rounded top corners of the active tab.
pub const TAB_RADIUS: i32 = 8;
/// Thickness of the accent line along the top edge of the active tab.
pub const ACCENT_LINE_H: i32 = 2;
/// Diameter of the tab color dot.
pub const DOT_SIZE: i32 = 8;
/// Space between the color dot and the tab name.
pub const DOT_GAP: i32 = 6;
/// Size of the close (x) box.
pub const CLOSE_SIZE: i32 = 16;
/// Distance between the close box and the right edge of the tab.
pub const CLOSE_RIGHT_INSET: i32 = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PxRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl PxRect {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }
    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }
}

/// Visible tab shape inside its slot: the slot minus the gap, from the top
/// inset down to the bottom of the strip.
pub fn tab_rect(slot: Slot, strip_h: i32) -> PxRect {
    PxRect {
        left: slot.x + TAB_GAP / 2,
        top: TAB_TOP_INSET,
        right: slot.x + slot.w - TAB_GAP / 2,
        bottom: strip_h,
    }
}

/// Close (x) box of a tab, vertically centered in it.
pub fn close_rect(tab: &PxRect) -> PxRect {
    let top = tab.top + (tab.height() - CLOSE_SIZE) / 2;
    let right = tab.right - CLOSE_RIGHT_INSET;
    PxRect { left: right - CLOSE_SIZE, top, right, bottom: top + CLOSE_SIZE }
}

/// Color dot of a tab, vertically centered, at the left padding.
pub fn dot_rect(tab: &PxRect) -> PxRect {
    let top = tab.top + (tab.height() - DOT_SIZE) / 2;
    let left = tab.left + TAB_PAD_X;
    PxRect { left, top, right: left + DOT_SIZE, bottom: top + DOT_SIZE }
}

/// Text area of a tab: after the padding (and the dot), before the close box
/// or the right padding. Never inverted.
pub fn text_rect(tab: &PxRect, has_dot: bool, show_close: bool) -> PxRect {
    let left = tab.left + TAB_PAD_X + if has_dot { DOT_SIZE + DOT_GAP } else { 0 };
    let right = if show_close { close_rect(tab).left - 2 } else { tab.right - TAB_PAD_X };
    PxRect { left, top: tab.top, right: right.max(left), bottom: tab.bottom }
}

/// Does a click at (`x`, `y`) in the tab strip land on the close box of the
/// tab drawn in `slot`? Same geometry as the painting.
pub fn close_hit(slot: Slot, strip_h: i32, x: i32, y: i32) -> bool {
    close_rect(&tab_rect(slot, strip_h)).contains(x, y)
}

/// Width a tab needs besides its text: padding, color dot, close box.
pub fn tab_chrome_w(has_dot: bool, show_close: bool) -> i32 {
    let slot = Slot { x: 0, w: 1000 };
    let tr = tab_rect(slot, TAB_STRIP_H);
    let text = text_rect(&tr, has_dot, show_close);
    slot.w - text.width()
}

/// Pixels added to a measured name so a rounding difference between the
/// measurement and the painting never clips a name that fits.
pub const TEXT_SLACK: i32 = 2;

/// Width of each tab: the full name as measured by `text_w` (display text
/// includes the "•" modified prefix) plus the padding, the dot and the close
/// box. Never an estimate of the text: a name is not cut while there is room.
pub fn tab_widths(tabs: &[Tab], text_w: &dyn Fn(&str) -> i32) -> Vec<i32> {
    let show_close = tabs.len() > 1;
    tabs.iter()
        .map(|t| text_w(&t.display_text()).max(0) + TEXT_SLACK + tab_chrome_w(tab_has_dot(t), show_close))
        .collect()
}

/// A tab shows a color dot when its color name resolves in the theme.
pub fn tab_has_dot(t: &Tab) -> bool {
    t.color.as_deref().and_then(crate::theme::get_tab_color).is_some()
}

#[derive(Debug, PartialEq, Eq)]
pub struct TabLayout {
    /// Drawn tabs: (tab index, slot), left to right.
    pub slots: Vec<(usize, Slot)>,
    /// Tabs listed in the "…" menu, ascending.
    pub hidden: Vec<usize>,
    /// x of the "…" button when some tabs are hidden.
    pub overflow_x: Option<i32>,
    pub plus_x: i32,
    /// The active tab was narrowed to fit (drawn with an ellipsis).
    pub truncated_active: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TabLayoutError {
    ActiveOutOfRange { active: usize, len: usize },
}

/// Lay out the tabs in `[0, right_limit)` followed by "…" (if needed) and "+".
///
/// Names are never cut to make room: every tab keeps the width of its full
/// name; when they do not all fit, the drop-down button sits at the right
/// end of the strip and lists the tabs.
///
/// Rules: no drawn tab crosses the room left for the buttons; the active
/// tab is always drawn (swapped into the last visible position when it
/// would be hidden, narrowed with an ellipsis when it is wider than the
/// room — decision e); below `MIN_SLOT_W` of room no tab is drawn, only
/// "…" and "+". Every non-drawn tab is in `hidden`.
pub fn compute_tab_layout(widths: &[i32], right_limit: i32, active: usize) -> Result<TabLayout, TabLayoutError> {
    let len = widths.len();
    if len == 0 {
        return Ok(TabLayout { slots: Vec::new(), hidden: Vec::new(), overflow_x: None, plus_x: 0, truncated_active: false });
    }
    if active >= len {
        return Err(TabLayoutError::ActiveOutOfRange { active, len });
    }
    let widths: Vec<i32> = widths.iter().map(|w| (*w).max(0)).collect();
    let limit = right_limit.max(0);

    // Everything fits next to "+": no overflow button.
    if widths.iter().sum::<i32>() + PLUS_BTN_W <= limit {
        let mut x = 0;
        let slots = widths.iter().enumerate().map(|(i, &w)| { let s = (i, Slot { x, w }); x += w; s }).collect();
        return Ok(TabLayout { slots, hidden: Vec::new(), overflow_x: None, plus_x: x, truncated_active: false });
    }

    // A single tab too wide: narrow it, still no overflow button.
    let solo_room = limit - PLUS_BTN_W;
    if len == 1 && solo_room >= MIN_SLOT_W {
        return Ok(TabLayout {
            slots: vec![(0, Slot { x: 0, w: solo_room })],
            hidden: Vec::new(),
            overflow_x: None,
            plus_x: solo_room,
            truncated_active: true,
        });
    }

    // Overflow: tabs share the room left of "…" and "+".
    let room = limit - PLUS_BTN_W - OVERFLOW_BTN_W - OVERFLOW_RIGHT_INSET;
    let mut visible: Vec<usize> = Vec::new();
    let mut used = 0;
    if room >= MIN_SLOT_W {
        for (i, &w) in widths.iter().enumerate() {
            if used + w > room {
                break;
            }
            visible.push(i);
            used += w;
        }
        if !visible.contains(&active) {
            // Make room for the active tab at the end.
            while used + widths[active] > room {
                match visible.pop() {
                    Some(i) => used -= widths[i],
                    None => break,
                }
            }
            visible.push(active);
        }
    }

    let mut slots = Vec::with_capacity(visible.len());
    let mut x = 0;
    let mut truncated_active = false;
    for &i in &visible {
        let mut w = widths[i];
        if x + w > room {
            // Only the active tab can reach here (alone, wider than room).
            w = room - x;
            truncated_active = true;
        }
        slots.push((i, Slot { x, w }));
        x += w;
    }
    let hidden: Vec<usize> = (0..len).filter(|i| !visible.contains(i)).collect();
    // "+" follows the last tab; the drop-down button is right-aligned.
    Ok(TabLayout { slots, hidden, overflow_x: Some((limit - OVERFLOW_RIGHT_INSET - OVERFLOW_BTN_W).max(x + PLUS_BTN_W).min((limit - OVERFLOW_BTN_W).max(0))), plus_x: x, truncated_active })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(l: &TabLayout, idx: usize) -> Option<Slot> {
        l.slots.iter().find(|(i, _)| *i == idx).map(|(_, s)| *s)
    }

    fn assert_non_negative(l: &TabLayout) {
        for (_, s) in &l.slots {
            assert!(s.x >= 0 && s.w >= 0, "{:?}", s);
        }
        assert!(l.overflow_x.map_or(true, |x| x >= 0));
        assert!(l.plus_x >= 0);
    }

    #[test]
    fn drag_target_rules() {
        // three tabs of 100 px: [0,100) [100,200) [200,300)
        let l = compute_tab_layout(&[100, 100, 100], 1000, 0).unwrap();
        assert_eq!(drag_target(&l, 50, 250, 0), Some(2));
        assert_eq!(drag_target(&l, 250, 50, 2), Some(0));
        // below the threshold: a plain click, not a drag
        assert_eq!(drag_target(&l, 98, 98 + DRAG_THRESHOLD - 1, 0), None);
        assert_eq!(drag_target(&l, 98, 98 + DRAG_THRESHOLD, 0), Some(1));
        // released on the dragged tab itself, or past the tabs ("+")
        assert_eq!(drag_target(&l, 10, 90, 0), None);
        assert_eq!(drag_target(&l, 10, 310, 0), None);
    }

    #[test]
    fn drop_mark_is_on_the_landing_edge() {
        let s = Slot { x: 100, w: 80 };
        assert_eq!(drop_edge_x(s, 0, 1), 180); // moving right: after the target
        assert_eq!(drop_edge_x(s, 3, 1), 100); // moving left: before the target
    }

    #[test]
    fn toolbar_slots_matches_layout_controls() {
        use ToolbarBtn::*;
        let s = toolbar_slots(900, &[Lock, Attach, Totp, Vault, Prompts]);
        assert_eq!(
            s,
            vec![
                (Lock, Slot { x: 864, w: 32 }),
                (Attach, Slot { x: 809, w: 50 }),
                (Totp, Slot { x: 754, w: 50 }),
                (Vault, Slot { x: 699, w: 50 }),
                (Prompts, Slot { x: 634, w: 60 }),
            ]
        );
    }

    // Issue 058: Prompts is the left-most button, left of Vault.
    #[test]
    fn prompts_button_sits_left_of_vault() {
        use ToolbarBtn::*;
        assert_eq!(toolbar_buttons(), vec![Lock, Attach, Totp, Vault, Prompts]);
        assert_eq!(tabs_right_limit(900, &toolbar_buttons()), 634 - TOOLBAR_GAP);
    }

    #[test]
    fn right_limit_is_left_of_toolbar() {
        use ToolbarBtn::*;
        assert_eq!(tabs_right_limit(900, &[Lock, Attach, Totp, Vault]), 699 - TOOLBAR_GAP);
        assert_eq!(tabs_right_limit(900, &[]), 900);
    }

    #[test]
    fn hit_test_regions() {
        let l = compute_tab_layout(&[100; 10], 500, 8).unwrap();
        let (last_idx, last) = *l.slots.last().unwrap();
        assert_eq!(hit_test(&l, last.x + 1), TabHit::Tab { index: last_idx, slot: last });
        assert_eq!(hit_test(&l, l.overflow_x.unwrap() + 1), TabHit::Overflow);
        assert_eq!(hit_test(&l, l.plus_x + 1), TabHit::Plus);
        assert_eq!(hit_test(&l, l.plus_x + PLUS_BTN_W + 5), TabHit::Nothing);
        // A right click on "…" must not resolve to a hidden tab.
        assert!(!matches!(hit_test(&l, l.overflow_x.unwrap()), TabHit::Tab { .. }));
    }

    #[test]
    fn no_visible_tab_crosses_limit() {
        let l = compute_tab_layout(&[100; 10], 500, 0).unwrap();
        for (_, s) in &l.slots {
            assert!(s.x + s.w <= 500 - PLUS_BTN_W - OVERFLOW_BTN_W, "{:?}", s);
        }
        assert!(l.plus_x + PLUS_BTN_W <= 500);
    }

    #[test]
    fn last_tab_overflow_detected() {
        let l = compute_tab_layout(&[100, 100, 400], 270, 0).unwrap();
        assert_eq!(l.hidden, vec![2]);
        assert!(l.overflow_x.is_some());
    }

    #[test]
    fn active_hidden_swapped_in() {
        let l = compute_tab_layout(&[100; 10], 500, 8).unwrap();
        assert_eq!(l.slots.last().map(|(i, _)| *i), Some(8));
        assert!(!l.hidden.contains(&8));
        for (_, s) in &l.slots {
            assert!(s.x + s.w <= 500 - PLUS_BTN_W - OVERFLOW_BTN_W);
        }
    }

    #[test]
    fn hidden_lists_every_masked_tab() {
        let l = compute_tab_layout(&[100; 10], 500, 8).unwrap();
        let mut all: Vec<usize> = l.slots.iter().map(|(i, _)| *i).chain(l.hidden.iter().copied()).collect();
        all.sort();
        assert_eq!(all, (0..10).collect::<Vec<_>>());
        let mut h = l.hidden.clone();
        h.dedup();
        assert_eq!(h, l.hidden);
    }

    #[test]
    fn all_fit() {
        let l = compute_tab_layout(&[100, 100], 500, 1).unwrap();
        assert_eq!(l.hidden, Vec::<usize>::new());
        assert_eq!(l.overflow_x, None);
        assert_eq!(l.plus_x, 200);
        assert_eq!(slot(&l, 1), Some(Slot { x: 100, w: 100 }));
    }

    #[test]
    fn active_wider_than_area_is_truncated() {
        let l = compute_tab_layout(&[600], 300, 0).unwrap();
        assert_eq!(slot(&l, 0), Some(Slot { x: 0, w: 300 - PLUS_BTN_W }));
        assert!(l.truncated_active);
        assert_eq!(l.overflow_x, None);
    }

    #[test]
    fn active_truncated_next_to_overflow() {
        let l = compute_tab_layout(&[100, 600], 300, 1).unwrap();
        assert_eq!(l.slots, vec![(1, Slot { x: 0, w: 300 - PLUS_BTN_W - OVERFLOW_BTN_W - OVERFLOW_RIGHT_INSET })]);
        assert_eq!(l.hidden, vec![0]);
        assert!(l.truncated_active);
    }

    #[test]
    fn names_are_never_narrowed_to_make_room() {
        // 3 x 200 + "+" does not fit in 500: nobody is narrowed, the
        // drop-down button appears at the right end.
        let l = compute_tab_layout(&[200, 200, 200], 500, 1).unwrap();
        for (_, s) in &l.slots {
            assert_eq!(s.w, 200);
        }
        assert_eq!(l.hidden, vec![2]);
        assert_eq!(l.overflow_x, Some(500 - OVERFLOW_RIGHT_INSET - OVERFLOW_BTN_W));
        assert!(!l.truncated_active);
    }

    #[test]
    fn dropdown_is_right_aligned_and_plus_follows_tabs() {
        let l = compute_tab_layout(&[100; 10], 500, 0).unwrap();
        let last = l.slots.last().unwrap().1;
        assert_eq!(l.plus_x, last.x + last.w);
        assert_eq!(l.overflow_x, Some(500 - OVERFLOW_RIGHT_INSET - OVERFLOW_BTN_W));
        assert!(l.plus_x + PLUS_BTN_W <= l.overflow_x.unwrap());
    }

    #[test]
    fn full_width_tabs_when_everything_fits() {
        let l = compute_tab_layout(&[300, 120], 600, 0).unwrap();
        assert_eq!(l.overflow_x, None);
        assert_eq!(slot(&l, 0).unwrap().w, 300);
        assert_eq!(slot(&l, 1).unwrap().w, 120);
    }

    #[test]
    fn tab_width_follows_measured_text() {
        let tabs = vec![Tab::new("Short"), Tab::new("A much longer tab name")];
        let w = tab_widths(&tabs, &|t| t.chars().count() as i32 * 8);
        assert_eq!(w[1] - w[0], (22 - 5) * 8);
        assert_eq!(w[0], 5 * 8 + TEXT_SLACK + tab_chrome_w(false, true));
    }

    #[test]
    fn chrome_reserves_dot_and_close() {
        assert!(tab_chrome_w(true, true) > tab_chrome_w(false, true));
        assert!(tab_chrome_w(false, true) > tab_chrome_w(false, false));
    }

    #[test]
    fn controls_never_leave_the_room_for_tiny_limits() {
        for limit in [0, 1, 36, 67] {
            let l = compute_tab_layout(&[100, 100], limit, 0).unwrap();
            if let Some(ox) = l.overflow_x {
                assert!(ox + OVERFLOW_BTN_W <= limit.max(OVERFLOW_BTN_W), "limit {limit}: {ox}");
            }
            assert!(l.plus_x + PLUS_BTN_W <= limit.max(PLUS_BTN_W), "limit {limit}: plus {}", l.plus_x);
        }
    }

    #[test]
    fn tiny_area_degrades() {
        let l = compute_tab_layout(&[200], 50, 0).unwrap();
        assert!(l.slots.is_empty());
        assert_eq!(l.hidden, vec![0]);
        assert_non_negative(&l);
    }

    #[test]
    fn negative_limit() {
        let l = compute_tab_layout(&[100, 100], -100, 0).unwrap();
        assert!(l.slots.is_empty());
        assert_non_negative(&l);
    }

    #[test]
    fn empty_widths() {
        let l = compute_tab_layout(&[], 500, 0).unwrap();
        assert!(l.slots.is_empty());
        assert_eq!(l.plus_x, 0);
    }

    #[test]
    fn active_out_of_range() {
        assert_eq!(
            compute_tab_layout(&[100, 100], 500, 5),
            Err(TabLayoutError::ActiveOutOfRange { active: 5, len: 2 })
        );
    }

    /// Fixed-pitch stand-in for the GDI measurement.
    fn mw(t: &str) -> i32 {
        t.chars().count() as i32 * 8
    }

    #[test]
    fn bullet_prefix_counted() {
        let mut t = Tab::new("nom");
        let clean = tab_widths(std::slice::from_ref(&t), &mw)[0];
        t.is_modified = true;
        let modified = tab_widths(std::slice::from_ref(&t), &mw)[0];
        assert_eq!(modified, mw("\u{2022} nom") + TEXT_SLACK + tab_chrome_w(false, false));
        assert!(modified >= clean);
    }

    #[test]
    fn tab_rect_keeps_gap_and_sits_on_bottom() {
        let r = tab_rect(Slot { x: 100, w: 120 }, TAB_STRIP_H);
        assert_eq!((r.left, r.right), (102, 218));
        assert_eq!((r.top, r.bottom), (TAB_TOP_INSET, TAB_STRIP_H));
        // Two adjacent slots leave exactly TAB_GAP between their tabs.
        let n = tab_rect(Slot { x: 220, w: 100 }, TAB_STRIP_H);
        assert_eq!(n.left - r.right, TAB_GAP);
    }

    #[test]
    fn close_and_dot_are_inside_the_tab_and_centered() {
        let t = tab_rect(Slot { x: 0, w: 80 }, TAB_STRIP_H);
        let c = close_rect(&t);
        assert_eq!((c.width(), c.height()), (CLOSE_SIZE, CLOSE_SIZE));
        assert!(c.left >= t.left && c.right <= t.right && c.top >= t.top && c.bottom <= t.bottom);
        assert_eq!(c.top - t.top, t.bottom - c.bottom);
        let d = dot_rect(&t);
        assert_eq!((d.width(), d.height()), (DOT_SIZE, DOT_SIZE));
        assert_eq!(d.left - t.left, TAB_PAD_X);
        assert_eq!(d.top - t.top, t.bottom - d.bottom);
    }

    #[test]
    fn text_rect_follows_dot_and_close() {
        let t = tab_rect(Slot { x: 0, w: 200 }, TAB_STRIP_H);
        let plain = text_rect(&t, false, false);
        assert_eq!((plain.left, plain.right), (t.left + TAB_PAD_X, t.right - TAB_PAD_X));
        let dot = text_rect(&t, true, true);
        assert_eq!(dot.left, t.left + TAB_PAD_X + DOT_SIZE + DOT_GAP);
        assert!(dot.right <= close_rect(&t).left);
        // Tiny tab: never inverted.
        let tiny = tab_rect(Slot { x: 0, w: MIN_SLOT_W }, TAB_STRIP_H);
        let r = text_rect(&tiny, true, true);
        assert!(r.right >= r.left);
    }

    #[test]
    fn close_hit_matches_close_rect() {
        let slot = Slot { x: 50, w: 120 };
        let c = close_rect(&tab_rect(slot, TAB_STRIP_H));
        assert!(close_hit(slot, TAB_STRIP_H, c.left, c.top));
        assert!(close_hit(slot, TAB_STRIP_H, c.right - 1, c.bottom - 1));
        assert!(!close_hit(slot, TAB_STRIP_H, c.left - 1, c.top));
        assert!(!close_hit(slot, TAB_STRIP_H, c.right, c.top));
        assert!(!close_hit(slot, TAB_STRIP_H, c.left, c.top - 1));
        assert!(!close_hit(slot, TAB_STRIP_H, c.left, c.bottom));
    }

    #[test]
    fn colored_tab_reserves_room_for_dot() {
        let mut t = Tab::new("nom");
        let plain = tab_widths(std::slice::from_ref(&t), &mw)[0];
        t.color = Some("blue".to_string());
        assert!(tab_has_dot(&t));
        assert_eq!(tab_widths(std::slice::from_ref(&t), &mw)[0], plain + DOT_SIZE + DOT_GAP);
        // Unknown color name: no dot, no reserved room.
        t.color = Some("no-such-color".to_string());
        assert!(!tab_has_dot(&t));
        assert_eq!(tab_widths(std::slice::from_ref(&t), &mw)[0], plain);
    }
}
