// Side panels — pure geometry of the right-side panels (Vault, TOTP,
// Attach, Prompts).
//
// Single source for the width of the open panel (used by layout_controls and
// the resize grip): the bounds, the room the editor keeps, the width a drag
// asks for and the one stored in the settings, and the width of the controls
// inside a panel. Only one panel is open at a time (close_other_panels).

use crate::settings::Settings;

/// A resizable right-side panel. A new panel adds a variant here, its
/// default width, and its `*_panel_width` setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidePanel {
    Vault,
    Totp,
    Attach,
    Prompts,
}

/// Narrowest panel a drag or the settings can ask for.
pub const MIN_PANEL_W: u32 = 220;
/// Upper bound of a stored width (same as `window_width`).
pub const MAX_PANEL_W: u32 = 8000;
/// Width the editor keeps next to an open panel (gutter not included).
pub const MIN_EDITOR_W: u32 = 200;
/// Width of the resize grip, between the editor and the open panel.
pub const GRIP_W: u32 = 5;
/// Horizontal space around the controls of a panel: 8 px on the left,
/// 12 px on the right (frame border included).
pub const PANEL_INNER_INSET: u32 = 20;

/// Default width of the prompts panel (issue 058). Kept here, not in
/// `ui::prompts_panel`: `Settings` takes its default from `default_width`.
pub const PROMPTS_PANEL_WIDTH: u32 = 320;

/// Width of a panel never resized (the fixed widths of earlier versions).
pub fn default_width(p: SidePanel) -> u32 {
    match p {
        SidePanel::Vault => crate::ui::vault_panel::VAULT_PANEL_WIDTH,
        SidePanel::Totp => crate::ui::totp_panel::TOTP_PANEL_WIDTH,
        SidePanel::Attach => crate::ui::attachments_panel::ATTACHMENTS_PANEL_WIDTH,
        SidePanel::Prompts => PROMPTS_PANEL_WIDTH,
    }
}

/// Width of the open panel in a client area `client_w` wide whose gutter is
/// `gutter_w`: `requested`, at least `MIN_PANEL_W`, but never more than what
/// leaves `MIN_EDITOR_W` to the editor and room for the grip. A window too
/// small shrinks the panel under its minimum, down to 0.
pub fn clamp_panel_width(requested: u32, client_w: u32, gutter_w: u32) -> u32 {
    let room = client_w
        .saturating_sub(gutter_w)
        .saturating_sub(MIN_EDITOR_W)
        .saturating_sub(GRIP_W);
    requested.max(MIN_PANEL_W).min(room)
}

/// Width asked by a drag of the grip that started at screen x `start_x` on a
/// panel `start_w` wide and is now at `cursor_x`: the panel is on the right,
/// so moving left widens it. Not bounded here (see `clamp_panel_width`).
pub fn drag_width(start_w: u32, start_x: i32, cursor_x: i32) -> u32 {
    let w = i64::from(start_w) + i64::from(start_x) - i64::from(cursor_x);
    w.clamp(0, i64::from(u32::MAX)) as u32
}

/// Width written to the settings at the end of a drag, from the width the
/// panel was shown at.
pub fn width_to_store(shown: u32) -> u32 {
    shown.clamp(MIN_PANEL_W, MAX_PANEL_W)
}

/// Width to write at the end of a drag: the panel was shown `start_shown`
/// wide when the button went down and `end_shown` wide at release. `None`
/// when the shown width did not change (a click, or a window too narrow to
/// let the panel move): the setting, possibly wider, stays.
pub fn width_to_store_after_drag(start_shown: u32, end_shown: u32) -> Option<u32> {
    (end_shown != start_shown).then(|| width_to_store(end_shown))
}

/// Copy this session's panel widths from `from` into `into`. The widths are
/// machine state set by dragging: when the settings are replaced (unlock
/// reads them back from disk, an import from a backup), the session's
/// widths are kept, whether or not they were saved yet.
pub fn keep_machine_panel_widths(from: &Settings, into: &mut Settings) {
    into.vault_panel_width = from.vault_panel_width;
    into.totp_panel_width = from.totp_panel_width;
    into.attach_panel_width = from.attach_panel_width;
    into.prompts_panel_width = from.prompts_panel_width;
}

/// x of the frame of the open panel, `panel_w` wide, in a client area
/// `client_w` wide: against the right edge. Only one panel is open at a
/// time, so no other panel width counts (and the grip is not inside it).
pub fn frame_x(client_w: u32, panel_w: u32) -> i32 {
    client_w as i32 - panel_w as i32
}

