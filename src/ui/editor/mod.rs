// EditorForm — Main editor window with menus, status bar, and tray icon
//
// Layout (top to bottom):
//   1. MenuStrip (File, View, Edit, Help, Tab)
//   2. RichTextBox (main editor area)
//   3. StatusBar (word count, character count, line count)
//
// Find/Replace is a popup dialog (ui::dialogs::find_replace).

use native_windows_gui as nwg;
use std::cell::RefCell;
use std::cell::Cell;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use zeroize::Zeroizing;


/// The text of an i18n key in the current language.
fn t(key: &str) -> String {
    crate::features::i18n::get(key).to_string()
}

/// Main editor window holding all NWG controls and application state.
pub struct EditorForm {
    // ── Window ──
    pub window: nwg::Window,
    /// Saved placement was maximized: `run` shows the window maximized.
    start_maximized: bool,

    // ── Menu: File ──
    menu_file: nwg::Menu,
    menu_file_save: nwg::MenuItem,
    menu_file_sep1: nwg::MenuSeparator,
    menu_file_print: nwg::MenuItem,
    menu_file_export: nwg::MenuItem,
    menu_file_backup_export: nwg::MenuItem,
    menu_file_backup_import: nwg::MenuItem,
    menu_file_import_exe: nwg::MenuItem,
    menu_file_backups: nwg::MenuItem,
    menu_file_sep2: nwg::MenuSeparator,
    // Settings submenu
    menu_file_settings: nwg::Menu,
    menu_file_settings_preferences: nwg::MenuItem,
    menu_file_settings_security: nwg::MenuItem,
    menu_file_settings_tempfiles: nwg::MenuItem,
    menu_file_sep3: nwg::MenuSeparator,
    menu_file_quit: nwg::MenuItem,

    // ── Menu: Edit ──
    menu_edit: nwg::Menu,
    menu_edit_cut: nwg::MenuItem,
    menu_edit_copy: nwg::MenuItem,
    menu_edit_paste: nwg::MenuItem,
    menu_edit_paste_plain: nwg::MenuItem,
    menu_edit_sep1: nwg::MenuSeparator,
    menu_edit_find: nwg::MenuItem,
    menu_edit_replace: nwg::MenuItem,
    menu_edit_goto_line: nwg::MenuItem,
    menu_edit_sep2: nwg::MenuSeparator,
    menu_edit_duplicate_line: nwg::MenuItem,
    menu_edit_delete_line: nwg::MenuItem,
    menu_edit_select_all: nwg::MenuItem,
    menu_edit_insert_timestamp: nwg::MenuItem,

    // ── Menu: View ──
    menu_view: nwg::Menu,
    menu_view_always_on_top: nwg::MenuItem,
    menu_view_word_wrap: nwg::MenuItem,

    // ── Menu: Tab ──
    menu_tab: nwg::Menu,
    menu_tab_new: nwg::MenuItem,
    menu_tab_close: nwg::MenuItem,
    menu_tab_rename: nwg::MenuItem,
    menu_tab_next: nwg::MenuItem,
    menu_tab_prev: nwg::MenuItem,

    // ── Menu: Tools ──
    menu_tools: nwg::Menu,
    menu_tools_vault: nwg::MenuItem,
    menu_tools_prompts: nwg::MenuItem,
    menu_tools_totp: nwg::MenuItem,
    menu_tools_attachments: nwg::MenuItem,
    menu_tools_passgen: nwg::MenuItem,
    // ── Toolbar toggle buttons ──
    toolbar_prompts: nwg::Button,
    toolbar_vault: nwg::Button,
    toolbar_totp: nwg::Button,
    toolbar_attach: nwg::Button,
    toolbar_lock: nwg::Button,
    menu_tools_stego: nwg::Menu,
    menu_tools_stego_hide: nwg::MenuItem,
    menu_tools_stego_extract: nwg::MenuItem,
    menu_tools_ssh: nwg::Menu,
    menu_tools_ssh_generate: nwg::MenuItem,
    menu_tools_ssh_agent: nwg::MenuItem,

    // ── Menu: Help ──
    menu_help: nwg::Menu,
    menu_help_updates: nwg::MenuItem,
    menu_help_about: nwg::MenuItem,

    // ── Context menu (right-click on editor) ──
    context_menu: nwg::Menu,
    ctx_cut: nwg::MenuItem,
    ctx_copy: nwg::MenuItem,
    ctx_paste: nwg::MenuItem,
    ctx_paste_plain: nwg::MenuItem,
    ctx_sep1: nwg::MenuSeparator,
    ctx_select_all: nwg::MenuItem,

