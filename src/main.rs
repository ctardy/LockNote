// LockNote — Self-contained encrypted notepad for Windows
// Single .exe that serves as both text editor and encrypted vault
#![windows_subsystem = "windows"]

mod crypto;
mod storage;
mod settings;
mod theme;
mod updater;
mod ui;

#[cfg(feature = "pro")]
mod pro;

fn main() {
    // Panic hook: log to file + show MessageBox
    std::panic::set_hook(Box::new(|info| {
        let msg = format!("LockNote crashed:\n\n{}", info);

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

    // Phase 1: Force TLS (handled by ureq/rustls natively)
    // Phase 2: Clean stale .tmp files
    storage::cleanup_stale_tmp_files();

    // Phase 3-4: Try read encrypted data
    let exe_path = std::env::current_exe().expect("Failed to get executable path");
    let data = storage::read_data(&exe_path);

    // Phase 5-6: Launch UI
    ui::run(exe_path, data);
}
