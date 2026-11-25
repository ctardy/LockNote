// Dialog metrics: the pure, unit-tested half of the shared dialog look.
//
// Everything here is plain arithmetic on integers and palette colors, with no
// Win32 call: DPI scaling, the font hierarchy (title / section / body /
// caption), button geometry, the vertical reflow that keeps long translations
// from being clipped, the right-aligned footer row, and the colors of the
// owner-drawn buttons. `style.rs` measures the real controls and applies the
// results; the decisions are made here so they can be tested.
//
// All "dp" constants are in 96-DPI pixels, the unit the dialogs are authored in.

use crate::theme::{Color, ThemePalette};

pub const BASE_DPI: u32 = 96;
/// Fallback outer margin when the dialog's own left margin cannot be inferred.
pub const MARGIN_DP: i32 = 20;
/// Space between two buttons of the footer row.
pub const GAP_DP: i32 = 8;
pub const BUTTON_MIN_W_DP: i32 = 88;
pub const BUTTON_H_DP: i32 = 30;
/// Horizontal padding on each side of a button caption.
pub const BUTTON_PAD_DP: i32 = 16;
pub const CORNER_DP: i32 = 4;
/// Checkbox / radio glyph plus the gap before its caption.
pub const CHECK_CHROME_DP: i32 = 20;

// ---------------------------------------------------------------------------
// DPI scaling
// ---------------------------------------------------------------------------

/// `v` authored at 96 DPI, scaled to `dpi`, rounded half away from zero.
pub fn scale(v: i32, dpi: u32) -> i32 {
    let n = v as i64 * dpi as i64;
    let half = (BASE_DPI / 2) as i64;
    ((n + if n >= 0 { half } else { -half }) / BASE_DPI as i64) as i32
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub l: i32,
    pub t: i32,
    pub r: i32,
    pub b: i32,
}

impl Rect {
    pub const fn new(l: i32, t: i32, r: i32, b: i32) -> Self {
        Self { l, t, r, b }
    }
    pub fn w(&self) -> i32 {
        self.r - self.l
    }
    pub fn h(&self) -> i32 {
        self.b - self.t
    }
    pub fn overlaps_vertically(&self, o: &Rect) -> bool {
        self.t < o.b && o.t < self.b
    }
    pub fn overlaps_horizontally(&self, o: &Rect) -> bool {
        self.l < o.r && o.l < self.r
    }
    /// Scale each edge (not the size) so adjacent controls stay adjacent.
    pub fn scaled(&self, dpi: u32) -> Rect {
        Rect::new(scale(self.l, dpi), scale(self.t, dpi), scale(self.r, dpi), scale(self.b, dpi))
    }
}

// ---------------------------------------------------------------------------
// Font hierarchy
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    Title,
    Section,
    Body,
    Caption,
}

/// Role of a control from the font it was authored with: `cell_height` is the
/// absolute `lfHeight` (the dialogs use nwg pixel sizes, 16 = 9 pt body).
pub fn role_for(cell_height: i32, weight: i32) -> Role {
    let h = cell_height.abs();
    if h >= 22 {
        Role::Title
    } else if weight >= 600 {
        Role::Section
    } else if h <= 15 {
        Role::Caption
    } else {
        Role::Body
    }
}

/// (size in points, weight) of a role. One Segoe UI family, weights 400 / 600.
pub fn role_font(role: Role) -> (i32, i32) {
    match role {
        Role::Title => (13, 600),
        Role::Section => (10, 600),
        Role::Body => (9, 400),
        Role::Caption => (8, 400),
    }
}

/// Negative `lfHeight` (character height) for `pt` points at `dpi`.
pub fn font_height_px(pt: i32, dpi: u32) -> i32 {
    -((pt as i64 * dpi as i64 + 36) / 72) as i32
}

/// Text color of a role: captions use the secondary text color.
pub fn role_text_color(pal: &ThemePalette, role: Role) -> Color {
    match role {
        Role::Caption => pal.text_secondary,
        _ => pal.text_primary,
    }
}

// ---------------------------------------------------------------------------
// Buttons
// ---------------------------------------------------------------------------