    // ── Line number gutter ──
    gutter_hwnd: RefCell<isize>,
    /// Caret line last painted in the gutter (see `repaint_gutter_if_caret_moved`).
    gutter_caret_line: RefCell<usize>,
    /// A double-click reached the editor; the word widening runs on the
    /// button release that ends it, once RichEdit's own word-select is final.
    dblclick_armed: RefCell<bool>,

    // ── Editor ──
    pub editor: nwg::RichTextBox,

    // ── Editor font ──
    editor_font: nwg::Font,

    // ── Status bar ──
    status_bar: nwg::StatusBar,
    /// Where the path of the LockNote file was last painted (click target).
    status_path_slot: Cell<Option<content::PathSlot>>,
    /// Text shown instead of the path for a moment after a click
    /// (`status.path_copied` or `status.path_copy_failed`).
    status_path_feedback: RefCell<Option<String>>,
    /// The left button went down on the path (a click needs both ends on it).
    status_path_pressed: Cell<bool>,
    /// A modal dialog with its own message loop is open (Temporary files):
    /// `on_close` refuses meanwhile.
    modal_dialog_open: Cell<bool>,

    // ── Tray icon ──
    tray_icon: nwg::TrayNotification,
    tray_menu: nwg::Menu,
    tray_menu_restore: nwg::MenuItem,
    tray_menu_quit: nwg::MenuItem,


    // __ Vault panel __
    vault_panel: super::vault_panel::VaultPanel,
    vault_frame: nwg::Frame,
    /// Filter on the secret names (case- and accent-insensitive, never the
    /// values), above the tree.
    vault_search: nwg::TextInput,
    /// One-level tree: categories → secret names (never values).
    vault_tree: nwg::TreeView,
    /// Read-only field showing a revealed value for 5 s (issue 048, F27).
    vault_reveal: nwg::TextInput,
    /// "Copied: <name>" indicator under the tree, shown 2 s after a copy.
    vault_copied: nwg::Label,
    /// Set while the tree is rebuilt: TVN_SELCHANGED fires synchronously
    /// during clear()/insert and must be ignored (F30).
    suppress_tree_events: Cell<bool>,
    /// Storage index of the secret being dragged in the tree (issue 053).
    vault_drag: Cell<Option<usize>>,
    vault_btn_add: nwg::Button,
    vault_btn_edit: nwg::Button,
    vault_btn_delete: nwg::Button,
    vault_btn_copy: nwg::Button,
    vault_btn_reveal: nwg::Button,
    vault_timer: nwg::AnimationTimer,
    copy_feedback_time: Cell<Option<std::time::Instant>>,

    // __ Prompts panel (issue 058) __
    prompts_panel: super::prompts_panel::PromptsPanel,
    prompts_frame: nwg::Frame,
    /// Filter on the name and the text (case- and accent-insensitive).
    prompts_search: nwg::TextInput,
    /// "Category · Name", sorted by category then name.
    prompts_list: nwg::ListBox<String>,
    /// Read-only text of the selected prompt.
    prompts_preview: nwg::TextBox,
    prompts_btn_add: nwg::Button,
    prompts_btn_edit: nwg::Button,
    prompts_btn_delete: nwg::Button,
    prompts_btn_copy: nwg::Button,
    /// Count, "Copied: <name>", or why the list cannot be shown.
    prompts_status: nwg::Label,

    // __ TOTP panel __
    totp_panel: super::totp_panel::TotpPanel,
    totp_frame: nwg::Frame,
    totp_list: nwg::ListBox<String>,
    totp_lbl_countdown: nwg::Label,
    totp_btn_copy: nwg::Button,
    totp_btn_add: nwg::Button,
    totp_btn_remove: nwg::Button,
    totp_lbl_status: nwg::Label,
    totp_timer: nwg::AnimationTimer,

    // __ Attachments panel __
    attach_panel: super::attachments_panel::AttachmentsPanel,
    attach_frame: nwg::Frame,
    attach_list: nwg::ListBox<String>,
    attach_btn_add: nwg::Button,
    attach_btn_open: nwg::Button,
    attach_btn_save: nwg::Button,
    attach_btn_delete: nwg::Button,
    attach_lbl_status: nwg::Label,

    // ── Occurrence highlighting ──
    highlight_timer: nwg::AnimationTimer,
    last_highlighted: RefCell<String>,
    suppress_change: RefCell<bool>,

