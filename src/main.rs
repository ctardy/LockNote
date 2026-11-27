// LockNote — Self-contained encrypted notepad for Windows
// Single .exe that serves as both text editor and encrypted vault
#![windows_subsystem = "windows"]

mod crypto;
mod storage;
mod settings;
mod theme;
mod updater;
mod ui;

mod features;

#[cfg(test)]
mod integration_tests;

/// Text of a source file of this crate, line endings turned into `\n`, for
/// the tests that check the code as text. A Windows checkout with
/// `core.autocrlf=true` (the CI runner) has CRLF where the repository has LF,
/// and the scans look for `\n`-based patterns.
#[cfg(test)]
pub(crate) fn read_source(path: impl AsRef<std::path::Path>) -> String {
    let path = path.as_ref();
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
        .replace("\r\n", "\n")
}

fn main() {
    // ── Single-instance guard ──
    // Create a named mutex; if it already exists, another instance is running.
    let mutex = unsafe {
        windows::Win32::System::Threading::CreateMutexW(
            None,
            false,
            windows::core::w!("LockNote_SingleInstance_Mutex"),
        )
    };
    if unsafe { windows::Win32::Foundation::GetLastError() }
        == windows::Win32::Foundation::ERROR_ALREADY_EXISTS
    {
        // Another instance is running — find its window and bring it to front
        unsafe {
            if let Ok(hwnd) = windows::Win32::UI::WindowsAndMessaging::FindWindowW(
                None,
                windows::core::w!("LockNote"),
            ) {
                if windows::Win32::UI::WindowsAndMessaging::IsIconic(hwnd).as_bool() {
                    let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(
                        hwnd,
                        windows::Win32::UI::WindowsAndMessaging::SW_RESTORE,
                    );
                }
                let _ = windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(hwnd);
            }
        }
        return;
    }
    // Keep the mutex handle alive for the entire process lifetime
    let _mutex_guard = mutex;

    // Panic hook: log to file + show MessageBox.
    // Redacted: we never emit `info.payload()` (user-supplied panic message)
    // because a future `panic!("bad pw: {:?}", pw)` or `expect()` on a secret
    // would leak plaintext into a persistent file. Only location and thread
    // are written — enough for triage, inert for confidentiality.
    std::panic::set_hook(Box::new(|info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "<unknown>".to_string());
        let thread = std::thread::current()
            .name()
            .unwrap_or("<unnamed>")
            .to_string();
        let msg = format!("LockNote crashed at {} (thread: {})", location, thread);

        // Write crash log next to the executable
        if let Ok(exe) = std::env::current_exe() {
            let log_path = exe.with_file_name("LockNote-crash.log");
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let entry = format!("[{}] {}\n", timestamp, msg);
            // Append so we keep history across crashes
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
            {
                let _ = f.write_all(entry.as_bytes());
            }
        }

        unsafe {
            let wide_title: Vec<u16> = "LockNote - Crash".encode_utf16().chain(std::iter::once(0)).collect();
            let wide_msg: Vec<u16> = msg.encode_utf16().chain(std::iter::once(0)).collect();
            windows::Win32::UI::WindowsAndMessaging::MessageBoxW(
                None,
                windows::core::PCWSTR(wide_msg.as_ptr()),
                windows::core::PCWSTR(wide_title.as_ptr()),
                windows::Win32::UI::WindowsAndMessaging::MB_OK
                    | windows::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
            );
        }
    }));

    // Allow drag-and-drop through UIPI (needed when running elevated)
    // ChangeWindowMessageFilter applies process-wide, no HWND needed
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::{ChangeWindowMessageFilter, MSGFLT_ADD};
        let _ = ChangeWindowMessageFilter(0x0233, MSGFLT_ADD); // WM_DROPFILES
        let _ = ChangeWindowMessageFilter(0x0049, MSGFLT_ADD); // WM_COPYGLOBALDATA
    }

    // Phase 1: Force TLS (handled by ureq/rustls natively)
    // Language of every window until the note is decrypted (its own
    // language setting is inside it): the Windows display language when
    // LockNote has it, English otherwise (issue #34).
    features::i18n::apply_startup_language();

    // Phase 1b: Wait until no swap helper of this exe is at work (a previous
    // session's save being moved over the exe), before anything of the notes
    // is read or cleaned. The lock is kept for the whole session and handed
    // to the helper at exit (storage::launch_swap_helper). Not obtained in
    // time: say so and quit without touching anything.
    let exe_path = std::env::current_exe().expect("Failed to get executable path");
    if let Err(detail) = storage::acquire_swap_lock(&exe_path) {
        show_startup_error(&features::i18n::get("startup.swap_locked")
            .replace("{exe}", &exe_path.display().to_string())
            .replace("{detail}", &detail));
        return;
    }

    // Phase 2: Clean what LockNote itself left behind (extracted swap
    // helpers, files of an interrupted save or swap). No .tmp is deleted
    // here: it may be the only copy of an exe's latest save.
    storage::cleanup_swap_leftovers();

    // Phase 2b: Warn in the crash log if a recent swap failure was recorded.
    // The helper writes to %LOCALAPPDATA%\LockNote\swap-error.log on failure.
    // We do not surface a dialog (no hard UI dependency at this point);
    // instead we append a crash-log note so investigators notice it. After
    // a failed swap the .tmp is the only copy of the latest save (the exe
    // still holds the previous one): it is read below, and promoted again
    // when this process exits.
    // TODO: optional in-UI toast when too many recent failures are logged.
    check_recent_swap_errors();

    // Phase 3-6: Launch UI. It settles the pending .tmp (asking when the
    // .tmp and the exe are in conflict), opens the notes, then on return
    // (any way out) launches the swap helper if a .tmp is to be promoted.
    ui::run(exe_path);
}

