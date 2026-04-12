// EditorForm — Main editor window with menus, status bar, search bar, and tray icon
//
// Layout (top to bottom):
//   1. MenuStrip (File, View, Edit, Help, + Tab for Pro)
//   2. SearchBar (hidden by default, toggled via Ctrl+F / Ctrl+H)
//   3. RichTextBox (main editor area)
//   4. StatusBar (word count, character count, line count)

use native_windows_gui as nwg;
use std::cell::RefCell;
use std::path::PathBuf;

use super::search_bar::SearchBar;

/// Main editor window holding all NWG controls and application state.
pub struct EditorForm {
    // ── Window ──
    pub window: nwg::Window,

    // ── Menu: File ──
    menu_file: nwg::Menu,
    menu_file_save: nwg::MenuItem,
    menu_file_change_password: nwg::MenuItem,
    #[cfg(feature = "pro")]
    menu_file_print: nwg::MenuItem,
    #[cfg(feature = "pro")]
    menu_file_export: nwg::MenuItem,
    menu_file_settings: nwg::MenuItem,
    menu_file_sep1: nwg::MenuSeparator,
    menu_file_quit: nwg::MenuItem,

    // ── Menu: View ──
    menu_view: nwg::Menu,
    menu_view_always_on_top: nwg::MenuItem,
    menu_view_word_wrap: nwg::MenuItem,
    #[cfg(feature = "vault")]
    menu_view_vault: nwg::MenuItem,

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

    // ── Menu: Tab (Pro) ──
    #[cfg(feature = "tabs")]
    menu_tab: nwg::Menu,
    #[cfg(feature = "tabs")]
    menu_tab_new: nwg::MenuItem,
    #[cfg(feature = "tabs")]
    menu_tab_close: nwg::MenuItem,
    #[cfg(feature = "tabs")]
    menu_tab_rename: nwg::MenuItem,
    #[cfg(feature = "tabs")]
    menu_tab_next: nwg::MenuItem,
    #[cfg(feature = "tabs")]
    menu_tab_prev: nwg::MenuItem,

    // ── Menu: Help ──
    menu_help: nwg::Menu,
    menu_help_updates: nwg::MenuItem,
    menu_help_about: nwg::MenuItem,

    // ── Editor ──
    pub editor: nwg::RichTextBox,

    // ── Editor font ──
    editor_font: nwg::Font,

    // ── Status bar ──
    status_bar: nwg::StatusBar,

    // ── Tray icon ──
    tray_icon: nwg::TrayNotification,
    tray_menu: nwg::Menu,
    tray_menu_restore: nwg::MenuItem,
    tray_menu_quit: nwg::MenuItem,

    // ── Search bar (managed separately) ──
    search_bar: RefCell<Option<SearchBar>>,

    // ── Application state ──
    password: RefCell<String>,
    is_modified: RefCell<bool>,
    exe_path: PathBuf,
    has_pending_tmp: RefCell<bool>,
    settings: RefCell<crate::settings::Settings>,
    always_on_top: RefCell<bool>,
}

impl EditorForm {
    /// Build and run the editor window. This blocks until the window is closed.
    pub fn run(
        exe_path: PathBuf,
        password: String,
        content: String,
        settings: crate::settings::Settings,
    ) {
        let form = Self::build(exe_path, password, settings);
        form.set_content(&content);
        form.update_status_bar();
        form.update_title();

        // Show the window
        form.window.set_visible(true);

        // Bind events
        let form = std::rc::Rc::new(form);
        Self::bind_events(form.clone());

        // NWG dispatch thread
        nwg::dispatch_thread_events();
    }

