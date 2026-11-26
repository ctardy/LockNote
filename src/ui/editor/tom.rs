// Text Object Model (TOM) of the editor RichEdit, for what its messages
// cannot do: suspend the undo, and read which end of the selection is the
// active one (EM_GETSEL always gives start <= end).

use windows::core::{IUnknown, Interface};
use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::UI::Controls::RichEdit::{tomResume, tomSelStartActive, tomSuspend, ITextDocument};
use windows::Win32::UI::WindowsAndMessaging::SendMessageW;

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
/// is kept. `f` is not run when the undo cannot be suspended.
pub(super) fn without_undo<R>(hwnd: HWND, f: impl FnOnce() -> R) -> Result<R, String> {
    let doc = text_document(hwnd)?;
    unsafe { doc.Undo(tomSuspend.0) }.map_err(|e| format!("Undo(tomSuspend) failed: {e}"))?;
    let r = f();
    unsafe { doc.Undo(tomResume.0) }.map_err(|e| format!("Undo(tomResume) failed, undo stays suspended: {e}"))?;
    Ok(r)
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