/// A message box before any window exists (same call as the panic hook).
fn show_startup_error(msg: &str) {
    unsafe {
        let wide_title: Vec<u16> = "LockNote".encode_utf16().chain(std::iter::once(0)).collect();
        let wide_msg: Vec<u16> = msg.encode_utf16().chain(std::iter::once(0)).collect();
        windows::Win32::UI::WindowsAndMessaging::MessageBoxW(
            None,
            windows::core::PCWSTR(wide_msg.as_ptr()),
            windows::core::PCWSTR(wide_title.as_ptr()),
            windows::Win32::UI::WindowsAndMessaging::MB_OK
                | windows::Win32::UI::WindowsAndMessaging::MB_ICONWARNING,
        );
    }
}

/// Best-effort check for a recent swap-helper failure.
///
/// If the swap log of the staging directory (`swap-error.log`, see
/// `storage::swap_log_path`) was modified in the last 5 minutes we append a
/// note to the crash log so it is visible next time the user investigates.
/// This is a diagnostic breadcrumb: after a failed swap the latest save is
/// in the .tmp staging file only, which storage handles (opened at startup,
/// promoted again at exit).
fn check_recent_swap_errors() {
    let log_path = storage::swap_log_path();

    let metadata = match std::fs::metadata(&log_path) {
        Ok(m) => m,
        Err(_) => return, // No log file → nothing to do
    };
    let modified = match metadata.modified() {
        Ok(t) => t,
        Err(_) => return,
    };
    let age = std::time::SystemTime::now()
        .duration_since(modified)
        .unwrap_or_default();

    const RECENT_WINDOW_SECS: u64 = 5 * 60;
    if age.as_secs() > RECENT_WINDOW_SECS {
        return;
    }

    // Append a breadcrumb next to the main crash log.
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return,
    };
    let crash_log = exe.with_file_name("LockNote-crash.log");
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let entry = format!(
        "[{}] NOTE: recent entries in the swap log (a failure or a pending-save event); see {}\n",
        ts,
        log_path.display()
    );
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&crash_log)
    {
        let _ = f.write_all(entry.as_bytes());
    }
}
