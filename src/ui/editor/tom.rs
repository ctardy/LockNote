// Text Object Model (TOM) of the editor RichEdit, for what its messages
// cannot do: suspend the undo, and read which end of the selection is the
// active one (EM_GETSEL always gives start <= end).

use windows::core::{IUnknown, Interface};
use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::UI::Controls::RichEdit::{tomResume, tomSelStartActive, tomSuspend, ITextDocument};
use windows::Win32::UI::WindowsAndMessaging::SendMessageW;

use super::undo_watch::{UndoError, UndoStuck};

/// The editor HWND of a nwg control handle, `None` before it is created.
pub(super) fn hwnd_of(handle: native_windows_gui::ControlHandle) -> Option<HWND> {
    match handle {
        native_windows_gui::ControlHandle::Hwnd(h) => Some(HWND(h as *mut _)),
        _ => None,
    }
}

/// The ITextDocument of a RichEdit (EM_GETOLEINTERFACE).
fn text_document(hwnd: HWND) -> Result<ITextDocument, String> {
    let mut ole: *mut core::ffi::c_void = std::ptr::null_mut();
    // EM_GETOLEINTERFACE = WM_USER + 60
    let ok = unsafe { SendMessageW(hwnd, 0x043C, None, Some(LPARAM(&mut ole as *mut _ as isize))) };
    if ok.0 == 0 || ole.is_null() {
        return Err("EM_GETOLEINTERFACE returned no interface".to_string());
    }
    // Takes over the reference EM_GETOLEINTERFACE added.
    let unknown = unsafe { IUnknown::from_raw(ole) };
    unknown.cast::<ITextDocument>().map_err(|e| format!("no ITextDocument: {e}"))
}

/// Run `f` with the undo of the RichEdit suspended. In rich-text mode a
/// character format change (EM_SETCHARFORMAT) is an undo action: without
/// this, a format pass that is only presentation (the occurrence
/// highlights) takes the place of the last edit on Ctrl+Z. The undo stack
/// is kept. `f` is not run when the undo cannot be suspended
/// (`NotSuspended`); `NotResumed` means `f` ran and the history may stay
/// suspended, which the window tells the user (`undo_watch`).
pub(super) fn without_undo<R>(hwnd: HWND, f: impl FnOnce() -> R) -> Result<R, UndoError> {
    let doc = text_document(hwnd).map_err(UndoError::NotSuspended)?;
    run_suspended(|| unsafe { doc.Undo(tomSuspend.0) }.map(drop), f, || unsafe { doc.Undo(tomResume.0) }.map(drop))
}

/// `without_undo` on its two TOM calls (tests give failing ones).
fn run_suspended<R>(
    suspend: impl FnOnce() -> windows::core::Result<()>,
    f: impl FnOnce() -> R,
    resume: impl FnMut() -> windows::core::Result<()>,
) -> Result<R, UndoError> {
    suspend().map_err(|e| UndoError::NotSuspended(format!("Undo(tomSuspend) failed: {e}")))?;
    let r = f();
    resume_with_retry(resume, "a format pass").map_err(UndoError::NotResumed)?;
    Ok(r)
}

/// Resume an undo history suspended for presentation (`why`). A history left
/// suspended would make every later edit impossible to undo: one retry,
/// then the failure is returned for the window to tell the user.
pub(super) fn resume_undo(doc: &ITextDocument, why: &str) -> Result<(), UndoStuck> {
    resume_with_retry(|| unsafe { doc.Undo(tomResume.0) }.map(drop), why)
}

/// `resume_undo` on its TOM call (tests give a failing one).
fn resume_with_retry(mut resume: impl FnMut() -> windows::core::Result<()>, why: &str) -> Result<(), UndoStuck> {
    let Err(first) = resume() else {
        return Ok(());
    };
    eprintln!("WARN undo: tomResume failed after {why} ({first}), retrying");
    resume().map_err(|e| UndoStuck(format!("Undo(tomResume) failed twice after {why} ({first}; {e})")))
}

/// The selection as (anchor, active end), positions as EM_GETSEL gives
/// them. A selection made backwards (Shift+Up) has its active end first;
/// EM_SETSEL(anchor, active) puts it back as it was.
pub(super) fn selection_ends(hwnd: HWND) -> Result<(u32, u32), String> {
    let sel = unsafe { text_document(hwnd)?.GetSelection() }.map_err(|e| format!("GetSelection failed: {e}"))?;
    let (start, end, flags) = unsafe {
        let start = sel.GetStart().map_err(|e| format!("GetStart failed: {e}"))?;
        let end = sel.GetEnd().map_err(|e| format!("GetEnd failed: {e}"))?;
        let flags = sel.GetFlags().map_err(|e| format!("GetFlags failed: {e}"))?;
        (start, end, flags)
    };
    let (start, end) = (start.max(0) as u32, end.max(0) as u32);
    Ok(if flags & tomSelStartActive.0 != 0 { (end, start) } else { (start, end) })
}

