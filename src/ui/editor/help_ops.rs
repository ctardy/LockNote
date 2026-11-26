// Help (updates, about) and tray icon.
// Part of the EditorForm implementation, split out of the former editor.rs.

#[allow(unused_imports)]
use super::*;

impl EditorForm {
    // =====================================================================
    // Help
    // =====================================================================

    pub(super) fn on_check_updates(&self) {
        match crate::updater::check_for_update() {
            crate::updater::UpdateCheckResult::Available { version, download_url } => {
                let msg = t("update.available.ask").replace("{version}", &version.to_string());
                let title = t("update.available.title");
                let params = nwg::MessageParams {
                    title: &title,
                    content: &msg,
                    buttons: nwg::MessageButtons::YesNo,
                    icons: nwg::MessageIcons::Question,
                };
                if nwg::message(&params) == nwg::MessageChoice::Yes {
                    // The swap replaces the exe that holds the notes: back them
                    // up first, and do not update without that backup.
                    {
                        if let Err(detail) = self.write_auto_backup(crate::features::auto_backup::Kind::PreUpdate) {
                            nwg::modal_info_message(
                                &self.window,
                                "LockNote",
                                &t("autobackup.failed.update")
                                    .replace("{msg}", &detail),
                            );
                            return;
                        }
                    }
                    match crate::updater::download_and_update(&download_url, &self.exe_path) {
                        Ok(_) => {
                            // The updater wrote the new binary into the .tmp staging
                            // file, and its companion says so (program=updater):
                            // the next save keeps that program (storage::write_data)
                            // and the exit promotes it
                            // (storage::promote_pending_on_exit), saved or not,
                            // notes or not.

                            let title = t("update.downloaded.title");
                            let content = t("update.downloaded.ask");
                            let restart_params = nwg::MessageParams {
                                title: &title,
                                content: &content,
                                buttons: nwg::MessageButtons::YesNo,
                                icons: nwg::MessageIcons::Question,
                            };
                            if nwg::message(&restart_params) == nwg::MessageChoice::Yes {
                                // Close the window — on_close runs the final save and
                                // zeroizes secrets; ui::run then launches the swap
                                // helper, which replaces the .exe after this process
                                // exits and starts it again.
                                if self.on_close() {
                                    crate::storage::request_relaunch_after_swap();
                                    nwg::stop_thread_dispatch();
                                }
                            }
                        }
                        Err(e) => {
                            nwg::modal_info_message(
                                &self.window,
                                &t("update.error.title"),
                                &t("update.error").replace("{error}", &e.to_string()),
                            );
                        }
                    }
                }
            }
            crate::updater::UpdateCheckResult::UpToDate => {
                nwg::modal_info_message(
                    &self.window,
                    &t("update.uptodate.title"),
                    &t("update.uptodate.msg").replace("{version}", &crate::updater::current_version()),
                );
            }
            crate::updater::UpdateCheckResult::Error(e) => {
                nwg::modal_info_message(
                    &self.window,
                    &t("update.checkfailed.title"),
                    &t("update.checkfailed.msg").replace("{error}", &e.to_string()),
                );
            }
        }
    }

    pub(super) fn on_about(&self) {
        let msg = format!(
            "LockNote\n{}\n\n{}\n\n{}\n\n\
             https://github.com/{}\n\
             https://uitguard.com",
            t("about.version").replace("{version}", &crate::updater::current_version()),
            t("about.desc"),
            // The same line as the About dialog, built from the crypto
            // constants so it cannot drift from them (issue #28).
            crate::ui::dialogs::about::crypto_summary(),
            crate::updater::github_repo(),
        );
        nwg::modal_info_message(&self.window, &t("about.title"), &msg);
    }

    // =====================================================================
    // Tray icon
    // =====================================================================

    pub(super) fn on_tray_restore(&self) {
        // If locked by auto-lock, show the window — the inline overlay is
        // already visible with the password field, just give it focus.
        if *self.is_locked.borrow() {
            self.window.set_visible(true);
            if let nwg::ControlHandle::Hwnd(hwnd) = self.window.handle {
                unsafe {
                    windows::Win32::UI::WindowsAndMessaging::ShowWindow(
                        windows::Win32::Foundation::HWND(hwnd as *mut _),
                        windows::Win32::UI::WindowsAndMessaging::SW_RESTORE,
                    );
                    let _ = windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(
                        windows::Win32::Foundation::HWND(hwnd as *mut _),
                    );
                }
            }
            self.focus_lock_input();
            return;
        }

        // If password was cleared (e.g. by manual lock), re-authenticate before restoring
        if self.password.borrow().is_empty() {
            if !self.unlock_from_disk() {
                return; // Unlock failed or cancelled — stay in tray
            }
        }
        self.window.set_visible(true);
        // Restore from minimized state via ShowWindow(SW_RESTORE)
        if let nwg::ControlHandle::Hwnd(hwnd) = self.window.handle {
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(
                    windows::Win32::Foundation::HWND(hwnd as *mut _),
                    windows::Win32::UI::WindowsAndMessaging::SW_RESTORE,
                );
            }
        }
    }
}
