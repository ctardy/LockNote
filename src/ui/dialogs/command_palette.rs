// CommandPaletteDialog — Ctrl+Shift+P command palette
//
// A filter field over a list. Typing re-ranks the list (fuzzy subsequence,
// `ui::command_palette::rank`); Up / Down / Page Up / Page Down move the
// selection while the focus stays in the field; Enter or a double-click
// runs the selected row; Esc closes.
// Returns Option<usize>: the index of the chosen entry, or None.

use native_windows_gui as nwg;
use std::cell::RefCell;
use std::rc::Rc;

use crate::ui::command_palette::{rank, row_text, PaletteEntry};

fn t(key: &str) -> String {
    crate::features::i18n::get(key).to_string()
}

const LBS_USETABSTOPS: u32 = 0x0080;
const LB_SETTABSTOPS: u32 = 0x0192;
const WM_KEYDOWN: u32 = 0x0100;
const WM_COMMAND: u32 = 0x0111;
const IDOK: usize = 1;
/// Column of the shortcuts, in quarters of the list font's average
/// character width (the unit of LB_SETTABSTOPS): about 48 characters.
const SHORTCUT_TAB_STOP: i32 = 4 * 48;
/// Rows moved by Page Up / Page Down.
const PAGE_ROWS: usize = 10;

/// New selection after an arrow key: `None` when the key does not move it.
/// Pure, so the clamping is tested (`len` rows, `current` selected).
pub(crate) fn moved_selection(vk: usize, current: Option<usize>, len: usize) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let last = len - 1;
    let cur = current.unwrap_or(0).min(last);
    match vk {
        0x26 => Some(cur.saturating_sub(1)),            // VK_UP
        0x28 => Some((cur + 1).min(last)),              // VK_DOWN
        0x21 => Some(cur.saturating_sub(PAGE_ROWS)),    // VK_PRIOR
        0x22 => Some((cur + PAGE_ROWS).min(last)),      // VK_NEXT
        _ => None,
    }
}

pub struct CommandPaletteDialog;

impl CommandPaletteDialog {
    /// Show the palette over `entries` (listed in this order while the
    /// filter is empty).
    pub fn show(entries: &[PaletteEntry]) -> Option<usize> {
        let mut window = Default::default();
        nwg::Window::builder()
            .size((560, 400))
            .position((350, 200))
            .title(&t("palette.title"))
            .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
            .build(&mut window)
            .expect("Failed to build CommandPaletteDialog window");

        super::apply_dialog_theme(&window);

        let mut font = Default::default();
        nwg::Font::builder()
            .family("Segoe UI")
            .size(16)
            .build(&mut font)
            .expect("Failed to build font");

        let mut txt_filter = Default::default();
        nwg::TextInput::builder()
            .text("")
            .parent(&window)
            .position((15, 15))
            .size((530, 25))
            .font(Some(&font))
            .focus(true)
            .build(&mut txt_filter)
            .expect("Failed to build filter input");

        // nwg's ListBoxFlags has no LBS_USETABSTOPS; the style must be given
        // at creation (the list box reads it once).
        // SAFETY: from_bits_unchecked only skips the "known bits" check of
        // bitflags; the value is passed unchanged as the window style.
        let list_flags = unsafe {
            nwg::ListBoxFlags::from_bits_unchecked(
                (nwg::ListBoxFlags::VISIBLE | nwg::ListBoxFlags::TAB_STOP).bits() | LBS_USETABSTOPS,
            )
        };
        let mut list: nwg::ListBox<String> = Default::default();
        nwg::ListBox::builder()
            .parent(&window)
            .position((15, 50))
            .size((530, 335))
            .font(Some(&font))
            .flags(list_flags)
            .build(&mut list)
            .expect("Failed to build command list");
        if let nwg::ControlHandle::Hwnd(h) = list.handle {
            let stops = [SHORTCUT_TAB_STOP];
            let ok = unsafe {
                windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                    windows::Win32::Foundation::HWND(h as *mut _),
                    LB_SETTABSTOPS,
                    Some(windows::Win32::Foundation::WPARAM(stops.len())),
                    Some(windows::Win32::Foundation::LPARAM(stops.as_ptr() as isize)),
                )
            };
            if ok.0 == 0 {
                // Rows stay readable, the shortcut just follows the label.
                eprintln!("WARN palette: LB_SETTABSTOPS refused, shortcuts not aligned");
            }
        }