/// x of the resize grip: right against the left edge of the open panel.
pub fn grip_x(client_w: u32, panel_w: u32) -> i32 {
    frame_x(client_w, panel_w) - GRIP_W as i32
}

/// Width of the lists, tree and fields of a panel `panel_w` wide.
pub fn inner_width(panel_w: u32) -> u32 {
    panel_w.saturating_sub(PANEL_INNER_INSET)
}

/// The stored width of `p`.
pub fn width_setting(s: &Settings, p: SidePanel) -> u32 {
    match p {
        SidePanel::Vault => s.vault_panel_width,
        SidePanel::Totp => s.totp_panel_width,
        SidePanel::Attach => s.attach_panel_width,
        SidePanel::Prompts => s.prompts_panel_width,
    }
}

pub fn set_width_setting(s: &mut Settings, p: SidePanel, w: u32) {
    match p {
        SidePanel::Vault => s.vault_panel_width = w,
        SidePanel::Totp => s.totp_panel_width = w,
        SidePanel::Attach => s.attach_panel_width = w,
        SidePanel::Prompts => s.prompts_panel_width = w,
    }
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [SidePanel; 4] = [SidePanel::Vault, SidePanel::Totp, SidePanel::Attach, SidePanel::Prompts];

    #[test]
    fn defaults_are_the_historical_widths() {
        assert_eq!(default_width(SidePanel::Vault), 300);
        assert_eq!(default_width(SidePanel::Totp), 280);
        assert_eq!(default_width(SidePanel::Attach), 280);
        // New with issue 058: room for "Category · Name" and four buttons.
        assert_eq!(default_width(SidePanel::Prompts), 320);
        for p in ALL {
            assert!(default_width(p) >= MIN_PANEL_W, "{p:?}");
        }
    }

    // Lot B regression (6f0c8db): the Attach frame was placed at the width
    // of every panel plus the grip, so it covered the grip and left a 5 px
    // strip uncovered on the right. Every panel: frame flush with the right
    // edge, grip just left of it, nothing overlapping.
    #[test]
    fn frame_is_flush_right_and_grip_just_left_of_it() {
        for (client, panel) in [(900u32, 300u32), (1920, 600), (500, 255), (400, 0)] {
            let x = frame_x(client, panel);
            assert_eq!(x + panel as i32, client as i32, "{client} {panel}");
            assert_eq!(grip_x(client, panel) + GRIP_W as i32, x, "{client} {panel}");
        }
    }

    #[test]
    fn inner_width_keeps_the_historical_margins() {
        // 300 -> 280 (vault tree) and 280 -> 260 (TOTP / Attach lists).
        assert_eq!(inner_width(300), 280);
        assert_eq!(inner_width(280), 260);
        assert_eq!(inner_width(500), 480);
        // Never negative.
        assert_eq!(inner_width(0), 0);
        assert_eq!(inner_width(PANEL_INNER_INSET - 1), 0);
    }

    #[test]
    fn requested_width_is_kept_when_the_window_has_room() {
        assert_eq!(clamp_panel_width(300, 900, 40), 300);
        assert_eq!(clamp_panel_width(600, 1920, 40), 600);
    }

    #[test]
    fn width_never_goes_under_the_minimum_when_there_is_room() {
        assert_eq!(clamp_panel_width(0, 900, 40), MIN_PANEL_W);
        assert_eq!(clamp_panel_width(MIN_PANEL_W - 1, 900, 40), MIN_PANEL_W);
    }

    #[test]
    fn editor_keeps_its_minimum_width() {
        let (client, gutter) = (900, 40);
        let w = clamp_panel_width(5000, client, gutter);
        assert_eq!(w, client - gutter - MIN_EDITOR_W - GRIP_W);
        assert_eq!(client - w - GRIP_W - gutter, MIN_EDITOR_W);
    }

    #[test]
    fn small_window_shrinks_the_panel_under_its_minimum() {
        // 500 - 40 - 200 - 5 = 255 of room: the default 300 shrinks.
        assert_eq!(clamp_panel_width(300, 500, 40), 255);
        // Less room than the minimum: the editor still wins.
        assert_eq!(clamp_panel_width(300, 400, 40), 155);
    }

    #[test]
    fn tiny_window_gives_zero_never_negative() {
        for client in [0u32, 1, 100, 200, 244, 245] {
            assert_eq!(clamp_panel_width(300, client, 40), 0, "client={client}");
        }
        assert_eq!(clamp_panel_width(300, 246, 40), 1);
        // A gutter wider than the window.
        assert_eq!(clamp_panel_width(300, 30, 40), 0);
    }

    #[test]
    fn huge_values_do_not_overflow() {
        assert_eq!(clamp_panel_width(u32::MAX, u32::MAX, 0), u32::MAX - MIN_EDITOR_W - GRIP_W);
        assert_eq!(clamp_panel_width(u32::MAX, 7680, 60), 7680 - 60 - MIN_EDITOR_W - GRIP_W);
        assert_eq!(clamp_panel_width(300, 7680, 60), 300);
    }

    #[test]
    fn dragging_left_widens_and_right_narrows() {
        assert_eq!(drag_width(300, 1000, 900), 400);
        assert_eq!(drag_width(300, 1000, 1100), 200);
        assert_eq!(drag_width(300, 1000, 1000), 300);
        // Screen coordinates can be negative (monitor left of the primary).
        assert_eq!(drag_width(300, -500, -600), 400);
    }

    #[test]
    fn drag_width_never_goes_negative_nor_overflows() {
        assert_eq!(drag_width(300, 0, 5000), 0);
        assert_eq!(drag_width(300, i32::MIN, i32::MAX), 0);
        assert_eq!(drag_width(u32::MAX, i32::MAX, i32::MIN), u32::MAX);
    }

    #[test]
    fn stored_width_stays_in_the_settings_range() {
        assert_eq!(width_to_store(450), 450);
        // A window too small showed the panel narrower than its minimum:
        // the setting keeps the minimum, the panel regrows with the window.
        assert_eq!(width_to_store(150), MIN_PANEL_W);
        assert_eq!(width_to_store(0), MIN_PANEL_W);
        assert_eq!(width_to_store(u32::MAX), MAX_PANEL_W);
    }

    #[test]
    fn drag_stores_only_a_width_the_panel_was_really_shown_at() {
        // Shown at 300, released at 450: 450.
        assert_eq!(width_to_store_after_drag(300, 450), Some(450));
        // Window too narrow: the panel stayed at 155 whatever the cursor
        // did. The setting (maybe 600) is not overwritten with 220.
        assert_eq!(width_to_store_after_drag(155, 155), None);
        // A real change in a narrow window is stored within the range.
        assert_eq!(width_to_store_after_drag(155, 120), Some(MIN_PANEL_W));
        assert_eq!(width_to_store_after_drag(300, 9000), Some(MAX_PANEL_W));
    }

    #[test]
    fn session_widths_survive_a_settings_replacement() {
        // Lock/unlock and imports replace the settings with the ones read
        // from disk or from a backup: this session's widths are kept.
        let mut session = crate::settings::Settings::default();
        session.vault_panel_width = 500;
        session.totp_panel_width = 333;
        session.attach_panel_width = 444;
        session.prompts_panel_width = 555;
        let mut from_disk = crate::settings::Settings::default();
        from_disk.vault_panel_width = 250;
        from_disk.auto_lock = 3;
        keep_machine_panel_widths(&session, &mut from_disk);
        assert_eq!(
            (from_disk.vault_panel_width, from_disk.totp_panel_width, from_disk.attach_panel_width),
            (500, 333, 444)
        );
        assert_eq!(from_disk.prompts_panel_width, 555);
        // Nothing else is taken from the session.
        assert_eq!(from_disk.auto_lock, 3);
    }

    #[test]
    fn settings_hold_one_width_per_panel() {
        let mut s = crate::settings::Settings::default();
        for p in ALL {
            assert_eq!(width_setting(&s, p), default_width(p), "{p:?}");
        }
        set_width_setting(&mut s, SidePanel::Totp, 333);
        assert_eq!(width_setting(&s, SidePanel::Totp), 333);
        assert_eq!(width_setting(&s, SidePanel::Vault), 300);
        assert_eq!(width_setting(&s, SidePanel::Attach), 280);
        set_width_setting(&mut s, SidePanel::Vault, 444);
        set_width_setting(&mut s, SidePanel::Attach, 555);
        assert_eq!(
            (s.vault_panel_width, s.totp_panel_width, s.attach_panel_width),
            (444, 333, 555)
        );
        assert_eq!(width_setting(&s, SidePanel::Prompts), 320);
        set_width_setting(&mut s, SidePanel::Prompts, 666);
        assert_eq!(s.prompts_panel_width, 666);
        assert_eq!(s.attach_panel_width, 555);
    }
}
