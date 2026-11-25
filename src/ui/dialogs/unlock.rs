// UnlockDialog — Password prompt with attempt limiting
//
// Max 5 attempts. On each failure displays "Wrong password (N/5)".
// Returns Option<(Zeroizing<String>, String)>: (password, decrypted_text), or None if cancelled/max reached.
// The password is wrapped in Zeroizing so its buffer is wiped when the caller drops it.
// Clears TextBox on drop for security.
//
// Look: the shared card of `ui::lock_card` (padlock, title, prompt, password
// field with a show/hide button, status line with Caps Lock hint, primary
// Unlock button and a quieter Cancel), sized from the window's DPI.
// Keyboard: Enter unlocks, Esc cancels, Tab goes field -> eye -> Unlock -> Cancel.

use crate::ui::lock_card::{self, CardMode, Tone};
use native_windows_gui as nwg;
use std::cell::RefCell;
use std::rc::Rc;
use zeroize::Zeroizing;

fn t(key: &str) -> String {
    crate::features::i18n::get(key).to_string()
}

const MAX_ATTEMPTS: u32 = 5;

/// Exponential backoff schedule for failed password attempts.
/// Index = attempts counter after the failure (1-based): 1->0s, 2->1s, 3->2s, 4->4s, 5->8s.
fn backoff_seconds(att: u32) -> u32 {
    match att {
        0 | 1 => 0,
        2 => 1,
        3 => 2,
        4 => 4,
        _ => 8,
    }
}

/// Put the keyboard focus on the password field.
fn focus_field(h: nwg::ControlHandle) {
    if let nwg::ControlHandle::Hwnd(h) = h {
        unsafe {
            let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(Some(windows::Win32::Foundation::HWND(
                h as *mut _,
            )));
        }
    }
}

pub struct UnlockDialog;

impl UnlockDialog {
    /// Show the unlock dialog.
    ///
    /// `encrypted_data` is the raw encrypted payload.
    /// Uses `crate::crypto::decrypt` to attempt decryption.
    /// Returns Some((password, decrypted_text)) on success, None on cancel/max attempts.
    pub fn show(encrypted_data: &[u8]) -> Option<(Zeroizing<String>, String)> {
        Self::show_with_parent(encrypted_data, None)
    }

    /// Show the unlock dialog, optionally centered on a parent window.
    pub fn show_with_parent(encrypted_data: &[u8], parent_hwnd: Option<isize>) -> Option<(Zeroizing<String>, String)> {
        // Fix #036: hold the global persistence lock for the entire unlock
        // session. An in-flight autosave (non-blocking, try_acquire) will
        // simply skip its tick, and a concurrent autolock (blocking acquire)
        // will wait until this dialog closes. Argon2id dominates the per-
        // attempt cost (~1s), so blocking briefly is imperceptible compared
        // to the decrypt workload itself.
        let _persistence_guard = crate::features::persistence_lock::acquire();

        let data = encrypted_data.to_vec();

        // --- Window: created hidden, sized for its DPI, then shown ---
        // This position only decides which monitor (hence which DPI) the
        // window is created on: the parent's, else the primary one. The real
        // placement is `lock_card::place_popup` below.
        let parent_center = parent_hwnd.and_then(|raw| {
            let hwnd = windows::Win32::Foundation::HWND(raw as *mut _);
            let mut rect: windows::Win32::Foundation::RECT = unsafe { std::mem::zeroed() };
            if unsafe { windows::Win32::UI::WindowsAndMessaging::GetWindowRect(hwnd, &mut rect) }.is_ok() {
                Some(((rect.left + rect.right) / 2, (rect.top + rect.bottom) / 2))
            } else {
                eprintln!("WARN unlock: GetWindowRect failed on the parent hwnd=0x{raw:x}; centering on the screen");
                None
            }
        });
        let (probe_x, probe_y) = parent_center.unwrap_or((0, 0));

        let mut window = Default::default();
        nwg::Window::builder()
            .size((440, 340))
            .position((probe_x, probe_y))
            .title("LockNote")
            .flags(nwg::WindowFlags::WINDOW)
            .topmost(true)
            .build(&mut window)
            .expect("Failed to build UnlockDialog window");

        // Taskbar entry: see `taskbar_ex_style` (set while the window is still
        // hidden, so it is what the taskbar sees when the window appears).
        let window_hwnd = lock_card::hwnd_of(&window.handle);
        unsafe {
            use windows::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE};
            let hwnd_w = windows::Win32::Foundation::HWND(window_hwnd as *mut _);
            let ex_style = GetWindowLongPtrW(hwnd_w, GWL_EXSTYLE);
            SetWindowLongPtrW(hwnd_w, GWL_EXSTYLE, taskbar_ex_style(ex_style, parent_hwnd.is_some()));
        }
        lock_card::add_clip_children(&window.handle);

