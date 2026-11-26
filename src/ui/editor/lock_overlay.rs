// Lock overlay: window procedure and theming (free items). The look itself
// (card, colours, painting, eye button, status line) is `ui::lock_card`.
// Split out of the former editor.rs.

#[allow(unused_imports)]
use super::*;

// =========================================================================
// Helper functions
// =========================================================================

/// Exponential backoff schedule for failed inline-unlock attempts.
/// Mirrors the schedule used by the legacy UnlockDialog popup so the UX
/// feels identical: 1->0s, 2->1s, 3->2s, 4->4s, 5+->8s.
pub(super) fn unlock_backoff_seconds(att: u32) -> u32 {
    match att {
        0 | 1 => 0,
        2 => 1,
        3 => 2,
        4 => 4,
        _ => 8,
    }
}

/// Wrong passwords on the lock screen before LockNote quits.
pub(super) const LOCK_SCREEN_MAX_ATTEMPTS: u32 = 5;

/// The unlock field when the lock screen is shown.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct LockScreenField {
    /// The typed text is cleared.
    pub(super) clear_text: bool,
    /// The status message is cleared.
    pub(super) clear_status: bool,
    pub(super) attempts: u32,
    /// Seconds of a running backoff (0: none, field enabled).
    pub(super) backoff_remaining: u32,
}

/// What the lock screen keeps of the unlock field (`backoff_remaining`:
/// the current one) when it is shown;
/// `curtain_was_up`: the curtain becomes the real lock screen.
pub(super) fn lock_screen_field(curtain_was_up: bool, backoff_remaining: u32) -> LockScreenField {
    if curtain_was_up {
        // Pass 9: the same screen goes on, only the note behind it is gone:
        // the typed text and a running backoff (with its message) stay. The
        // curtain has no cap, so its wrong passwords do not count towards
        // the lock screen's (a first typo would quit LockNote); any other
        // message (Quit refused, wrong password) is cleared.
        LockScreenField { clear_text: false, clear_status: backoff_remaining == 0, attempts: 0, backoff_remaining }
    } else {
        LockScreenField { clear_text: true, clear_status: true, attempts: 0, backoff_remaining: 0 }
    }
}

#[cfg(test)]
mod lock_screen_field_tests {
    use super::*;

    /// Pass 9: a retry that succeeds under the curtain turns it into the
    /// lock screen without clearing a half-typed password or a running
    /// backoff; a lock from the open note starts fresh.
    #[test]
    fn the_curtain_becoming_the_lock_screen_keeps_the_typed_password_and_the_backoff() {
        let f = lock_screen_field(true, 2);
        assert!(!f.clear_text && f.backoff_remaining == 2, "{:?}", f);
        assert_eq!(lock_screen_field(false, 2), LockScreenField { clear_text: true, clear_status: true, attempts: 0, backoff_remaining: 0 });
    }

    /// Final fixes, 1: wrong passwords on the curtain (no cap there) do not
    /// count towards the lock screen's cap: its first typo never quits.
    #[test]
    fn curtain_attempts_never_make_the_first_lock_screen_typo_quit() {
        for backoff in [0, 8] {
            let f = lock_screen_field(true, backoff);
            assert!(f.attempts + 1 < LOCK_SCREEN_MAX_ATTEMPTS, "carried: {:?}", f);
            assert_eq!(f.attempts, 0, "the lock screen gives its normal attempts");
            assert_eq!(f.backoff_remaining, backoff, "a running backoff is kept");
        }
    }

    /// Final fixes, 2: only a running backoff keeps its message (the
    /// field stays disabled with it); any other curtain message (Quit
    /// refused, wrong password) is cleared.
    #[test]
    fn only_the_backoff_message_survives_the_curtain() {
        assert!(!lock_screen_field(true, 2).clear_status, "backoff running");
        assert!(lock_screen_field(true, 0).clear_status, "quit refused or wrong password, no backoff");
        assert!(lock_screen_field(false, 2).clear_status);
    }
}

use crate::ui::lock_card::{self, CardHwnds, CardMode};

thread_local! {
    /// Child windows the frame procedure needs to know (it is a plain
    /// function and cannot capture). Set once by `install_lock_overlay_theme`;
    /// the UI is single-threaded, so a thread-local is exact.
    static OVERLAY_HWNDS: std::cell::Cell<CardHwnds> = const { std::cell::Cell::new(CardHwnds {
        input: 0, eye: 0, primary: 0, secondary: 0,
    }) };
}

/// The lock screen's child windows, for the keyboard-focus and reveal helpers.
pub(super) fn lock_overlay_hwnds() -> CardHwnds {
    OVERLAY_HWNDS.with(|c| c.get())
}