    /// Construct all NWG controls via builders.
    fn build(
        exe_path: PathBuf,
        password: String,
        settings: crate::settings::Settings,
    ) -> Self {
        // ── Window ──
        let mut window = nwg::Window::default();
        nwg::Window::builder()
            .title("LockNote")
            .size((900, 650))
            .position((300, 200))
            .flags(
                nwg::WindowFlags::WINDOW
                    | nwg::WindowFlags::VISIBLE
                    | nwg::WindowFlags::RESIZABLE
                    | nwg::WindowFlags::MAXIMIZE_BOX
                    | nwg::WindowFlags::MINIMIZE_BOX,
            )
            .build(&mut window)
            .expect("Failed to build main window");

        // ── Menu: File ──
        let mut menu_file = nwg::Menu::default();
        nwg::Menu::builder()
            .text("&File")
            .parent(&window)
            .build(&mut menu_file)
            .expect("Failed to build File menu");

        let mut menu_file_save = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text("&Save\tCtrl+S")
            .parent(&menu_file)
            .build(&mut menu_file_save)
            .expect("Failed to build Save menu item");

        let mut menu_file_change_password = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text("Change &Password...")
            .parent(&menu_file)
            .build(&mut menu_file_change_password)
            .expect("Failed to build Change Password menu item");

        #[cfg(feature = "pro")]
        let mut menu_file_print = nwg::MenuItem::default();
        #[cfg(feature = "pro")]
        nwg::MenuItem::builder()
            .text("&Print...\tCtrl+P")
            .parent(&menu_file)
            .build(&mut menu_file_print)
            .expect("Failed to build Print menu item");

        #[cfg(feature = "pro")]
        let mut menu_file_export = nwg::MenuItem::default();
        #[cfg(feature = "pro")]
        nwg::MenuItem::builder()
            .text("&Export...")
            .parent(&menu_file)
            .build(&mut menu_file_export)
            .expect("Failed to build Export menu item");

        let mut menu_file_settings = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text("S&ettings...")
            .parent(&menu_file)
            .build(&mut menu_file_settings)
            .expect("Failed to build Settings menu item");

        let mut menu_file_sep1 = nwg::MenuSeparator::default();
        nwg::MenuSeparator::builder()
            .parent(&menu_file)
            .build(&mut menu_file_sep1)
            .expect("Failed to build separator");

        let mut menu_file_quit = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text("&Quit\tCtrl+Q")
            .parent(&menu_file)
            .build(&mut menu_file_quit)
            .expect("Failed to build Quit menu item");

        // ── Menu: View ──
        let mut menu_view = nwg::Menu::default();
        nwg::Menu::builder()
            .text("&View")
            .parent(&window)
            .build(&mut menu_view)
            .expect("Failed to build View menu");

        let mut menu_view_always_on_top = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text("Always on &Top")
            .parent(&menu_view)
            .build(&mut menu_view_always_on_top)
            .expect("Failed to build Always on Top menu item");

        let mut menu_view_word_wrap = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text("&Word Wrap")
            .parent(&menu_view)
            .check(settings.word_wrap)
            .build(&mut menu_view_word_wrap)
            .expect("Failed to build Word Wrap menu item");

        #[cfg(feature = "vault")]
        let mut menu_view_vault = nwg::MenuItem::default();
        #[cfg(feature = "vault")]
        nwg::MenuItem::builder()
            .text("&Vault\tCtrl+B")
            .parent(&menu_view)
            .build(&mut menu_view_vault)
            .expect("Failed to build Vault menu item");

        // ── Menu: Edit ──
        let mut menu_edit = nwg::Menu::default();
        nwg::Menu::builder()
            .text("&Edit")
            .parent(&window)
            .build(&mut menu_edit)
            .expect("Failed to build Edit menu");

        let mut menu_edit_cut = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text("Cu&t\tCtrl+X")
            .parent(&menu_edit)
            .build(&mut menu_edit_cut)
            .expect("Failed to build Cut menu item");

        let mut menu_edit_copy = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text("&Copy\tCtrl+C")
            .parent(&menu_edit)
            .build(&mut menu_edit_copy)
            .expect("Failed to build Copy menu item");

        let mut menu_edit_paste = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text("&Paste\tCtrl+V")
            .parent(&menu_edit)
            .build(&mut menu_edit_paste)
            .expect("Failed to build Paste menu item");

        let mut menu_edit_paste_plain = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text("Paste Plain &Text\tCtrl+Shift+V")
            .parent(&menu_edit)
            .build(&mut menu_edit_paste_plain)
            .expect("Failed to build Paste Plain Text menu item");

        let mut menu_edit_sep1 = nwg::MenuSeparator::default();
        nwg::MenuSeparator::builder()
            .parent(&menu_edit)
            .build(&mut menu_edit_sep1)
            .expect("Failed to build separator");

        let mut menu_edit_find = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text("&Find\tCtrl+F")
            .parent(&menu_edit)
            .build(&mut menu_edit_find)
            .expect("Failed to build Find menu item");

        let mut menu_edit_replace = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text("Find && &Replace\tCtrl+H")
            .parent(&menu_edit)
            .build(&mut menu_edit_replace)
            .expect("Failed to build Find & Replace menu item");

        let mut menu_edit_goto_line = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text("&Go to Line\tCtrl+G")
            .parent(&menu_edit)
            .build(&mut menu_edit_goto_line)
            .expect("Failed to build Go to Line menu item");

        let mut menu_edit_sep2 = nwg::MenuSeparator::default();
        nwg::MenuSeparator::builder()
            .parent(&menu_edit)
            .build(&mut menu_edit_sep2)
            .expect("Failed to build separator");

        let mut menu_edit_duplicate_line = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text("&Duplicate Line\tCtrl+D")
            .parent(&menu_edit)
            .build(&mut menu_edit_duplicate_line)
            .expect("Failed to build Duplicate Line menu item");

        let mut menu_edit_delete_line = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text("De&lete Line\tCtrl+Shift+K")
            .parent(&menu_edit)
            .build(&mut menu_edit_delete_line)
            .expect("Failed to build Delete Line menu item");

        let mut menu_edit_select_all = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text("Select &All\tCtrl+A")
            .parent(&menu_edit)
            .build(&mut menu_edit_select_all)
            .expect("Failed to build Select All menu item");

        let mut menu_edit_insert_timestamp = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text("Insert &Timestamp\tF5")
            .parent(&menu_edit)
            .build(&mut menu_edit_insert_timestamp)
            .expect("Failed to build Insert Timestamp menu item");

        // ── Menu: Tab (Pro) ──
        #[cfg(feature = "tabs")]
        let mut menu_tab = nwg::Menu::default();
        #[cfg(feature = "tabs")]
        nwg::Menu::builder()
            .text("&Tab")
            .parent(&window)
            .build(&mut menu_tab)
            .expect("Failed to build Tab menu");

        #[cfg(feature = "tabs")]
        let mut menu_tab_new = nwg::MenuItem::default();
        #[cfg(feature = "tabs")]
        nwg::MenuItem::builder()
            .text("&New Tab\tCtrl+T")
            .parent(&menu_tab)
            .build(&mut menu_tab_new)
            .expect("Failed to build New Tab menu item");

        #[cfg(feature = "tabs")]
        let mut menu_tab_close = nwg::MenuItem::default();
        #[cfg(feature = "tabs")]
        nwg::MenuItem::builder()
            .text("&Close Tab\tCtrl+W")
            .parent(&menu_tab)
            .build(&mut menu_tab_close)
            .expect("Failed to build Close Tab menu item");

        #[cfg(feature = "tabs")]
        let mut menu_tab_rename = nwg::MenuItem::default();
        #[cfg(feature = "tabs")]
        nwg::MenuItem::builder()
            .text("&Rename Tab...")
            .parent(&menu_tab)
            .build(&mut menu_tab_rename)
            .expect("Failed to build Rename Tab menu item");

        #[cfg(feature = "tabs")]
        let mut menu_tab_next = nwg::MenuItem::default();
        #[cfg(feature = "tabs")]
        nwg::MenuItem::builder()
            .text("Ne&xt Tab\tCtrl+Tab")
            .parent(&menu_tab)
            .build(&mut menu_tab_next)
            .expect("Failed to build Next Tab menu item");

        #[cfg(feature = "tabs")]
        let mut menu_tab_prev = nwg::MenuItem::default();
        #[cfg(feature = "tabs")]
        nwg::MenuItem::builder()
            .text("&Previous Tab\tCtrl+Shift+Tab")
            .parent(&menu_tab)
            .build(&mut menu_tab_prev)
            .expect("Failed to build Previous Tab menu item");

        // ── Menu: Help ──
        let mut menu_help = nwg::Menu::default();
        nwg::Menu::builder()
            .text("&Help")
            .parent(&window)
            .build(&mut menu_help)
            .expect("Failed to build Help menu");

        let mut menu_help_updates = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text("Check for &Updates...")
            .parent(&menu_help)
            .build(&mut menu_help_updates)
            .expect("Failed to build Check for Updates menu item");

        let mut menu_help_about = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text("&About LockNote...")
            .parent(&menu_help)
            .build(&mut menu_help_about)
            .expect("Failed to build About menu item");

        // ── Editor font ──
        let mut editor_font = nwg::Font::default();
        nwg::Font::builder()
            .family(&settings.font_family)
            .size(font_size_to_nwg(settings.font_size))
            .build(&mut editor_font)
            .expect("Failed to build editor font");

        // ── Editor (RichTextBox) ──
        let mut editor = nwg::RichTextBox::default();
        nwg::RichTextBox::builder()
            .parent(&window)
            .font(Some(&editor_font))
            .focus(true)
            .build(&mut editor)
            .expect("Failed to build editor RichTextBox");

        // ── Status bar ──
        let mut status_bar = nwg::StatusBar::default();
        nwg::StatusBar::builder()
            .parent(&window)
            .build(&mut status_bar)
            .expect("Failed to build status bar");

        // ── Tray icon ──
        let mut tray_icon_res = nwg::Icon::default();
        nwg::Icon::builder()
            .source_system(Some(nwg::OemIcon::Information))
            .build(&mut tray_icon_res)
            .expect("Failed to build tray icon resource");

        let mut tray_icon = nwg::TrayNotification::default();
        nwg::TrayNotification::builder()
            .parent(&window)
            .icon(Some(&tray_icon_res))
            .tip(Some("LockNote"))
            .build(&mut tray_icon)
            .expect("Failed to build tray icon");

        let mut tray_menu = nwg::Menu::default();
        nwg::Menu::builder()
            .parent(&window)
            .popup(true)
            .build(&mut tray_menu)
            .expect("Failed to build tray menu");

        let mut tray_menu_restore = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text("&Restore")
            .parent(&tray_menu)
            .build(&mut tray_menu_restore)
            .expect("Failed to build tray Restore item");

        let mut tray_menu_quit = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text("&Quit")
            .parent(&tray_menu)
            .build(&mut tray_menu_quit)
            .expect("Failed to build tray Quit item");

        EditorForm {
            window,
            // File menu
            menu_file,
            menu_file_save,
            menu_file_change_password,
            #[cfg(feature = "pro")]
            menu_file_print,
            #[cfg(feature = "pro")]
            menu_file_export,
            menu_file_settings,
            menu_file_sep1,
            menu_file_quit,
            // View menu
            menu_view,
            menu_view_always_on_top,
            menu_view_word_wrap,
            #[cfg(feature = "vault")]
            menu_view_vault,
            // Edit menu
            menu_edit,
            menu_edit_cut,
            menu_edit_copy,
            menu_edit_paste,
            menu_edit_paste_plain,
            menu_edit_sep1,
            menu_edit_find,
            menu_edit_replace,
            menu_edit_goto_line,
            menu_edit_sep2,
            menu_edit_duplicate_line,
            menu_edit_delete_line,
            menu_edit_select_all,
            menu_edit_insert_timestamp,
            // Tab menu (Pro)
            #[cfg(feature = "tabs")]
            menu_tab,
            #[cfg(feature = "tabs")]
            menu_tab_new,
            #[cfg(feature = "tabs")]
            menu_tab_close,
            #[cfg(feature = "tabs")]
            menu_tab_rename,
            #[cfg(feature = "tabs")]
            menu_tab_next,
            #[cfg(feature = "tabs")]
            menu_tab_prev,
            // Help menu
            menu_help,
            menu_help_updates,
            menu_help_about,
            // Editor
            editor,
            editor_font,
            // Status bar
            status_bar,
            // Tray
            tray_icon,
            tray_menu,
            tray_menu_restore,
            tray_menu_quit,
            // Search bar
            search_bar: RefCell::new(None),
            // State
            password: RefCell::new(password),
            is_modified: RefCell::new(false),
            exe_path,
            has_pending_tmp: RefCell::new(false),
            settings: RefCell::new(settings),
            always_on_top: RefCell::new(false),
        }
    }