        // Dark/light title bar. Installed first so the card handler below,
        // bound later, sees WM_CTLCOLOR* before it.
        super::apply_window_chrome(&window);

        let dpi = lock_card::window_dpi(window_hwnd);
        let fonts = lock_card::CardFonts::build(dpi).expect("Failed to build the unlock fonts");

        // --- Controls (creation order = Tab order) ---
        let mut lbl_title = Default::default();
        nwg::Label::builder()
            .text(&t("unlock.title"))
            .parent(&window)
            .h_align(nwg::HTextAlign::Center)
            .build(&mut lbl_title)
            .expect("Failed to build title label");

        let mut lbl_desc = Default::default();
        nwg::Label::builder()
            .text(&t("unlock.desc"))
            .parent(&window)
            .h_align(nwg::HTextAlign::Center)
            .build(&mut lbl_desc)
            .expect("Failed to build description label");

        // Borderless from birth; the focus is given once the card is ready.
        let txt_password = lock_card::build_password_input(&window.handle);

        let mut btn_eye = Default::default();
        nwg::Button::builder()
            .text(&t("unlock.show"))
            .parent(&window)
            .build(&mut btn_eye)
            .expect("Failed to build the show/hide button");
        lock_card::make_owner_draw(&btn_eye.handle);

        let mut lbl_status = Default::default();
        nwg::Label::builder()
            .text("")
            .parent(&window)
            .h_align(nwg::HTextAlign::Center)
            .build(&mut lbl_status)
            .expect("Failed to build status label");

        let mut btn_unlock = Default::default();
        nwg::Button::builder()
            .text(&t("unlock.btn"))
            .parent(&window)
            .build(&mut btn_unlock)
            .expect("Failed to build Unlock button");
        lock_card::make_owner_draw(&btn_unlock.handle);

        let mut btn_cancel = Default::default();
        nwg::Button::builder()
            .text(&t("unlock.cancel"))
            .parent(&window)
            .build(&mut btn_cancel)
            .expect("Failed to build Cancel button");
        lock_card::make_owner_draw(&btn_cancel.handle);

        lock_card::set_role(&lbl_title.handle, lock_card::Role::Title);
        lock_card::set_role(&lbl_desc.handle, lock_card::Role::Subtitle);
        lock_card::set_role(&lbl_status.handle, lock_card::Role::Status);

        // --- Geometry and fonts for this DPI ---
        let (client_w, client_h) = lock_card::CardLayout::popup_client_size(dpi);
        let layout = lock_card::CardLayout::compute(CardMode::Popup, client_w, client_h, dpi);
        {
            let card_controls = lock_card::CardControls {
                title: &lbl_title,
                subtitle: &lbl_desc,
                input: &txt_password,
                eye: &btn_eye,
                status: &lbl_status,
                primary: &btn_unlock,
                secondary: Some(&btn_cancel),
            };
            card_controls.apply_fonts(&fonts);
            card_controls.apply_layout(&layout);
        }
        lock_card::place_popup(window_hwnd, parent_hwnd, client_w, client_h);

        // --- Shared card behaviour: painting, colours, eye button, status ---
        let hw = lock_card::CardHwnds {
            input: lock_card::hwnd_of(&txt_password.handle),
            eye: lock_card::hwnd_of(&btn_eye.handle),
            primary: lock_card::hwnd_of(&btn_unlock.handle),
            secondary: lock_card::hwnd_of(&btn_cancel.handle),
        };
        lock_card::set_eye_names(&t("unlock.show"), &t("unlock.hide"));
        let status = lock_card::StatusMsg::new();
        status.bind_label(&lbl_status.handle);
        status.set_caps_text(&t("unlock.capslock"));
        let card_handler = nwg::bind_raw_event_handler(&window.handle, 0x20010, move |hwnd, msg, wparam, lparam| {
            lock_card::handle_message(hwnd as isize, msg, wparam, lparam, CardMode::Popup, &hw)
        })
        .expect("Failed to bind the unlock card handler");
        let input_hook = lock_card::hook_password_input(&txt_password.handle, 0x20011, status.clone())
            .expect("Failed to hook the password input");

