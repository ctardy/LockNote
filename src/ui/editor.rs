// EditorForm — Main editor window with menus, status bar, search bar, and tray icon
//
// Layout (top to bottom):
//   1. MenuStrip (File, View, Edit, Help)
//   2. SearchBar (hidden by default, toggled via Ctrl+F / Ctrl+H)
//   3. RichTextBox (main editor area)
//   4. StatusBar (word count, character count, line count)

use native_windows_gui as nwg;
use std::cell::RefCell;
use std::path::PathBuf;

use super::search_bar::SearchBar;
use crate::theme;

/// Main editor window holding all NWG controls and application state.
pub struct EditorForm {
    // ── Window ──
    pub window: nwg::Window,

    // ── Menu: File ──
    menu_file: nwg::Menu,
    menu_file_save: nwg::MenuItem,
    menu_file_sep1: nwg::MenuSeparator,
    // Settings submenu
    menu_file_settings: nwg::Menu,
    menu_file_settings_preferences: nwg::MenuItem,
    menu_file_settings_security: nwg::MenuItem,
    menu_file_sep2: nwg::MenuSeparator,
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
        form.apply_theme();
        form.update_status_bar();
        form.update_title();
        form.layout_controls();

        // Apply initial word wrap setting
        let wrap = form.settings.borrow().word_wrap;
        form.apply_word_wrap(wrap);

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
        let (win_w, win_h) = (settings.window_width as i32, settings.window_height as i32);
        let (win_x, win_y) = if settings.window_x == i32::MIN {
            // First launch: center on screen
            (300, 200)
        } else {
            // Validate saved position is still on a visible monitor
            let x = settings.window_x;
            let y = settings.window_y;
            if is_position_visible(x, y, win_w, win_h) {
                (x, y)
            } else {
                (300, 200)
            }
        };

        // ── App icon (embedded at compile time) ──
        let mut app_icon = nwg::Icon::default();
        nwg::Icon::builder()
            .source_bin(Some(include_bytes!("../../assets/icon.ico")))
            .build(&mut app_icon)
            .expect("Failed to load app icon");

        let mut window = nwg::Window::default();
        nwg::Window::builder()
            .title("LockNote")
            .size((win_w, win_h))
            .position((win_x, win_y))
            .icon(Some(&app_icon))
            .flags(
                nwg::WindowFlags::WINDOW
                    | nwg::WindowFlags::VISIBLE
                    | nwg::WindowFlags::RESIZABLE
                    | nwg::WindowFlags::MAXIMIZE_BOX
                    | nwg::WindowFlags::MINIMIZE_BOX,
            )
            .build(&mut window)
            .expect("Failed to build main window");

        // Restore maximized state if saved
        if settings.window_maximized {
            use nwg::ControlHandle;
            if let ControlHandle::Hwnd(hwnd) = window.handle {
                unsafe {
                    let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(
                        windows::Win32::Foundation::HWND(hwnd as *mut _),
                        windows::Win32::UI::WindowsAndMessaging::SW_MAXIMIZE,
                    );
                }
            }
        }

        // ── Menu: File ──
        let mut menu_file = nwg::Menu::default();
        nwg::Menu::builder()
            .text("&File")
            .parent(&window)
            .build(&mut menu_file)
            .expect("Failed to build File menu");

