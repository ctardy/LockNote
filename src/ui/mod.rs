// UI module — Main window, editor, dialogs
//
// Entry point: `ui::run(exe_path, data)` called from main.rs.
// Handles the startup flow: create-password or unlock, then launches EditorForm.

use std::path::PathBuf;

pub mod editor;
pub mod dialogs;
pub mod dark_chrome;
pub mod lock_card;
pub mod window_geometry;
pub mod tab_bar;
pub mod tab_layout;
pub mod vault_panel;
pub mod prompts_panel;
pub mod attachments_panel;
pub mod totp_panel;
pub mod side_panel;

fn t(key: &str) -> String {
    crate::features::i18n::get(key).to_string()
}

/// Main UI entry point. Initializes NWG, settles the pending .tmp, opens the
/// notes, handles password flow, launches editor.
///
/// When the session is over, whatever the way out (editor closed, closed
/// while locked, too many wrong passwords, password / TOTP dialog
/// cancelled), the pending .tmp staging file is handed to the swap helper.
/// This is the only place where that happens. The swap lock taken in `main`
/// is released last (`launch_swap_helper` hands it to the helper already).
///
/// After "Restart LockNote now?" (an update), the helper restarts LockNote
/// once it has replaced the exe. When no helper was queued (nothing to
/// promote, a conflict, a launch failure: all logged), nothing would come
/// back: the user is told, before the swap lock is released.
pub fn run(exe_path: PathBuf) {
    run_session(exe_path.clone());
    let queued = crate::storage::promote_pending_on_exit(&exe_path);
    if crate::storage::relaunch_requested() && !queued {
        let log = crate::storage::staging_dir().join(crate::storage::SWAP_LOG_NAME);
        eprintln!("WARN update: restart asked but no swap helper queued exe={}", exe_path.display());
        nwg::message(&nwg::MessageParams {
            title: "LockNote",
            content: &t("update.restart.failed")
                .replace("{log}", &log.display().to_string()),
            buttons: nwg::MessageButtons::Ok,
            icons: nwg::MessageIcons::Warning,
        });
    }
    crate::storage::release_swap_lock();
}

/// Text of the startup question when the .tmp and the exe do not hold the
/// same notes and the exe was written by something else since the pending
/// save started. Dates are Unix seconds, shown in UTC like the other dates
/// of the application.
///
/// Shown before the notes are decrypted, so before the language of the
/// encrypted settings is known: like the unlock window, it is in the
/// language in force at startup (English).
/// The status text after a clipboard write (issue #31): `copied` only when
/// the write succeeded, the translated failure with its reason otherwise.
pub(crate) fn copy_feedback(result: &Result<(), String>, copied: &str) -> String {
    match result {
        Ok(()) => copied.to_string(),
        Err(e) => {
            eprintln!("WARN clipboard: copy failed err={}", e);
            t("clipboard.failed").replace("{err}", e)
        }
    }
}

fn conflict_message(
    tmp_unix: u64,
    exe_unix: u64,
    exe_has_data: bool,
    exe_path: &std::path::Path,
    conflicts_dir: &std::path::Path,
) -> String {
    let mut text = t("conflict.body")
        .replace("{tmp_date}", &editor::format_timestamp(tmp_unix))
        .replace("{exe_date}", &editor::format_timestamp(exe_unix))
        .replace("{exe}", &exe_path.display().to_string())
        .replace("{dir}", &conflicts_dir.display().to_string());
    if !exe_has_data {
        text.push_str("\n\n");
        text.push_str(&t("conflict.exe_empty"));
    }
    text
}