    /// Bind all event handlers to the form controls.
    fn bind_events(form: std::rc::Rc<EditorForm>) {
        let f = form.clone();
        let handler = nwg::full_bind_event_handler(
            &form.window.handle,
            move |evt, _evt_data, handle| {
                use nwg::Event;
                match evt {
                    Event::OnWindowClose => {
                        f.on_close();
                    }
                    Event::OnMenuItemSelected => {
                        // File menu
                        if handle == f.menu_file_save.handle {
                            f.on_save();
                        } else if handle == f.menu_file_change_password.handle {
                            f.on_change_password();
                        } else if handle == f.menu_file_settings.handle {
                            f.on_settings();
                        } else if handle == f.menu_file_quit.handle {
                            f.on_close();
                        }
                        // View menu
                        else if handle == f.menu_view_always_on_top.handle {
                            f.on_toggle_always_on_top();
                        } else if handle == f.menu_view_word_wrap.handle {
                            f.on_toggle_word_wrap();
                        }
                        // Edit menu
                        else if handle == f.menu_edit_cut.handle {
                            f.on_cut();
                        } else if handle == f.menu_edit_copy.handle {
                            f.on_copy();
                        } else if handle == f.menu_edit_paste.handle {
                            f.on_paste();
                        } else if handle == f.menu_edit_paste_plain.handle {
                            f.on_paste_plain();
                        } else if handle == f.menu_edit_find.handle {
                            f.on_show_find();
                        } else if handle == f.menu_edit_replace.handle {
                            f.on_show_replace();
                        } else if handle == f.menu_edit_goto_line.handle {
                            f.on_goto_line();
                        } else if handle == f.menu_edit_duplicate_line.handle {
                            f.on_duplicate_line();
                        } else if handle == f.menu_edit_delete_line.handle {
                            f.on_delete_line();
                        } else if handle == f.menu_edit_select_all.handle {
                            f.on_select_all();
                        } else if handle == f.menu_edit_insert_timestamp.handle {
                            f.on_insert_timestamp();
                        }
                        // Help menu
                        else if handle == f.menu_help_updates.handle {
                            f.on_check_updates();
                        } else if handle == f.menu_help_about.handle {
                            f.on_about();
                        }
                        // Tray menu
                        else if handle == f.tray_menu_restore.handle {
                            f.on_tray_restore();
                        } else if handle == f.tray_menu_quit.handle {
                            f.on_close();
                        }
                    }
                    Event::OnTextInput => {
                        if handle == f.editor.handle {
                            f.mark_modified();
                            f.update_status_bar();
                        }
                    }
                    Event::OnMinMaxInfo => {
                        // Set minimum window size
                    }
                    Event::OnResize | Event::OnResizeEnd => {
                        f.layout_controls();
                    }
                    // Keyboard shortcuts are handled in raw event handler below
                    _ => {}
                }
            },
        );

        // Keep the handler alive for the lifetime of the dispatch loop
        std::mem::forget(handler);

        // Bind keyboard accelerators via a raw event handler for Ctrl+key combos
        let f2 = form.clone();
        let raw_handler = nwg::bind_raw_event_handler(
            &form.window.handle,
            0x10000,
            move |_hwnd, msg, w_param, _l_param| {
                // WM_KEYDOWN = 0x0100
                if msg == 0x0100 {
                    let ctrl = unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(0x11) } < 0; // VK_CONTROL
                    let shift = unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(0x10) } < 0; // VK_SHIFT
                    let vk = w_param as u32;

                    if ctrl && !shift {
                        match vk {
                            0x53 => { f2.on_save(); return Some(0); }        // Ctrl+S
                            0x51 => { f2.on_close(); return Some(0); }       // Ctrl+Q
                            0x46 => { f2.on_show_find(); return Some(0); }   // Ctrl+F
                            0x48 => { f2.on_show_replace(); return Some(0); }// Ctrl+H
                            0x47 => { f2.on_goto_line(); return Some(0); }   // Ctrl+G
                            0x44 => { f2.on_duplicate_line(); return Some(0); } // Ctrl+D
                            0x41 => { f2.on_select_all(); return Some(0); }  // Ctrl+A
                            #[cfg(feature = "vault")]
                            0x42 => { /* Ctrl+B: vault */ return Some(0); }
                            #[cfg(feature = "tabs")]
                            0x54 => { /* Ctrl+T: new tab */ return Some(0); }
                            #[cfg(feature = "tabs")]
                            0x57 => { /* Ctrl+W: close tab */ return Some(0); }
                            _ => {}
                        }
                    } else if ctrl && shift {
                        match vk {
                            0x56 => { f2.on_paste_plain(); return Some(0); } // Ctrl+Shift+V
                            0x4B => { f2.on_delete_line(); return Some(0); } // Ctrl+Shift+K
                            _ => {}
                        }
                    } else if !ctrl && !shift {
                        match vk {
                            0x74 => { f2.on_insert_timestamp(); return Some(0); } // F5
                            0x1B => { f2.on_escape(); return Some(0); }           // Escape
                            _ => {}
                        }
                    }
                }
                None
            },
        );
        std::mem::forget(raw_handler);
    }

    // =====================================================================
    // Content management
    // =====================================================================

    /// Set the editor text content.
    fn set_content(&self, text: &str) {
        self.editor.set_text(text);
        *self.is_modified.borrow_mut() = false;
    }

    /// Get the current editor text content.
    fn get_content(&self) -> String {
        self.editor.text()
    }

    /// Mark the document as modified and update the title.
    fn mark_modified(&self) {
        *self.is_modified.borrow_mut() = true;
        self.update_title();
    }

    /// Mark the document as clean and update the title.
    fn mark_clean(&self) {
        *self.is_modified.borrow_mut() = false;
        self.update_title();
    }

    /// Update window title: "LockNote" or "LockNote *" if modified.
    fn update_title(&self) {
        let title = if *self.is_modified.borrow() {
            "LockNote *"
        } else {
            "LockNote"
        };
        self.window.set_text(title);
    }

    // =====================================================================
    // Status bar
    // =====================================================================

    /// Update the status bar with word/character/line counts.
    fn update_status_bar(&self) {
        let text = self.get_content();
        let char_count = text.len();
        let line_count = if text.is_empty() { 1 } else { text.lines().count().max(1) };
        let word_count = text.split_whitespace().count();

        let status_text = format!(
            "{} word{} | {} character{} | {} line{}",
            word_count,
            if word_count == 1 { "" } else { "s" },
            char_count,
            if char_count == 1 { "" } else { "s" },
            line_count,
            if line_count == 1 { "" } else { "s" },
        );
        self.status_bar.set_text(0, &status_text);
    }

    // =====================================================================
    // Layout
    // =====================================================================

    /// Reposition controls when the window is resized.
    fn layout_controls(&self) {
        let (w, h) = self.window.size();
        let status_h: u32 = 22;
        // Search bar height (if visible)
        let search_h: u32 = if self.search_bar.borrow().is_some() { 34 } else { 0 };

        let editor_y = search_h as i32;
        let editor_h = (h as i32) - (status_h as i32) - (search_h as i32);
        let editor_h = if editor_h < 0 { 0 } else { editor_h as u32 };

        self.editor.set_position(0, editor_y);
        self.editor.set_size(w, editor_h);
    }

    // =====================================================================
    // File operations
    // =====================================================================

    /// Save the current note.
    fn on_save(&self) {
        let note_text = self.get_content();
        let settings = self.settings.borrow();
        let combined = settings.serialize(&note_text);
        let password = self.password.borrow();

        let encrypted = crate::crypto::encrypt(&combined, &password);

        match crate::storage::write_data(&self.exe_path, &encrypted) {
            Ok(()) => {
                *self.has_pending_tmp.borrow_mut() = true;
                self.mark_clean();
            }
            Err(e) => {
                nwg::modal_info_message(
                    &self.window,
                    "Save Error",
                    &format!("Failed to save: {}", e),
                );
            }
        }
    }

    /// Change the master password.
    fn on_change_password(&self) {
        let min_len = self.settings.borrow().min_password_length;
        if let Some(new_password) = super::dialogs::create_password::show(min_len) {
            *self.password.borrow_mut() = new_password;
            self.mark_modified();
        }
    }

    /// Open settings dialog.
    fn on_settings(&self) {
        let current = self.settings.borrow().clone();
        if let Some(changes) = super::dialogs::settings_dialog::SettingsDialog::show(&current) {
            let mut settings = self.settings.borrow_mut();
            settings.save_on_close = changes.save_on_close;
            settings.theme = changes.theme;
            settings.min_password_length = changes.min_password_length;
            settings.minimize_to_tray = changes.minimize_to_tray;
            settings.font_family = changes.font_family.clone();
            settings.font_size = changes.font_size;
            // Apply theme change immediately
            drop(settings);
            self.mark_modified();
        }
    }

    // =====================================================================
    // View operations
    // =====================================================================

    /// Toggle always-on-top.
    fn on_toggle_always_on_top(&self) {
        let mut on_top = self.always_on_top.borrow_mut();
        *on_top = !*on_top;
        self.menu_view_always_on_top.set_checked(*on_top);
        // NWG does not have direct topmost API; would need raw Win32 call
        // SetWindowPos(hwnd, HWND_TOPMOST/HWND_NOTOPMOST, ...)
        // TODO: implement via raw Win32 when windows crate is wired up
    }

    /// Toggle word wrap.
    fn on_toggle_word_wrap(&self) {
        let mut settings = self.settings.borrow_mut();
        settings.word_wrap = !settings.word_wrap;
        self.menu_view_word_wrap.set_checked(settings.word_wrap);
        // NWG RichTextBox word wrap is set at build time;
        // toggling at runtime requires sending EM_SETTARGETDEVICE message.
        // TODO: implement via raw Win32 SendMessage
    }

    // =====================================================================
    // Edit operations
    // =====================================================================

    fn on_cut(&self) {
        // WM_CUT = 0x0300
        self.send_editor_message(0x0300, 0, 0);
    }

    fn on_copy(&self) {
        // WM_COPY = 0x0301
        self.send_editor_message(0x0301, 0, 0);
    }

    fn on_paste(&self) {
        // WM_PASTE = 0x0302
        self.send_editor_message(0x0302, 0, 0);
    }

    /// Paste as plain text (strip RTF formatting).
    fn on_paste_plain(&self) {
        // Read plain text from clipboard and insert via EM_REPLACESEL
        if let Ok(text) = clipboard_win::get_clipboard_string() {
            let sel = self.editor.selection();
            self.editor.set_selection(sel.start..sel.end);
            // EM_REPLACESEL = 0x00C2, wParam=1 (can undo)
            let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
            self.send_editor_message(0x00C2, 1, wide.as_ptr() as isize);
        }
    }

    /// Send a Windows message to the editor's HWND.
    fn send_editor_message(&self, msg: u32, wparam: usize, lparam: isize) {
        use nwg::ControlHandle;
        if let ControlHandle::Hwnd(hwnd) = self.editor.handle {
            unsafe {
                windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                    windows::Win32::Foundation::HWND(hwnd as *mut _),
                    msg,
                    Some(windows::Win32::Foundation::WPARAM(wparam)),
                    Some(windows::Win32::Foundation::LPARAM(lparam)),
                );
            }
        }
    }

    fn on_select_all(&self) {
        let len = self.editor.len() as u32;
        self.editor.set_selection(0..len);
    }

    /// Duplicate the current line.
    fn on_duplicate_line(&self) {
        let text = self.get_content();
        let sel = self.editor.selection();
        let sel_start = (sel.start as usize).min(text.len());

        // Find line boundaries
        let line_start = text[..sel_start]
            .rfind('\n')
            .map(|p| p + 1)
            .unwrap_or(0);
        let line_end = text[sel_start..]
            .find('\n')
            .map(|p| sel_start + p)
            .unwrap_or(text.len());

        let line = &text[line_start..line_end];
        let insertion = format!("\n{}", line);

        // Insert after the current line
        self.editor.set_selection(line_end as u32..line_end as u32);
        // Use the Windows EM_REPLACESEL approach — for now, rebuild text
        let new_text = format!("{}{}{}", &text[..line_end], insertion, &text[line_end..]);
        self.editor.set_text(&new_text);
        let new_pos = (line_end + insertion.len()) as u32;
        self.editor.set_selection(new_pos..new_pos);
        self.mark_modified();
    }

    /// Delete the current line.
    fn on_delete_line(&self) {
        let text = self.get_content();
        let sel = self.editor.selection();
        let sel_start = (sel.start as usize).min(text.len());

        let line_start = text[..sel_start]
            .rfind('\n')
            .map(|p| p + 1)
            .unwrap_or(0);
        let line_end = text[sel_start..]
            .find('\n')
            .map(|p| sel_start + p + 1) // include the newline
            .unwrap_or(text.len());

        // If deleting last line (no trailing \n), also remove preceding \n
        let del_start = if line_end == text.len() && line_start > 0 {
            line_start - 1
        } else {
            line_start
        };

        let new_text = format!("{}{}", &text[..del_start], &text[line_end..]);
        self.editor.set_text(&new_text);
        let new_pos = del_start.min(new_text.len()) as u32;
        self.editor.set_selection(new_pos..new_pos);
        self.mark_modified();
    }

    /// Insert a timestamp at the cursor position.
    fn on_insert_timestamp(&self) {
        use std::time::SystemTime;
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default();
        // Format as a simple ISO-like timestamp
        let secs = now.as_secs();
        // Simple formatting without chrono dependency
        let timestamp = format_timestamp(secs);

        let text = self.get_content();
        let sel = self.editor.selection();
        let sel_start = (sel.start as usize).min(text.len());
        let sel_end = (sel.end as usize).min(text.len());
        let new_text = format!(
            "{}{}{}",
            &text[..sel_start],
            timestamp,
            &text[sel_end..]
        );
        self.editor.set_text(&new_text);
        let new_pos = sel_start as u32 + timestamp.len() as u32;
        self.editor.set_selection(new_pos..new_pos);
        self.mark_modified();
    }

    // =====================================================================
    // Search / Replace
    // =====================================================================

    /// Show the find bar (find-only mode).
    fn on_show_find(&self) {
        // TODO: create and show SearchBar in find-only mode
        // For now, use a simple message box approach as placeholder
        let mut sb = self.search_bar.borrow_mut();
        if sb.is_none() {
            *sb = Some(SearchBar::new());
        }
        if let Some(ref bar) = *sb {
            bar.show(false); // find-only mode
        }
        drop(sb);
        self.layout_controls();
    }

    /// Show the find & replace bar.
    fn on_show_replace(&self) {
        let mut sb = self.search_bar.borrow_mut();
        if sb.is_none() {
            *sb = Some(SearchBar::new());
        }
        if let Some(ref bar) = *sb {
            bar.show(true); // replace mode
        }
        drop(sb);
        self.layout_controls();
    }

    /// Hide search bar on Escape.
    fn on_escape(&self) {
        let sb = self.search_bar.borrow();
        if let Some(ref bar) = *sb {
            if bar.is_visible() {
                bar.hide();
                drop(sb);
                self.layout_controls();
                self.editor.set_focus();
                return;
            }
        }
    }

    // =====================================================================
    // Go to Line
    // =====================================================================

    fn on_goto_line(&self) {
        // TODO: show GoToLineDialog when implemented
        // For now, placeholder
    }

    // =====================================================================
    // Help
    // =====================================================================

    fn on_check_updates(&self) {
        match crate::updater::check_for_update() {
            crate::updater::UpdateCheckResult::Available { version, download_url } => {
                let msg = format!(
                    "A new version ({}) is available.\nWould you like to download it?",
                    version
                );
                let params = nwg::MessageParams {
                    title: "Update Available",
                    content: &msg,
                    buttons: nwg::MessageButtons::YesNo,
                    icons: nwg::MessageIcons::Question,
                };
                if nwg::message(&params) == nwg::MessageChoice::Yes {
                    match crate::updater::download_and_update(&download_url, &self.exe_path) {
                        Ok(msg) => {
                            nwg::modal_info_message(&self.window, "Update", &msg);
                        }
                        Err(e) => {
                            nwg::modal_info_message(
                                &self.window,
                                "Update Error",
                                &format!("Failed to update: {}", e),
                            );
                        }
                    }
                }
            }
            crate::updater::UpdateCheckResult::UpToDate => {
                nwg::modal_info_message(
                    &self.window,
                    "Up to Date",
                    &format!(
                        "You are running the latest version ({}).",
                        crate::updater::current_version()
                    ),
                );
            }
            crate::updater::UpdateCheckResult::Error(e) => {
                nwg::modal_info_message(
                    &self.window,
                    "Update Check Failed",
                    &format!("Could not check for updates: {}", e),
                );
            }
        }
    }

    fn on_about(&self) {
        let msg = format!(
            "LockNote v{}\n\n\
             Self-contained encrypted notepad for Windows.\n\n\
             Encryption: AES-256-CBC + HMAC-SHA256\n\
             KDF: PBKDF2-SHA256 ({} iterations)\n\n\
             https://github.com/{}\n\
             https://uitguard.com",
            crate::updater::current_version(),
            crate::crypto::PBKDF2_ITERATIONS,
            crate::updater::github_repo(),
        );
        nwg::modal_info_message(&self.window, "About LockNote", &msg);
    }

    // =====================================================================
    // Tray icon
    // =====================================================================

    fn on_tray_restore(&self) {
        self.window.set_visible(true);
        // Restore from minimized state
        // Would need ShowWindow(hwnd, SW_RESTORE) via raw Win32
    }

    // =====================================================================
    // Close flow
    // =====================================================================

    fn on_close(&self) {
        let modified = *self.is_modified.borrow();
        let save_on_close = self.settings.borrow().save_on_close;

        if modified {
            match save_on_close {
                crate::settings::CloseAction::Always => {
                    self.on_save();
                }
                crate::settings::CloseAction::Ask => {
                    // Show close confirmation dialog
                    let params = nwg::MessageParams {
                        title: "Unsaved Changes",
                        content: "You have unsaved changes. Save before closing?",
                        buttons: nwg::MessageButtons::YesNoCancel,
                        icons: nwg::MessageIcons::Warning,
                    };
                    match nwg::message(&params) {
                        nwg::MessageChoice::Yes => {
                            self.on_save();
                        }
                        nwg::MessageChoice::No => {
                            // Don't save, just close
                        }
                        _ => {
                            return; // Cancel — don't close
                        }
                    }
                }
                crate::settings::CloseAction::Never => {
                    // Don't save, just close
                }
            }
        }

        // Clear sensitive data from memory
        {
            use zeroize::Zeroize;
            let mut pw = self.password.borrow_mut();
            pw.zeroize();
        }

        // Clear editor text
        self.editor.set_text("");

        // If we have a pending .tmp file, spawn the atomic swap command
        if *self.has_pending_tmp.borrow() {
            let tmp_path = crate::storage::get_tmp_path(&self.exe_path);
            if tmp_path.exists() {
                let mut cmd = crate::storage::atomic_swap_command(&tmp_path, &self.exe_path);
                let _ = cmd.spawn();
            }
        }

        nwg::stop_thread_dispatch();
    }
}

