// LockNote — Self-contained encrypted notepad for Windows
// Single .exe that serves as both text editor and encrypted vault

mod crypto;
mod storage;
mod settings;
mod theme;
mod updater;
mod ui;

#[cfg(feature = "pro")]
mod pro;

fn main() {
    // Phase 1: Force TLS (handled by ureq/rustls natively)
    // Phase 2: Clean stale .tmp files
    storage::cleanup_stale_tmp_files();

    // Phase 3-4: Try read encrypted data
    let exe_path = std::env::current_exe().expect("Failed to get executable path");
    let data = storage::read_data(&exe_path);

    // Phase 5-6: Launch UI
    ui::run(exe_path, data);
}