        let list = Rc::new(list);
        let labels: Vec<String> = entries.iter().map(|e| e.label.clone()).collect();
        let rows_text: Vec<String> = entries.iter().map(row_text).collect();
        // Entry index shown on each row.
        let shown: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));
        let result: Rc<RefCell<Option<usize>>> = Rc::new(RefCell::new(None));

        let refill = {
            let list = list.clone();
            let shown = shown.clone();
            move |query: &str| {
                let order = rank(query, &labels);
                list.set_collection(order.iter().map(|&i| rows_text[i].clone()).collect());
                list.set_selection(if order.is_empty() { None } else { Some(0) });
                *shown.borrow_mut() = order;
            }
        };
        refill("");

        // Run the selected row (Enter or double-click).
        let choose = {
            let list = list.clone();
            let shown = shown.clone();
            let result = result.clone();
            move || {
                let picked = list.selection().and_then(|row| shown.borrow().get(row).copied());
                if let Some(i) = picked {
                    *result.borrow_mut() = Some(i);
                    nwg::stop_thread_dispatch();
                }
            }
        };

        // Arrows in the filter field move the list selection.
        let keys = {
            let list = list.clone();
            nwg::bind_raw_event_handler(&txt_filter.handle, 0x30100, move |_hwnd, msg, wparam, _lparam| {
                if msg != WM_KEYDOWN {
                    return None;
                }
                let next = moved_selection(wparam as usize, list.selection(), list.len())?;
                list.set_selection(Some(next));
                Some(0)
            })
        };
        let keys = match keys {
            Ok(h) => Some(h),
            Err(e) => {
                eprintln!("WARN palette: arrow keys in the filter not bound err={:?}", e);
                None
            }
        };

        // IsDialogMessage turns Enter into WM_COMMAND(IDOK) on the window.
        let enter = {
            let choose = choose.clone();
            nwg::bind_raw_event_handler(&window.handle, 0x30101, move |_hwnd, msg, wparam, _lparam| {
                if msg == WM_COMMAND && (wparam & 0xFFFF) == IDOK {
                    choose();
                    return Some(0);
                }
                None
            })
        };
        let enter = match enter {
            Ok(h) => Some(h),
            Err(e) => {
                eprintln!("WARN palette: Enter not bound err={:?}", e);
                None
            }
        };

        let window_handle = window.handle;
        let list_handle = list.handle;
        let filter_handle = txt_filter.handle;
        let handler = nwg::full_bind_event_handler(&window_handle, move |evt, _evt_data, handle| {
            match evt {
                nwg::Event::OnTextInput if handle == filter_handle => {
                    refill(&txt_filter.text());
                }
                nwg::Event::OnListBoxDoubleClick if handle == list_handle => {
                    choose();
                }
                nwg::Event::OnWindowClose => {
                    nwg::stop_thread_dispatch();
                }
                _ => {}
            }
        });

        nwg::dispatch_thread_events();
        nwg::unbind_event_handler(&handler);
        for raw in [keys, enter].into_iter().flatten() {
            if let Err(e) = nwg::unbind_raw_event_handler(&raw) {
                eprintln!("WARN palette: raw handler not unbound err={:?}", e);
            }
        }

        let out = result.borrow_mut().take();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::moved_selection;

    #[test]
    fn arrows_move_and_clamp_the_selection() {
        assert_eq!(moved_selection(0x28, Some(0), 3), Some(1));
        assert_eq!(moved_selection(0x28, Some(2), 3), Some(2));
        assert_eq!(moved_selection(0x26, Some(0), 3), Some(0));
        assert_eq!(moved_selection(0x26, Some(2), 3), Some(1));
        assert_eq!(moved_selection(0x22, Some(1), 30), Some(11));
        assert_eq!(moved_selection(0x22, Some(25), 30), Some(29));
        assert_eq!(moved_selection(0x21, Some(5), 30), Some(0));
    }

    #[test]
    fn no_selection_starts_from_the_top_and_an_empty_list_does_not_move() {
        assert_eq!(moved_selection(0x28, None, 3), Some(1));
        assert_eq!(moved_selection(0x26, None, 3), Some(0));
        assert_eq!(moved_selection(0x28, None, 0), None);
        assert_eq!(moved_selection(0x28, Some(9), 3), Some(2)); // stale selection clamped
    }

    #[test]
    fn other_keys_are_left_to_the_field() {
        for vk in [0x25, 0x27, 0x0D, 0x1B, 0x41, 0x24, 0x23] {
            assert_eq!(moved_selection(vk, Some(1), 3), None, "{vk:#x}");
        }
    }
}
