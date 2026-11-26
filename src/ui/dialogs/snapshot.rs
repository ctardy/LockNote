// Visual check of every dialog (developer tool, not a regression test).
//
// Each dialog is opened for real, its own window is captured with PrintWindow
// (never the screen) into a PNG, then closed with WM_CLOSE. It needs a desktop
// session and writes files, so the test is `#[ignore]`:
//
//   LOCKNOTE_DIALOG_SNAPSHOTS=<dir> [SNAP_LANG=en|fr|es|de] [SNAP_THEME=dark|light]
//   [SNAP_DPI=96|120|144|192] [SNAP_ONLY=<name prefix>]
//   cargo test dialog_snapshots -- --ignored --test-threads=1
//
// The dialogs run with an empty / default state; nothing is saved anywhere.

use std::ffi::c_void;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use native_windows_gui as nwg;
use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits, ReleaseDC, SelectObject,
    BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HGDIOBJ,
};
use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumThreadWindows, GetWindowRect, GetWindowTextLengthW, IsWindowVisible, PostMessageW, WM_CLOSE,
};

use crate::theme::{self, ThemeMode};

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (n, slot) in table.iter_mut().enumerate() {
        let mut c = n as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *slot = c;
    }
    let mut c = 0xFFFF_FFFFu32;
    for b in data {
        c = table[((c ^ *b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

fn write_png(path: &Path, w: u32, h: u32, bgra: &[u8]) {
    let mut raw = Vec::with_capacity((w as usize * 3 + 1) * h as usize);
    for y in 0..h as usize {
        raw.push(0);
        for x in 0..w as usize {
            let i = (y * w as usize + x) * 4;
            raw.extend_from_slice(&[bgra[i + 2], bgra[i + 1], bgra[i]]);
        }
    }
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    z.write_all(&raw).expect("deflate");
    let idat = z.finish().expect("deflate finish");
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut chunk = |kind: &[u8; 4], data: &[u8]| {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        out.extend_from_slice(&body);
        out.extend_from_slice(&crc32(&body).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &idat);
    chunk(b"IEND", &[]);
    std::fs::write(path, out).expect("write png");
}

fn dialog_window(tid: u32) -> Option<HWND> {
    unsafe extern "system" fn cb(h: HWND, lp: LPARAM) -> windows::core::BOOL {
        let v = &mut *(lp.0 as *mut Vec<HWND>);
        if IsWindowVisible(h).as_bool() && GetWindowTextLengthW(h) > 0 {
            v.push(h);
        }
        true.into()
    }
    let mut found: Vec<HWND> = Vec::new();
    unsafe {
        let _ = EnumThreadWindows(tid, Some(cb), LPARAM(&mut found as *mut Vec<HWND> as isize));
    }
    found.last().copied()
}

/// Pixels of the last captured window (BGRA, top-down) and where its client
/// area starts inside the capture, for the assertions that follow a `snap`.
struct Capture {
    w: i32,
    client_dx: i32,
    client_dy: i32,
    client_w: i32,
    client_h: i32,
    bgra: Vec<u8>,
}

impl Capture {
    /// RGB of the pixel at client coordinates (x, y).
    fn client_px(&self, x: i32, y: i32) -> (u8, u8, u8) {
        let i = (((y + self.client_dy) * self.w + x + self.client_dx) * 4) as usize;
        (self.bgra[i + 2], self.bgra[i + 1], self.bgra[i])
    }
}

static LAST_CAPTURE: std::sync::Mutex<Option<Capture>> = std::sync::Mutex::new(None);

fn capture_window(h: HWND, path: &Path) {
    unsafe {
        let mut r = RECT::default();
        GetWindowRect(h, &mut r).expect("GetWindowRect");
        let (w, ht) = ((r.right - r.left) as u32, (r.bottom - r.top) as u32);
        let mut c = RECT::default();
        let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(h, &mut c);
        eprintln!("snapshot {}: window {}x{} client {}x{}", path.display(), w, ht, c.right, c.bottom);
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(Some(screen));
        let bmp = CreateCompatibleBitmap(screen, w as i32, ht as i32);
        let old = SelectObject(mem, HGDIOBJ(bmp.0));
        // PW_RENDERFULLCONTENT = 2
        let _ = PrintWindow(h, mem, PRINT_WINDOW_FLAGS(2));
        let mut bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                biHeight: -(ht as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut px = vec![0u8; (w * ht * 4) as usize];
        GetDIBits(mem, bmp, 0, ht, Some(px.as_mut_ptr() as *mut c_void), &mut bi, DIB_RGB_COLORS);
        SelectObject(mem, old);
        let _ = DeleteObject(HGDIOBJ(bmp.0));
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        write_png(path, w, ht, &px);
        let mut origin = windows::Win32::Foundation::POINT { x: 0, y: 0 };
        let _ = windows::Win32::Graphics::Gdi::ClientToScreen(h, &mut origin);
        *LAST_CAPTURE.lock().expect("capture lock") = Some(Capture {
            w: w as i32,
            client_dx: origin.x - r.left,
            client_dy: origin.y - r.top,
            client_w: c.right,
            client_h: c.bottom,
            bgra: px,
        });
    }
}

/// Open `show`, capture its window as `<dir>/<name>.png`, close it.
fn snap<R>(dir: &Path, name: &str, show: impl FnOnce() -> R) -> R {
    let tid = unsafe { GetCurrentThreadId() };
    let path = dir.join(format!("{name}.png"));
    let watcher = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(1500));
        match dialog_window(tid) {
            Some(h) => {
                capture_window(h, &path);
                unsafe {
                    let _ = PostMessageW(Some(h), WM_CLOSE, windows::Win32::Foundation::WPARAM(0), LPARAM(0));
                }
            }
            None => eprintln!("snapshot {}: no dialog window found", path.display()),
        }
    });
    let r = show();
    watcher.join().expect("watcher");
    r
}

#[test]
#[ignore = "opens real windows and writes PNG files; see the header of this file"]
fn dialog_snapshots() {
    use crate::features::i18n::{set_language, Language};
    let dir = PathBuf::from(std::env::var("LOCKNOTE_DIALOG_SNAPSHOTS").expect("set LOCKNOTE_DIALOG_SNAPSHOTS=<dir>"));
    std::fs::create_dir_all(&dir).expect("create dir");
    let lang = std::env::var("SNAP_LANG").unwrap_or_else(|_| "en".into());
    set_language(match lang.as_str() {
        "fr" => Language::French,
        "es" => Language::Spanish,
        "de" => Language::German,
        _ => Language::English,
    });
    let mode = std::env::var("SNAP_THEME").unwrap_or_else(|_| "dark".into());
    theme::set_mode(if mode == "light" { ThemeMode::Light } else { ThemeMode::Dark });
    if let Ok(d) = std::env::var("SNAP_DPI") {
        super::style::set_test_dpi(Some(d.parse().expect("SNAP_DPI")));
    }
    let only = std::env::var("SNAP_ONLY").unwrap_or_default();
    nwg::init().expect("nwg init");
    let mut font = nwg::Font::default();
    nwg::Font::builder().family("Segoe UI").size(17).build(&mut font).expect("font");
    nwg::Font::set_global_default(Some(font));

    let tag = format!("{lang}-{mode}");
    let want = |n: &str| only.is_empty() || n.starts_with(&only);
    let settings = crate::settings::Settings::default();
    macro_rules! shot {
        ($name:expr, $body:expr) => {
            if want($name) {
                let _ = snap(&dir, &format!("{}-{}", $name, tag), || $body);
            }
        };
    }

    shot!("about", super::about::AboutDialog::show());
    shot!("backup_password", super::backup_password::BackupPasswordDialog::show(
        "Export full backup", "Password to protect the backup:", "Export", "Cancel", "The password cannot be empty."));
    shot!("backups", super::backups_dialog::BackupsDialog::show(&dir.join("no-backups")));
    shot!("close_confirm", super::close_confirm::CloseConfirmDialog::show());
    shot!("confirm_password", super::confirm_password::ConfirmPasswordDialog::show("pw", &crate::features::i18n::get("confirmpwd.export")));
    shot!("create_password", super::create_password::CreatePasswordDialog::show(8));
    shot!("drop_choice", super::drop_choice::DropChoiceDialog::show("report-2026.txt", true));
    shot!("export_encoding", super::export_encoding::ExportEncodingDialog::show());
    shot!("find", super::find_replace::FindReplaceDialog::show(0, false, "needle"));
    shot!("replace", super::find_replace::FindReplaceDialog::show(0, true, "needle"));
    shot!("goto_line", super::goto_line::GoToLineDialog::show(12, 340));
    shot!("preferences", super::preferences_dialog::PreferencesDialog::show(&settings));
    shot!("rename_tab", super::rename_tab::RenameTabDialog::show("Notes"));
    shot!("secret_edit", {
        let cats = vec!["General".to_string(), "Email".to_string()];
        let mut copied = None;
        let mut passgen = crate::features::passgen::Options::default();
        super::secret_edit::SecretEditDialog::show(None, &cats, &mut copied, &mut passgen)
    });
    shot!("password_generator", super::password_generator::PasswordGeneratorDialog::show(
        &crate::features::passgen::Options::default(), true, None));
    shot!("security", super::security_dialog::SecurityDialog::show(&settings));
    shot!("settings", super::settings_dialog::SettingsDialog::show(&settings));
    shot!("ssh_keygen", super::ssh_keygen::SshKeygenDialog::show(&[]));
    shot!("ssh_agent", super::ssh_keygen::SshAgentDialog::show(&["deploy key".to_string()]));
    shot!("totp_manager", super::totp_manager::TotpManagerDialog::show(&[]));
    shot!("totp_setup", super::totp_setup::TotpSetupDialog::show("Mail", None));
    shot!("totp_verify", super::totp_verify::TotpVerifyDialog::show("JBSWY3DPEHPK3PXP"));
    // The unlock popup (shared card of ui::lock_card, not the shared dialog pass).
    shot!("unlock", super::unlock::UnlockDialog::show(&[0u8; 4]));
    if want("unlock") {
        assert_card_bands_are_painted(theme::current_mode());
    }
}

/// The unlock card's label rows and the password field must show the card /
/// field colour on their whole height, not the default white of the
/// non-client bands nwg leaves above and below the centred text.
fn assert_card_bands_are_painted(mode: ThemeMode) {
    use crate::ui::lock_card::{CardColors, CardLayout, CardMode};
    let guard = LAST_CAPTURE.lock().expect("capture lock");
    let cap = guard.as_ref().expect("the unlock popup was not captured");
    // Recover the DPI from the client width (440 px at 96 DPI).
    let dpi = ((cap.client_w * 96 + 220) / 440) as u32;
    let layout = CardLayout::compute(CardMode::Popup, cap.client_w, cap.client_h, dpi);
    let colors = CardColors::for_mode(mode);
    let rgb = |c: crate::theme::Color| (c.r, c.g, c.b);
    // Left edge of a label: the centred text never reaches it.
    for (name, r) in [("title", layout.title), ("subtitle", layout.subtitle), ("status", layout.status)] {
        for y in r.y..r.bottom() {
            assert_eq!(cap.client_px(r.x + 2, y), rgb(colors.card_bg), "{name} row y={y}: band not painted");
        }
    }
    // The empty password field is one flat fill: no line around the edit, on
    // any of its four sides (the caret sits at the left, away from these).
    let input = layout.input;
    let mid_x = input.x + input.w / 2;
    for y in input.y - 1..=input.bottom() {
        assert_eq!(cap.client_px(mid_x, y), rgb(colors.field_bg), "input column y={y}: not the field fill");
    }
    let mid_y = input.y + input.h / 2;
    for x in (input.right() - 12..=input.right()).chain(input.x - 2..input.x) {
        assert_eq!(cap.client_px(x, mid_y), rgb(colors.field_bg), "input row x={x}: not the field fill");
    }
}