        // --- Enter key → Unlock button ---
        super::bind_enter_to_button(window.handle, btn_unlock.handle);

        // --- Backoff countdown timer (ticks every 1s while rate-limited) ---
        let mut backoff_timer = nwg::AnimationTimer::default();
        nwg::AnimationTimer::builder()
            .parent(&window)
            .interval(std::time::Duration::from_secs(1))
            .active(false)
            .build(&mut backoff_timer)
            .expect("Failed to build backoff timer");

        // Remaining seconds in the current backoff window. 0 = no backoff active.
        let backoff_remaining: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));

        // Result: Some((password, decrypted_text))
        let result: Rc<RefCell<Option<(Zeroizing<String>, String)>>> = Rc::new(RefCell::new(None));
        let attempts: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));

        let window_handle = window.handle;
        let txt_password_handle = txt_password.handle;
        let result_clone = result.clone();
        let attempts_clone = attempts.clone();
        let backoff_remaining_clone = backoff_remaining.clone();
        let handler_status = status.clone();
        let handler = nwg::full_bind_event_handler(&window_handle, move |evt, _evt_data, handle| {
            let status = &handler_status;
            // Cancel / close / Esc: wipe the field and leave the dialog.
            let cancel = |backoff_timer: &nwg::AnimationTimer, txt_password: &nwg::TextInput| {
                backoff_timer.stop();
                txt_password.set_text("");
                nwg::stop_thread_dispatch();
            };
            match evt {
                nwg::Event::OnTimerTick => {
                    if handle == backoff_timer.handle {
                        let mut remaining = backoff_remaining_clone.borrow_mut();
                        if *remaining > 0 {
                            *remaining -= 1;
                        }
                        if *remaining == 0 {
                            backoff_timer.stop();
                            btn_unlock.set_enabled(true);
                            txt_password.set_enabled(true);
                            status.clear();
                            // Restore focus to the password field
                            focus_field(txt_password.handle);
                        } else {
                            status.set(
                                Tone::Error,
                                &t("unlock.retryin").replace("{seconds}", &remaining.to_string()),
                            );
                        }
                    }
                }
                nwg::Event::OnButtonClick => {
                    if handle == btn_unlock.handle {
                        // Wrap the password in Zeroizing so it is wiped from the stack
                        // whether decryption succeeds or fails. On success we move the
                        // wrapper out to the caller — no plain String clone — so the
                        // caller keeps the zeroize-on-drop guarantee end-to-end.
                        let pw: Zeroizing<String> = Zeroizing::new(txt_password.text());
                        status.set(Tone::Neutral, &t("unlock.decrypting"));

                        match crate::crypto::decrypt(&data, pw.as_str()) {
                            Ok(plaintext) => {
                                *result_clone.borrow_mut() = Some((pw, plaintext));
                                txt_password.set_text("");
                                nwg::stop_thread_dispatch();
                            }
                            Err(err) => {
                                // Classify: HmacMismatch or Decryption -> generic
                                // "wrong password" (anti-oracle). Anything else is a
                                // real crypto error and we surface it so the user
                                // can report the issue.
                                let is_wrong_password = matches!(
                                    &err,
                                    crate::crypto::CryptoError::HmacMismatch
                                        | crate::crypto::CryptoError::Decryption(_)
                                );
                                let mut att = attempts_clone.borrow_mut();
                                *att += 1;
                                txt_password.set_text("");

                                if !is_wrong_password {
                                    // Structural crypto error (bad params, KDF failure, ...)
                                    status.set(
                                        Tone::Error,
                                        &t("unlock.cryptoerror").replace("{msg}", &err.to_string()),
                                    );
                                    // Do not count this as a password attempt toward max-lock:
                                    // it's not a guess, it's a malformed payload.
                                    *att -= 1;
                                    focus_field(txt_password.handle);
                                } else if *att >= MAX_ATTEMPTS {
                                    status.set(
                                        Tone::Error,
                                        &t("unlock.maxattempts"),
                                    );
                                    nwg::modal_info_message(
                                        &window_handle,
                                        &t("unlock.maxattempts"),
                                        &t("unlock.maxattempts"),
                                    );
                                    nwg::stop_thread_dispatch();
                                } else {
                                    let delay = backoff_seconds(*att);
                                    if delay == 0 {
                                        status.set(
                                            Tone::Error,
                                            &format!("{} ({}/{})", t("unlock.wrong"), att, MAX_ATTEMPTS),
                                        );
                                        // Keep focus on password input
                                        focus_field(txt_password.handle);
                                    } else {
                                        // Start exponential backoff: disable controls and
                                        // run the 1s tick timer until the delay elapses.
                                        btn_unlock.set_enabled(false);
                                        txt_password.set_enabled(false);
                                        *backoff_remaining_clone.borrow_mut() = delay;
                                        let retry_msg = t("unlock.retryin")
                                            .replace("{seconds}", &delay.to_string());
                                        status.set(
                                            Tone::Error,
                                            &format!(
                                                "{} ({}/{}) — {}",
                                                t("unlock.wrong"),
                                                att,
                                                MAX_ATTEMPTS,
                                                retry_msg,
                                            ),
                                        );
                                        backoff_timer.start();
                                    }
                                }
                            }
                        }
                    } else if handle == btn_cancel.handle {
                        cancel(&backoff_timer, &txt_password);
                    }
                }
                // Esc (IsDialogMessage -> IDCANCEL) behaves like Cancel.
                nwg::Event::OnKeyEsc => cancel(&backoff_timer, &txt_password),
                nwg::Event::OnWindowClose => cancel(&backoff_timer, &txt_password),
                _ => {}
            }
        });

        // Show only now that the window is sized, placed and painted by the card.
        window.set_visible(true);
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(windows::Win32::Foundation::HWND(
                window_hwnd as *mut _,
            ));
        }

        // Force focus on password input before entering the message loop
        focus_field(txt_password_handle);

        // Restore focus to the password field whenever the dialog is
        // re-activated (user returns from another app). Without this the
        // user has to click the field before typing.
        let activate_handler = lock_card::keep_focus_on(&window_handle, &txt_password_handle)
            .expect("Failed to bind the focus handler");

        nwg::dispatch_thread_events();
        nwg::unbind_event_handler(&handler);
        nwg::unbind_raw_event_handler(&activate_handler).ok();
        nwg::unbind_raw_event_handler(&card_handler).ok();
        nwg::unbind_raw_event_handler(&input_hook).ok();

        let out = result.borrow_mut().take();
        out
    }
}

