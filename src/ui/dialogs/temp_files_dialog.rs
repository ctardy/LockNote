// TempFilesDialog — the staging directory of LockNote (issue 057).
//
// Explains what the temporary files are and how to reach them, lists the
// pending .tmp files (`storage::inventory`), and offers what can be done with
// them: open the folder, copy its path, delete an orphan .tmp, see whether a
// save of this exe is waiting (and close LockNote to write it), open the logs.
//
// Reading the folder can take long (an exe named on a share that does not
// answer: about 20 s per server; the pending decision reads the whole .tmp
// and exe): it runs on a worker thread, and the dialog shows "Reading..."
// meanwhile. The editor window is disabled while the dialog is open.

use native_windows_gui as nwg;
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use crate::storage::inventory::{self, Owner, StagedEntry};
use crate::storage::PendingDecision;

fn t(key: &str) -> String {
    crate::features::i18n::get(key).to_string()
}

/// What the user chose when the dialog closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TempFilesAction {
    Close,
    /// Close LockNote now, so the pending save is written into the exe.
    CloseApp,
}

/// `template` with each `{name}` of `values` replaced, in one pass: a value
/// that itself contains `{...}` (a folder named `{date}`) is not replaced again.
fn fill(template: &str, values: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(i) = rest.find('{') {
        out.push_str(&rest[..i]);
        let at = &rest[i..];
        let found = values.iter().find(|(name, _)| {
            at.strip_prefix('{').and_then(|s| s.strip_prefix(name)).is_some_and(|s| s.starts_with('}'))
        });
        match found {
            Some((name, value)) => {
                out.push_str(value);
                rest = &at[name.len() + 2..];
            }
            None => {
                out.push('{');
                rest = &at[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Size in B, KiB, MiB (one decimal); "?" when unknown.
fn format_size(bytes: Option<u64>) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    match bytes {
        None => "?".to_string(),
        Some(b) if b < KIB => format!("{} B", b),
        Some(b) if b < MIB => format!("{:.1} KiB", b as f64 / KIB as f64),
        Some(b) => format!("{:.1} MiB", b as f64 / MIB as f64),
    }
}

/// Date in UTC; "?" when unknown or before 1970.
fn format_date(time: Option<std::time::SystemTime>) -> String {
    match time.and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()) {
        Some(d) => crate::ui::editor::format_timestamp(d.as_secs()),
        None => "?".to_string(),
    }
}

/// Short status shown in the list for a pending .tmp.
fn owner_label(owner: &Owner) -> String {
    match owner {
        Owner::Current => t("tempfiles.owner.current"),
        Owner::ExePresent(_) => t("tempfiles.owner.present"),
        Owner::Orphan(_) => t("tempfiles.owner.orphan"),
        Owner::FolderMissing(_) => t("tempfiles.owner.folder_missing"),
        Owner::OtherVolume(_) => t("tempfiles.owner.other_volume"),
        Owner::Unreachable(..) => t("tempfiles.owner.unreachable"),
        Owner::NoCompanion => t("tempfiles.owner.no_companion"),
        Owner::CompanionUnusable(_) => t("tempfiles.owner.companion_unusable"),
        Owner::TmpUnreadable(_) => t("tempfiles.owner.tmp_unreadable"),
    }
}

/// What a status means and whether the file may be deleted.
fn owner_meaning(owner: &Owner) -> String {
    match owner {
        Owner::Current => t("tempfiles.meaning.current"),
        Owner::ExePresent(_) => t("tempfiles.meaning.present"),
        Owner::Orphan(_) => t("tempfiles.meaning.orphan"),
        Owner::FolderMissing(_) => t("tempfiles.meaning.folder_missing"),
        Owner::OtherVolume(_) => t("tempfiles.meaning.other_volume"),
        Owner::Unreachable(_, e) => fill(
            &t("tempfiles.meaning.unreachable"),
            &[("error", e)],
        ),
        Owner::NoCompanion => t("tempfiles.meaning.no_companion"),
        Owner::CompanionUnusable(why) => fill(
            &t("tempfiles.meaning.companion_unusable"),
            &[("reason", why)],
        ),
        Owner::TmpUnreadable(e) => fill(
            &t("tempfiles.meaning.tmp_unreadable"),
            &[("error", e)],
        ),
    }
}

fn list_label(e: &StagedEntry) -> String {
    format!(
        "{}  \u{2014}  {}  \u{2014}  {}",
        format_date(e.modified),
        format_size(e.size),
        owner_label(&e.owner)
    )
}

fn details_text(e: &StagedEntry) -> String {
    let exe = e
        .owner
        .exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| t("tempfiles.dlg.exe_unknown"));
    fill(
        &t("tempfiles.dlg.details"),
        &[
            ("tmp", &e.tmp.display().to_string()),
            ("exe", &exe),
            ("size", &format_size(e.size)),
            ("date", &format_date(e.modified)),
            ("meaning", &owner_meaning(&e.owner)),
        ],
    )
    .replace('\n', "\r\n")
}

/// What the "apply now" part says about the .tmp of the open exe, and
/// whether closing LockNote would write it.
fn apply_now_text(decision: &PendingDecision, tmp: &Path, tmp_size: Option<u64>, exe: &Path) -> (String, bool) {
    let undecided = |reason: String| {
        (
            t("tempfiles.apply.undecided"),
            reason,
        )
    };
    let ((template, reason), can_close) = match decision {
        PendingDecision::Promote => (
            (
                t("tempfiles.apply.pending"),
                String::new(),
            ),
            true,
        ),
        PendingDecision::None | PendingDecision::Stale => {
            ((t("tempfiles.apply.nothing"), String::new()), false)
        }
        PendingDecision::Conflict { .. } => {
            (undecided(t("tempfiles.apply.reason.conflict")), false)
        }
        PendingDecision::TmpUnreadable(_) => {
            (undecided(t("tempfiles.apply.reason.tmp_unreadable")), false)
        }
        PendingDecision::ExeUnreadable(_) => {
            (undecided(t("tempfiles.apply.reason.exe_unreadable")), false)
        }
        PendingDecision::Ignored(_) => {
            (undecided(t("tempfiles.apply.reason.ignored")), false)
        }
    };
    let text = fill(
        &template,
        &[
            ("tmp", &tmp.display().to_string()),
            ("exe", &exe.display().to_string()),
            ("size", &format_size(tmp_size)),
            ("reason", &reason),
        ],
    );
    (text, can_close)
}

fn error_box(content: &str) {
    nwg::message(&nwg::MessageParams {
        title: "LockNote",
        content,
        buttons: nwg::MessageButtons::Ok,
        icons: nwg::MessageIcons::Error,
    });
}

fn info_box(content: &str) {
    nwg::message(&nwg::MessageParams {
        title: "LockNote",
        content,
        buttons: nwg::MessageButtons::Ok,
        icons: nwg::MessageIcons::Info,
    });
}

/// Open `file` with the program associated with its type. A missing file
/// says that nothing was recorded.
fn open_log(file: &Path) {
    let failed = |msg: &str| {
        eprintln!("WARN tempfiles: cannot open {}: {}", file.display(), msg);
        error_box(&fill(
            &t("tempfiles.log.failed"),
            &[("file", &file.display().to_string()), ("msg", msg)],
        ));
    };
    match std::fs::symlink_metadata(file) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            info_box(&fill(
                &t("tempfiles.log.empty"),
                &[("file", &file.display().to_string())],
            ));
            return;
        }
        Err(e) => return failed(&e.to_string()),
    }
    let wide_op: Vec<u16> = "open\0".encode_utf16().collect();
    let wide_file: Vec<u16> = file.as_os_str().to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect();
    let result = unsafe {
        windows::Win32::UI::Shell::ShellExecuteW(
            None,
            windows::core::PCWSTR(wide_op.as_ptr()),
            windows::core::PCWSTR(wide_file.as_ptr()),
            windows::core::PCWSTR::null(),
            windows::core::PCWSTR::null(),
            windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
        )
    };
    // ShellExecuteW returns a value greater than 32 on success.
    if (result.0 as isize) <= 32 {
        failed(&format!("ShellExecute error {}", result.0 as isize));
    }
}

