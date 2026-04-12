// AboutDialog — Product info, crypto details, GitHub link
//
// Shows shield icon (text), product name, version, description,
// crypto info, and a clickable GitHub link. OK button to close.

use native_windows_gui as nwg;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const GITHUB_URL: &str = "https://github.com/ctardy/LockNote";

pub struct AboutDialog;

impl AboutDialog {
    /// Show the About dialog (modal, blocks until closed).
    pub fn show() {
        let mut window = Default::default();
        nwg::Window::builder()
            .size((400, 310))
            .position((300, 200))
            .title("About LockNote")
            .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
            .build(&mut window)
            .expect("Failed to build AboutDialog window");

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
            .text(&format!("Version {}", VERSION))
            .parent(&window)
            .position((65, 48))
            .size((300, 20))
            .font(Some(&font))
            .build(&mut lbl_version)
            .expect("Failed to build version label");

        // Description
        let mut lbl_desc = Default::default();
        nwg::Label::builder()
            .text("Self-contained encrypted notepad for Windows")
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
            .text("AES-256-CBC + HMAC-SHA256 | PBKDF2 100k iterations")
            .parent(&window)
            .position((20, 130))
            .size((360, 20))
            .font(Some(&font_mono))
            .build(&mut lbl_crypto)
            .expect("Failed to build crypto label");

        // Separator 2
        let mut sep2 = Default::default();
        nwg::Frame::builder()
            .parent(&window)
            .position((20, 160))
            .size((360, 1))
            .build(&mut sep2)
            .expect("Failed to build separator 2");

        // GitHub link label
        let mut lbl_github = Default::default();
        nwg::Label::builder()
            .text(GITHUB_URL)
            .parent(&window)
            .position((20, 175))
            .size((360, 20))
            .font(Some(&font))
            .build(&mut lbl_github)
            .expect("Failed to build GitHub label");

        // Open link button
        let mut btn_link = Default::default();
        nwg::Button::builder()
            .text("Open in browser")
            .parent(&window)
            .position((20, 200))
            .size((140, 28))
            .font(Some(&font))
            .build(&mut btn_link)
            .expect("Failed to build link button");

        // OK button
        let mut btn_ok = Default::default();
        nwg::Button::builder()
            .text("OK")
            .parent(&window)
            .position((290, 250))
            .size((85, 30))
            .font(Some(&font))
            .build(&mut btn_ok)
            .expect("Failed to build OK button");

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

/// Open a URL in the default browser using `cmd /C start`.
fn open_url(url: &str) {
    let _ = std::process::Command::new("cmd")
        .args(&["/C", "start", "", url])
        .spawn();
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
}