/// Custom WndProc for the lock-overlay frame.
///
/// NWG_FRAME's class WndProc (`blank_window_proc`) delegates WM_ERASEBKGND
/// to `DefWindowProcW`, which uses the class brush `COLOR_WINDOW` (system
/// white in Light theme) — that's why the overlay rendered as "white on
/// white" even with subclass paint handlers installed. Rather than race
/// the subclass chain, we replace this instance's WndProc outright via
/// `SetWindowLongPtrW(GWLP_WNDPROC)` (see `install_lock_overlay_theme`).
/// Everything visual is delegated to `lock_card::handle_message`; messages it
/// does not handle fall through to `DefWindowProcW`.
pub(super) unsafe extern "system" fn lock_overlay_wndproc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::Foundation::LRESULT;
    use windows::Win32::UI::WindowsAndMessaging::DefWindowProcW;

    let hwnds = lock_overlay_hwnds();
    match lock_card::handle_message(hwnd.0 as isize, msg, wparam.0, lparam.0, CardMode::Overlay, &hwnds) {
        Some(result) => LRESULT(result),
        None => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Install the permanent paint path on the lock overlay frame.
///
/// **Why replace the WndProc instead of subclassing?** Previous attempts
/// hooked WM_ERASEBKGND via `bind_raw_event_handler` (SetWindowSubclass),
/// but on NWG_FRAME the default class WndProc (`blank_window_proc`) still
/// reaches `DefWindowProcW`, which paints with the class brush COLOR_WINDOW
/// (system white) — so the overlay came up all-white in Light theme no
/// matter what our handlers did. Swapping the instance's WndProc via
/// `SetWindowLongPtrW(GWLP_WNDPROC)` makes `lock_overlay_wndproc` the
/// terminal proc for this window, so there is no path back to the class
/// brush. NWG_FRAME is only used in four places (vault / totp / attach /
/// lock); this change is scoped to the lock instance only.
pub(super) fn install_lock_overlay_theme(form: &EditorForm) {
    use windows::Win32::Foundation::{HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumChildWindows, GetWindowLongPtrW, SetWindowLongPtrW, GWLP_WNDPROC, GWL_STYLE,
    };

    const WS_CLIPSIBLINGS: u32 = 0x04000000;

    // Add WS_CLIPSIBLINGS to every child of the main window. Without this
    // style, sibling windows can paint over each other regardless of Z-order
    // — the RichEdit editor was stomping on our overlay's backdrop every
    // time it repainted (cursor blink, focus, etc.), even though lock_frame
    // was correctly placed at the top of the Z-order.
    unsafe extern "system" fn add_clipsiblings_cb(
        child: HWND,
        _l: LPARAM,
    ) -> windows::core::BOOL {
        unsafe {
            let style = GetWindowLongPtrW(child, GWL_STYLE) as u32;
            if style & WS_CLIPSIBLINGS == 0 {
                SetWindowLongPtrW(
                    child,
                    GWL_STYLE,
                    (style | WS_CLIPSIBLINGS) as isize,
                );
            }
        }
        windows::core::BOOL(1)
    }
    if let nwg::ControlHandle::Hwnd(h) = form.window.handle {
        unsafe {
            let _ = EnumChildWindows(
                Some(HWND(h as *mut _)),
                Some(add_clipsiblings_cb),
                LPARAM(0),
            );
        }
    }

    // The frame never paints over its controls (the card paints around them).
    lock_card::add_clip_children(&form.lock_frame.handle);

    // Publish the child HWNDs so the WndProc can reach them.
    OVERLAY_HWNDS.with(|c| {
        c.set(CardHwnds {
            input: lock_card::hwnd_of(&form.lock_input.handle),
            eye: lock_card::hwnd_of(&form.lock_eye.handle),
            primary: lock_card::hwnd_of(&form.lock_btn.handle),
            secondary: 0,
        })
    });

    // Owner-drawn buttons (painted by lock_card), borderless edit, and no
    // visual theme on any overlay child so WM_CTLCOLOR* is honoured: in the
    // dark theme the default STATIC visual theme repaints the label
    // background in a light colour after our brush, producing horizontal
    // white bars between controls.
    lock_card::make_owner_draw(&form.lock_btn.handle);
    lock_card::make_owner_draw(&form.lock_eye.handle);
    lock_card::disable_theme(&form.lock_input.handle);
    lock_card::disable_theme(&form.lock_label.handle);
    lock_card::disable_theme(&form.lock_subtitle.handle);
    lock_card::disable_theme(&form.lock_status.handle);
    lock_card::set_role(&form.lock_label.handle, lock_card::Role::Title);
    lock_card::set_role(&form.lock_subtitle.handle, lock_card::Role::Subtitle);
    lock_card::set_role(&form.lock_status.handle, lock_card::Role::Status);

    // Status line: Caps Lock hint and focus tracking on the password field.
    form.lock_status_msg.bind_label(&form.lock_status.handle);
    match lock_card::hook_password_input(&form.lock_input.handle, 0x20003, form.lock_status_msg.clone()) {
        // Lives as long as the field, i.e. the process.
        Ok(hook) => std::mem::forget(hook),
        Err(e) => panic!("lock overlay: cannot hook the password field: {e}"),
    }

    // Replace the frame's WndProc with our own. This is the terminal proc
    // for this window instance; subclasses still chain through first.
    if let nwg::ControlHandle::Hwnd(h) = form.lock_frame.handle {
        unsafe {
            SetWindowLongPtrW(
                HWND(h as *mut _),
                GWLP_WNDPROC,
                lock_overlay_wndproc as usize as isize,
            );
        }
    }
}