/// Open the staging folder in the Explorer. A folder that does not exist yet
/// is not created for the occasion.
fn open_folder(dir: &Path) {
    let failed = |msg: String| {
        eprintln!("WARN tempfiles: cannot open the folder {}: {}", dir.display(), msg);
        error_box(&fill(&t("tempfiles.dlg.folder_failed"), &[("msg", &msg)]));
    };
    match std::fs::metadata(dir) {
        Ok(m) if m.is_dir() => {
            if let Err(e) = std::process::Command::new("explorer").arg(dir).spawn() {
                failed(e.to_string());
            }
        }
        Ok(_) => failed("not a folder".to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => info_box(&t("tempfiles.dlg.folder_absent")),
        Err(e) => failed(e.to_string()),
    }
}

/// What a worker thread found: the list, then the decision on the .tmp of
/// the open exe; and, after a deletion, how it went.
struct Snapshot {
    list: std::io::Result<Vec<StagedEntry>>,
    decision: PendingDecision,
    tmp: PathBuf,
    tmp_size: Option<u64>,
    deleted: Option<Result<PathBuf, String>>,
}

fn read_snapshot(dir: &Path, exe: &Path, delete: Option<&str>) -> Snapshot {
    let deleted = delete.map(|hash| inventory::delete_orphan_in(dir, exe, hash));
    let list = inventory::list_staging_in(dir, exe);
    let tmp = crate::storage::get_tmp_path_in(dir, exe);
    let tmp_size = match std::fs::metadata(&tmp) {
        Ok(m) => Some(m.len()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            eprintln!("WARN tempfiles: size of {}: {}", tmp.display(), e);
            None
        }
    };
    let decision = crate::storage::pending_decision_in(dir, exe);
    Snapshot { list, decision, tmp, tmp_size, deleted }
}

/// The controls the dialog updates after it is built, and what they show.
struct Live {
    dir: PathBuf,
    exe: PathBuf,
    entries: RefCell<Vec<StagedEntry>>,
    list_box: nwg::ListBox<String>,
    txt_details: nwg::TextBox,
    btn_delete: nwg::Button,
    lbl_apply: nwg::Label,
    btn_close_app: nwg::Button,
    notice: nwg::Notice,
    slot: Arc<Mutex<Option<Snapshot>>>,
    busy: Cell<bool>,
}

impl Live {
    fn selected(&self) -> Option<StagedEntry> {
        self.list_box.selection().and_then(|i| self.entries.borrow().get(i).cloned())
    }

    /// Read the folder again on a worker thread, after deleting the orphan
    /// `delete` when given; `show` applies the result.
    fn start(&self, delete: Option<String>) {
        if self.busy.replace(true) {
            return;
        }
        self.list_box.set_collection(Vec::new());
        self.entries.borrow_mut().clear();
        self.btn_delete.set_enabled(false);
        self.btn_close_app.set_enabled(false);
        let reading = t("tempfiles.dlg.reading");
        self.txt_details.set_text(&reading);
        self.lbl_apply.set_text(&reading);
        let (dir, exe, slot, sender) = (self.dir.clone(), self.exe.clone(), self.slot.clone(), self.notice.sender());
        std::thread::spawn(move || {
            let snapshot = read_snapshot(&dir, &exe, delete.as_deref());
            match slot.lock() {
                Ok(mut s) => *s = Some(snapshot),
                Err(e) => eprintln!("WARN tempfiles: result slot poisoned: {}", e),
            }
            sender.notice();
        });
    }

    /// Show what the worker thread found.
    fn show(&self) {
        self.busy.set(false);
        let taken = match self.slot.lock() {
            Ok(mut s) => s.take(),
            Err(e) => {
                eprintln!("WARN tempfiles: result slot poisoned: {}", e);
                None
            }
        };
        let Some(snap) = taken else {
            self.txt_details.set_text(&fill(
                &t("tempfiles.dlg.list_failed"),
                &[("msg", "no result")],
            ));
            return;
        };
        if let Some(Err(why)) = &snap.deleted {
            error_box(&fill(&t("tempfiles.delete.failed"), &[("msg", why)]));
        }
        match snap.list {
            Ok(fresh) => {
                self.list_box.set_collection(fresh.iter().map(list_label).collect());
                self.txt_details.set_text(&if fresh.is_empty() {
                    t("tempfiles.dlg.empty")
                } else {
                    String::new()
                });
                *self.entries.borrow_mut() = fresh;
            }
            Err(e) => {
                eprintln!("WARN tempfiles: cannot list {}: {}", self.dir.display(), e);
                self.txt_details.set_text(&fill(
                    &t("tempfiles.dlg.list_failed"),
                    &[("msg", &e.to_string())],
                ));
            }
        }
        let (text, can_close) = apply_now_text(&snap.decision, &snap.tmp, snap.tmp_size, &self.exe);
        self.lbl_apply.set_text(&text);
        self.btn_close_app.set_enabled(can_close);
    }
}

pub struct TempFilesDialog;

impl TempFilesDialog {
    /// Show the staging directory of the LockNote file `exe`, modal to the
    /// window `parent` (disabled meanwhile).
    pub fn show(parent: nwg::ControlHandle, exe: &Path) -> TempFilesAction {
        let dir = crate::storage::staging_dir();
        // Modal to the editor; its previous state comes back (pass 6).
        let _modal = super::disable_owner(Some(parent), true);

        let mut window = Default::default();
        nwg::Window::builder()
            .size((780, 640))
            .position((240, 140))
            .title(&t("tempfiles.dlg.title"))
            .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
            .parent(Some(parent))
            .build(&mut window)
            .expect("Failed to build TempFilesDialog window");

        super::apply_dialog_theme(&window);

        let mut font = Default::default();
        nwg::Font::builder().family("Segoe UI").size(16).build(&mut font).expect("Failed to build font");
        let mut font_bold = Default::default();
        nwg::Font::builder()
            .family("Segoe UI")
            .size(16)
            .weight(700)
            .build(&mut font_bold)
            .expect("Failed to build bold font");

        let label = |text: &str, x: i32, y: i32, w: i32, h: i32, bold: bool| {
            let mut l = nwg::Label::default();
            nwg::Label::builder()
                .text(text)
                .parent(&window)
                .position((x, y))
                .size((w, h))
                .font(Some(if bold { &font_bold } else { &font }))
                .build(&mut l)
                .expect("Failed to build label");
            l
        };
        let button = |text: String, x: i32, y: i32, w: i32| {
            let mut b = nwg::Button::default();
            nwg::Button::builder()
                .text(&text)
                .parent(&window)
                .position((x, y))
                .size((w, 32))
                .font(Some(&font))
                .build(&mut b)
                .expect("Failed to build button");
            b
        };

        let intro = t("tempfiles.dlg.intro")
            .replace('\n', "\r\n");
        let lbl_intro = label(&intro, 15, 10, 750, 150, false);

        let lbl_folder = label(
            &fill(&t("tempfiles.dlg.folder"), &[("dir", &dir.display().to_string())]),
            15,
            168,
            470,
            40,
            false,
        );
        let btn_open: nwg::Button = button(t("tempfiles.dlg.open_folder"), 495, 170, 130);
        let btn_copy: nwg::Button = button(t("tempfiles.dlg.copy_path"), 635, 170, 130);

        let lbl_list = label(&t("tempfiles.dlg.list"), 15, 215, 450, 20, true);
        let mut list_box: nwg::ListBox<String> = Default::default();
        nwg::ListBox::builder()
            .parent(&window)
            .position((15, 240))
            .size((440, 200))
            .font(Some(&font))
            .build(&mut list_box)
            .expect("Failed to build list box");
        let mut txt_details: nwg::TextBox = Default::default();
        nwg::TextBox::builder()
            .parent(&window)
            .position((465, 240))
            .size((300, 200))
            .font(Some(&font))
            .readonly(true)
            .flags(nwg::TextBoxFlags::VISIBLE | nwg::TextBoxFlags::VSCROLL)
            .build(&mut txt_details)
            .expect("Failed to build details box");
        let btn_delete: nwg::Button = button(t("tempfiles.dlg.delete"), 15, 448, 160);

        let lbl_apply_title = label(&t("tempfiles.dlg.apply"), 15, 495, 450, 20, true);
        let lbl_apply = label("", 15, 518, 560, 60, false);
        let btn_close_app: nwg::Button = button(t("tempfiles.dlg.close_app"), 585, 525, 180);

        let btn_swap_log: nwg::Button = button(t("tempfiles.dlg.swap_log"), 15, 595, 130);
        let btn_import_log: nwg::Button = button(t("tempfiles.dlg.import_log"), 155, 595, 130);
        let btn_close: nwg::Button = button(t("tempfiles.dlg.close"), 665, 595, 100);

        let mut notice = nwg::Notice::default();
        nwg::Notice::builder().parent(&window).build(&mut notice).expect("Failed to build notice");

        let ui = Rc::new(Live {
            dir: dir.clone(),
            exe: exe.to_path_buf(),
            entries: RefCell::new(Vec::new()),
            list_box,
            txt_details,
            btn_delete,
            lbl_apply,
            btn_close_app,
            notice,
            slot: Arc::new(Mutex::new(None)),
            busy: Cell::new(false),
        });
        ui.start(None);

        let action: Rc<RefCell<TempFilesAction>> = Rc::new(RefCell::new(TempFilesAction::Close));
        let action_h = action.clone();
        let window_handle = window.handle;
        let handler = nwg::full_bind_event_handler(&window_handle, move |evt, evt_data, handle| match evt {
            nwg::Event::OnNotice if handle == ui.notice.handle => ui.show(),
            nwg::Event::OnListBoxSelect if handle == ui.list_box.handle => {
                if let Some(e) = ui.selected() {
                    ui.txt_details.set_text(&details_text(&e));
                    ui.btn_delete.set_enabled(e.owner.deletable() && !ui.busy.get());
                }
            }
            nwg::Event::OnButtonClick => {
                if handle == btn_open.handle {
                    open_folder(&ui.dir);
                } else if handle == btn_copy.handle {
                    use clipboard_win::{formats, set_clipboard};
                    if let Err(e) = set_clipboard(formats::Unicode, ui.dir.display().to_string()) {
                        eprintln!("WARN tempfiles: clipboard: {}", e);
                        error_box(&fill(
                            &t("tempfiles.dlg.copy_failed"),
                            &[("msg", &e.to_string())],
                        ));
                    }
                } else if handle == ui.btn_delete.handle {
                    if let Some(e) = ui.selected().filter(|e| e.owner.deletable()) {
                        let exe_gone = e.owner.exe().map(|p| p.display().to_string()).unwrap_or_default();
                        let confirm = fill(
                            &t("tempfiles.delete.confirm"),
                            &[("tmp", &e.tmp.display().to_string()), ("exe", &exe_gone)],
                        );
                        let params = nwg::MessageParams {
                            title: &t("tempfiles.delete.title"),
                            content: &confirm,
                            buttons: nwg::MessageButtons::YesNo,
                            icons: nwg::MessageIcons::Warning,
                        };
                        if nwg::message(&params) == nwg::MessageChoice::Yes {
                            ui.start(Some(e.hash.clone()));
                        }
                    }
                } else if handle == ui.btn_close_app.handle {
                    *action_h.borrow_mut() = TempFilesAction::CloseApp;
                    nwg::stop_thread_dispatch();
                } else if handle == btn_swap_log.handle {
                    open_log(&crate::storage::swap_log_path());
                } else if handle == btn_close.handle {
                    // A deletion in progress finishes, and its outcome is
                    // shown, before the dialog goes away.
                    if !ui.busy.get() {
                        nwg::stop_thread_dispatch();
                    }
                } else {
                    if handle == btn_import_log.handle {
                        open_log(&crate::features::import_log::log_path());
                    }
                }
            }
            nwg::Event::OnWindowClose => {
                if ui.busy.get() {
                    if let nwg::EventData::OnWindowClose(close) = evt_data {
                        close.close(false);
                    }
                } else {
                    nwg::stop_thread_dispatch();
                }
            }
            _ => {}
        });

        nwg::dispatch_thread_events();
        nwg::unbind_event_handler(&handler);

        drop((lbl_intro, lbl_folder, lbl_list, lbl_apply_title));
        let chosen = *action.borrow();
        chosen
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::SystemTime;

    /// Texts come from the current language: keep other tests from switching
    /// it in the middle of one of these.
    fn same_language() -> impl Drop {
        crate::features::i18n::test_support::lang_guard()
    }

    fn paths() -> (PathBuf, PathBuf) {
        (PathBuf::from(r"C:\Users\x\AppData\Local\LockNote\0123456789abcdef.tmp"), PathBuf::from(r"X:\notes\LockNote.exe"))
    }

    #[test]
    fn fill_replaces_each_name_once() {
        assert_eq!(fill("{a} and {b}", &[("a", "1"), ("b", "2")]), "1 and 2");
        assert_eq!(fill("{a}", &[("a", "{b}"), ("b", "no")]), "{b}", "a value is not filled again");
        assert_eq!(fill("{unknown} {a", &[("a", "1")]), "{unknown} {a");
        assert_eq!(fill("{a}{a}", &[("a", "x")]), "xx");
        assert_eq!(fill("", &[]), "");
    }

    // Refuter A7: a folder named like a placeholder is shown as it is.
    #[test]
    fn details_show_paths_as_they_are() {
        let _lang = same_language();
        let exe = PathBuf::from(r"D:\{date}\{size}\LockNote.exe");
        let e = StagedEntry {
            hash: "0123456789abcdef".into(),
            tmp: PathBuf::from(r"C:\x\0123456789abcdef.tmp"),
            size: Some(10),
            modified: Some(SystemTime::UNIX_EPOCH),
            owner: Owner::Orphan(exe.clone()),
        };
        let d = details_text(&e);
        assert!(d.contains(&exe.display().to_string()), "{}", d);
        assert!(!d.replace("\r\n", "").contains('\n'));
    }

    #[test]
    fn unknown_size_and_date_are_marked() {
        assert_eq!(format_size(None), "?");
        assert_eq!(format_date(None), "?");
        let before_1970 = SystemTime::UNIX_EPOCH - std::time::Duration::from_secs(10);
        assert_eq!(format_date(Some(before_1970)), "?");
        assert_eq!(format_size(Some(3 * 1024 * 1024)), "3.0 MiB");
    }

    #[test]
    fn a_pending_save_can_be_written_by_closing() {
        let _lang = same_language();
        let (tmp, exe) = paths();
        let (text, can_close) = apply_now_text(&PendingDecision::Promote, &tmp, Some(3 * 1024 * 1024), &exe);
        assert!(can_close);
        assert!(text.contains(&tmp.display().to_string()), "{}", text);
        assert!(text.contains(&exe.display().to_string()), "{}", text);
        assert!(text.contains("3.0 MiB"), "{}", text);
    }

    #[test]
    fn nothing_pending_cannot_be_applied() {
        let _lang = same_language();
        let (tmp, exe) = paths();
        for d in [PendingDecision::None, PendingDecision::Stale] {
            let (text, can_close) = apply_now_text(&d, &tmp, None, &exe);
            assert!(!can_close, "{:?}", d);
            assert!(text.contains(&exe.display().to_string()), "{}", text);
        }
    }

    #[test]
    fn undecided_states_wait_for_the_next_start() {
        let _lang = same_language();
        let (tmp, exe) = paths();
        let now = SystemTime::now();
        let undecided = [
            PendingDecision::Conflict {
                tmp_modified: now,
                exe_modified: now,
                exe_has_data: true,
                cause: crate::storage::ConflictCause::ExeChanged,
            },
            PendingDecision::TmpUnreadable("held".into()),
            PendingDecision::ExeUnreadable("held".into()),
            PendingDecision::Ignored(crate::storage::IgnoreReason::NoMarker),
        ];
        let promote_text = apply_now_text(&PendingDecision::Promote, &tmp, Some(10), &exe).0;
        let none_text = apply_now_text(&PendingDecision::None, &tmp, None, &exe).0;
        for d in undecided {
            let (text, can_close) = apply_now_text(&d, &tmp, Some(10), &exe);
            assert!(!can_close, "{:?}", d);
            assert!(text != promote_text && text != none_text, "{:?}: {}", d, text);
            assert!(text.contains(&tmp.display().to_string()), "{}", text);
            assert!(!text.contains("{reason}"), "{}", text);
        }
    }

    // The worker thread's reading, on a real folder: a deletion asked of a
    // file that is not an orphan is reported, and the list is still read.
    #[test]
    fn snapshot_reports_a_refused_deletion_and_the_fresh_list() {
        let root = std::env::temp_dir().join(format!("locknote_tempfiles_snapshot_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let exe = root.join("open.exe");
        std::fs::write(root.join("0123456789abcdef.tmp"), b"pending").unwrap();
        let snap = read_snapshot(&root, &exe, Some("0123456789abcdef"));
        assert!(matches!(snap.deleted, Some(Err(_))), "no companion: not an orphan");
        let list = snap.list.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].owner, Owner::NoCompanion);
        assert_eq!(snap.decision, PendingDecision::None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn every_owner_has_its_own_label() {
        let _lang = same_language();
        let p = PathBuf::from(r"D:\x.exe");
        let owners = [
            Owner::Current,
            Owner::ExePresent(p.clone()),
            Owner::Orphan(p.clone()),
            Owner::FolderMissing(p.clone()),
            Owner::OtherVolume(p.clone()),
            Owner::Unreachable(p.clone(), "denied".into()),
            Owner::NoCompanion,
            Owner::CompanionUnusable("bad".into()),
            Owner::TmpUnreadable("busy".into()),
        ];
        let labels: Vec<String> = owners.iter().map(owner_label).collect();
        for (o, l) in owners.iter().zip(&labels) {
            assert!(!l.trim().is_empty(), "{:?}", o);
        }
        let mut unique = labels.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), labels.len(), "{:?}", labels);
        let meanings: Vec<String> = owners.iter().map(owner_meaning).collect();
        let mut unique = meanings.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), meanings.len(), "{:?}", meanings);
    }
}
