// BackupsDialog — history of the automatic safety backups.
//
// Lists the files of `features::auto_backup` (newest first) and lets the user
// restore one, force a new one, delete one or open the folder. Restoring and
// creating are done by the caller (they need the note); the dialog only says
// what was asked.

use native_windows_gui as nwg;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use crate::features::auto_backup::{self, Entry, Kind};

fn t(key: &str) -> String {
    crate::features::i18n::get(key).to_string()
}

/// What the user chose when the dialog closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackupsAction {
    Close,
    /// Create a backup now (`Kind::Manual`).
    CreateNow,
    /// Restore this file (a name returned by `auto_backup::list`).
    Restore(String),
}

pub struct BackupsDialog;

fn kind_label(kind: Kind) -> String {
    match kind {
        Kind::PreImport => t("autobackup.kind.pre_import"),
        Kind::PreUpdate => t("autobackup.kind.pre_update"),
        Kind::Manual => t("autobackup.kind.manual"),
    }
}

fn list_label(e: &Entry) -> String {
    format!(
        "{}  \u{2014}  {}  \u{2014}  {}",
        crate::features::snapshots::format_timestamp(e.unix),
        kind_label(e.kind),
        auto_backup::format_size(e.size)
    )
}

fn details_text(e: &Entry) -> String {
    t("autobackup.dlg.details")
        .replace("{kind}", &kind_label(e.kind))
        .replace("{date}", &crate::features::snapshots::format_timestamp(e.unix))
        .replace("{size}", &auto_backup::format_size(e.size))
        .replace('\n', "\r\n")
}

