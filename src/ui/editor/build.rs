// Window and control construction (`EditorForm::build`).
// Part of the EditorForm implementation, split out of the former editor.rs.

#[allow(unused_imports)]
use super::*;

impl EditorForm {
    /// Construct all NWG controls via builders.
    pub(super) fn build(
        exe_path: PathBuf,
        password: Zeroizing<String>,
        settings: crate::settings::Settings,
    ) -> Self {
        // ── App icon (embedded at compile time) ──
        let mut app_icon = nwg::Icon::default();
        nwg::Icon::builder()
            .source_bin(Some(include_bytes!("../../../assets/icon.ico")))
            .build(&mut app_icon)
            .expect("Failed to load app icon");

        // ── Window ──
        let mut window = nwg::Window::default();
        nwg::Window::builder()
            .title(&t("app.title"))
            .size((crate::ui::window_geometry::DEFAULT_W, crate::ui::window_geometry::DEFAULT_H))
            .position((crate::ui::window_geometry::DEFAULT_X, crate::ui::window_geometry::DEFAULT_Y))
            .icon(Some(&app_icon))
            // Created hidden: the saved placement is applied first, then
            // `run` shows it (normal or maximized) — no visible jump.
            .flags(
                nwg::WindowFlags::WINDOW
                    | nwg::WindowFlags::RESIZABLE
                    | nwg::WindowFlags::MAXIMIZE_BOX
                    | nwg::WindowFlags::MINIMIZE_BOX,
            )
            .accept_files(true)
            .build(&mut window)
            .expect("Failed to build main window");
        crate::ui::dark_chrome::apply_main_window(&window.handle);
        let start_maximized = apply_initial_placement(&window, &settings);

        // ── Menu: File ──
        let mut menu_file = nwg::Menu::default();
        nwg::Menu::builder()
            .text(&t("menu.file"))
            .parent(&window)
            .build(&mut menu_file)
            .expect("Failed to build File menu");

        let save_shortcut_text = {
            let mut parts = Vec::new();
            if settings.save_modifiers & 0x02 != 0 { parts.push("Ctrl"); }
            if settings.save_modifiers & 0x04 != 0 { parts.push("Shift"); }
            if settings.save_modifiers & 0x01 != 0 { parts.push("Alt"); }
            let key_char = char::from(settings.save_key as u8);
            parts.push(match key_char { 'A'..='Z' => "", _ => "" });
            // Remove empty last element and add the key
            parts.retain(|s| !s.is_empty());
            format!("{}+{}", parts.join("+"), key_char)
        };
        let save_label = t("menu.file.save");
        let save_menu_text = if let Some(tab_pos) = save_label.find('\t') {
            format!("{}\t{}", &save_label[..tab_pos], save_shortcut_text)
        } else {
            format!("{}\t{}", save_label, save_shortcut_text)
        };
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

        let mut menu_file_print = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("menu.file.print"))
            .parent(&menu_file)
            .build(&mut menu_file_print)
            .expect("Failed to build Print menu item");