/// Width of a button from the pixel width of its caption (already at `dpi`).
pub fn button_width(text_w: i32, dpi: u32) -> i32 {
    (text_w + 2 * scale(BUTTON_PAD_DP, dpi)).max(scale(BUTTON_MIN_W_DP, dpi))
}

/// X positions of a right-aligned row of buttons. Returns `(xs, client_w)`:
/// `client_w` grows when the row does not fit between the two margins.
pub fn footer_layout(client_w: i32, margin: i32, widths: &[i32], gap: i32) -> (Vec<i32>, i32) {
    if widths.is_empty() {
        return (Vec::new(), client_w);
    }
    let total: i32 = widths.iter().sum::<i32>() + gap * (widths.len() as i32 - 1);
    let client_w = client_w.max(total + 2 * margin);
    let mut x = client_w - margin - total;
    let mut xs = Vec::with_capacity(widths.len());
    for w in widths {
        xs.push(x);
        x += w + gap;
    }
    (xs, client_w)
}

/// Grow a button that is too narrow for its caption while keeping its right
/// edge: the nearest sibling on its left (typically the input it belongs to)
/// gives up the space. With no sibling on the left, the button grows rightwards.
pub fn widen_trailing(btn: Rect, left_sibling: Option<Rect>, needed_w: i32, gap: i32) -> (Rect, Option<Rect>) {
    if btn.w() >= needed_w {
        return (btn, left_sibling);
    }
    match left_sibling {
        Some(s) => {
            let b = Rect::new(btn.r - needed_w, btn.t, btn.r, btn.b);
            let s = Rect::new(s.l, s.t, s.r.min(b.l - gap), s.b);
            (b, Some(s))
        }
        None => (Rect::new(btn.l, btn.t, btn.l + needed_w, btn.b), None),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonKind {
    Primary,
    Secondary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonState {
    Normal,
    Hot,
    Pressed,
    Disabled,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ButtonColors {
    pub fill: Color,
    pub border: Color,
    pub text: Color,
}

const BLACK: Color = Color::new(0, 0, 0);

/// Border of a secondary button: the muted text color toned down towards the
/// window background (at least 3:1 against it in both palettes, tested).
pub fn secondary_border(pal: &ThemePalette) -> Color {
    pal.text_muted.mix(&pal.background, 15)
}

/// Colors of an owner-drawn button. Primary = accent fill with the more
/// legible of white / near-black as caption; secondary = the palette's button
/// colors with a visible border.
pub fn button_colors(pal: &ThemePalette, kind: ButtonKind, state: ButtonState) -> ButtonColors {
    match (kind, state) {
        (_, ButtonState::Disabled) => ButtonColors {
            fill: pal.button_background,
            border: pal.button_background,
            text: pal.text_muted,
        },
        (ButtonKind::Primary, s) => {
            let fill = match s {
                ButtonState::Hot => pal.accent_hover,
                ButtonState::Pressed => pal.accent.mix(&BLACK, 20),
                _ => pal.accent,
            };
            ButtonColors { fill, border: fill, text: fill.readable_text() }
        }
        (ButtonKind::Secondary, s) => {
            let (fill, border) = match s {
                ButtonState::Hot => (pal.button_secondary, pal.accent),
                ButtonState::Pressed => (pal.button_secondary.mix(&BLACK, 15), pal.accent),
                _ => (pal.button_background, secondary_border(pal)),
            };
            ButtonColors { fill, border, text: pal.button_text }
        }
    }
}

/// Colors of a check box drawn by the Dark theme: the box, its border and the
/// check mark. Unchecked boxes look like input fields; a checked one is filled
/// with the accent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CheckColors {
    pub fill: Color,
    pub border: Color,
    pub mark: Color,
    pub text: Color,
}

pub fn check_colors(pal: &ThemePalette, state: ButtonState, checked: bool) -> CheckColors {
    if state == ButtonState::Disabled {
        return CheckColors {
            fill: pal.button_background,
            border: pal.button_background,
            mark: pal.text_muted,
            text: pal.text_muted,
        };
    }
    let border = match state {
        ButtonState::Hot | ButtonState::Pressed => pal.accent,
        _ => secondary_border(pal),
    };
    if checked {
        CheckColors { fill: pal.accent, border: pal.accent, mark: pal.accent.readable_text(), text: pal.text_primary }
    } else {
        CheckColors { fill: pal.input_background, border, mark: pal.text_primary, text: pal.text_primary }
    }
}

// ---------------------------------------------------------------------------
// Contrast pairs (single source for the code and the tests)
// ---------------------------------------------------------------------------

/// Every text / background pair the dialogs draw, with the minimum contrast
/// ratio it must reach: 4.5 for text, 3.0 for borders and disabled captions.
#[cfg(test)]
pub fn contrast_pairs(pal: &ThemePalette) -> Vec<(&'static str, Color, Color, f64)> {
    let mut v = vec![
        ("title/body text on window", pal.text_primary, pal.background, 4.5),
        ("caption text on window", role_text_color(pal, Role::Caption), pal.background, 4.5),
        ("muted text on window", pal.text_muted, pal.background, 4.5),
        ("error text on window", pal.error_text, pal.background, 4.5),
        ("input text on input", pal.text_primary, pal.input_background, 4.5),
        ("secondary border on window", secondary_border(pal), pal.background, 3.0),
    ];
    let states = [
        ("normal", ButtonState::Normal),
        ("hot", ButtonState::Hot),
        ("pressed", ButtonState::Pressed),
    ];
    for (sn, s) in states {
        let p = button_colors(pal, ButtonKind::Primary, s);
        let q = button_colors(pal, ButtonKind::Secondary, s);
        let name_p: &'static str = match sn {
            "normal" => "primary button text (normal)",
            "hot" => "primary button text (hot)",
            _ => "primary button text (pressed)",
        };
        let name_q: &'static str = match sn {
            "normal" => "secondary button text (normal)",
            "hot" => "secondary button text (hot)",
            _ => "secondary button text (pressed)",
        };
        v.push((name_p, p.text, p.fill, 4.5));
        v.push((name_q, q.text, q.fill, 4.5));
    }
    let d = button_colors(pal, ButtonKind::Secondary, ButtonState::Disabled);
    v.push(("disabled button text", d.text, d.fill, 3.0));
    let c = check_colors(pal, ButtonState::Normal, true);
    v.push(("check mark on checked box", c.mark, c.fill, 3.0));
    let u = check_colors(pal, ButtonState::Normal, false);
    v.push(("unchecked box border on window", u.border, pal.background, 3.0));
    v.push(("check text on window", u.text, pal.background, 4.5));
    let cd = check_colors(pal, ButtonState::Disabled, false);
    v.push(("disabled check text on window", cd.text, pal.background, 3.0));
    v
}

// ---------------------------------------------------------------------------
// Placement
// ---------------------------------------------------------------------------

/// Top-left of a `w` x `h` window centered on `over`, kept inside `work`.
pub fn center_in(over: Rect, w: i32, h: i32, work: Rect) -> (i32, i32) {
    let x = over.l + (over.w() - w) / 2;
    let y = over.t + (over.h() - h) / 2;
    let x = x.min(work.r - w).max(work.l);
    let y = y.min(work.b - h).max(work.t);
    (x, y)
}

// ---------------------------------------------------------------------------
// Vertical reflow
// ---------------------------------------------------------------------------

/// Shift every rect that sits below a growing one. `grow[i]` is the extra
/// height rect `i` needs. Growers whose vertical ranges overlap form a band
/// that pushes by its largest growth only (two labels side by side must not
/// add up). Returns the new rects and the total height added.
pub fn push_down(rects: &[Rect], grow: &[i32]) -> (Vec<Rect>, i32) {
    debug_assert_eq!(rects.len(), grow.len());
    // Bands of overlapping growers: (top, bottom, max growth).
    let mut growers: Vec<(i32, i32, i32)> = rects
        .iter()
        .zip(grow)
        .filter(|(_, g)| **g > 0)
        .map(|(r, g)| (r.t, r.b, *g))
        .collect();
    growers.sort();
    let mut bands: Vec<(i32, i32, i32)> = Vec::new();
    for (t, b, g) in growers {
        match bands.last_mut() {
            Some(last) if t < last.1 => {
                last.1 = last.1.max(b);
                last.2 = last.2.max(g);
            }
            _ => bands.push((t, b, g)),
        }
    }
    let total: i32 = bands.iter().map(|b| b.2).sum();
    let out = rects
        .iter()
        .zip(grow)
        .map(|(r, g)| {
            let shift: i32 = bands.iter().filter(|b| r.t >= b.1).map(|b| b.2).sum();
            Rect::new(r.l, r.t + shift, r.r, r.b + shift + g.max(&0))
        })
        .collect();
    (out, total)
}

// ---------------------------------------------------------------------------
// Whole-dialog layout
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemKind {
    /// Static text; wraps when it does not fit.
    Label,
    /// Check box / radio button; wraps when it does not fit.
    Check,
    PushButton,
    /// Edit, combo box, list: never resized by the reflow.
    Input,
    /// Hairline frame.
    Separator,
    Other,
}

#[derive(Clone, Copy, Debug)]
pub struct LayoutItem {
    pub kind: ItemKind,
    /// Rect at the target DPI (already scaled).
    pub rect: Rect,
    /// Width of the caption on one line at the target DPI, 0 without caption.
    pub text_w: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LayoutResult {
    pub rects: Vec<Rect>,
    pub client_w: i32,
    pub client_h: i32,
    /// Labels / check boxes whose text had to wrap (need a multi-line style).
    pub wrapped: Vec<usize>,
}

/// Re-flow a dialog whose controls were scaled to `dpi`:
/// 1. labels and check boxes whose caption does not fit first take free width
///    on their row, then wrap (height measured by `wrap_h(index, width)`),
///    pushing everything below down;
/// 2. push buttons get the uniform height; those of a clean bottom row become
///    one right-aligned footer of equal-width buttons, the others only grow
///    to fit their caption.
pub fn layout(
    items: &[LayoutItem],
    client_w: i32,
    client_h: i32,
    dpi: u32,
    wrap_h: &mut dyn FnMut(usize, i32) -> i32,
) -> LayoutResult {
    let n = items.len();
    let mut rects: Vec<Rect> = items.iter().map(|i| i.rect).collect();
    let gap = scale(GAP_DP, dpi);
    let margin = left_margin(items, dpi);
    let mut client_w = client_w;
    let mut wrapped = Vec::new();

    // 1. Text that does not fit.
    let mut grow = vec![0; n];
    for i in 0..n {
        let it = &items[i];
        let chrome = if it.kind == ItemKind::Check { scale(CHECK_CHROME_DP, dpi) } else { 0 };
        if !matches!(it.kind, ItemKind::Label | ItemKind::Check) || it.text_w == 0 {
            continue;
        }
        let need_w = it.text_w + chrome;
        let r = rects[i];
        if need_w <= r.w() {
            continue;
        }
        // Free width on the row: up to the right margin, stopping at any
        // control that shares the row and sits to the right.
        let mut limit = client_w - margin;
        for (j, o) in rects.iter().enumerate() {
            if j != i && o.l >= r.r && r.overlaps_vertically(o) && items[j].kind != ItemKind::Separator {
                limit = limit.min(o.l - gap);
            }
        }
        let widened = (limit - r.l).max(r.w());
        if need_w <= widened {
            rects[i].r = r.l + widened.min(need_w.max(r.w()));
            continue;
        }
        rects[i].r = r.l + widened;
        let h = wrap_h(i, widened - chrome);
        if h > r.h() {
            grow[i] = h - r.h();
        }
        wrapped.push(i);
    }
    let (pushed, mut added) = push_down(&rects, &grow);
    rects = pushed;

    // 2. Buttons.
    let bh = scale(BUTTON_H_DP, dpi);
    let push_idx: Vec<usize> = (0..n).filter(|&i| items[i].kind == ItemKind::PushButton).collect();
    for &i in &push_idx {
        let c = (rects[i].t + rects[i].b) / 2;
        rects[i].t = c - bh / 2;
        rects[i].b = rects[i].t + bh;
    }
    let mut footer: Vec<usize> = Vec::new();
    if let Some(&bottom) = push_idx.iter().max_by_key(|&&i| rects[i].t) {
        let row: Vec<usize> = push_idx
            .iter()
            .copied()
            .filter(|&i| (rects[i].t - rects[bottom].t).abs() <= scale(4, dpi))
            .collect();
        let widths: Vec<i32> = {
            let uniform = row.iter().map(|&i| button_width(items[i].text_w, dpi)).max().unwrap_or(0);
            vec![uniform; row.len()]
        };
        let mut order = row.clone();
        order.sort_by_key(|&i| rects[i].l);
        let (xs, new_w) = footer_layout(client_w, margin, &widths, gap);
        let proposed: Vec<Rect> = order
            .iter()
            .zip(xs.iter().zip(&widths))
            .map(|(&i, (&x, &w))| Rect::new(x, rects[i].t, x + w, rects[i].b))
            .collect();
        let clear = (0..n).all(|j| {
            if row.contains(&j) || items[j].kind == ItemKind::Separator {
                return true;
            }
            proposed.iter().all(|p| !(p.overlaps_vertically(&rects[j]) && p.overlaps_horizontally(&rects[j])))
        });
        // Only inputs on the row make it a field row rather than a footer.
        let field_on_row = (0..n).any(|j| {
            !row.contains(&j) && items[j].kind == ItemKind::Input && rects[j].overlaps_vertically(&rects[bottom])
        });
        if clear && !field_on_row {
            for (k, &i) in order.iter().enumerate() {
                rects[i] = proposed[k];
            }
            client_w = new_w;
            footer = row;
        }
    }
    // Buttons outside the footer only grow to fit their caption.
    for &i in &push_idx {
        if footer.contains(&i) {
            continue;
        }
        let need = items[i].text_w + 2 * scale(BUTTON_PAD_DP, dpi);
        if rects[i].w() >= need {
            continue;
        }
        let left = (0..n)
            .filter(|&j| j != i && rects[j].r <= rects[i].l && rects[j].overlaps_vertically(&rects[i]))
            .filter(|&j| items[j].kind != ItemKind::Separator)
            .max_by_key(|&j| rects[j].r);
        let (b, s) = widen_trailing(rects[i], left.map(|j| rects[j]), need, gap);
        rects[i] = b;
        if let (Some(j), Some(s)) = (left, s) {
            rects[j] = s;
        }
    }

    // Window height: the pushed content plus any footer growth below.
    let old_bottom = items.iter().map(|i| i.rect.b).max().unwrap_or(0);
    let new_bottom = rects.iter().map(|r| r.b).max().unwrap_or(0);
    added = added.max(new_bottom - old_bottom);
    let client_h = client_h + added.max(0);
    LayoutResult { rects, client_w, client_h, wrapped }
}

/// The dialog's own left margin: the smallest left edge among real controls,
/// when it is a plausible margin (positive, at most 40 dp), else `MARGIN_DP`.
pub fn left_margin(items: &[LayoutItem], dpi: u32) -> i32 {
    let m = items
        .iter()
        .filter(|i| !matches!(i.kind, ItemKind::Separator))
        .map(|i| i.rect.l)
        .min()
        .unwrap_or(0);
    if m > 0 && m <= scale(40, dpi) {
        m
    } else {
        scale(MARGIN_DP, dpi)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{get_palette, ThemeMode};

    fn item(kind: ItemKind, l: i32, t: i32, r: i32, b: i32, text_w: i32) -> LayoutItem {
        LayoutItem { kind, rect: Rect::new(l, t, r, b), text_w }
    }

    #[test]
    fn scale_identity_and_rounding() {
        assert_eq!(scale(100, 96), 100);
        assert_eq!(scale(100, 144), 150);
        assert_eq!(scale(15, 120), 19); // 18.75
        assert_eq!(scale(-15, 120), -19);
        assert_eq!(scale(0, 192), 0);
        assert_eq!(scale(1, 144), 2); // 1.5 rounds away from zero
    }

    #[test]
    fn scaled_rect_keeps_adjacency() {
        let a = Rect::new(10, 10, 55, 40).scaled(144);
        let b = Rect::new(55, 10, 100, 40).scaled(144);
        assert_eq!(a.r, b.l);
    }

    #[test]
    fn font_roles_follow_authored_sizes() {
        assert_eq!(role_for(16, 400), Role::Body);
        assert_eq!(role_for(-16, 400), Role::Body);
        assert_eq!(role_for(15, 400), Role::Caption);
        assert_eq!(role_for(14, 400), Role::Caption);
        assert_eq!(role_for(16, 700), Role::Section);
        assert_eq!(role_for(22, 700), Role::Title);
        assert_eq!(role_for(24, 700), Role::Title);
        assert_eq!(role_for(22, 400), Role::Title);
    }

    #[test]
    fn font_sizes_descend_with_importance() {
        let pt = |r| role_font(r).0;
        assert!(pt(Role::Title) > pt(Role::Section));
        assert!(pt(Role::Section) > pt(Role::Body));
        assert!(pt(Role::Body) > pt(Role::Caption));
        assert_eq!(font_height_px(9, 96), -12);
        assert_eq!(font_height_px(9, 144), -18);
        assert_eq!(font_height_px(13, 96), -17);
    }

    #[test]
    fn button_width_has_minimum_and_padding() {
        assert_eq!(button_width(20, 96), 88);
        assert_eq!(button_width(120, 96), 152);
        assert_eq!(button_width(120, 144), 168);
    }

    #[test]
    fn footer_is_right_aligned_with_gaps() {
        let (xs, w) = footer_layout(400, 20, &[88, 88], 8);
        assert_eq!(w, 400);
        assert_eq!(xs, vec![196, 292]);
        assert_eq!(xs[1] + 88, 380); // right margin respected
    }

    #[test]
    fn footer_grows_the_window_when_it_does_not_fit() {
        let (xs, w) = footer_layout(200, 20, &[100, 100], 8);
        assert_eq!(w, 248);
        assert_eq!(xs, vec![20, 128]);
        assert_eq!(footer_layout(300, 20, &[], 8), (vec![], 300));
    }

    #[test]
    fn widen_trailing_takes_space_from_the_left_sibling() {
        let (b, s) = widen_trailing(Rect::new(300, 0, 340, 25), Some(Rect::new(20, 0, 292, 25)), 80, 8);
        assert_eq!(b, Rect::new(260, 0, 340, 25));
        assert_eq!(s, Some(Rect::new(20, 0, 252, 25)));
        let (b, s) = widen_trailing(Rect::new(20, 0, 40, 25), None, 80, 8);
        assert_eq!(b, Rect::new(20, 0, 100, 25));
        assert_eq!(s, None);
        let keep = Rect::new(0, 0, 100, 20);
        assert_eq!(widen_trailing(keep, None, 80, 8).0, keep);
    }

    #[test]
    fn push_down_shifts_only_what_is_below() {
        let rects = [Rect::new(0, 0, 100, 20), Rect::new(0, 30, 100, 50), Rect::new(110, 0, 200, 20)];
        let (out, total) = push_down(&rects, &[15, 0, 0]);
        assert_eq!(total, 15);
        assert_eq!(out[0], Rect::new(0, 0, 100, 35)); // grew
        assert_eq!(out[1], Rect::new(0, 45, 100, 65)); // pushed
        assert_eq!(out[2], rects[2]); // same row: untouched
    }

    #[test]
    fn push_down_does_not_add_growth_of_a_shared_row() {
        let rects = [Rect::new(0, 0, 50, 20), Rect::new(60, 0, 110, 20), Rect::new(0, 30, 110, 50)];
        let (out, total) = push_down(&rects, &[10, 14, 0]);
        assert_eq!(total, 14);
        assert_eq!(out[2].t, 44);
    }

    #[test]
    fn push_down_stacks_separate_bands() {
        let rects = [Rect::new(0, 0, 50, 20), Rect::new(0, 30, 50, 50), Rect::new(0, 60, 50, 80)];
        let (out, total) = push_down(&rects, &[10, 5, 0]);
        assert_eq!(total, 15);
        assert_eq!(out[1].t, 40);
        assert_eq!(out[2].t, 75);
    }

    #[test]
    fn center_stays_inside_the_work_area() {
        let work = Rect::new(0, 0, 1920, 1040);
        assert_eq!(center_in(Rect::new(100, 100, 900, 700), 400, 200, work), (300, 300));
        // Owner near the right edge: clamped.
        assert_eq!(center_in(Rect::new(1700, 0, 2100, 100), 400, 200, work), (1520, 0));
        // Owner above the work area: clamped to its top.
        assert_eq!(center_in(Rect::new(0, -500, 100, -400), 50, 50, work), (25, 0));
    }

    #[test]
    fn long_label_widens_into_free_space_before_wrapping() {
        let items = [
            item(ItemKind::Label, 20, 10, 120, 30, 200),
            item(ItemKind::PushButton, 300, 100, 380, 130, 40),
        ];
        let mut called = false;
        let r = layout(&items, 400, 150, 96, &mut |_, _| {
            called = true;
            40
        });
        assert!(!called, "fits after widening, no wrap measurement needed");
        assert_eq!(r.rects[0].r, 220);
        assert!(r.wrapped.is_empty());
    }

    #[test]
    fn long_label_wraps_and_pushes_the_rest_down() {
        let items = [
            item(ItemKind::Label, 20, 10, 120, 30, 900),
            item(ItemKind::Input, 20, 40, 380, 65, 0),
            item(ItemKind::PushButton, 300, 100, 380, 130, 40),
        ];
        let r = layout(&items, 400, 150, 96, &mut |i, w| {
            assert_eq!(i, 0);
            assert_eq!(w, 360); // up to the right margin
            60
        });
        assert_eq!(r.wrapped, vec![0]);
        assert_eq!(r.rects[0], Rect::new(20, 10, 380, 70));
        assert_eq!(r.rects[1].t, 80); // 40 + 40 grown
        assert_eq!(r.client_h, 190);
    }

    #[test]
    fn label_does_not_widen_over_a_neighbour_on_its_row() {
        let items = [
            item(ItemKind::Label, 20, 10, 80, 30, 150),
            item(ItemKind::Input, 100, 10, 380, 35, 0),
        ];
        let r = layout(&items, 400, 100, 96, &mut |_, w| {
            assert!(w <= 72, "limited by the input at x=100, got {w}");
            50
        });
        assert!(r.rects[0].r <= 92);
        assert_eq!(r.wrapped, vec![0]);
    }

    #[test]
    fn check_box_accounts_for_its_glyph() {
        let items = [item(ItemKind::Check, 20, 10, 150, 32, 135)];
        let r = layout(&items, 400, 100, 96, &mut |_, _| unreachable!());
        assert_eq!(r.rects[0].r, 175); // 20 + (135 + 20 glyph)
    }

    #[test]
    fn footer_buttons_are_uniform_right_aligned_and_cleared() {
        let items = [
            item(ItemKind::Input, 20, 10, 380, 35, 0),
            item(ItemKind::PushButton, 120, 100, 200, 128, 30),
            item(ItemKind::PushButton, 215, 100, 295, 128, 70),
        ];
        let r = layout(&items, 400, 150, 96, &mut |_, _| 0);
        let (a, b) = (r.rects[1], r.rects[2]);
        assert_eq!(a.w(), b.w());
        assert_eq!(a.h(), 30);
        assert_eq!(b.r, 380);
        assert_eq!(b.l - a.r, 8);
        assert_eq!(a.w(), 102); // 70 + 2*16
    }

    #[test]
    fn buttons_beside_an_input_are_not_a_footer() {
        let items = [
            item(ItemKind::Input, 20, 100, 290, 125, 0),
            item(ItemKind::PushButton, 300, 100, 380, 125, 40),
        ];
        let r = layout(&items, 400, 150, 96, &mut |_, _| 0);
        assert_eq!(r.rects[1].r, 380); // not moved
        assert_eq!(r.rects[0], items[0].rect);
    }

    #[test]
    fn narrow_trailing_button_grows_at_the_expense_of_its_input() {
        let items = [
            item(ItemKind::Input, 20, 100, 330, 125, 0),
            item(ItemKind::PushButton, 340, 100, 380, 125, 100),
        ];
        let r = layout(&items, 400, 150, 96, &mut |_, _| 0);
        assert_eq!(r.rects[1].w(), 132); // 100 + 2*16
        assert_eq!(r.rects[1].r, 380);
        assert!(r.rects[0].r + 8 <= r.rects[1].l);
    }

    #[test]
    fn footer_widens_the_window_for_long_captions() {
        let items = [
            item(ItemKind::Label, 20, 10, 200, 30, 50),
            item(ItemKind::PushButton, 20, 100, 100, 130, 180),
            item(ItemKind::PushButton, 110, 100, 190, 130, 180),
        ];
        let r = layout(&items, 220, 150, 96, &mut |_, _| 20);
        // Two buttons of 212 + gap 8 + two margins of 20.
        assert_eq!(r.client_w, 212 * 2 + 8 + 40);
        assert_eq!(r.rects[2].r, r.client_w - 20);
    }

    #[test]
    fn layout_is_dpi_consistent() {
        // The same authored dialog at 150 %: scaled input, same structure.
        let a = [
            item(ItemKind::Input, 30, 15, 570, 53, 0),
            item(ItemKind::PushButton, 180, 150, 300, 195, 45),
            item(ItemKind::PushButton, 315, 150, 435, 195, 105),
        ];
        let r = layout(&a, 600, 240, 144, &mut |_, _| 0);
        assert_eq!(r.rects[1].h(), 45);
        assert_eq!(r.rects[2].r, 570);
        assert_eq!(r.rects[1].w(), r.rects[2].w());
    }

    #[test]
    fn left_margin_is_inferred_or_defaulted() {
        let items = [item(ItemKind::Label, 25, 0, 100, 20, 10), item(ItemKind::Separator, 0, 50, 400, 51, 0)];
        assert_eq!(left_margin(&items, 96), 25);
        let none = [item(ItemKind::Label, 0, 0, 100, 20, 10)];
        assert_eq!(left_margin(&none, 96), 20);
        assert_eq!(left_margin(&none, 144), 30);
    }

    fn both() -> [(&'static str, &'static ThemePalette); 2] {
        [("dark", get_palette(ThemeMode::Dark)), ("light", get_palette(ThemeMode::Light))]
    }

    #[test]
    fn every_dialog_text_pair_is_legible_in_both_palettes() {
        for (pn, pal) in both() {
            for (name, fg, bg, min) in contrast_pairs(pal) {
                let ratio = fg.contrast_with(&bg);
                assert!(ratio >= min, "{pn}: {name} contrast {ratio:.2} is below {min}:1");
            }
        }
    }

    #[test]
    fn primary_is_accent_filled_and_distinct_from_secondary() {
        for (pn, pal) in both() {
            let p = button_colors(pal, ButtonKind::Primary, ButtonState::Normal);
            let s = button_colors(pal, ButtonKind::Secondary, ButtonState::Normal);
            assert_eq!(p.fill, pal.accent, "{pn}");
            assert_ne!(p.fill, s.fill, "{pn}");
            assert_eq!(button_colors(pal, ButtonKind::Primary, ButtonState::Hot).fill, pal.accent_hover, "{pn}");
        }
    }

    #[test]
    fn pressed_state_differs_from_normal_for_both_kinds() {
        for (pn, pal) in both() {
            for k in [ButtonKind::Primary, ButtonKind::Secondary] {
                let n = button_colors(pal, k, ButtonState::Normal);
                let p = button_colors(pal, k, ButtonState::Pressed);
                assert_ne!(n.fill, p.fill, "{pn} {k:?}");
            }
        }
    }

    #[test]
    fn readable_text_picks_the_better_of_white_and_ink() {
        let white = Color::new(255, 255, 255);
        assert_eq!(Color::new(0, 0, 0).readable_text(), white);
        assert_ne!(Color::new(250, 250, 250).readable_text(), white);
        assert!(Color::new(255, 255, 255).contrast_with(&Color::new(0, 0, 0)) > 20.9);
    }

    #[test]
    fn checked_box_is_accent_filled_and_unchecked_is_not() {
        for (pn, pal) in both() {
            let on = check_colors(pal, ButtonState::Normal, true);
            let off = check_colors(pal, ButtonState::Normal, false);
            assert_eq!(on.fill, pal.accent, "{pn}");
            assert_eq!(off.fill, pal.input_background, "{pn}");
            assert_eq!(check_colors(pal, ButtonState::Hot, false).border, pal.accent, "{pn}");
        }
    }

    #[test]
    fn caption_role_uses_the_secondary_color() {
        let pal = get_palette(ThemeMode::Dark);
        assert_eq!(role_text_color(pal, Role::Caption), pal.text_secondary);
        assert_eq!(role_text_color(pal, Role::Body), pal.text_primary);
        assert_eq!(role_text_color(pal, Role::Title), pal.text_primary);
    }
}