    // ── Feature state ──
    autolock: RefCell<crate::features::autolock::AutoLock>,
    autolock_timer: nwg::AnimationTimer,
    autosave: RefCell<crate::features::autosave::AutoSave>,
    autosave_timer: nwg::AnimationTimer,
    clipboard_guard: RefCell<crate::features::clipboard_guard::ClipboardGuard>,
    clipboard_timer: nwg::AnimationTimer,
    global_hotkey: RefCell<Option<crate::features::hotkey::GlobalHotkey>>,
    detected_encoding: RefCell<String>,
    tab_bar: RefCell<super::tab_bar::TabBar>,
    tab_bar_hwnd: RefCell<isize>,
    /// Resize grip of the open side panel (0 = not created), see panel_grip.rs.
    panel_grip_hwnd: isize,
    /// Drag of the side panel grip in progress.
    panel_drag: Cell<Option<panel_grip::PanelDrag>>,
    right_clicked_tab: RefCell<Option<usize>>,
    /// Tab pressed with the left button and the x of the press, until the
    /// button is released (drag & drop reordering).
    tab_drag: Cell<Option<(usize, i32)>>,
    /// Tab the dragged one would land on right now (drives the insertion
    /// mark); `None` when not dragging or not over a valid target.
    tab_drag_over: Cell<Option<usize>>,

    // ── Lock overlay ──
    lock_frame: nwg::Frame,
    lock_label: nwg::Label,
    /// Prompt under the title ("Enter your password to unlock your notes.").
    lock_subtitle: nwg::Label,
    lock_input: nwg::TextInput,
    /// Show/hide (eye) button inside the password field.
    lock_eye: nwg::Button,
    lock_btn: nwg::Button,
    lock_status: nwg::Label,
    /// Text/tone of `lock_status` (wrong password, retry delay, Caps Lock
    /// hint); every write goes through it, never through `lock_status`.
    lock_status_msg: crate::ui::lock_card::StatusMsg,
    /// Fonts of the lock screen for the DPI they were built for.
    lock_fonts: RefCell<Option<(u32, crate::ui::lock_card::CardFonts)>>,
    lock_attempts: RefCell<u32>,
    is_locked: RefCell<bool>,
    /// Active tab + caret line captured when locking (not a secret), put
    /// back after unlock. Taken only once decryption succeeded.
    locked_session_state: Cell<Option<crate::features::session_state::SessionState>>,
    lock_backoff_timer: nwg::AnimationTimer,
    lock_backoff_remaining: Cell<u32>,
    /// HWNDs of main-window siblings we hid when showing the overlay, so
    /// they can be restored on unlock. The overlay masks them completely;
    /// hiding them is the only reliable way to stop them from repainting
    /// on top of our backdrop (WS_CLIPSIBLINGS alone wasn't enough — the
    /// RichEdit kept repainting through on cursor blinks and focus events).
    lock_hidden_siblings: RefCell<Vec<isize>>,

    // ── Application state ──
    // Wrapped in Zeroizing so the buffer is wiped when the RefCell is
    // replaced or dropped, and when `.zeroize()` is called at auto-lock.
    password: RefCell<Zeroizing<String>>,
    is_modified: Arc<AtomicBool>,
    exe_path: PathBuf,
    settings: RefCell<crate::settings::Settings>,
    always_on_top: RefCell<bool>,
    attachments: RefCell<Vec<crate::features::attachments::Attachment>>,
    /// Public lines of the SSH keys this file loaded into ssh-agent and has
    /// not removed yet (issue 054). Persisted in the session state.
    ssh_agent_keys: RefCell<Vec<String>>,
    /// Keys of `ssh_agent_keys` whose add timed out in this session (the
    /// request may still land): kept while absent. Not persisted: the next
    /// run sees them for what they are by then.
    ssh_agent_unconfirmed: RefCell<Vec<String>>,
}

impl EditorForm {

    /// Build and run the editor window. This blocks until the window is closed.
    pub fn run(
        exe_path: PathBuf,
        password: Zeroizing<String>,
        content: String,
        settings: crate::settings::Settings,
    ) {
        // Initialize i18n language BEFORE building UI so menus use the correct language
        {
            let i18n_lang = match settings.language {
                crate::settings::Language::En => crate::features::i18n::Language::English,
                crate::settings::Language::Fr => crate::features::i18n::Language::French,
                crate::settings::Language::Es => crate::features::i18n::Language::Spanish,
                crate::settings::Language::De => crate::features::i18n::Language::German,
            };
            crate::features::i18n::set_language(i18n_lang);
        }

        let form = Self::build(exe_path, password, settings);
        // Split the payload into its blocks and restore the saved session
        // (single pipeline shared with the unlock paths, see load_payload).
        let (content, cursor_line) = form.load_payload(&content, None);

        form.set_content(&content);
        form.update_status_bar();
        form.update_title();

        if let Some(line) = cursor_line {
            form.apply_cursor_line(line);
        }

        // Apply initial word wrap setting
        let wrap = form.settings.borrow().word_wrap;
        form.apply_word_wrap(wrap);

        // Show the window (built hidden, placement already applied)
        if let nwg::ControlHandle::Hwnd(hwnd) = form.window.handle {
            use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_SHOWMAXIMIZED, SW_SHOWNORMAL};
            let cmd = if form.start_maximized { SW_SHOWMAXIMIZED } else { SW_SHOWNORMAL };
            unsafe {
                let _ = ShowWindow(windows::Win32::Foundation::HWND(hwnd as *mut _), cmd);
            }
        }

