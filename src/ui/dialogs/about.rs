// AboutDialog — Product info, crypto details, GitHub link
//
// Shows shield icon (text), product name, version, description,
// crypto info, and a clickable GitHub link. OK button to close.

use native_windows_gui as nwg;

fn t(key: &str) -> String {
    crate::features::i18n::get(key).to_string()
}

const VERSION: &str = env!("CARGO_PKG_VERSION");
const GITHUB_URL: &str = "https://github.com/ctardy/LockNote";

/// The crypto line of the About box, built from the constants `encrypt` is
/// called with: the hand-written line had drifted to PBKDF2 after the move to
/// Argon2id (issue #28). Argon2 notation (m, t, p) needs no translation.
pub(crate) fn crypto_summary() -> String {
    format!(
        "AES-256-CBC + HMAC-SHA256 | Argon2id m={} MiB, t={}, p={}",
        crate::crypto::DEFAULT_M_COST / 1024,
        crate::crypto::DEFAULT_T_COST,
        crate::crypto::DEFAULT_P_LANES,
    )
}

pub struct AboutDialog;

impl AboutDialog {
    /// Show the About dialog (modal, blocks until closed).
    pub fn show() {
        let mut window = Default::default();
        nwg::Window::builder()
            .size((400, 324))
            .position((300, 200))
            .title(&t("about.title"))
            .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
            .build(&mut window)
            .expect("Failed to build AboutDialog window");

        super::apply_dialog_theme(&window);

        let mut font = Default::default();
        nwg::Font::builder()
            .family("Segoe UI")
            .size(16)
            .build(&mut font)
            .expect("Failed to build font");

        let mut font_large = Default::default();
        nwg::Font::builder()
            .family("Segoe UI")
            .size(22)
            .weight(700)
            .build(&mut font_large)
            .expect("Failed to build large font");

        let mut font_mono = Default::default();
        nwg::Font::builder()
            .family("Consolas")
            .size(14)
            .build(&mut font_mono)
            .expect("Failed to build mono font");

        // Shield icon (text representation)
        let mut lbl_shield = Default::default();
        nwg::Label::builder()
            .text("\u{1F6E1}")  // shield emoji (may not render, fallback below)
            .parent(&window)
            .position((20, 15))
            .size((40, 40))
            .font(Some(&font_large))
            .build(&mut lbl_shield)
            .expect("Failed to build shield label");

        // Product name
        let mut lbl_name = Default::default();
        nwg::Label::builder()
            .text("LockNote")
            .parent(&window)
            .position((65, 15))
            .size((300, 30))
            .font(Some(&font_large))
            .build(&mut lbl_name)
            .expect("Failed to build name label");

        // Version
        let mut lbl_version = Default::default();
        nwg::Label::builder()
            .text(&t("about.version").replace("{version}", VERSION))
            .parent(&window)
            .position((65, 48))
            .size((300, 20))
            .font(Some(&font))
            .build(&mut lbl_version)
            .expect("Failed to build version label");

        // Description
        let mut lbl_desc = Default::default();
        nwg::Label::builder()
            .text(&t("about.desc"))
            .parent(&window)
            .position((20, 85))
            .size((360, 20))
            .font(Some(&font))
            .build(&mut lbl_desc)
            .expect("Failed to build description label");

        // Separator
        let mut sep = Default::default();
        nwg::Frame::builder()
            .parent(&window)
            .position((20, 115))
            .size((360, 1))
            .build(&mut sep)
            .expect("Failed to build separator");

        // Crypto info
        let mut lbl_crypto = Default::default();
        nwg::Label::builder()
            .text(&crypto_summary())
            .parent(&window)
            .position((20, 130))
            // Two lines: the Argon2id parameters do not fit on one at 360 px.
            .size((360, 34))
            .font(Some(&font_mono))
            .build(&mut lbl_crypto)
            .expect("Failed to build crypto label");

        let y_offset: i32 = 174;

        // Separator 2
        let mut sep2 = Default::default();
        nwg::Frame::builder()
            .parent(&window)
            .position((20, y_offset))
            .size((360, 1))
            .build(&mut sep2)
            .expect("Failed to build separator 2");

        // GitHub link label
        let mut lbl_github = Default::default();
        nwg::Label::builder()
            .text(GITHUB_URL)
            .parent(&window)
            .position((20, y_offset + 15))
            .size((360, 20))
            .font(Some(&font))
            .build(&mut lbl_github)
            .expect("Failed to build GitHub label");

        // Open link button
        let mut btn_link = Default::default();
        nwg::Button::builder()
            .text(&t("about.open"))
            .parent(&window)
            .position((20, y_offset + 40))
            .size((140, 28))
            .font(Some(&font))
            .build(&mut btn_link)
            .expect("Failed to build link button");

        // OK button
        let mut btn_ok = Default::default();
        nwg::Button::builder()
            .text(&t("settings.ok"))
            .parent(&window)
            .position((290, y_offset + 90))
            .size((85, 30))
            .font(Some(&font))
            .build(&mut btn_ok)
            .expect("Failed to build OK button");

        super::bind_enter_to_button(window.handle, btn_ok.handle);

        let window_handle = window.handle;

        let handler = nwg::full_bind_event_handler(&window_handle, move |evt, _evt_data, handle| {
            match evt {
                nwg::Event::OnButtonClick => {
                    if handle == btn_ok.handle {
                        nwg::stop_thread_dispatch();
                    } else if handle == btn_link.handle {
                        // Open URL via ShellExecute (Win32)
                        let _ = open_url(GITHUB_URL);
                    }
                }
                nwg::Event::OnWindowClose => {
                    nwg::stop_thread_dispatch();
                }
                _ => {}
            }
        });

        nwg::dispatch_thread_events();
        nwg::unbind_event_handler(&handler);
    }
}