        let save_shortcut_text = {
            let mut parts = Vec::new();
            if settings.save_modifiers & 0x02 != 0 { parts.push("Ctrl"); }
            if settings.save_modifiers & 0x04 != 0 { parts.push("Shift"); }
            if settings.save_modifiers & 0x01 != 0 { parts.push("Alt"); }
            let key_char = char::from(settings.save_key as u8);
            format!("{}+{}", parts.join("+"), key_char)
        };
        let save_menu_text = format!("&Save\t{}", save_shortcut_text);
        let mut menu_file_save = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&save_menu_text)
            .parent(&menu_file)
            .build(&mut menu_file_save)
            .expect("Failed to build Save menu item");

        let mut menu_file_sep1 = nwg::MenuSeparator::default();
        nwg::MenuSeparator::builder()
            .parent(&menu_file)
            .build(&mut menu_file_sep1)
            .expect("Failed to build separator");

        // Settings submenu
        let mut menu_file_settings = nwg::Menu::default();
        nwg::Menu::builder()
            .text("S&ettings")
            .parent(&menu_file)
            .build(&mut menu_file_settings)
            .expect("Failed to build Settings submenu");

        let mut menu_file_settings_preferences = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text("&Preferences...")
            .parent(&menu_file_settings)
            .build(&mut menu_file_settings_preferences)
            .expect("Failed to build Preferences menu item");

        let mut menu_file_settings_security = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text("&Security...")
            .parent(&menu_file_settings)
            .build(&mut menu_file_settings_security)
            .expect("Failed to build Security menu item");

        let mut menu_file_sep2 = nwg::MenuSeparator::default();
        nwg::MenuSeparator::builder()
            .parent(&menu_file)
            .build(&mut menu_file_sep2)
            .expect("Failed to build separator");

        let mut menu_file_quit = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text("&Quit\tCtrl+Q")
            .parent(&menu_file)
            .build(&mut menu_file_quit)
            .expect("Failed to build Quit menu item");

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

        // Enable EN_CHANGE notifications so OnTextInput fires
        if let nwg::ControlHandle::Hwnd(hwnd) = editor.handle {
            const EM_SETEVENTMASK: u32 = 0x0445;
            const EM_GETEVENTMASK: u32 = 0x043B;
            const ENM_CHANGE: u32 = 0x01;
            let hwnd_w = windows::Win32::Foundation::HWND(hwnd as *mut _);
            unsafe {
                let mask = windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                    hwnd_w, EM_GETEVENTMASK,
                    Some(windows::Win32::Foundation::WPARAM(0)),
                    Some(windows::Win32::Foundation::LPARAM(0)),
                );
                windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                    hwnd_w, EM_SETEVENTMASK,
                    Some(windows::Win32::Foundation::WPARAM(0)),
                        // ENM_CHANGE=0x01, ENM_MOUSEEVENTS=0x00020000
                Some(windows::Win32::Foundation::LPARAM(mask.0 as isize | ENM_CHANGE as isize | 0x00020000)),
                );
            }
        }

        // ── Status bar ──
        let mut status_bar = nwg::StatusBar::default();
        nwg::StatusBar::builder()
            .parent(&window)
            .build(&mut status_bar)
            .expect("Failed to build status bar");

        // ── Tray icon ──
        let mut tray_icon = nwg::TrayNotification::default();
        nwg::TrayNotification::builder()
            .parent(&window)
            .icon(Some(&app_icon))
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
            menu_file_sep1,
            menu_file_settings,
            menu_file_settings_preferences,
            menu_file_settings_security,
            menu_file_sep2,
            menu_file_quit,
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
            // View menu
            menu_view,
            menu_view_always_on_top,
            menu_view_word_wrap,
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
                        } else if handle == f.menu_file_settings_preferences.handle {
                            f.on_preferences();
                        } else if handle == f.menu_file_settings_security.handle {
                            f.on_security();
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
                        // Minimize to tray: hide window when minimized
                        if f.settings.borrow().minimize_to_tray {
                            if let nwg::ControlHandle::Hwnd(hwnd) = f.window.handle {
                                let hwnd = windows::Win32::Foundation::HWND(hwnd as *mut _);
                                if unsafe { windows::Win32::UI::WindowsAndMessaging::IsIconic(hwnd) }.as_bool() {
                                    f.window.set_visible(false);
                                    return;
                                }
                            }
                        }
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

                    // Dynamic save shortcut (configurable modifier+key)
                    {
                        let s = f2.settings.borrow();
                        let sm = s.save_modifiers;
                        let sk = s.save_key;
                        drop(s);
                        let want_ctrl = sm & 0x02 != 0;
                        let want_shift = sm & 0x04 != 0;
                        let want_alt = sm & 0x01 != 0;
                        let alt = unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(0x12) } < 0;
                        if ctrl == want_ctrl && shift == want_shift && alt == want_alt && vk == sk {
                            f2.on_save();
                            return Some(0);
                        }
                    }

                    if ctrl && !shift {
                        match vk {
                            0x51 => { f2.on_close(); return Some(0); }       // Ctrl+Q
                            0x46 => { f2.on_show_find(); return Some(0); }   // Ctrl+F
                            0x48 => { f2.on_show_replace(); return Some(0); }// Ctrl+H
                            0x47 => { f2.on_goto_line(); return Some(0); }   // Ctrl+G
                            0x44 => { f2.on_duplicate_line(); return Some(0); } // Ctrl+D
                            0x41 => { f2.on_select_all(); return Some(0); }  // Ctrl+A
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

                // WM_COMMAND = 0x0111 — handle RichEdit notifications
                // nwg doesn't route EN_CHANGE for "RICHEDIT50W" (only "Edit"),
                // so we intercept it here to update status bar and modified state.
                if msg == 0x0111 {
                    let notification = ((w_param >> 16) & 0xFFFF) as u16;
                    // EN_CHANGE = 0x0300
                    if notification == 0x0300 {
                        f2.mark_modified();
                        f2.update_status_bar();
                    }
                }

                // WM_NOTIFY = 0x004E — handle EN_MSGFILTER for double-click trim
                if msg == 0x004E {
                    #[repr(C)]
                    struct NMHDR {
                        hwnd_from: isize,
                        id_from: usize,
                        code: u32,
                    }
                    let nmhdr = unsafe { &*(_l_param as *const NMHDR) };
                    // EN_MSGFILTER = 0x0700
                    if nmhdr.code == 0x0700 {
                        #[repr(C)]
                        struct MSGFILTER {
                            nmhdr: NMHDR,
                            msg: u32,
                            _pad: u32,
                            wparam: usize,
                            lparam: isize,
                        }
                        let mf = unsafe { &*(_l_param as *const MSGFILTER) };
                        // WM_LBUTTONDBLCLK = 0x0203
                        if mf.msg == 0x0203 {
                            if let nwg::ControlHandle::Hwnd(win_hwnd) = f2.window.handle {
                                unsafe {
                                    let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                                        Some(windows::Win32::Foundation::HWND(win_hwnd as *mut _)),
                                        0x8004, // WM_APP+4
                                        windows::Win32::Foundation::WPARAM(0),
                                        windows::Win32::Foundation::LPARAM(0),
                                    );
                                }
                            }
                        }
                    }
                }

                // WM_APP+4 = 0x8004 — deferred double-click trim & copy
                if msg == 0x8004 {
                    f2.on_dblclick_trim_and_copy();
                    return Some(0);
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

    /// Apply theme colors to all controls.
    fn apply_theme(&self) {
        use nwg::ControlHandle;
        use windows::Win32::Foundation::{HWND, WPARAM, LPARAM};
        use windows::Win32::UI::WindowsAndMessaging::SendMessageW;

        let pal = theme::current();

        // ── Editor background ──
        if let ControlHandle::Hwnd(hwnd) = self.editor.handle {
            let hwnd = HWND(hwnd as *mut _);
            const EM_SETBKGNDCOLOR: u32 = 0x0443;
            unsafe {
                SendMessageW(
                    hwnd,
                    EM_SETBKGNDCOLOR,
                    Some(WPARAM(0)),
                    Some(LPARAM(pal.editor_background.to_colorref() as isize)),
                );
            }

            // Set default text color via CHARFORMAT2
            #[repr(C)]
            #[allow(non_snake_case)]
            struct CHARFORMAT2W {
                cbSize: u32,
                dwMask: u32,
                dwEffects: u32,
                yHeight: i32,
                yOffset: i32,
                crTextColor: u32,
                bCharSet: u8,
                bPitchAndFamily: u8,
                szFaceName: [u16; 32],
                wWeight: u16,
                sSpacing: i16,
                crBackColor: u32,
                lcid: u32,
                _reserved: u32,
                sStyle: i16,
                wKerning: u16,
                bUnderlineType: u8,
                bAnimation: u8,
                bRevAuthor: u8,
                bUnderlineColor: u8,
            }

            let mut cf = unsafe { std::mem::zeroed::<CHARFORMAT2W>() };
            cf.cbSize = std::mem::size_of::<CHARFORMAT2W>() as u32;
            cf.dwMask = 0x40000000; // CFM_COLOR
            cf.crTextColor = pal.editor_text.to_colorref();

            const EM_SETCHARFORMAT: u32 = 0x0444;
            const SCF_ALL: u32 = 0x0004;
            unsafe {
                SendMessageW(
                    hwnd,
                    EM_SETCHARFORMAT,
                    Some(WPARAM(SCF_ALL as usize)),
                    Some(LPARAM(&cf as *const _ as isize)),
                );
            }
        }

        // ── Window background ──
        if let ControlHandle::Hwnd(hwnd) = self.window.handle {
            let hwnd = HWND(hwnd as *mut _);
            let brush = unsafe {
                windows::Win32::Graphics::Gdi::CreateSolidBrush(
                    windows::Win32::Foundation::COLORREF(pal.background.to_colorref()),
                )
            };
            unsafe {
                windows::Win32::UI::WindowsAndMessaging::SetClassLongPtrW(
                    hwnd,
                    windows::Win32::UI::WindowsAndMessaging::GCL_HBRBACKGROUND,
                    brush.0 as isize,
                );
            }
        }
    }

    /// Reposition controls when the window is resized.
    fn layout_controls(&self) {
        let (w, h) = self.window.size();
        let status_h: u32 = 22;
        // Search bar height (only when visible)
        let search_h: u32 = {
            let sb = self.search_bar.borrow();
            match *sb {
                Some(ref bar) if bar.is_visible() => 34,
                _ => 0,
            }
        };

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

    /// Open preferences dialog (appearance + behavior).
    fn on_preferences(&self) {
        let current = self.settings.borrow().clone();
        if let Some(changes) = super::dialogs::preferences_dialog::PreferencesDialog::show(&current) {
            let mut settings = self.settings.borrow_mut();
            settings.save_on_close = changes.save_on_close;
            settings.theme = changes.theme;
            settings.minimize_to_tray = changes.minimize_to_tray;
            settings.font_family = changes.font_family.clone();
            settings.font_size = changes.font_size;
            settings.save_key = changes.save_key;
            settings.save_modifiers = changes.save_modifiers;
            let theme_mode = match settings.theme {
                crate::settings::ThemeChoice::Dark => theme::ThemeMode::Dark,
                crate::settings::ThemeChoice::Light => theme::ThemeMode::Light,
            };
            theme::set_mode(theme_mode);
            drop(settings);
            self.apply_theme();
            self.mark_modified();
        }
    }

    /// Open security dialog (password, min length).
    fn on_security(&self) {
        let current = self.settings.borrow().clone();
        if let Some(changes) = super::dialogs::security_dialog::show(&current) {
            let mut settings = self.settings.borrow_mut();
            settings.min_password_length = changes.min_password_length;
            drop(settings);
            if let Some(new_password) = changes.new_password {
                *self.password.borrow_mut() = new_password;
            }
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

        use nwg::ControlHandle;
        if let ControlHandle::Hwnd(hwnd) = self.window.handle {
            use windows::Win32::UI::WindowsAndMessaging::*;
            use windows::Win32::Foundation::HWND;
            let insert_after = if *on_top { HWND_TOPMOST } else { HWND_NOTOPMOST };
            unsafe {
                let _ = SetWindowPos(
                    HWND(hwnd as *mut _),
                    Some(insert_after),
                    0, 0, 0, 0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
        }
    }

    /// Toggle word wrap.
    fn on_toggle_word_wrap(&self) {
        let mut settings = self.settings.borrow_mut();
        settings.word_wrap = !settings.word_wrap;
        self.menu_view_word_wrap.set_checked(settings.word_wrap);
        self.apply_word_wrap(settings.word_wrap);
    }

    /// Apply word wrap setting to the editor control.
    fn apply_word_wrap(&self, wrap: bool) {
        // EM_SETTARGETDEVICE: wParam=0 (HDC), lParam=0 wraps to window, lParam=1 disables wrap
        let lparam = if wrap { 0isize } else { 1isize };
        self.send_editor_message(0x0448, 0, lparam);
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

    /// After a double-click in the editor, trim leading/trailing whitespace from the
    /// selection and copy the trimmed word to the clipboard.
    fn on_dblclick_trim_and_copy(&self) {
        let sel = self.editor.selection();
        if sel.start >= sel.end {
            return;
        }

        let text = self.get_content();
        let (byte_start, byte_end) = utf16_range_to_byte_range(&text, sel.start as usize, sel.end as usize);
        if byte_start >= byte_end || byte_end > text.len() {
            return;
        }

        let selected = &text[byte_start..byte_end];

        // Trim trailing whitespace
        let trimmed_end = selected.trim_end();
        if trimmed_end.is_empty() {
            return;
        }

        // Trim leading whitespace, but only if not at the start of a line
        let at_line_start = byte_start == 0
            || text.as_bytes().get(byte_start - 1) == Some(&b'\n');
        let trimmed = if at_line_start {
            trimmed_end
        } else {
            trimmed_end.trim_start()
        };
        if trimmed.is_empty() {
            return;
        }

        // Compute new byte boundaries
        let leading_stripped = trimmed_end.len() - trimmed.len();
        let new_byte_start = byte_start + leading_stripped;
        let new_byte_end = new_byte_start + trimmed.len();

        // Update selection if it changed
        if new_byte_start != byte_start || new_byte_end != byte_end {
            let utf16_offsets = build_utf16_offset_map(&text);
            let new_u16_start = utf16_offsets[new_byte_start] as u32;
            let new_u16_end = utf16_offsets[new_byte_end] as u32;
            self.editor.set_selection(new_u16_start..new_u16_end);
        }

        // Copy trimmed word to clipboard
        use clipboard_win::{formats, set_clipboard};
        let _ = set_clipboard(formats::Unicode, trimmed);
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
        let text = self.get_content();
        let max_lines = if text.is_empty() {
            1
        } else {
            text.lines().count().max(1) as u32
        };

        // Determine current line from cursor position
        let sel = self.editor.selection();
        let sel_start = (sel.start as usize).min(text.len());
        let current_line = text[..sel_start].matches('\n').count() as u32 + 1;

        if let Some(target_line) = super::dialogs::goto_line::GoToLineDialog::show(current_line, max_lines) {
            // Find byte offset of the target line start
            let mut offset = 0usize;
            for (i, line) in text.split('\n').enumerate() {
                if i as u32 + 1 == target_line {
                    break;
                }
                offset += line.len() + 1;
            }
            let pos = offset.min(text.len()) as u32;
            self.editor.set_selection(pos..pos);
            self.editor.set_focus();
        }
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
        // Restore from minimized state via ShowWindow(SW_RESTORE)
        if let nwg::ControlHandle::Hwnd(hwnd) = self.window.handle {
            unsafe {
                windows::Win32::UI::WindowsAndMessaging::ShowWindow(
                    windows::Win32::Foundation::HWND(hwnd as *mut _),
                    windows::Win32::UI::WindowsAndMessaging::SW_RESTORE,
                );
            }
        }
    }

    // =====================================================================
    // Window state persistence
    // =====================================================================

    /// Save the current window size, position, and maximized state to settings.
    fn save_window_state(&self) {
        use nwg::ControlHandle;
        if let ControlHandle::Hwnd(hwnd) = self.window.handle {
            let hwnd = windows::Win32::Foundation::HWND(hwnd as *mut _);
            let mut wp = windows::Win32::UI::WindowsAndMessaging::WINDOWPLACEMENT::default();
            wp.length = std::mem::size_of::<windows::Win32::UI::WindowsAndMessaging::WINDOWPLACEMENT>() as u32;
            let ok = unsafe {
                windows::Win32::UI::WindowsAndMessaging::GetWindowPlacement(hwnd, &mut wp)
            };
            if ok.is_ok() {
                let mut settings = self.settings.borrow_mut();
                let rc = wp.rcNormalPosition;
                settings.window_width = (rc.right - rc.left).max(200) as u32;
                settings.window_height = (rc.bottom - rc.top).max(150) as u32;
                settings.window_x = rc.left;
                settings.window_y = rc.top;
                settings.window_maximized = wp.showCmd == windows::Win32::UI::WindowsAndMessaging::SW_MAXIMIZE.0 as u32;
            }
        }
    }

    // =====================================================================
    // Close flow
    // =====================================================================

    fn on_close(&self) {
        // Capture window position/size before any save so it is included
        self.save_window_state();

        let modified = *self.is_modified.borrow();
        let save_on_close = self.settings.borrow().save_on_close;

        let mut user_declined_save = false;

        if modified {
            match save_on_close {
                crate::settings::CloseAction::Always => {
                    self.on_save();
                }
                crate::settings::CloseAction::Ask => {
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
                            user_declined_save = true;
                        }
                        _ => {
                            return; // Cancel — don't close
                        }
                    }
                }
                crate::settings::CloseAction::Never => {
                    user_declined_save = true;
                }
            }
        }

        // Always do a final save to persist settings (window state, etc.)
        // unless the user explicitly declined saving.
        if !user_declined_save {
            self.on_save();
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

/// Check if a window position is at least partially visible on any monitor.
/// Uses MonitorFromRect with MONITOR_DEFAULTTONULL to check visibility.
fn is_position_visible(x: i32, y: i32, w: i32, h: i32) -> bool {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Gdi::{MonitorFromRect, MONITOR_DEFAULTTONULL};
    let rect = RECT {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    };
    let monitor = unsafe { MonitorFromRect(&rect, MONITOR_DEFAULTTONULL) };
    !monitor.is_invalid()
}

/// Convert a UTF-16 code-unit range to a UTF-8 byte range in a string.
fn utf16_range_to_byte_range(text: &str, utf16_start: usize, utf16_end: usize) -> (usize, usize) {
    let mut byte_start = text.len();
    let mut byte_end = text.len();
    let mut u16_pos: usize = 0;
    let bytes = text.as_bytes();

    for (byte_idx, ch) in text.char_indices() {
        if u16_pos == utf16_start {
            byte_start = byte_idx;
        }
        if u16_pos == utf16_end {
            byte_end = byte_idx;
            return (byte_start, byte_end);
        }
        // RichEdit counts \r\n as a single character — skip \n after \r
        if ch == '\n' && byte_idx > 0 && bytes[byte_idx - 1] == b'\r' {
            continue;
        }
        u16_pos += ch.len_utf16();
    }

    if u16_pos == utf16_start {
        byte_start = text.len();
    }
    if u16_pos == utf16_end {
        byte_end = text.len();
    }

    (byte_start, byte_end)
}

/// Build a lookup table mapping each UTF-8 byte offset to a UTF-16 code-unit offset.
fn build_utf16_offset_map(text: &str) -> Vec<usize> {
    let mut map = vec![0usize; text.len() + 1];
    let mut u16_pos: usize = 0;
    let bytes = text.as_bytes();

    for (byte_idx, ch) in text.char_indices() {
        map[byte_idx] = u16_pos;
        // RichEdit counts \r\n as a single character — skip \n after \r
        if ch == '\n' && byte_idx > 0 && bytes[byte_idx - 1] == b'\r' {
            continue;
        }
        u16_pos += ch.len_utf16();
    }
    map[text.len()] = u16_pos;

    map
}

/// Find all case-insensitive occurrences of `needle` in `text`.
/// Returns byte-offset pairs `(start, end)` in the original `text`.
fn find_all_case_insensitive(text: &str, needle: &str) -> Vec<(usize, usize)> {
    if needle.is_empty() {
        return vec![];
    }
    let needle_lower: Vec<char> = needle.chars().flat_map(|c| c.to_lowercase()).collect();
    let text_chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut results = vec![];

    'outer: for i in 0..text_chars.len() {
        let mut ni = 0;
        let mut ti = i;
        while ni < needle_lower.len() && ti < text_chars.len() {
            let lower_chars: Vec<char> = text_chars[ti].1.to_lowercase().collect();
            for &lc in &lower_chars {
                if ni >= needle_lower.len() || lc != needle_lower[ni] {
                    continue 'outer;
                }
                ni += 1;
            }
            ti += 1;
        }
        if ni == needle_lower.len() {
            let start = text_chars[i].0;
            let end = if ti < text_chars.len() { text_chars[ti].0 } else { text.len() };
            results.push((start, end));
        }
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_range_ascii() {
        assert_eq!(utf16_range_to_byte_range("hello world", 0, 5), (0, 5));
        assert_eq!(utf16_range_to_byte_range("hello world", 6, 11), (6, 11));
    }

    #[test]
    fn utf16_range_multibyte() {
        let text = "café";
        assert_eq!(utf16_range_to_byte_range(text, 0, 4), (0, 5));
        assert_eq!(utf16_range_to_byte_range(text, 3, 4), (3, 5));
    }

    #[test]
    fn utf16_range_emoji() {
        let text = "a😀b";
        assert_eq!(utf16_range_to_byte_range(text, 0, 1), (0, 1));
        assert_eq!(utf16_range_to_byte_range(text, 1, 3), (1, 5));
        assert_eq!(utf16_range_to_byte_range(text, 3, 4), (5, 6));
    }

    #[test]
    fn offset_map_ascii() {
        assert_eq!(build_utf16_offset_map("abc"), vec![0, 1, 2, 3]);
    }

    #[test]
    fn offset_map_multibyte() {
        let map = build_utf16_offset_map("café");
        assert_eq!(map[0], 0);
        assert_eq!(map[3], 3);
        assert_eq!(map[5], 4);
    }

    #[test]
    fn find_case_insensitive_basic() {
        let matches = find_all_case_insensitive("Hello World hello", "hello");
        assert_eq!(matches, vec![(0, 5), (12, 17)]);
    }

    #[test]
    fn find_case_insensitive_accented() {
        let text = "Café café CAFÉ";
        let matches = find_all_case_insensitive(text, "café");
        assert_eq!(matches.len(), 3);
    }

    #[test]
    fn find_case_insensitive_no_match() {
        assert!(find_all_case_insensitive("Hello", "xyz").is_empty());
    }

    #[test]
    fn find_case_insensitive_empty() {
        assert!(find_all_case_insensitive("Hello", "").is_empty());
    }
}
