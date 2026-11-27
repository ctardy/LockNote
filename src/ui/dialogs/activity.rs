// Input in LockNote's own dialogs counts for the auto-lock (issue #32).
//
// Every dialog runs its own message loop (nwg's dispatch loop, or the
// system's for a message box), so the main window's handlers never see what
// is typed in one. One thread hook (WH_GETMESSAGE, this thread only) sees
// every message any loop of the UI thread takes from the queue: a key press,
// a left click or a real mouse move in a LockNote dialog (the same kinds as
// in the main window, `counts_as_activity`) records when it happened. The
// auto-lock timer takes that time at its next tick (`take_input`,
// `ui::editor::lock_flow::autolock_tick`), which ignores it under the curtain.
//
// The hook only reads: it records an instant in a thread-local cell and
// always passes the message on. It never touches the window state (no
// `RefCell` of the form is borrowed inside a dialog's loop).

use std::cell::Cell;

/// WM_KEYDOWN, WM_MOUSEMOVE, WM_LBUTTONDOWN.
const WM_KEYDOWN: u32 = 0x0100;
const WM_MOUSEMOVE: u32 = 0x0200;
const WM_LBUTTONDOWN: u32 = 0x0201;

/// Whether a message is activity: `in_own_dialog`, its window belongs to a
/// LockNote dialog or box (`is_own_dialog`); a key press, a left click, or a
/// mouse move to another position than the last one seen on this thread
/// (`last_pos`, `pos`: packed screen positions, as `mouse_really_moved`).
pub(crate) fn counts_as_activity(msg: u32, in_own_dialog: bool, last_pos: Option<isize>, pos: isize) -> bool {
    if !in_own_dialog {
        return false;
    }
    match msg {
        WM_KEYDOWN | WM_LBUTTONDOWN => true,
        WM_MOUSEMOVE => crate::ui::editor::mouse_really_moved(last_pos, pos),
        _ => false,
    }
}

/// Whether the top-level window `root` is one of LockNote's dialogs or
/// boxes: the windows the lock cancels (`cancel_open_dialogs`) — a
/// registered LockNote dialog, the unlock window, or a standard dialog
/// `#32770` of this thread (message boxes of `confirm()` / `inform()`, file
/// dialogs). Popup menus are not dialogs.
pub(crate) fn is_own_dialog(root: isize) -> bool {
    if root == 0 {
        return false;
    }
    if super::style::is_registered_dialog(root) || super::unlock::open_window() == Some(root) {
        return true;
    }
    let mut buf = [0u16; 16];
    let n = unsafe { windows::Win32::UI::WindowsAndMessaging::GetClassNameW(windows::Win32::Foundation::HWND(root as *mut _), &mut buf) } as usize;
    buf[..n].iter().copied().eq("#32770".encode_utf16())
}

thread_local! {
    /// When the last input in a dialog happened, not yet taken by the timer.
    static LAST_INPUT: Cell<Option<std::time::Instant>> = const { Cell::new(None) };
    /// The cursor position of the last WM_MOUSEMOVE of this thread, any
    /// window: a dialog shown under a still cursor gets a move at the same
    /// place, which is not activity (as the main window, pass 6).
    static LAST_POS: Cell<Option<isize>> = const { Cell::new(None) };
    /// The installed hook (0: none yet).
    static HOOK: Cell<isize> = const { Cell::new(0) };
}

/// Record an input in a dialog, now.
fn record_input() {
    LAST_INPUT.with(|c| c.set(Some(std::time::Instant::now())));
}

/// A screen position packed as a mouse message's lParam.
fn pack(pt: windows::Win32::Foundation::POINT) -> isize {
    ((pt.y as u16 as isize) << 16) | pt.x as u16 as isize
}