fn unix_seconds(when: std::time::SystemTime) -> u64 {
    when.duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Text shown when the notes cannot be opened safely (the program file or
/// the pending save cannot be read, or the pending save cannot be taken up).
/// `detail` names the file.
fn open_failed_message(detail: &str) -> String {
    t("pending.failed")
        .replace("{msg}", detail)
}

/// Show an error box before the editor exists.
fn startup_error(title: &str, content: &str) {
    let params = nwg::MessageParams {
        title,
        content,
        buttons: nwg::MessageButtons::Ok,
        icons: nwg::MessageIcons::Error,
    };
    nwg::message(&params);
}

/// Startup step on the pending .tmp staging file, before any password is
/// asked: what needs no question is applied by storage; a conflict (the exe
/// no longer holds the notes the pending save was based on) is put to the
/// user. Then the notes are opened for this session.
///
/// Returns the payload to unlock (None: no notes yet), or `Err(())` when
/// the application must quit without changing anything (cancelled, or a
/// failure that was shown and logged).
fn settle_and_open(exe_path: &std::path::Path) -> Result<Option<Vec<u8>>, ()> {
    use crate::storage::PendingDecision;

    let title = t("conflict.title");
    let fail = |stage: &str, detail: String| {
        crate::storage::log_swap_event(&format!(
            "startup stopped ({}): tmp={} exe={} err={}",
            stage,
            crate::storage::get_tmp_path(exe_path).display(),
            exe_path.display(),
            detail
        ));
        startup_error("LockNote", &open_failed_message(&detail));
    };

    match crate::storage::settle_pending_at_startup(exe_path) {
        Err(detail) => {
            fail("settle", detail);
            return Err(());
        }
        Ok(PendingDecision::ExeUnreadable(detail)) => {
            fail("program file unreadable", detail);
            return Err(());
        }
        Ok(PendingDecision::TmpUnreadable(detail)) => {
            // It may hold the latest save: nothing is opened over it.
            fail("pending save unreadable", detail);
            return Err(());
        }
        Ok(PendingDecision::Conflict { tmp_modified, exe_modified, exe_has_data, .. }) => {
            let content = conflict_message(
                unix_seconds(tmp_modified),
                unix_seconds(exe_modified),
                exe_has_data,
                exe_path,
                &crate::storage::conflicts_dir(),
            );
            let params = nwg::MessageParams {
                title: &title,
                content: &content,
                buttons: nwg::MessageButtons::YesNoCancel,
                icons: nwg::MessageIcons::Warning,
            };
            let resolved = match nwg::message(&params) {
                nwg::MessageChoice::Yes => crate::storage::resolve_conflict_keep_tmp(exe_path),
                nwg::MessageChoice::No => crate::storage::resolve_conflict_keep_exe(exe_path),
                _ => {
                    // Cancel: quit, both versions stay where they are.
                    crate::storage::log_swap_event(&format!(
                        "conflict question cancelled, nothing changed: exe={}",
                        exe_path.display()
                    ));
                    return Err(());
                }
            };
            if let Err(e) = resolved {
                let detail = e.to_string();
                crate::storage::log_swap_event(&format!(
                    "conflict not resolved: tmp={} exe={} err={}",
                    crate::storage::get_tmp_path(exe_path).display(),
                    exe_path.display(),
                    detail
                ));
                let content = t("conflict.failed")
                    .replace("{msg}", &detail);
                startup_error(&title, &content);
                return Err(());
            }
        }
        Ok(_) => {}
    }

    crate::storage::open_session(exe_path).map_err(|detail| fail("open", detail))
}

/// The session itself: startup dialogs, then the editor until it closes.
fn run_session(exe_path: PathBuf) {
    // Initialize NWG
    nwg::init().expect("Failed to init Native Windows GUI");

    // Set global default font (Segoe UI 9pt)
    let mut default_font = nwg::Font::default();
    nwg::Font::builder()
        .family("Segoe UI")
        .size(17) // 9pt in NWG logical units
        .build(&mut default_font)
        .expect("Failed to build default font");
    nwg::Font::set_global_default(Some(default_font));

    // Pending .tmp staging file: settle it, then open the notes, before any
    // password is asked.
    let data = match settle_and_open(&exe_path) {
        Ok(data) => data,
        Err(()) => return, // Shown and logged: quit without changing anything
    };

    match data {
        None => {
            // No existing data — first run, create a new password
            let settings = crate::settings::Settings::default();
            let password = match dialogs::create_password::show(settings.min_password_length) {
                Some(result) => result,
                None => return, // User cancelled
            };
            editor::EditorForm::run(exe_path, password, String::new(), settings);
        }
        Some(raw_payload) => {
            // Strip legacy hint prefix if present (backward compatibility)
            let (_hint, encrypted) = crate::storage::split_hint_and_encrypted(&raw_payload);

            // Existing data — prompt to unlock
            match dialogs::unlock::show(&encrypted) {
                Some((password, decrypted_text)) => {
                    // Decompress after decryption
                    let decrypted_text = crate::features::compression::decompress(&decrypted_text);
                    let (settings, note_text) = crate::settings::Settings::parse(&decrypted_text);

                    // Initialize i18n language from saved settings (before any further dialogs)
                    {
                        let i18n_lang = match settings.language {
                            crate::settings::Language::En => crate::features::i18n::Language::English,
                            crate::settings::Language::Fr => crate::features::i18n::Language::French,
                            crate::settings::Language::Es => crate::features::i18n::Language::Spanish,
                            crate::settings::Language::De => crate::features::i18n::Language::German,
                        };
                        crate::features::i18n::set_language(i18n_lang);
                    }

                    // TOTP 2FA verification
                    if settings.totp_2fa && !settings.totp_2fa_secret.is_empty() {
                        if !dialogs::totp_verify::TotpVerifyDialog::show(&settings.totp_2fa_secret) {
                            return; // TOTP verification failed
                        }
                    }

                    // Apply theme from settings
                    crate::theme::set_mode(crate::theme::mode_for_choice(settings.theme));

                    editor::EditorForm::run(exe_path, password, note_text, settings);
                }
                None => {
                    // Unlock failed or cancelled
                    return;
                }
            }
        }
    }
}

// Re-export nwg for convenience within the ui module
use native_windows_gui as nwg;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Issue #31: "Copied" only after a successful copy.
    #[test]
    fn copy_feedback_says_copied_only_after_success() {
        let _g = crate::features::i18n::test_support::lang_guard();
        crate::features::i18n::set_language(crate::features::i18n::Language::English);
        assert_eq!(copy_feedback(&Ok(()), "Copied: Gmail"), "Copied: Gmail");
        let failed = copy_feedback(&Err("OpenClipboard failed".into()), "Copied: Gmail");
        assert!(!failed.contains("Copied"), "{}", failed);
        assert!(failed.contains("OpenClipboard failed"), "{}", failed);
    }

    /// Issue #31: no clipboard write may discard its result.
    #[test]
    fn no_clipboard_write_discards_its_result() {
        fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
            for e in std::fs::read_dir(dir).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p.extension().map_or(false, |x| x == "rs") {
                    out.push(p);
                }
            }
        }
        let mut files = Vec::new();
        walk(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
        let mut found = Vec::new();
        for f in files {
            let text = std::fs::read_to_string(&f).unwrap();
            for (i, line) in text.lines().enumerate() {
                let l = line.trim_start();
                let writes = ["set_clipboard", "SetClipboardData", "CloseClipboard", "EmptyClipboard"]
                    .iter()
                    .any(|w| l.contains(w));
                if l.starts_with("let _ =") && writes {
                    found.push(format!("{}:{}: {}", f.display(), i + 1, l));
                }
            }
        }
        assert!(found.is_empty(), "clipboard results discarded:\n{}", found.join("\n"));
    }

    // The wording depends on the current language, which other tests switch:
    // only what every language must show is checked.
    #[test]
    fn conflict_message_shows_both_dates_the_file_and_where_the_other_version_goes() {
        let text = conflict_message(
            1_700_000_000,
            1_700_086_400,
            true,
            Path::new(r"X:\notes\LockNote.exe"),
            Path::new(r"C:\Users\me\AppData\Local\LockNote\conflicts"),
        );
        assert!(text.contains("2023-11-14 22:13:20"), "{}", text);
        assert!(text.contains("2023-11-15 22:13:20"), "{}", text);
        assert!(text.contains(r"X:\notes\LockNote.exe"), "{}", text);
        assert!(text.contains(r"C:\Users\me\AppData\Local\LockNote\conflicts"), "{}", text);
        assert!(!text.contains('{') && !text.contains('}'), "placeholder left: {}", text);
        assert!(!text.starts_with('['), "missing translation key: {}", text);
    }

    #[test]
    fn open_failed_message_shows_the_detail() {
        let text = open_failed_message(r"the program file X:\notes\x.exe cannot be read: access denied");
        assert!(text.contains(r"X:\notes\x.exe cannot be read: access denied"), "{}", text);
        assert!(!text.contains("{msg}"), "placeholder left: {}", text);
        assert!(!text.starts_with('['), "missing translation key: {}", text);
    }

    #[test]
    fn conflict_message_says_when_the_program_file_holds_no_notes() {
        let _lang = crate::features::i18n::test_support::lang_guard();
        crate::features::i18n::set_language(crate::features::i18n::Language::English);
        let exe = Path::new(r"C:\a\LockNote.exe");
        let dir = Path::new(r"C:\b\conflicts");
        let with_data = conflict_message(1, 2, true, exe, dir);
        let without = conflict_message(1, 2, false, exe, dir);
        assert!(!without.contains("[conflict."), "missing translation key: {}", without);
        // Issue 055, pass 3: judged by content, the program file may be
        // dated before the pending save.
        assert!(!with_data.contains("changed since then"), "{}", with_data);
        assert!(without.starts_with(&with_data));
        assert!(without.ends_with("the program file holds no notes at all."), "{}", without);
        assert!(!with_data.contains("holds no notes"), "{}", with_data);
    }
}