/// Open a URL in the default browser via `ShellExecuteW`. Bypasses cmd.exe
/// so characters like `&`, `|`, `"`, `^` in the URL are never re-parsed by
/// a shell (the previous `cmd /c start` path was vulnerable to command
/// injection via crafted URLs pasted into a note).
fn open_url(url: &str) {
    // Refuse anything that is not a plain http(s) URL. cmd.exe accepts
    // `file:`, `javascript:`, etc., which could trigger surprising
    // behaviour even through ShellExecute. Belt-and-suspenders.
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return;
    }
    unsafe {
        let wide_op: Vec<u16> = "open\0".encode_utf16().collect();
        let wide_url: Vec<u16> = url.encode_utf16().chain(std::iter::once(0)).collect();
        let _ = windows::Win32::UI::Shell::ShellExecuteW(
            None,
            windows::core::PCWSTR(wide_op.as_ptr()),
            windows::core::PCWSTR(wide_url.as_ptr()),
            windows::core::PCWSTR::null(),
            windows::core::PCWSTR::null(),
            windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_set() {
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn github_url_is_https() {
        assert!(GITHUB_URL.starts_with("https://"));
    }

    /// Issue #28: the About box said PBKDF2 while the key is derived with
    /// Argon2id; the line must state the constants `encrypt` is called with.
    #[test]
    fn crypto_summary_states_the_argon2id_constants() {
        let s = crypto_summary();
        // The MiB figure is exact only for a whole number of MiB.
        assert_eq!(crate::crypto::DEFAULT_M_COST % 1024, 0);
        assert!(s.contains("Argon2id"), "{}", s);
        assert!(!s.contains("PBKDF2"), "{}", s);
        let m = format!("m={} MiB", crate::crypto::DEFAULT_M_COST / 1024);
        let t = format!("t={}", crate::crypto::DEFAULT_T_COST);
        let p = format!("p={}", crate::crypto::DEFAULT_P_LANES);
        for part in [&m, &t, &p] {
            assert!(s.contains(part.as_str()), "{} not in {}", part, s);
        }
    }
}