impl BackupsDialog {
    /// Show the backups found in `dir`. Deleting and opening the folder are
    /// handled here; restoring and creating are returned.
    pub fn show(dir: &Path) -> BackupsAction {
        let initial = match auto_backup::list(dir) {
            Ok(v) => v,
            Err(e) => {
                nwg::modal_error_message(
                    nwg::ControlHandle::NoHandle,
                    "LockNote",
                    &t("autobackup.dlg.list_failed")
                        .replace("{msg}", &e.to_string()),
                );
                return BackupsAction::Close;
            }
        };

        let mut window = Default::default();
        nwg::Window::builder()
            .size((720, 500))
            .position((250, 200))
            .title(&t("autobackup.dlg.title"))
            .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
            .build(&mut window)
            .expect("Failed to build BackupsDialog window");

        super::apply_dialog_theme(&window);

        let mut font = Default::default();
        nwg::Font::builder()
            .family("Segoe UI")
            .size(16)
            .build(&mut font)
            .expect("Failed to build font");

        let mut lbl_list = Default::default();
        nwg::Label::builder()
            .text(&t("autobackup.dlg.list"))
            .parent(&window)
            .position((15, 10))
            .size((420, 20))
            .font(Some(&font))
            .build(&mut lbl_list)
            .expect("Failed to build label");

        let mut list_box = Default::default();
        nwg::ListBox::builder()
            .parent(&window)
            .position((15, 35))
            .size((440, 370))
            .font(Some(&font))
            .build(&mut list_box)
            .expect("Failed to build list box");
        for e in &initial {
            list_box.push(list_label(e));
        }

        let mut txt_details = Default::default();
        nwg::TextBox::builder()
            .parent(&window)
            .position((470, 35))
            .size((235, 370))
            .font(Some(&font))
            .readonly(true)
            .flags(nwg::TextBoxFlags::VISIBLE)
            .text(&Self::empty_hint(initial.is_empty()))
            .build(&mut txt_details)
            .expect("Failed to build details box");

        let button = |text: String, x: i32, w: i32| {
            let mut b = Default::default();
            nwg::Button::builder()
                .text(&text)
                .parent(&window)
                .position((x, 420))
                .size((w, 35))
                .font(Some(&font))
                .build(&mut b)
                .expect("Failed to build button");
            b
        };
        let btn_restore: nwg::Button = button(t("autobackup.dlg.restore"), 15, 100);
        let btn_create: nwg::Button = button(t("autobackup.dlg.create"), 125, 170);
        let btn_delete: nwg::Button = button(t("autobackup.dlg.delete"), 305, 100);
        let btn_folder: nwg::Button = button(t("autobackup.dlg.folder"), 415, 120);
        let btn_close: nwg::Button = button(t("autobackup.dlg.close"), 605, 100);

        let entries: Rc<RefCell<Vec<Entry>>> = Rc::new(RefCell::new(initial));
        let action: Rc<RefCell<BackupsAction>> = Rc::new(RefCell::new(BackupsAction::Close));

        let dir = dir.to_path_buf();
        let entries_h = entries.clone();
        let action_h = action.clone();
        let window_handle = window.handle;

        let handler = nwg::full_bind_event_handler(&window_handle, move |evt, _evt_data, handle| {
            match evt {
                nwg::Event::OnListBoxSelect => {
                    if handle == list_box.handle {
                        if let Some(sel) = list_box.selection() {
                            if let Some(e) = entries_h.borrow().get(sel) {
                                txt_details.set_text(&details_text(e));
                            }
                        }
                    }
                }
                nwg::Event::OnButtonClick => {
                    if handle == btn_restore.handle {
                        if let Some(sel) = list_box.selection() {
                            if let Some(e) = entries_h.borrow().get(sel) {
                                *action_h.borrow_mut() = BackupsAction::Restore(e.name.clone());
                                nwg::stop_thread_dispatch();
                            }
                        }
                    } else if handle == btn_create.handle {
                        *action_h.borrow_mut() = BackupsAction::CreateNow;
                        nwg::stop_thread_dispatch();
                    } else if handle == btn_delete.handle {
                        if let Some(sel) = list_box.selection() {
                            let name = entries_h.borrow().get(sel).map(|e| e.name.clone());
                            if let Some(name) = name {
                                let params = nwg::MessageParams {
                                    title: &t("autobackup.dlg.delete.title"),
                                    content: &t("autobackup.dlg.delete.confirm"),
                                    buttons: nwg::MessageButtons::YesNo,
                                    icons: nwg::MessageIcons::Warning,
                                };
                                if nwg::message(&params) == nwg::MessageChoice::Yes {
                                    match auto_backup::delete(&dir, &name) {
                                        Ok(()) => {
                                            crate::features::import_log::note("auto_backup: deleted 1 by user");
                                            match auto_backup::list(&dir) {
                                                Ok(fresh) => {
                                                    while list_box.len() > 0 {
                                                        list_box.remove(list_box.len() - 1);
                                                    }
                                                    for e in &fresh {
                                                        list_box.push(list_label(e));
                                                    }
                                                    txt_details.set_text(&Self::empty_hint(fresh.is_empty()));
                                                    *entries_h.borrow_mut() = fresh;
                                                }
                                                Err(e) => {
                                                    nwg::message(&nwg::MessageParams {
                                                        title: "LockNote",
                                                        content: &t("autobackup.dlg.list_failed")
                                                            .replace("{msg}", &e.to_string()),
                                                        buttons: nwg::MessageButtons::Ok,
                                                        icons: nwg::MessageIcons::Error,
                                                    });
                                                    nwg::stop_thread_dispatch();
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            crate::features::import_log::note("WARN auto_backup: delete by user failed");
                                            nwg::message(&nwg::MessageParams {
                                                title: "LockNote",
                                                content: &t("autobackup.dlg.delete.failed")
                                                    .replace("{msg}", &e.to_string()),
                                                buttons: nwg::MessageButtons::Ok,
                                                icons: nwg::MessageIcons::Error,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    } else if handle == btn_folder.handle {
                        // The folder may not exist yet (no backup made so far).
                        let opened = std::fs::create_dir_all(&dir)
                            .and_then(|_| std::process::Command::new("explorer").arg(&dir).spawn().map(|_| ()));
                        if let Err(e) = opened {
                            nwg::message(&nwg::MessageParams {
                                title: "LockNote",
                                content: &t("autobackup.dlg.folder.failed")
                                    .replace("{msg}", &e.to_string()),
                                buttons: nwg::MessageButtons::Ok,
                                icons: nwg::MessageIcons::Error,
                            });
                        }
                    } else if handle == btn_close.handle {
                        nwg::stop_thread_dispatch();
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

        let chosen = action.borrow().clone();
        chosen
    }

    fn empty_hint(empty: bool) -> String {
        if empty {
            t("autobackup.dlg.empty")
        } else {
            String::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: Kind) -> Entry {
        Entry { name: auto_backup::file_name(kind, 1_700_000_000), kind, unix: 1_700_000_000, size: 2048 }
    }

    #[test]
    fn list_label_shows_date_kind_and_size() {
        let l = list_label(&entry(Kind::Manual));
        assert!(l.contains("2023-11-14 22:13:20"), "{}", l);
        assert!(l.contains("2.0 KiB"), "{}", l);
    }

    #[test]
    fn details_use_windows_line_breaks() {
        let d = details_text(&entry(Kind::PreImport));
        assert!(!d.replace("\r\n", "").contains('\n'));
        assert!(d.contains("2023-11-14 22:13:20"));
    }
}