/// WH_GETMESSAGE: called inside GetMessage / PeekMessage of every loop of
/// this thread. Reads the message, never changes it, always passes it on.
unsafe extern "system" fn hook_proc(code: i32, wparam: windows::Win32::Foundation::WPARAM, lparam: windows::Win32::Foundation::LPARAM) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::{CallNextHookEx, GetAncestor, GA_ROOT, MSG};
    // HC_ACTION = 0; PM_REMOVE = 1: the message leaves the queue (a peek
    // that leaves it there sees it again when it is taken).
    if code == 0 && wparam.0 == 1 && lparam.0 != 0 {
        let m = unsafe { &*(lparam.0 as *const MSG) };
        if matches!(m.message, WM_KEYDOWN | WM_MOUSEMOVE | WM_LBUTTONDOWN) {
            let pos = pack(m.pt);
            let last = if m.message == WM_MOUSEMOVE { LAST_POS.with(|c| c.replace(Some(pos))) } else { None };
            let root = unsafe { GetAncestor(m.hwnd, GA_ROOT) }.0 as isize;
            if counts_as_activity(m.message, is_own_dialog(root), last, pos) {
                record_input();
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// The time of the last input in a dialog since the previous call.
pub(crate) fn take_input() -> Option<std::time::Instant> {
    LAST_INPUT.with(|c| c.take())
}

/// Install the hook on the UI thread (once, from `bind_events`).
pub(crate) fn install() {
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::WindowsAndMessaging::{SetWindowsHookExW, WH_GETMESSAGE};
    if HOOK.with(|h| h.get()) != 0 {
        return;
    }
    match unsafe { SetWindowsHookExW(WH_GETMESSAGE, Some(hook_proc), None, GetCurrentThreadId()) } {
        Ok(h) => {
            HOOK.with(|c| c.set(h.0 as isize));
            eprintln!("INFO autolock: input hook of the dialogs installed");
        }
        // Not fatal: the lock then only counts the main window's input
        // (it comes sooner, never later); said loudly.
        Err(e) => eprintln!("ERROR autolock: input hook of the dialogs not installed, input in a dialog will not postpone the lock err={:?}", e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use native_windows_gui as nwg;

    const WM_KEYUP: u32 = 0x0101;
    const WM_CHAR: u32 = 0x0102;
    const WM_SYSKEYDOWN: u32 = 0x0104;
    const WM_TIMER: u32 = 0x0113;
    const WM_LBUTTONUP: u32 = 0x0202;
    const WM_RBUTTONDOWN: u32 = 0x0204;
    const WM_MOUSEWHEEL: u32 = 0x020A;

    /// The decision of issue #32: input in a LockNote dialog is activity
    /// (the kinds the main window counts: key press, left click, real
    /// mouse move), nothing else is, and nothing outside LockNote's dialogs.
    #[test]
    fn input_in_a_dialog_is_activity_and_nothing_else_is() {
        let at = |x: i16, y: i16| ((y as u16 as isize) << 16) | x as u16 as isize;
        let p = at(100, 200);
        let rows: &[(&str, u32, bool, Option<isize>, bool)] = &[
            ("key press in a dialog", WM_KEYDOWN, true, None, true),
            ("left click in a dialog", WM_LBUTTONDOWN, true, None, true),
            ("mouse moved in a dialog", WM_MOUSEMOVE, true, Some(at(101, 200)), true),
            ("first mouse move seen", WM_MOUSEMOVE, true, None, true),
            ("mouse move, cursor where it was", WM_MOUSEMOVE, true, Some(p), false),
            ("key press elsewhere", WM_KEYDOWN, false, None, false),
            ("left click elsewhere", WM_LBUTTONDOWN, false, None, false),
            ("mouse moved elsewhere", WM_MOUSEMOVE, false, Some(at(1, 1)), false),
            ("key release", WM_KEYUP, true, None, false),
            ("character (its key press counted)", WM_CHAR, true, None, false),
            ("Alt+key", WM_SYSKEYDOWN, true, None, false),
            ("left button up", WM_LBUTTONUP, true, None, false),
            ("right click", WM_RBUTTONDOWN, true, None, false),
            ("wheel", WM_MOUSEWHEEL, true, None, false),
            ("timer (no one there)", WM_TIMER, true, None, false),
        ];
        for &(name, msg, own, last, expected) in rows {
            assert_eq!(counts_as_activity(msg, own, last, p), expected, "{}", name);
        }
    }

    /// A recorded input is taken once: the next tick without input gets
    /// nothing.
    #[test]
    fn a_recorded_input_is_taken_once() {
        let _ = take_input();
        let before = std::time::Instant::now();
        record_input();
        let t = take_input().expect("recorded");
        assert!(t >= before);
        assert_eq!(take_input(), None);
    }

    fn hwnd_of(h: nwg::ControlHandle) -> windows::Win32::Foundation::HWND {
        windows::Win32::Foundation::HWND(h.hwnd().expect("hwnd") as *mut _)
    }

    /// Take every queued message of this thread, as a dialog's loop does
    /// (the hook runs inside PeekMessage).
    fn pump() {
        use windows::Win32::UI::WindowsAndMessaging::{DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE};
        let mut msg = MSG::default();
        unsafe {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }

    fn post(h: windows::Win32::Foundation::HWND, msg: u32, w: usize) {
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        unsafe { windows::Win32::UI::WindowsAndMessaging::PostMessageW(Some(h), msg, WPARAM(w), LPARAM(0)) }.expect("post");
    }

    /// The real hook: a key press in a registered dialog (on its edit box,
    /// a child) is recorded; one in a window that is not a LockNote dialog
    /// is not, and nor is a message that is not input.
    #[test]
    fn live_the_hook_records_input_in_a_registered_dialog_only() {
        let _ = nwg::init();
        install();
        let mut dialog = nwg::Window::default();
        nwg::Window::builder().size((200, 100)).position((-3000, -3000)).title("dialog #32").flags(nwg::WindowFlags::WINDOW).build(&mut dialog).expect("dialog");
        crate::ui::dialogs::apply_dialog_theme(&dialog);
        let mut edit = nwg::TextInput::default();
        nwg::TextInput::builder().parent(&dialog).build(&mut edit).expect("edit");
        let mut other = nwg::Window::default();
        nwg::Window::builder().size((200, 100)).position((-3000, -3000)).title("not a dialog").flags(nwg::WindowFlags::WINDOW).build(&mut other).expect("other");
        pump();
        let _ = take_input();

        post(hwnd_of(other.handle), WM_KEYDOWN, 0x41);
        pump();
        assert_eq!(take_input(), None, "input in another window counted");

        post(hwnd_of(edit.handle), WM_TIMER, 0);
        pump();
        assert_eq!(take_input(), None, "a timer is not input");

        let before = std::time::Instant::now();
        post(hwnd_of(edit.handle), WM_KEYDOWN, 0x41);
        pump();
        let t = take_input().expect("a key press in the dialog's edit box is activity");
        assert!(t >= before);

        post(hwnd_of(dialog.handle), WM_LBUTTONDOWN, 0);
        pump();
        assert!(take_input().is_some(), "a click in the dialog is activity");
    }

    /// A message box (MessageBoxW, as `confirm()` / `inform()` show) runs
    /// the system's own loop: the hook sees it too. The box is found and
    /// answered by a thread timer running inside that loop.
    #[test]
    fn live_the_hook_records_input_in_a_message_box() {
        use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering as O};
        use windows::Win32::Foundation::{HWND, LPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{EnumThreadWindows, GetClassNameW, KillTimer, MessageBoxW, SetTimer, MB_OK};
        static FOUND: AtomicIsize = AtomicIsize::new(0);
        static STEP: AtomicU32 = AtomicU32::new(0);
        static POST_FAILED: AtomicU32 = AtomicU32::new(0);
        unsafe extern "system" fn find(h: HWND, _: LPARAM) -> windows::core::BOOL {
            let mut buf = [0u16; 16];
            let n = GetClassNameW(h, &mut buf) as usize;
            if String::from_utf16_lossy(&buf[..n]) == "#32770" {
                FOUND.store(h.0 as isize, O::SeqCst);
                return false.into();
            }
            true.into()
        }
        unsafe extern "system" fn on_timer(_: HWND, _: u32, _: usize, _: u32) {
            let tid = windows::Win32::System::Threading::GetCurrentThreadId();
            let _stopped_early = EnumThreadWindows(tid, Some(find), LPARAM(0));
            let h = FOUND.load(O::SeqCst);
            if h == 0 {
                return;
            }
            // No panic in a callback of the system (it would abort): a
            // failed post is counted and asserted after the box.
            let send = |msg: u32, w: usize| {
                use windows::Win32::Foundation::WPARAM;
                if windows::Win32::UI::WindowsAndMessaging::PostMessageW(Some(HWND(h as *mut _)), msg, WPARAM(w), LPARAM(0)).is_err() {
                    POST_FAILED.fetch_add(1, O::SeqCst);
                }
            };
            if STEP.fetch_add(1, O::SeqCst) == 0 {
                // In the box's loop: a key press on the box.
                send(WM_KEYDOWN, 0x41);
            } else {
                // IDCANCEL = 2 then WM_CLOSE: whichever the box takes.
                send(0x0111, 2);
                send(0x0010, 0);
            }
        }
        let _ = nwg::init();
        install();
        pump();
        let _ = take_input();
        FOUND.store(0, O::SeqCst);
        STEP.store(0, O::SeqCst);
        let timer = unsafe { SetTimer(None, 0, 30, Some(on_timer)) };
        assert_ne!(timer, 0, "SetTimer");
        let before = std::time::Instant::now();
        let answer = unsafe { MessageBoxW(None, windows::core::w!("issue #32 test"), windows::core::w!("LockNote test"), MB_OK) };
        let shown = answer.0 != 0;
        let err = windows::core::Error::from_thread();
        let _ = unsafe { KillTimer(None, timer) };
        assert!(shown, "MessageBoxW failed: {:?}", err);
        assert_eq!(POST_FAILED.load(O::SeqCst), 0, "a post to the box failed");
        let t = take_input().expect("a key press in a message box is activity");
        assert!(t >= before);
    }

    /// The production wiring, read as text: the hook is installed, takes
    /// its decision from `counts_as_activity` and `is_own_dialog`, records,
    /// and always passes the message on; the auto-lock timer hands the
    /// recorded input to `autolock_tick`, which applies it only without a
    /// curtain; the window step moves the AutoLock clock.
    #[test]
    fn the_dialog_input_is_wired_from_the_hook_to_the_auto_lock() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ui");
        let read = |f: &str| crate::read_source(src.join("editor").join(f));
        let body = |src: &str, fun: &str, end: &str| -> String {
            let start = src.find(fun).unwrap_or_else(|| panic!("{}", fun));
            let stop = src[start..].find(end).map_or(src.len(), |e| start + e);
            src[start..stop].to_string()
        };
        let events = read("events.rs");
        let bind = body(&events, "pub(super) fn bind_events(", "\n    }\n");
        assert!(bind.contains("crate::ui::dialogs::activity::install();"), "hook not installed");
        let tick_branch = body(&events, "if handle == f.autolock_timer.handle", "\n                        }\n");
        assert!(tick_branch.contains("f.on_autolock_tick();"), "{}", tick_branch);
        assert!(!events.contains("lock_decision()"), "the timer decides outside autolock_tick");

        let guard = read("modal_guard.rs");
        let on_tick = body(&guard, "pub(super) fn on_autolock_tick(", "\n    }\n");
        assert!(
            on_tick.contains("super::lock_flow::autolock_tick(&mut FormLockSteps { form: self }, crate::ui::dialogs::activity::take_input())"),
            "{}",
            on_tick
        );
        assert!(body(&guard, "    fn note_activity(&mut self", "\n    }\n").contains("self.form.autolock.borrow_mut().note_activity_at(at);"));

        let flow = read("lock_flow.rs");
        let tick = body(&flow, "pub(super) fn autolock_tick<", "\n}\n");
        assert!(tick.contains("if let Some(at) = dialog_input"), "{}", tick);
        assert!(tick.contains("s.note_activity(at);"), "{}", tick);
        assert!(tick.contains("if s.curtain_up()"), "{}", tick);

        let own = crate::read_source(src.join("dialogs").join("activity.rs"));
        let hook = body(&own, "unsafe extern \"system\" fn hook_proc(", "\n}\n");
        assert!(hook.contains("counts_as_activity(m.message, is_own_dialog(root), last, pos)"), "{}", hook);
        assert!(hook.contains("record_input();"), "{}", hook);
        assert!(hook.trim_end().ends_with("CallNextHookEx(None, code, wparam, lparam) }"), "{}", hook);
        let installed = body(&own, "pub(crate) fn install()", "\n}\n");
        assert!(installed.contains("SetWindowsHookExW(WH_GETMESSAGE, Some(hook_proc), None, GetCurrentThreadId())"), "{}", installed);
    }
}