/// Tests: what Shift+Right does, move the active end one character right.
#[cfg(test)]
pub(super) fn extend_selection_right(hwnd: HWND) -> Result<(), String> {
    use windows::Win32::UI::Controls::RichEdit::{tomCharacter, tomExtend};
    let sel = unsafe { text_document(hwnd)?.GetSelection() }.map_err(|e| format!("GetSelection failed: {e}"))?;
    unsafe { sel.MoveRight(tomCharacter.0, 1, tomExtend.0) }.map_err(|e| format!("MoveRight failed: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn fail() -> windows::core::Result<()> {
        Err(windows::core::Error::from_hresult(windows::Win32::Foundation::E_FAIL))
    }

    /// A resume that fails its first `failures` calls; counts them all.
    fn resume_failing(failures: u32, calls: &Cell<u32>) -> impl FnMut() -> windows::core::Result<()> + '_ {
        move || {
            calls.set(calls.get() + 1);
            if calls.get() <= failures { fail() } else { Ok(()) }
        }
    }

    #[test]
    fn a_resume_is_retried_once_then_reported() {
        let calls = Cell::new(0);
        assert_eq!(resume_with_retry(resume_failing(0, &calls), "a font change"), Ok(()));
        assert_eq!(calls.get(), 1);

        let calls = Cell::new(0);
        assert_eq!(resume_with_retry(resume_failing(1, &calls), "a font change"), Ok(()), "the retry worked");
        assert_eq!(calls.get(), 2);

        let calls = Cell::new(0);
        let r = resume_with_retry(resume_failing(2, &calls), "a font change");
        assert_eq!(calls.get(), 2, "one retry, no more");
        let UndoStuck(detail) = r.expect_err("two failures are returned, not only logged");
        assert!(detail.contains("a font change"), "{detail}");
    }

    #[test]
    fn without_undo_types_its_two_failures() {
        let ran = Cell::new(false);
        let r = run_suspended(fail, || ran.set(true), || Ok(()));
        assert!(matches!(r, Err(UndoError::NotSuspended(_))), "{r:?}");
        assert!(!ran.get(), "nothing is painted with the undo on");

        let calls = Cell::new(0);
        let r = run_suspended(|| Ok(()), || 7, resume_failing(2, &calls));
        assert!(matches!(r, Err(UndoError::NotResumed(_))), "{r:?}");
        assert_eq!(calls.get(), 2, "the highlights get the same retry as the font");

        let calls = Cell::new(0);
        assert_eq!(run_suspended(|| Ok(()), || 7, resume_failing(1, &calls)), Ok(7));
    }

    // -----------------------------------------------------------------
    // Live RichEdit test: what undo_watch relies on. It creates a
    // RICHEDIT50W in the test process (never the application).
    // -----------------------------------------------------------------

    mod live {
        use super::*;
        use windows::core::w;
        use windows::Win32::Foundation::WPARAM;
        use windows::Win32::System::LibraryLoader::LoadLibraryW;
        use windows::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WINDOW_STYLE, WM_SETTEXT, WS_CHILD, WS_POPUP,
        };

        const EM_REPLACESEL: u32 = 0x00C2;
        const EM_CANUNDO: u32 = 0x00C6;
        const EM_EMPTYUNDOBUFFER: u32 = 0x00CD;

        fn send(hwnd: HWND, msg: u32, wparam: usize, lparam: isize) -> isize {
            unsafe { SendMessageW(hwnd, msg, Some(WPARAM(wparam)), Some(LPARAM(lparam))) }.0
        }

        /// Type one character as an undoable edit, then whether Ctrl+Z
        /// could undo it; the undo buffer is emptied before and after.
        fn edit_is_undoable(ed: HWND) -> bool {
            send(ed, EM_EMPTYUNDOBUFFER, 0, 0);
            let x: Vec<u16> = "x".encode_utf16().chain(std::iter::once(0)).collect();
            send(ed, EM_REPLACESEL, 1, x.as_ptr() as isize);
            let undoable = send(ed, EM_CANUNDO, 0, 0) != 0;
            send(ed, EM_EMPTYUNDOBUFFER, 0, 0);
            undoable
        }

        #[test]
        fn tom_suspend_and_resume_are_an_on_off_switch() {
            unsafe {
                LoadLibraryW(w!("msftedit.dll")).expect("msftedit.dll");
                let top = CreateWindowExW(
                    WINDOW_EX_STYLE(0), w!("STATIC"), None, WS_POPUP, 0, 0, 200, 100, None, None, None, None,
                )
                .expect("host window");
                // ES_MULTILINE, as the editor.
                let ed = CreateWindowExW(
                    WINDOW_EX_STYLE(0), w!("RICHEDIT50W"), None, WS_CHILD | WINDOW_STYLE(0x0004), 0, 0, 180, 80,
                    Some(top), None, None, None,
                )
                .expect("RICHEDIT50W");
                let text: Vec<u16> = "abc".encode_utf16().chain(std::iter::once(0)).collect();
                send(ed, WM_SETTEXT, 0, text.as_ptr() as isize);
                let doc = text_document(ed).expect("ITextDocument");
                let suspend = || doc.Undo(tomSuspend.0).map(drop).expect("tomSuspend");
                let resume = || doc.Undo(tomResume.0).map(drop).expect("tomResume");

                assert!(edit_is_undoable(ed), "the history works to begin with");
                suspend();
                assert!(!edit_is_undoable(ed), "suspended: the test sees a stuck history");
                resume();
                assert!(edit_is_undoable(ed));
                // A failed resume, then a later suspend + resume that works
                // (undo_watch: Stuck -> Live): not a nesting counter.
                suspend();
                suspend();
                resume();
                assert!(edit_is_undoable(ed), "one resume brings back a history suspended twice");
                // A resume with nothing suspended does no harm.
                resume();
                assert!(edit_is_undoable(ed));
                suspend();
                assert!(!edit_is_undoable(ed), "an extra resume does not cancel the next suspend");
                resume();

                let _ = DestroyWindow(top);
            }
        }
    }
}
