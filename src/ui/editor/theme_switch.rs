// Live theme switch: re-apply everything that is themed at build time or
// cached, without restarting.
//
// `theme::set_mode` alone changes nothing on screen: the title bar, the popup
// menu mode, the RichEdit colours and scrollbar theme, the tooltip theme and
// the panels' brushes are all set once, and the owner-painted parts (gutter,
// tab strip, status bar, toolbar buttons) only read the palette when they
// paint. `refresh_plan` is the pure list of what a change must refresh;
// `EditorForm::apply_theme_live` performs each step (an exhaustive `match`, so
// a new `ThemeRefresh` variant cannot be forgotten).

#[allow(unused_imports)]
use super::*;

use crate::theme::ThemeMode;

/// One thing a theme change must refresh.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ThemeRefresh {
    /// DWM immersive dark flag and caption / text / border colours.
    TitleBar,
    /// Preferred app mode of the popup menus (+ FlushMenuThemes).
    PopupMenus,
    /// Owner-painted menu bar (DrawMenuBar + frame repaint).
    MenuBar,
    /// RichEdit background and default text colour.
    EditorColors,
    /// RichEdit scrollbar visual theme.
    EditorScrollbars,
    /// Tooltip control of the toolbar buttons.
    Tooltips,
    /// Vault tree colours (the other panels read the palette per message).
    SidePanels,
    Gutter,
    TabStrip,
    StatusBar,
    ToolbarButtons,
    /// Every child window (lock overlay, panels), erased and repainted.
    AllWindows,
}

/// What a switch from `old` to `new` must refresh, in execution order.
/// Nothing when the mode does not change.
pub(super) fn refresh_plan(old: ThemeMode, new: ThemeMode) -> Vec<ThemeRefresh> {
    use ThemeRefresh::*;
    if old == new {
        return Vec::new();
    }
    // Order matters: the popup mode is set before the bar is redrawn, the
    // controls take their colours before every window is repainted, and the
    // owner-painted surfaces (which draw straight to a DC) come last.
    vec![
        TitleBar, PopupMenus, MenuBar, EditorColors, EditorScrollbars, Tooltips, SidePanels,
        AllWindows, Gutter, TabStrip, StatusBar, ToolbarButtons,
    ]
}

impl EditorForm {
    /// Switch the whole window to `new` without a restart: set the global
    /// mode, then run every step of `refresh_plan`. No text, modified flag,
    /// undo stack or selection is touched. A no-op when the mode is unchanged.
    pub(super) fn apply_theme_live(&self, new: ThemeMode) {
        let old = crate::theme::current_mode();
        let plan = refresh_plan(old, new);
        if plan.is_empty() {
            return;
        }
        crate::theme::set_mode(new);
        eprintln!("INFO theme: live switch {old:?} -> {new:?} steps={}", plan.len());
        for step in plan {
            match step {
                // Also resets the class background brush shared by every nwg window.
                ThemeRefresh::TitleBar => crate::ui::dialogs::apply_window_chrome(&self.window),
                ThemeRefresh::PopupMenus => crate::ui::dark_chrome::apply_popup_menu_theme(),
                ThemeRefresh::MenuBar => crate::ui::dark_chrome::redraw_menu_bar(&self.window.handle),
                ThemeRefresh::EditorColors | ThemeRefresh::EditorScrollbars => {
                    // One pass sets both (colours and scrollbar class); run it
                    // once, at the first of the two steps.
                    if step == ThemeRefresh::EditorColors {
                        // Noted only: the highlight tick tells the user if
                        // the undo history stayed suspended (undo_watch).
                        let colours = surface::retheme(&self.editor);
                        self.note_undo(colours, "a colour change");
                        // Occurrence highlights carry the old theme's back
                        // colour in the text runs: force a recompute.
                        *self.last_highlighted.borrow_mut() = "\0".to_string();
                        self.update_occurrence_highlights();
                    }
                }
                ThemeRefresh::Tooltips => {
                    super::toolbar::retheme_tooltips();
                }
                ThemeRefresh::SidePanels => {
                    self.apply_vault_tree_colors();
                }
                ThemeRefresh::AllWindows => self.redraw_all_windows(),
                ThemeRefresh::Gutter => self.paint_gutter(),
                ThemeRefresh::TabStrip => {
                    self.paint_tab_bar();
                }
                ThemeRefresh::StatusBar => {
                    if let nwg::ControlHandle::Hwnd(h) = self.status_bar.handle {
                        unsafe {
                            let _ = windows::Win32::Graphics::Gdi::InvalidateRect(
                                Some(windows::Win32::Foundation::HWND(h as *mut _)),
                                None,
                                true,
                            );
                        }
                    }
                }
                ThemeRefresh::ToolbarButtons => {
                    self.repaint_toolbar_buttons();
                }
            }
        }
    }

    /// Erase and repaint the main window and all its children.
    fn redraw_all_windows(&self) {
        use windows::Win32::Graphics::Gdi::{RedrawWindow, RDW_ALLCHILDREN, RDW_ERASE, RDW_FRAME, RDW_INVALIDATE};
        let nwg::ControlHandle::Hwnd(h) = self.window.handle else {
            eprintln!("WARN theme: the main window has no handle; children were not repainted");
            return;
        };
        let ok = unsafe {
            RedrawWindow(
                Some(windows::Win32::Foundation::HWND(h as *mut _)),
                None,
                None,
                RDW_INVALIDATE | RDW_ERASE | RDW_FRAME | RDW_ALLCHILDREN,
            )
        };
        if !ok.as_bool() {
            eprintln!("WARN theme: RedrawWindow(ALLCHILDREN) failed; some surfaces repaint on their next event");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_change_refreshes_nothing() {
        for m in [ThemeMode::Dark, ThemeMode::Light] {
            assert!(refresh_plan(m, m).is_empty());
        }
    }

    #[test]
    fn a_change_refreshes_every_themed_surface_once_in_both_directions() {
        use ThemeRefresh::*;
        for (a, b) in [(ThemeMode::Dark, ThemeMode::Light), (ThemeMode::Light, ThemeMode::Dark)] {
            let plan = refresh_plan(a, b);
            for needed in [
                TitleBar, PopupMenus, MenuBar, EditorColors, EditorScrollbars, Tooltips, SidePanels,
                Gutter, TabStrip, StatusBar, ToolbarButtons, AllWindows,
            ] {
                assert_eq!(plan.iter().filter(|s| **s == needed).count(), 1, "{needed:?} {a:?}->{b:?}");
            }
        }
    }

    #[test]
    fn popup_menu_mode_is_set_before_the_menu_bar_is_redrawn() {
        let plan = refresh_plan(ThemeMode::Dark, ThemeMode::Light);
        let pos = |s| plan.iter().position(|x| *x == s).expect("step in plan");
        assert!(pos(ThemeRefresh::PopupMenus) < pos(ThemeRefresh::MenuBar));
        assert!(pos(ThemeRefresh::TitleBar) < pos(ThemeRefresh::AllWindows));
        // The owner-painted windows repaint last, after the controls took their new colours.
        assert!(pos(ThemeRefresh::EditorColors) < pos(ThemeRefresh::AllWindows));
    }
}