        // Post a deferred gutter repaint so line numbers render after the
        // message loop has started and the RichEdit has finished its layout.
        if let nwg::ControlHandle::Hwnd(hwnd) = form.window.handle {
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                    Some(windows::Win32::Foundation::HWND(hwnd as *mut _)),
                    0x8003, // WM_APP+3: deferred gutter repaint
                    windows::Win32::Foundation::WPARAM(0),
                    windows::Win32::Foundation::LPARAM(0),
                );
            }
        }

        // Register global hotkey
        {
            let s = form.settings.borrow();
            let hk = crate::features::hotkey::GlobalHotkey::from_settings(s.hotkey_modifiers, s.hotkey_key);
            if hk.is_valid() {
                let hwnd = form.window.handle.hwnd().unwrap() as isize;
                if hk.register(hwnd) {
                    *form.global_hotkey.borrow_mut() = Some(hk);
                }
            }
        }

        // Bind events
        let form = std::rc::Rc::new(form);

        // Fix #035: register the dirty-state callback on AutoLock so that
        // `lock_decision()` can choose between `LockNow` and `SaveThenLock`.
        // The callback only reads the Arc<AtomicBool> flag, which is
        // `Send + Sync`, so the closure satisfies the trait bounds.
        {
            let dirty = Arc::clone(&form.is_modified);
            form.autolock.borrow_mut().set_dirty_callback(Box::new(move || {
                dirty.load(Ordering::Relaxed)
            }));
        }

        Self::bind_events(form.clone());

        // The vault panel is open at launch (the timer tick is bound by
        // bind_events, so the panel is opened after it).
        if !form.vault_panel.is_visible() {
            form.on_toggle_vault();
        }

        // Keys left in ssh-agent by an abnormal end of the previous run.
        form.unload_agent_keys(true);

        // NWG dispatch thread
        nwg::dispatch_thread_events();

        // Unregister global hotkey on exit
        {
            let hk_opt = form.global_hotkey.borrow().clone();
            if let Some(hk) = hk_opt {
                let hwnd = form.window.handle.hwnd().unwrap() as isize;
                hk.unregister(hwnd);
            }
        }
    }
}

// The implementation of `EditorForm` is split by feature; every module adds an
// `impl EditorForm` block. Private items stay reachable from the submodules.
mod build;
mod events;
mod content;
mod tab_paint;
mod layout;
mod file_ops;
mod backup_ops;
mod stego_ops;
mod print_ops;
mod prefs_ops;
mod edit_ops;
mod help_ops;
mod vault_ops;
mod prompts_ops;
mod ssh_ops;
mod attach_ops;
mod highlight;
mod close_flow;
mod tab_ops;
mod lock_ops;
mod runtime_ops;
mod lock_overlay;
mod helpers;
mod surface;
mod theme_switch;
mod toolbar;
mod panel_grip;

#[allow(unused_imports)]
use self::helpers::*;
// The startup conflict dialog (ui/mod.rs) shows dates in the same format.
pub(in crate::ui) use self::helpers::format_timestamp;
#[allow(unused_imports)]
use self::content::*;
#[allow(unused_imports)]
use self::lock_overlay::*;

#[cfg(test)]
mod tests;

/// The source of every file of this module, joined, for tests that check the
/// code as text (see `features::payload` and `ui::window_geometry`). Reads the
/// directory, so a new module is covered without being listed.
#[cfg(test)]
pub(crate) fn all_source() -> String {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("ui").join("editor");
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
        .expect("read src/ui/editor")
        .map(|e| e.expect("directory entry").path())
        .filter(|p| p.extension().map_or(false, |e| e == "rs"))
        .collect();
    files.sort();
    files
        .iter()
        .map(|p| std::fs::read_to_string(p).expect("read an editor source file"))
        .collect::<Vec<String>>()
        .join("\n")
}