        let mut menu_file_export = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("menu.file.export"))
            .parent(&menu_file)
            .build(&mut menu_file_export)
            .expect("Failed to build Export menu item");

        let mut menu_file_backup_export = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("menu.file.backup_export"))
            .parent(&menu_file)
            .build(&mut menu_file_backup_export)
            .expect("Failed to build Export full backup menu item");

        let mut menu_file_backup_import = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("menu.file.backup_import"))
            .parent(&menu_file)
            .build(&mut menu_file_backup_import)
            .expect("Failed to build Import full backup menu item");

        let mut menu_file_import_exe = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("menu.file.import_exe"))
            .parent(&menu_file)
            .build(&mut menu_file_import_exe)
            .expect("Failed to build Import from another LockNote menu item");

        let mut menu_file_backups = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("menu.file.backups"))
            .parent(&menu_file)
            .build(&mut menu_file_backups)
            .expect("Failed to build Safety backups menu item");

        let mut menu_file_sep2 = nwg::MenuSeparator::default();
        nwg::MenuSeparator::builder()
            .parent(&menu_file)
            .build(&mut menu_file_sep2)
            .expect("Failed to build separator");

        // Settings submenu
        let mut menu_file_settings = nwg::Menu::default();
        nwg::Menu::builder()
            .text(&t("menu.file.settings"))
            .parent(&menu_file)
            .build(&mut menu_file_settings)
            .expect("Failed to build Settings submenu");

        let mut menu_file_settings_preferences = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("menu.settings.preferences"))
            .parent(&menu_file_settings)
            .build(&mut menu_file_settings_preferences)
            .expect("Failed to build Preferences menu item");

        let mut menu_file_settings_security = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("menu.settings.security"))
            .parent(&menu_file_settings)
            .build(&mut menu_file_settings_security)
            .expect("Failed to build Security menu item");

        let mut menu_file_settings_tempfiles = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("menu.settings.tempfiles"))
            .parent(&menu_file_settings)
            .build(&mut menu_file_settings_tempfiles)
            .expect("Failed to build Temporary files menu item");

        let mut menu_file_sep3 = nwg::MenuSeparator::default();
        nwg::MenuSeparator::builder()
            .parent(&menu_file)
            .build(&mut menu_file_sep3)
            .expect("Failed to build separator");

        let mut menu_file_quit = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("menu.file.quit"))
            .parent(&menu_file)
            .build(&mut menu_file_quit)
            .expect("Failed to build Quit menu item");

        // ── Menu: Edit ──
        let mut menu_edit = nwg::Menu::default();
        nwg::Menu::builder()
            .text(&t("menu.edit"))
            .parent(&window)
            .build(&mut menu_edit)
            .expect("Failed to build Edit menu");

        let mut menu_edit_cut = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("menu.edit.cut"))
            .parent(&menu_edit)
            .build(&mut menu_edit_cut)
            .expect("Failed to build Cut menu item");

        let mut menu_edit_copy = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("menu.edit.copy"))
            .parent(&menu_edit)
            .build(&mut menu_edit_copy)
            .expect("Failed to build Copy menu item");

        let mut menu_edit_paste = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("menu.edit.paste"))
            .parent(&menu_edit)
            .build(&mut menu_edit_paste)
            .expect("Failed to build Paste menu item");

        let mut menu_edit_paste_plain = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("menu.edit.pasteplain"))
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
            .text(&t("menu.edit.find"))
            .parent(&menu_edit)
            .build(&mut menu_edit_find)
            .expect("Failed to build Find menu item");

        let mut menu_edit_replace = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("menu.edit.replace"))
            .parent(&menu_edit)
            .build(&mut menu_edit_replace)
            .expect("Failed to build Find & Replace menu item");

        let mut menu_edit_goto_line = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("menu.edit.goto"))
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
            .text(&t("menu.edit.dupline"))
            .parent(&menu_edit)
            .build(&mut menu_edit_duplicate_line)
            .expect("Failed to build Duplicate Line menu item");

        let mut menu_edit_delete_line = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("menu.edit.delline"))
            .parent(&menu_edit)
            .build(&mut menu_edit_delete_line)
            .expect("Failed to build Delete Line menu item");

        let mut menu_edit_select_all = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("menu.edit.selectall"))
            .parent(&menu_edit)
            .build(&mut menu_edit_select_all)
            .expect("Failed to build Select All menu item");

        let mut menu_edit_insert_timestamp = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("menu.edit.timestamp"))
            .parent(&menu_edit)
            .build(&mut menu_edit_insert_timestamp)
            .expect("Failed to build Insert Timestamp menu item");

        // ── Menu: View ──
        let mut menu_view = nwg::Menu::default();
        nwg::Menu::builder()
            .text(&t("menu.view"))
            .parent(&window)
            .build(&mut menu_view)
            .expect("Failed to build View menu");

        let mut menu_view_always_on_top = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("menu.view.ontop"))
            .parent(&menu_view)
            .build(&mut menu_view_always_on_top)
            .expect("Failed to build Always on Top menu item");

        let mut menu_view_word_wrap = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("menu.view.wordwrap"))
            .parent(&menu_view)
            .check(settings.word_wrap)
            .build(&mut menu_view_word_wrap)
            .expect("Failed to build Word Wrap menu item");

        let mut menu_view_line_numbers = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("menu.view.linenumbers"))
            .parent(&menu_view)
            .check(settings.show_line_numbers)
            .build(&mut menu_view_line_numbers)
            .expect("Failed to build Line Numbers menu item");

        // ── Menu: Tab ──
        let mut menu_tab = nwg::Menu::default();
        nwg::Menu::builder()
            .text(&t("menu.tab"))
            .parent(&window)
            .build(&mut menu_tab)
            .expect("Failed to build Tab menu");

        let mut menu_tab_new = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("menu.tab.new"))
            .parent(&menu_tab)
            .build(&mut menu_tab_new)
            .expect("Failed to build New Tab menu item");

        let mut menu_tab_close = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("menu.tab.close"))
            .parent(&menu_tab)
            .build(&mut menu_tab_close)
            .expect("Failed to build Close Tab menu item");

        let mut menu_tab_rename = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("menu.tab.rename"))
            .parent(&menu_tab)
            .build(&mut menu_tab_rename)
            .expect("Failed to build Rename Tab menu item");

        let mut menu_tab_next = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("menu.tab.next"))
            .parent(&menu_tab)
            .build(&mut menu_tab_next)
            .expect("Failed to build Next Tab menu item");

        let mut menu_tab_prev = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("menu.tab.prev"))
            .parent(&menu_tab)
            .build(&mut menu_tab_prev)
            .expect("Failed to build Previous Tab menu item");

        // ── Menu: Tools ──
        let mut menu_tools = nwg::Menu::default();
        nwg::Menu::builder()
            .text(&t("menu.tools"))
            .parent(&window)
            .build(&mut menu_tools)
            .expect("Failed to build Tools menu");

        let mut menu_tools_vault = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("menu.view.vault"))
            .parent(&menu_tools)
            .build(&mut menu_tools_vault)
            .expect("Failed to build Vault Manager menu item");

        let mut menu_tools_prompts = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("menu.tools.prompts"))
            .parent(&menu_tools)
            .build(&mut menu_tools_prompts)
            .expect("Failed to build Prompts menu item");

        let mut menu_tools_totp = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("menu.tools.totp"))
            .parent(&menu_tools)
            .build(&mut menu_tools_totp)
            .expect("Failed to build TOTP Manager menu item");

        let mut menu_tools_attachments = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("menu.tools.attachments"))
            .parent(&menu_tools)
            .build(&mut menu_tools_attachments)
            .expect("Failed to build Attachments menu item");

        let mut menu_tools_passgen = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("menu.tools.passgen"))
            .parent(&menu_tools)
            .build(&mut menu_tools_passgen)
            .expect("Failed to build Password Generator menu item");

        let mut menu_tools_stego = nwg::Menu::default();
        nwg::Menu::builder()
            .text(&t("menu.tools.stego"))
            .parent(&menu_tools)
            .build(&mut menu_tools_stego)
            .expect("Failed to build Steganography submenu");

        let mut menu_tools_stego_hide = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("menu.tools.stego_hide"))
            .parent(&menu_tools_stego)
            .build(&mut menu_tools_stego_hide)
            .expect("Failed to build Stego Hide menu item");

        let mut menu_tools_stego_extract = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("menu.tools.stego_extract"))
            .parent(&menu_tools_stego)
            .build(&mut menu_tools_stego_extract)
            .expect("Failed to build Stego Extract menu item");

        let mut menu_tools_ssh = nwg::Menu::default();
        nwg::Menu::builder()
            .text(&t("menu.tools.ssh"))
            .parent(&menu_tools)
            .build(&mut menu_tools_ssh)
            .expect("Failed to build SSH Keys submenu");

        let mut menu_tools_ssh_generate = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("menu.tools.ssh_generate"))
            .parent(&menu_tools_ssh)
            .build(&mut menu_tools_ssh_generate)
            .expect("Failed to build SSH Generate menu item");

        let mut menu_tools_ssh_agent = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("menu.tools.ssh_agent"))
            .parent(&menu_tools_ssh)
            .build(&mut menu_tools_ssh_agent)
            .expect("Failed to build SSH Agent menu item");

        // ── Menu: Help ──
        let mut menu_help = nwg::Menu::default();
        nwg::Menu::builder()
            .text(&t("menu.help"))
            .parent(&window)
            .build(&mut menu_help)
            .expect("Failed to build Help menu");

        let mut menu_help_updates = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("menu.help.update"))
            .parent(&menu_help)
            .build(&mut menu_help_updates)
            .expect("Failed to build Check for Updates menu item");

        let mut menu_help_about = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("menu.help.about"))
            .parent(&menu_help)
            .build(&mut menu_help_about)
            .expect("Failed to build About menu item");

        // ── Menu bar toggle buttons ──
        // These appear as top-level "menus" with no children. Clicking them
        // triggers WM_INITMENUPOPUP which we intercept to toggle the panel
        // and immediately close the empty popup.
        // ── Toolbar toggle buttons (positioned in tab bar area, right side) ──
        let mut toolbar_prompts = nwg::Button::default();
        nwg::Button::builder()
            .text(&t("menu.toggle.prompts"))
            .parent(&window)
            .position((0, 0)) // repositioned in layout_controls
            .size((60, 24))
            .build(&mut toolbar_prompts)
            .expect("Failed to build prompts toolbar button");

        let mut toolbar_vault = nwg::Button::default();
        nwg::Button::builder()
            .text(&t("menu.toggle.vault"))
            .parent(&window)
            .position((0, 0)) // repositioned in layout_controls
            .size((50, 24))
            .build(&mut toolbar_vault)
            .expect("Failed to build vault toolbar button");

        let mut toolbar_totp = nwg::Button::default();
        nwg::Button::builder()
            .text(&t("menu.toggle.totp"))
            .parent(&window)
            .position((0, 0))
            .size((50, 24))
            .build(&mut toolbar_totp)
            .expect("Failed to build totp toolbar button");

        let mut toolbar_attach = nwg::Button::default();
        nwg::Button::builder()
            .text(&t("menu.toggle.attach"))
            .parent(&window)
            .position((0, 0))
            .size((50, 24))
            .build(&mut toolbar_attach)
            .expect("Failed to build attach toolbar button");

        let mut toolbar_lock = nwg::Button::default();
        nwg::Button::builder()
            .text("") // icon drawn in WM_DRAWITEM; tooltip provides the label
            .parent(&window)
            .position((0, 0))
            .size((32, 24))
            .build(&mut toolbar_lock)
            .expect("Failed to build lock toolbar button");

        // ── Editor font ──
        let mut editor_font = nwg::Font::default();
        nwg::Font::builder()
            .family(&settings.font_family)
            .size(font_size_to_nwg(settings.font_size))
            .build(&mut editor_font)
            .expect("Failed to build editor font");

        // ── Editor (RichTextBox) ──
        // NOTE: do NOT call OleInitialize — it would make RichEdit register an
        // OLE IDropTarget that swallows shell drops and prevents WM_DROPFILES.
        let mut editor = nwg::RichTextBox::default();
        nwg::RichTextBox::builder()
            .parent(&window)
            .font(Some(&editor_font))
            .focus(true)
            .build(&mut editor)
            .expect("Failed to build editor RichTextBox");

        // Enable automatic URL detection in RichEdit + keep selection visible
        // when focus moves to a popup (e.g. Find/Replace dialog).
        // ES_NOHIDESEL = 0x100 — without this, EM_EXSETSEL invisibly selects
        // the match while the Find dialog holds focus and the user sees nothing.
        // EM_AUTOURLDETECT = 0x045B, AURL_ENABLEURL = 1
        {
            use nwg::ControlHandle;
            if let ControlHandle::Hwnd(hwnd) = editor.handle {
                let hwnd = windows::Win32::Foundation::HWND(hwnd as *mut _);
                unsafe {
                    let style = windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(
                        hwnd,
                        windows::Win32::UI::WindowsAndMessaging::GWL_STYLE,
                    );
                    windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(
                        hwnd,
                        windows::Win32::UI::WindowsAndMessaging::GWL_STYLE,
                        style | 0x100,
                    );
                    windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                        hwnd,
                        0x045B, // EM_AUTOURLDETECT
                        Some(windows::Win32::Foundation::WPARAM(1)),
                        Some(windows::Win32::Foundation::LPARAM(0)),
                    );
                    // Add ENM_LINK + ENM_CHANGE to the event mask
                    // EM_GETEVENTMASK = 0x043B, EM_SETEVENTMASK = 0x0445
                    // ENM_CHANGE = 0x01, ENM_LINK = 0x04000000
                    let current = windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                        hwnd,
                        0x043B, // EM_GETEVENTMASK
                        Some(windows::Win32::Foundation::WPARAM(0)),
                        Some(windows::Win32::Foundation::LPARAM(0)),
                    );
                    windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                        hwnd,
                        0x0445, // EM_SETEVENTMASK
                        Some(windows::Win32::Foundation::WPARAM(0)),
                        // ENM_CHANGE=0x01, ENM_SCROLL=0x04, ENM_LINK=0x04000000, ENM_MOUSEEVENTS=0x00020000
                    Some(windows::Win32::Foundation::LPARAM(current.0 as isize | 0x04000000 | 0x00020000 | 0x01 | 0x04)),
                    );
                    // Left inner margin of the text (EM_SETMARGINS, EC_LEFTMARGIN = 1).
                    windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                        hwnd,
                        0x00D3, // EM_SETMARGINS
                        Some(windows::Win32::Foundation::WPARAM(1)),
                        Some(windows::Win32::Foundation::LPARAM(EDITOR_LEFT_MARGIN as isize)),
                    );
                }
            }
        }

        // Line spacing, current-line band and dark scrollbars (surface.rs).
        surface::install(&editor);

        // ── Line number gutter (raw Win32 child window) ──
        // Register a custom window class that does nothing in WM_PAINT/WM_ERASEBKGND
        // so our direct GDI painting (via GetDC) is not overwritten.
        let gutter_hwnd: isize = {
            use nwg::ControlHandle;
            if let ControlHandle::Hwnd(parent_hwnd) = window.handle {
                use windows::Win32::UI::WindowsAndMessaging::*;
                use windows::Win32::Foundation::*;
                use windows::Win32::Graphics::Gdi::*;
                use windows::core::*;

                // Minimal WndProc: block WM_ERASEBKGND, on WM_PAINT validate then
                // request a deferred repaint from the parent so paint_gutter() runs.
                unsafe extern "system" fn gutter_wnd_proc(
                    hwnd: HWND,
                    msg: u32,
                    wparam: WPARAM,
                    lparam: LPARAM,
                ) -> LRESULT {
                    match msg {
                        0x0014 => LRESULT(1), // WM_ERASEBKGND → handled
                        0x000F => {
                            // WM_PAINT → validate the region, then ask parent to repaint gutter
                            let mut ps = std::mem::zeroed::<PAINTSTRUCT>();
                            let _ = BeginPaint(hwnd, &mut ps);
                            let _ = EndPaint(hwnd, &ps);
                            // Post WM_APP+3 to parent so paint_gutter() runs
                            let parent = GetAncestor(hwnd, GA_PARENT);
                            if !parent.is_invalid() {
                                let _ = PostMessageW(
                                    Some(parent),
                                    0x8003,
                                    WPARAM(0),
                                    LPARAM(0),
                                );
                            }
                            LRESULT(0)
                        }
                        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
                    }
                }

                let class_name = w!("LockNoteGutter");
                let wc = unsafe {
                    WNDCLASSW {
                        lpfnWndProc: Some(gutter_wnd_proc),
                        hInstance: windows::Win32::System::LibraryLoader::GetModuleHandleW(None).unwrap_or_default().into(),
                        lpszClassName: class_name,
                        hbrBackground: HBRUSH::default(),
                        ..std::mem::zeroed()
                    }
                };
                unsafe { let _ = RegisterClassW(&wc); }

                let hwnd = unsafe {
                    CreateWindowExW(
                        WINDOW_EX_STYLE(0),
                        class_name,
                        None,
                        WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS,
                        0, 0, 40, 600,
                        Some(HWND(parent_hwnd as *mut _)),
                        None,
                        None,
                        None,
                    )
                };
                match hwnd {
                    Ok(h) => h.0 as isize,
                    Err(_) => 0,
                }
            } else {
                0
            }
        };

        // ── Tab bar (raw Win32 child window) ──
        let tab_bar_hwnd: isize = {
            use nwg::ControlHandle;
            if let ControlHandle::Hwnd(parent_hwnd) = window.handle {
                use windows::Win32::UI::WindowsAndMessaging::*;
                use windows::Win32::UI::Input::KeyboardAndMouse::{GetCapture, ReleaseCapture, SetCapture};
                use windows::Win32::Foundation::*;
                use windows::Win32::Graphics::Gdi::*;
                use windows::core::*;

                unsafe extern "system" fn tab_bar_wnd_proc(
                    hwnd: HWND,
                    msg: u32,
                    wparam: WPARAM,
                    lparam: LPARAM,
                ) -> LRESULT {
                    match msg {
                        0x0020 => {
                            // WM_SETCURSOR — arrow instead of I-beam; the
                            // horizontal-resize arrow while a tab is dragged
                            let id = if GetCapture() == hwnd { IDC_SIZEWE } else { IDC_ARROW };
                            let cursor = LoadCursorW(None, id).unwrap_or_default();
                            SetCursor(Some(cursor));
                            LRESULT(1)
                        }
                        0x0014 => LRESULT(1), // WM_ERASEBKGND
                        0x000F => {
                            // WM_PAINT — validate region, then ask parent to
                            // run paint_tab_bar() so any system-triggered
                            // invalidation (resize, DWM restore, window shown)
                            // gets redrawn instead of leaving stale backbuffer
                            // pixels visible.
                            let mut ps = std::mem::zeroed::<PAINTSTRUCT>();
                            let _ = BeginPaint(hwnd, &mut ps);
                            let _ = EndPaint(hwnd, &ps);
                            if let Ok(parent) = GetParent(hwnd) {
                                let _ = PostMessageW(Some(parent), 0x8005, WPARAM(0), LPARAM(0));
                            }
                            LRESULT(0)
                        }
                        0x0201 => {
                            // WM_LBUTTONDOWN — forward click to parent as WM_APP+1
                            let x = (lparam.0 & 0xFFFF) as i16 as i32;
                            let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
                            if let Ok(parent) = GetParent(hwnd) {
                                let _ = PostMessageW(Some(parent), 0x8001, WPARAM(x as usize), LPARAM(y as isize));
                            }
                            // Keep receiving the mouse until the button is
                            // released: a tab can be dragged out of the bar.
                            SetCapture(hwnd);
                            LRESULT(0)
                        }
                        0x0200 => {
                            // WM_MOUSEMOVE while a tab is held: forward to
                            // the parent as WM_APP+7 (drag feedback)
                            if GetCapture() == hwnd {
                                let x = (lparam.0 & 0xFFFF) as i16 as i32;
                                if let Ok(parent) = GetParent(hwnd) {
                                    let _ = PostMessageW(Some(parent), 0x8007, WPARAM(x as usize), LPARAM(0));
                                }
                            }
                            LRESULT(0)
                        }
                        0x0202 => {
                            // WM_LBUTTONUP — end of a possible tab drag:
                            // forward the release to the parent as WM_APP+6
                            if GetCapture() == hwnd {
                                let _ = ReleaseCapture();
                                let x = (lparam.0 & 0xFFFF) as i16 as i32;
                                if let Ok(parent) = GetParent(hwnd) {
                                    let _ = PostMessageW(Some(parent), 0x8006, WPARAM(x as usize), LPARAM(0));
                                }
                            }
                            LRESULT(0)
                        }
                        0x0204 => {
                            // WM_RBUTTONDOWN — forward right-click to parent as WM_APP+2
                            let x = (lparam.0 & 0xFFFF) as i16 as i32;
                            let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
                            if let Ok(parent) = GetParent(hwnd) {
                                // Pack x in WPARAM, y in LPARAM
                                let _ = PostMessageW(Some(parent), 0x8002, WPARAM(x as usize), LPARAM(y as isize));
                            }
                            LRESULT(0)
                        }
                        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
                    }
                }

                let class_name = w!("LockNoteTabBar");
                let wc = unsafe {
                    WNDCLASSW {
                        lpfnWndProc: Some(tab_bar_wnd_proc),
                        hInstance: windows::Win32::System::LibraryLoader::GetModuleHandleW(None).unwrap_or_default().into(),
                        lpszClassName: class_name,
                        hbrBackground: HBRUSH::default(),
                        ..std::mem::zeroed()
                    }
                };
                unsafe { let _ = RegisterClassW(&wc); }

                let hwnd = unsafe {
                    CreateWindowExW(
                        WINDOW_EX_STYLE(0),
                        class_name,
                        None,
                        WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS,
                        0, 0, 800, 30,
                        Some(HWND(parent_hwnd as *mut _)),
                        None,
                        None,
                        None,
                    )
                };
                match hwnd {
                    Ok(h) => h.0 as isize,
                    Err(_) => 0,
                }
            } else {
                0
            }
        };

        // ── Resize grip of the side panels (raw Win32 child window, panel_grip.rs) ──
        let panel_grip_hwnd: isize = super::panel_grip::create_panel_grip(&window.handle);

        // ── Context menu (popup, right-click on editor) ──
        let mut context_menu = nwg::Menu::default();
        nwg::Menu::builder()
            .popup(true)
            .parent(&window)
            .build(&mut context_menu)
            .expect("Failed to build context menu");

        let mut ctx_cut = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("ctx.cut"))
            .parent(&context_menu)
            .build(&mut ctx_cut)
            .expect("ctx_cut");

        let mut ctx_copy = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("ctx.copy"))
            .parent(&context_menu)
            .build(&mut ctx_copy)
            .expect("ctx_copy");

        let mut ctx_paste = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("ctx.paste"))
            .parent(&context_menu)
            .build(&mut ctx_paste)
            .expect("ctx_paste");

        let mut ctx_paste_plain = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("ctx.pasteplain"))
            .parent(&context_menu)
            .build(&mut ctx_paste_plain)
            .expect("ctx_paste_plain");

        let mut ctx_sep1 = nwg::MenuSeparator::default();
        nwg::MenuSeparator::builder()
            .parent(&context_menu)
            .build(&mut ctx_sep1)
            .expect("ctx_sep1");

        let mut ctx_select_all = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("ctx.selectall"))
            .parent(&context_menu)
            .build(&mut ctx_select_all)
            .expect("ctx_select_all");

        // ── Status bar ──
        let mut status_bar = nwg::StatusBar::default();
        nwg::StatusBar::builder()
            .parent(&window)
            .build(&mut status_bar)
            .expect("Failed to build status bar");

        // Configure multi-segment status bar via SB_SETPARTS
        if let nwg::ControlHandle::Hwnd(sb_hwnd) = status_bar.handle {
            // The bar is custom-painted and positioned by layout_controls:
            // CCS_NORESIZE (0x4) | CCS_NOPARENTALIGN (0x8) stop the control
            // from docking itself with its default height.
            unsafe {
                let sb = windows::Win32::Foundation::HWND(sb_hwnd as *mut _);
                let style = windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(
                    sb,
                    windows::Win32::UI::WindowsAndMessaging::GWL_STYLE,
                );
                windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(
                    sb,
                    windows::Win32::UI::WindowsAndMessaging::GWL_STYLE,
                    style | 0x4 | 0x8,
                );
            }
            let parts: [i32; 4] = [200, 400, 550, -1];
            unsafe {
                windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                    windows::Win32::Foundation::HWND(sb_hwnd as *mut _),
                    0x0404, // SB_SETPARTS
                    Some(windows::Win32::Foundation::WPARAM(parts.len())),
                    Some(windows::Win32::Foundation::LPARAM(parts.as_ptr() as isize)),
                );
            }
        }

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
            .text(&t("tray.restore"))
            .parent(&tray_menu)
            .build(&mut tray_menu_restore)
            .expect("Failed to build tray Restore item");

        let mut tray_menu_quit = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text(&t("tray.quit"))
            .parent(&tray_menu)
            .build(&mut tray_menu_quit)
            .expect("Failed to build tray Quit item");


        // __ Vault panel controls __
        let mut vault_frame = nwg::Frame::default();
        nwg::Frame::builder()
            .parent(&window)
            .position((0, 0))
            .size((crate::ui::vault_panel::VAULT_PANEL_WIDTH as i32, 600))
            .build(&mut vault_frame)
            .expect("Failed to build vault frame");

        let mut vault_search = nwg::TextInput::default();
        nwg::TextInput::builder()
            .parent(&vault_frame)
            .position((8, 8))
            .size((280, 24))
            .placeholder_text(Some(&t("vault.search")))
            .build(&mut vault_search)
            .expect("Failed to build vault search field");

        let mut vault_tree = nwg::TreeView::default();
        nwg::TreeView::builder()
            .parent(&vault_frame)
            .position((8, 40))
            .size((280, 400))
            .build(&mut vault_tree)
            .expect("Failed to build vault tree");
        // nwg's default style has TVS_EDITLABELS (F2 would rename a secret
        // in place): not a vault action (A4). Drag stays enabled: a secret
        // can be dragged onto another category (issue 053).
        if let nwg::ControlHandle::Hwnd(h) = vault_tree.handle {
            use windows::Win32::UI::Controls::TVS_EDITLABELS;
            use windows::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrW, SetWindowLongPtrW, GWL_STYLE};
            let hwnd = windows::Win32::Foundation::HWND(h as *mut _);
            unsafe {
                let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
                let style = style & !TVS_EDITLABELS;
                SetWindowLongPtrW(hwnd, GWL_STYLE, style as isize);
            }
        }

        let mut vault_reveal = nwg::TextInput::default();
        nwg::TextInput::builder()
            .parent(&vault_frame)
            .position((8, 412))
            .size((280, 24))
            .readonly(true)
            .flags(nwg::TextInputFlags::TAB_STOP | nwg::TextInputFlags::AUTO_SCROLL)
            .build(&mut vault_reveal)
            .expect("Failed to build vault reveal field");

        let mut vault_copied = nwg::Label::default();
        nwg::Label::builder()
            .text("")
            .parent(&vault_frame)
            .position((8, 412))
            .size((280, 18))
            .build(&mut vault_copied)
            .expect("Failed to build vault copied label");

        let mut vault_btn_add = nwg::Button::default();
        nwg::Button::builder()
            .text(&t("vault.btn.add"))
            .parent(&vault_frame)
            .position((8, 448))
            .size((52, 26))
            .build(&mut vault_btn_add)
            .expect("Failed to build vault Add button");

        let mut vault_btn_edit = nwg::Button::default();
        nwg::Button::builder()
            .text(&t("vault.btn.edit"))
            .parent(&vault_frame)
            .position((64, 448))
            .size((52, 26))
            .build(&mut vault_btn_edit)
            .expect("Failed to build vault Edit button");

        let mut vault_btn_delete = nwg::Button::default();
        nwg::Button::builder()
            .text(&t("vault.btn.del"))
            .parent(&vault_frame)
            .position((120, 448))
            .size((52, 26))
            .build(&mut vault_btn_delete)
            .expect("Failed to build vault Delete button");

        let mut vault_btn_copy = nwg::Button::default();
        nwg::Button::builder()
            .text(&t("vault.btn.copy"))
            .parent(&vault_frame)
            .position((176, 448))
            .size((52, 26))
            .build(&mut vault_btn_copy)
            .expect("Failed to build vault Copy button");

        let mut vault_btn_reveal = nwg::Button::default();
        nwg::Button::builder()
            .text(&t("vault.btn.reveal"))
            .parent(&vault_frame)
            .position((232, 448))
            .size((56, 26))
            .build(&mut vault_btn_reveal)
            .expect("Failed to build vault Reveal button");

        let mut vault_timer = nwg::AnimationTimer::default();
        nwg::AnimationTimer::builder()
            .parent(&window)
            .interval(std::time::Duration::from_secs(1))
            .active(false)
            .build(&mut vault_timer)
            .expect("Failed to build vault timer");

        // __ Prompts panel controls (issue 058) __
        let mut prompts_frame = nwg::Frame::default();
        nwg::Frame::builder()
            .parent(&window)
            .position((0, 0))
            .size((crate::ui::side_panel::PROMPTS_PANEL_WIDTH as i32, 600))
            .build(&mut prompts_frame)
            .expect("Failed to build prompts frame");

        let mut prompts_search = nwg::TextInput::default();
        nwg::TextInput::builder()
            .parent(&prompts_frame)
            .position((8, 8))
            .size((300, 24))
            .placeholder_text(Some(&t("prompts.search")))
            .build(&mut prompts_search)
            .expect("Failed to build prompts search field");

        let mut prompts_list = nwg::ListBox::default();
        nwg::ListBox::builder()
            .parent(&prompts_frame)
            .position((8, 40))
            .size((300, 240))
            .build(&mut prompts_list)
            .expect("Failed to build prompts list");

        let mut prompts_preview = nwg::TextBox::default();
        nwg::TextBox::builder()
            .parent(&prompts_frame)
            .position((8, 286))
            .size((300, 140))
            .readonly(true)
            .flags(nwg::TextBoxFlags::VISIBLE | nwg::TextBoxFlags::VSCROLL | nwg::TextBoxFlags::AUTOVSCROLL | nwg::TextBoxFlags::TAB_STOP)
            .build(&mut prompts_preview)
            .expect("Failed to build prompts preview");

        let mut prompts_btn_add = nwg::Button::default();
        nwg::Button::builder()
            .text(&t("prompts.btn.add"))
            .parent(&prompts_frame)
            .position((8, 434))
            .size((58, 26))
            .build(&mut prompts_btn_add)
            .expect("Failed to build prompts Add button");

        let mut prompts_btn_edit = nwg::Button::default();
        nwg::Button::builder()
            .text(&t("prompts.btn.edit"))
            .parent(&prompts_frame)
            .position((70, 434))
            .size((58, 26))
            .enabled(false)
            .build(&mut prompts_btn_edit)
            .expect("Failed to build prompts Edit button");

        let mut prompts_btn_delete = nwg::Button::default();
        nwg::Button::builder()
            .text(&t("prompts.btn.del"))
            .parent(&prompts_frame)
            .position((132, 434))
            .size((58, 26))
            .enabled(false)
            .build(&mut prompts_btn_delete)
            .expect("Failed to build prompts Delete button");

        let mut prompts_btn_copy = nwg::Button::default();
        nwg::Button::builder()
            .text(&t("prompts.btn.copy"))
            .parent(&prompts_frame)
            .position((194, 434))
            .size((58, 26))
            .enabled(false)
            .build(&mut prompts_btn_copy)
            .expect("Failed to build prompts Copy button");

        let mut prompts_status = nwg::Label::default();
        nwg::Label::builder()
            .text("")
            .parent(&prompts_frame)
            .position((8, 466))
            .size((300, 18))
            .build(&mut prompts_status)
            .expect("Failed to build prompts status label");

        // __ TOTP panel controls __
        let mut totp_frame = nwg::Frame::default();
        nwg::Frame::builder()
            .parent(&window)
            .position((0, 0))
            .size((crate::ui::totp_panel::TOTP_PANEL_WIDTH as i32, 600))
            .build(&mut totp_frame)
            .expect("Failed to build TOTP frame");

        let mut totp_lbl_countdown = nwg::Label::default();
        nwg::Label::builder()
            .text("")
            .parent(&totp_frame)
            .position((8, 8))
            .size((260, 20))
            .build(&mut totp_lbl_countdown)
            .expect("Failed to build TOTP countdown label");

        let mut totp_list = nwg::ListBox::default();
        nwg::ListBox::builder()
            .parent(&totp_frame)
            .position((8, 32))
            .size((260, 400))
            .build(&mut totp_list)
            .expect("Failed to build TOTP list");

        let mut totp_btn_copy = nwg::Button::default();
        nwg::Button::builder()
            .text(&t("totp.manager.copy"))
            .parent(&totp_frame)
            .position((8, 440))
            .size((58, 26))
            .build(&mut totp_btn_copy)
            .expect("Failed to build TOTP Copy button");

        let mut totp_btn_add = nwg::Button::default();
        nwg::Button::builder()
            .text(&t("totp.add"))
            .parent(&totp_frame)
            .position((70, 440))
            .size((58, 26))
            .build(&mut totp_btn_add)
            .expect("Failed to build TOTP Add button");

        let mut totp_btn_remove = nwg::Button::default();
        nwg::Button::builder()
            .text(&t("totp.remove"))
            .parent(&totp_frame)
            .position((132, 440))
            .size((58, 26))
            .build(&mut totp_btn_remove)
            .expect("Failed to build TOTP Remove button");

        let mut totp_lbl_status = nwg::Label::default();
        nwg::Label::builder()
            .text("")
            .parent(&totp_frame)
            .position((8, 470))
            .size((260, 18))
            .build(&mut totp_lbl_status)
            .expect("Failed to build TOTP status label");

        let mut totp_timer = nwg::AnimationTimer::default();
        nwg::AnimationTimer::builder()
            .parent(&window)
            .interval(std::time::Duration::from_secs(1))
            .active(false)
            .build(&mut totp_timer)
            .expect("Failed to build TOTP timer");

        // __ Attachments panel controls __
        let mut attach_frame = nwg::Frame::default();
        nwg::Frame::builder()
            .parent(&window)
            .position((0, 0))
            .size((crate::ui::attachments_panel::ATTACHMENTS_PANEL_WIDTH as i32, 600))
            .build(&mut attach_frame)
            .expect("Failed to build attachments frame");

        let mut attach_list = nwg::ListBox::default();
        nwg::ListBox::builder()
            .parent(&attach_frame)
            .position((8, 8))
            .size((260, 400))
            .build(&mut attach_list)
            .expect("Failed to build attachments list");

        let mut attach_btn_add = nwg::Button::default();
        nwg::Button::builder()
            .text(&t("attach.btn.add"))
            .parent(&attach_frame)
            .position((8, 416))
            .size((58, 26))
            .build(&mut attach_btn_add)
            .expect("Failed to build attach Add button");

        let mut attach_btn_open = nwg::Button::default();
        nwg::Button::builder()
            .text(&t("attach.btn.open"))
            .parent(&attach_frame)
            .position((70, 416))
            .size((58, 26))
            .build(&mut attach_btn_open)
            .expect("Failed to build attach Open button");

        let mut attach_btn_save = nwg::Button::default();
        nwg::Button::builder()
            .text(&t("attach.btn.save"))
            .parent(&attach_frame)
            .position((132, 416))
            .size((58, 26))
            .build(&mut attach_btn_save)
            .expect("Failed to build attach Save button");

        let mut attach_btn_delete = nwg::Button::default();
        nwg::Button::builder()
            .text(&t("attach.btn.del"))
            .parent(&attach_frame)
            .position((194, 416))
            .size((58, 26))
            .build(&mut attach_btn_delete)
            .expect("Failed to build attach Delete button");

        let mut attach_lbl_status = nwg::Label::default();
        nwg::Label::builder()
            .text("")
            .parent(&attach_frame)
            .position((8, 448))
            .size((260, 18))
            .build(&mut attach_lbl_status)
            .expect("Failed to build attach status label");

        // ── Auto-lock timer ──
        let mut autolock_timer = nwg::AnimationTimer::default();
        nwg::AnimationTimer::builder()
            .parent(&window)
            .interval(std::time::Duration::from_secs(10)) // check every 10s
            .active(settings.auto_lock > 0)
            .build(&mut autolock_timer)
            .expect("Failed to build autolock timer");

        // ── Auto-save timer ──
        let mut autosave_timer = nwg::AnimationTimer::default();
        nwg::AnimationTimer::builder()
            .parent(&window)
            .interval(std::time::Duration::from_secs(5)) // check every 5s
            .active(settings.auto_save > 0)
            .build(&mut autosave_timer)
            .expect("Failed to build autosave timer");

        // ── Clipboard guard timer ──
        let mut clipboard_timer = nwg::AnimationTimer::default();
        nwg::AnimationTimer::builder()
            .parent(&window)
            .interval(std::time::Duration::from_secs(5)) // check every 5s
            .active(settings.clipboard_clear > 0)
            .build(&mut clipboard_timer)
            .expect("Failed to build clipboard timer");

        // ── Lock overlay ──
        let mut lock_frame = nwg::Frame::default();
        nwg::Frame::builder()
            .parent(&window)
            .position((0, 0))
            .size((900, 650))
            .build(&mut lock_frame)
            .expect("Failed to build lock frame");
        {
            // Hide lock frame initially
            if let nwg::ControlHandle::Hwnd(hwnd) = lock_frame.handle {
                unsafe {
                    windows::Win32::UI::WindowsAndMessaging::ShowWindow(
                        windows::Win32::Foundation::HWND(hwnd as *mut _),
                        windows::Win32::UI::WindowsAndMessaging::SW_HIDE,
                    );
                }
            }
        }

        // Sizes, positions and fonts of the lock screen controls are set by
        // `layout_lock_overlay` (DPI-aware) every time the overlay is shown or
        // the window is resized; only the kind of control matters here.
        // Creation order = Tab order: field, eye, Unlock.
        let mut lock_label = nwg::Label::default();
        nwg::Label::builder()
            .parent(&lock_frame)
            .text(&t("lock.overlay.title"))
            .h_align(nwg::HTextAlign::Center)
            .build(&mut lock_label)
            .expect("Failed to build lock label");

        let mut lock_subtitle = nwg::Label::default();
        nwg::Label::builder()
            .parent(&lock_frame)
            .text("")
            .h_align(nwg::HTextAlign::Center)
            .build(&mut lock_subtitle)
            .expect("Failed to build lock subtitle");

        let lock_input = crate::ui::lock_card::build_password_input(&lock_frame.handle);

        let mut lock_eye = nwg::Button::default();
        nwg::Button::builder()
            .parent(&lock_frame)
            .text(&t("unlock.show"))
            .build(&mut lock_eye)
            .expect("Failed to build lock show/hide button");

        let mut lock_btn = nwg::Button::default();
        nwg::Button::builder()
            .parent(&lock_frame)
            .text(&t("unlock.btn"))
            .build(&mut lock_btn)
            .expect("Failed to build lock button");

        let mut lock_status = nwg::Label::default();
        nwg::Label::builder()
            .parent(&lock_frame)
            .text("")
            .h_align(nwg::HTextAlign::Center)
            .build(&mut lock_status)
            .expect("Failed to build lock status label");

        crate::ui::dialogs::bind_enter_to_button(window.handle, lock_btn.handle);

        // Inline unlock: 1s countdown timer for exponential backoff after
        // failed attempts. Disabled by default; `on_unlock_overlay_submit`
        // starts it when a wrong password is entered.
        let mut lock_backoff_timer = nwg::AnimationTimer::default();
        nwg::AnimationTimer::builder()
            .parent(&window)
            .interval(std::time::Duration::from_secs(1))
            .active(false)
            .build(&mut lock_backoff_timer)
            .expect("Failed to build lock backoff timer");

        // ── Occurrence highlight timer ──
        let mut highlight_timer = nwg::AnimationTimer::default();
        nwg::AnimationTimer::builder()
            .parent(&window)
            .interval(std::time::Duration::from_millis(200))
            .active(true)
            .build(&mut highlight_timer)
            .expect("Failed to build highlight timer");

        EditorForm {
            window,
            // File menu
            menu_file,
            menu_file_save,
            menu_file_sep1,
            menu_file_print,
            menu_file_export,
            menu_file_backup_export,
            menu_file_backup_import,
            menu_file_import_exe,
            menu_file_backups,
            menu_file_sep2,
            menu_file_settings,
            menu_file_settings_preferences,
            menu_file_settings_security,
            menu_file_settings_tempfiles,
            menu_file_sep3,
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
            menu_view_line_numbers,
            // Tab menu
            menu_tab,
            menu_tab_new,
            menu_tab_close,
            menu_tab_rename,
            menu_tab_next,
            menu_tab_prev,
            // Tools menu
            menu_tools,
            menu_tools_vault,
            menu_tools_prompts,
            menu_tools_passgen,
            menu_tools_totp,
            menu_tools_attachments,
            // Toolbar toggle buttons
            toolbar_prompts,
            toolbar_vault,
            toolbar_totp,
            toolbar_attach,
            toolbar_lock,
            menu_tools_stego,
            menu_tools_stego_hide,
            menu_tools_stego_extract,
            menu_tools_ssh,
            menu_tools_ssh_generate,
            menu_tools_ssh_agent,
            // Help menu
            menu_help,
            menu_help_updates,
            menu_help_about,
            // Context menu
            context_menu,
            ctx_cut,
            ctx_copy,
            ctx_paste,
            ctx_paste_plain,
            ctx_sep1,
            ctx_select_all,
            // Line number gutter
            gutter_hwnd: RefCell::new(gutter_hwnd),
            gutter_caret_line: RefCell::new(0),
            dblclick_armed: RefCell::new(false),
            // Editor
            editor,
            editor_font,
            // Status bar
            status_bar,
            status_path_slot: Cell::new(None),
            status_path_feedback: RefCell::new(None),
            status_path_pressed: Cell::new(false),
            modal_depth: Default::default(),
            autosave_gate: Default::default(),
            lock_pending: Cell::new(None),
            close_box_open: Cell::new(false),
            close_pending: Cell::new(None),
            lock_epoch: Cell::new(0),
            curtain: Cell::new(self::modal_guard::Curtain::None),
            lock_save_failed_reason: RefCell::new(None),
            last_mouse_pos: Cell::new(None),
            shutdown_blocked: Cell::new(false),
            wipe_pending: Cell::new(false),
            exiting: Cell::new(false),
            clipboard_read_failures: Cell::new(0),
            // Tray
            tray_icon,
            tray_menu,
            tray_menu_restore,
            tray_menu_quit,
            // Vault panel
            vault_panel: crate::ui::vault_panel::VaultPanel::new(),
            vault_frame,
            vault_search,
            vault_tree,
            vault_reveal,
            vault_copied,
            suppress_tree_events: Cell::new(false),
            vault_drag: Cell::new(None),
            vault_btn_add,
            vault_btn_edit,
            vault_btn_delete,
            vault_btn_copy,
            vault_btn_reveal,
            vault_timer,
            copy_feedback_time: Cell::new(None),
            // Prompts panel
            prompts_panel: crate::ui::prompts_panel::PromptsPanel::new(),
            prompts_frame,
            prompts_search,
            prompts_list,
            prompts_preview,
            prompts_btn_add,
            prompts_btn_edit,
            prompts_btn_delete,
            prompts_btn_copy,
            prompts_status,
            // TOTP panel
            totp_panel: crate::ui::totp_panel::TotpPanel::new(),
            totp_frame,
            totp_list,
            totp_lbl_countdown,
            totp_btn_copy,
            totp_btn_add,
            totp_btn_remove,
            totp_lbl_status,
            totp_timer,
            // Attachments panel
            attach_panel: crate::ui::attachments_panel::AttachmentsPanel::new(),
            attach_frame,
            attach_list,
            attach_btn_add,
            attach_btn_open,
            attach_btn_save,
            attach_btn_delete,
            attach_lbl_status,
            // Occurrence highlighting
            highlight_timer,
            last_highlighted: RefCell::new(String::new()),
            suppress_change: RefCell::new(false),
            // Feature state
            autolock: RefCell::new(crate::features::autolock::AutoLock::new(settings.auto_lock)),
            autolock_timer,
            lock_frame,
            lock_label,
            lock_subtitle,
            lock_input,
            lock_eye,
            lock_btn,
            lock_status,
            lock_status_msg: crate::ui::lock_card::StatusMsg::new(),
            lock_fonts: RefCell::new(None),
            lock_attempts: RefCell::new(0),
            is_locked: RefCell::new(false),
            locked_session_state: Cell::new(None),
            lock_backoff_timer,
            lock_backoff_remaining: Cell::new(0),
            lock_hidden_siblings: RefCell::new(Vec::new()),
            autosave: RefCell::new(crate::features::autosave::AutoSave::new(settings.auto_save)),
            autosave_timer,
            clipboard_guard: RefCell::new(crate::features::clipboard_guard::ClipboardGuard::new(settings.clipboard_clear)),
            clipboard_timer,
            global_hotkey: RefCell::new(None), // registered after window is shown
            detected_encoding: RefCell::new("UTF-8".to_string()),
            tab_bar: RefCell::new(crate::ui::tab_bar::TabBar::new()),
            tab_bar_hwnd: RefCell::new(tab_bar_hwnd),
            panel_grip_hwnd,
            panel_drag: Cell::new(None),
            right_clicked_tab: RefCell::new(None),
            tab_drag: Cell::new(None),
            tab_drag_over: Cell::new(None),
            // State
            password: RefCell::new(password),
            is_modified: Arc::new(AtomicBool::new(false)),
            start_maximized,
            exe_path,
            settings: RefCell::new(settings),
            always_on_top: RefCell::new(false),
            attachments: RefCell::new(Vec::new()),
            ssh_agent_keys: RefCell::new(Vec::new()),
            ssh_agent_unconfirmed: RefCell::new(Vec::new()),
        }
    }
}