/// WS_EX_TOOLWINDOW: a window with it has no taskbar entry.
const WS_EX_TOOLWINDOW: isize = 0x0000_0080;

/// Extended style of the unlock window for the taskbar.
///
/// Over the main window (`has_parent`: unlock after a manual lock) it is a
/// tool window, so it does not add a second taskbar entry next to the main
/// one. At launch it is the only window of the application: it keeps its
/// taskbar entry, otherwise LockNote is nowhere in the taskbar until the
/// password is typed.
fn taskbar_ex_style(ex_style: isize, has_parent: bool) -> isize {
    if has_parent {
        ex_style | WS_EX_TOOLWINDOW
    } else {
        ex_style & !WS_EX_TOOLWINDOW
    }
}

// ---------------------------------------------------------------------------
// Free function (convenience API expected by callers)
// ---------------------------------------------------------------------------

/// Show the unlock dialog. Convenience wrapper.
pub fn show(encrypted_data: &[u8]) -> Option<(Zeroizing<String>, String)> {
    UnlockDialog::show(encrypted_data)
}

/// Show the unlock dialog centered on a parent window.
pub fn show_with_parent(encrypted_data: &[u8], parent_hwnd: isize) -> Option<(Zeroizing<String>, String)> {
    UnlockDialog::show_with_parent(encrypted_data, Some(parent_hwnd))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_attempts_constant() {
        assert_eq!(MAX_ATTEMPTS, 5);
    }

    #[test]
    fn unlock_window_is_in_the_taskbar_at_launch_only() {
        const WS_EX_WINDOWEDGE: isize = 0x0000_0100;
        // At launch (no main window yet): the only window, it keeps its entry.
        assert_eq!(taskbar_ex_style(WS_EX_WINDOWEDGE, false), WS_EX_WINDOWEDGE);
        assert_eq!(taskbar_ex_style(WS_EX_WINDOWEDGE | WS_EX_TOOLWINDOW, false), WS_EX_WINDOWEDGE);
        // Over the main window: no second entry, the other bits untouched.
        assert_eq!(taskbar_ex_style(WS_EX_WINDOWEDGE, true), WS_EX_WINDOWEDGE | WS_EX_TOOLWINDOW);
    }

    #[test]
    fn backoff_schedule_is_frozen() {
        let delays: Vec<u32> = (0..=6).map(backoff_seconds).collect();
        assert_eq!(delays, [0, 0, 1, 2, 4, 8, 8]);
    }
}