// =========================================================================
// Helper functions
// =========================================================================

/// Convert a font size in points to NWG logical units.
/// NWG uses roughly: nwg_size = points * (96/72) ≈ points * 1.333
/// But NWG Font builder `size` is in tenths of a point internally,
/// and the actual mapping depends on DPI. A practical formula:
/// nwg_size ≈ (points * 96 / 72) as i32
/// For 9pt → 12, for 11pt → ~15, etc.
/// Actually, NWG's size parameter maps more simply: it's the height
/// in logical pixels. The default font at size=17 gives roughly 9pt.
fn font_size_to_nwg(pt: f64) -> u32 {
    // NWG default: 17 for 9pt → ratio ~1.889
    // A more accurate mapping: size = pt * 96 / 72 * (4/3) ≈ pt * 1.78
    // Empirically from the spec: 9pt → 17, so ratio = 17/9 ≈ 1.889
    (pt * 17.0 / 9.0).round() as u32
}

/// Format a Unix timestamp as "YYYY-MM-DD HH:MM:SS" (UTC).
/// Simple implementation without chrono.
fn format_timestamp(secs: u64) -> String {
    // Days from epoch
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Date calculation (from Unix epoch 1970-01-01)
    let (year, month, day) = days_to_ymd(days);

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        year, month, day, hours, minutes, seconds
    )
}

/// Convert days since Unix epoch to (year, month, day).
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    // Civil calendar algorithm (from Howard Hinnant)
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}
