// CommandPaletteDialog — Ctrl+Shift+P command palette
//
// A filter field over a list. Typing re-ranks the list (fuzzy subsequence,
// `ui::command_palette::rank`); Up / Down move the selection by one row and
// Page Up / Page Down by one visible page while the focus stays in the
// field; Enter, or a double click on a row, runs that row; Esc closes.
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
const LB_GETITEMHEIGHT: u32 = 0x01A1;
const LB_ITEMFROMPOINT: u32 = 0x01A9;
const WM_KEYDOWN: u32 = 0x0100;
const WM_COMMAND: u32 = 0x0111;
const WM_LBUTTONDBLCLK: u32 = 0x0203;
const VK_PRIOR: usize = 0x21;
const VK_NEXT: usize = 0x22;
const IDOK: usize = 1;
/// Column of the shortcuts, in quarters of the list font's average
/// character width (the unit of LB_SETTABSTOPS): about 48 characters.
const SHORTCUT_TAB_STOP: i32 = 4 * 48;

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
        _ => None,
    }
}

/// New selection after Page Up / Page Down: one page of `page` rows up or
/// down, clamped to the first / last row. `None` for any other key and on
/// an empty list. Pure, like `moved_selection`.
pub(crate) fn paged_selection(vk: usize, current: Option<usize>, len: usize, page: usize) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let last = len - 1;
    let cur = current.unwrap_or(0).min(last);
    let page = page.max(1);
    match vk {
        VK_PRIOR => Some(cur.saturating_sub(page)),
        VK_NEXT => Some(cur.saturating_add(page).min(last)),
        _ => None,
    }
}

/// Entry run by a row: the entry `shown` on that row, or `None` for no
/// row, a row past the list, or when an entry was `already_chosen` (input
/// queued behind the first choice is still dispatched before the dialog
/// returns: the first choice wins). Pure, like `moved_selection`.
pub(crate) fn picked_entry(already_chosen: bool, row: Option<usize>, shown: &[usize]) -> Option<usize> {
    if already_chosen {
        return None;
    }
    row.and_then(|row| shown.get(row).copied())
}

/// Rows of one visible page: the whole rows that fit in the list's client
/// height (`item_height` from LB_GETITEMHEIGHT), at least one. An error
/// when the list gave no row height (LB_ERR or 0).
pub(crate) fn page_rows(client_height: i32, item_height: isize) -> Result<usize, String> {
    if item_height <= 0 {
        return Err(format!("no row height (LB_GETITEMHEIGHT={item_height})"));
    }
    let rows = client_height.max(0) as isize / item_height;
    Ok((rows as usize).max(1))
}

/// Visible rows of the list box `list`, read from the window.
fn visible_rows(list: windows::Win32::Foundation::HWND) -> Result<usize, String> {
    use windows::Win32::UI::WindowsAndMessaging::{GetClientRect, SendMessageW};
    let mut client = windows::Win32::Foundation::RECT::default();
    unsafe { GetClientRect(list, &mut client) }.map_err(|e| format!("GetClientRect: {e}"))?;
    let item_height = unsafe { SendMessageW(list, LB_GETITEMHEIGHT, Some(windows::Win32::Foundation::WPARAM(0)), None) };
    page_rows(client.bottom - client.top, item_height.0)
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

        // Run a row: the selected one (Enter) or the one double-clicked.
        // The first choice wins (see picked_entry).
        let choose = {
            let shown = shown.clone();
            let result = result.clone();
            move |row: Option<usize>| {
                let already_chosen = result.borrow().is_some();
                let picked = picked_entry(already_chosen, row, &shown.borrow());
                if let Some(i) = picked {
                    *result.borrow_mut() = Some(i);
                    nwg::stop_thread_dispatch();
                }
            }
        };

        // Arrows and Page Up / Page Down in the filter field move the list
        // selection.
        let keys = {
            let list = list.clone();
            nwg::bind_raw_event_handler(&txt_filter.handle, 0x30100, move |_hwnd, msg, wparam, _lparam| {
                if msg != WM_KEYDOWN {
                    return None;
                }
                let vk = wparam as usize;
                let (current, len) = (list.selection(), list.len());
                let next = if (vk == VK_PRIOR || vk == VK_NEXT) && len > 0 {
                    let rows = match list.handle.hwnd() {
                        Some(h) => visible_rows(windows::Win32::Foundation::HWND(h as *mut _)),
                        None => Err("the list has no window".to_string()),
                    };
                    let page = match rows {
                        Ok(page) => page,
                        Err(e) => {
                            eprintln!("WARN palette: page key ignored, visible rows unknown err={e}");
                            return None;
                        }
                    };
                    paged_selection(vk, current, len, page)
                } else {
                    moved_selection(vk, current, len)
                }?;
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
            let list = list.clone();
            let choose = choose.clone();
            nwg::bind_raw_event_handler(&window.handle, 0x30101, move |_hwnd, msg, wparam, _lparam| {
                if msg == WM_COMMAND && (wparam & 0xFFFF) == IDOK {
                    choose(list.selection());
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

        // A double click on a row runs it as Enter does. Only the row under
        // the pointer: LBN_DBLCLK also fires below the last row, where the
        // selection is some other row.
        let dblclick = {
            let list_handle = list.handle;
            let list = list.clone();
            let choose = choose.clone();
            nwg::bind_raw_event_handler(&list_handle, 0x30102, move |hwnd, msg, _wparam, lparam| {
                if msg == WM_LBUTTONDBLCLK {
                    // lparam holds the client coordinates LB_ITEMFROMPOINT
                    // takes.
                    let hit = unsafe {
                        windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                            windows::Win32::Foundation::HWND(hwnd as *mut _),
                            LB_ITEMFROMPOINT,
                            None,
                            Some(windows::Win32::Foundation::LPARAM(lparam)),
                        )
                    };
                    choose(crate::ui::prompts_panel::clicked_row(hit.0 as u32, list.len()));
                }
                None
            })
        };
        let dblclick = match dblclick {
            Ok(h) => Some(h),
            Err(e) => {
                eprintln!("WARN palette: double click on the list not bound err={:?}", e);
                None
            }
        };

        let window_handle = window.handle;
        let filter_handle = txt_filter.handle;
        let handler = nwg::full_bind_event_handler(&window_handle, move |evt, _evt_data, handle| {
            match evt {
                nwg::Event::OnTextInput if handle == filter_handle => {
                    refill(&txt_filter.text());
                }
                nwg::Event::OnWindowClose => {
                    nwg::stop_thread_dispatch();
                }
                _ => {}
            }
        });

        nwg::dispatch_thread_events();
        nwg::unbind_event_handler(&handler);
        for raw in [keys, enter, dblclick].into_iter().flatten() {
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
    use super::{moved_selection, page_rows, paged_selection, picked_entry};

    #[test]
    fn arrows_move_and_clamp_the_selection() {
        assert_eq!(moved_selection(0x28, Some(0), 3), Some(1));
        assert_eq!(moved_selection(0x28, Some(2), 3), Some(2));
        assert_eq!(moved_selection(0x26, Some(0), 3), Some(0));
        assert_eq!(moved_selection(0x26, Some(2), 3), Some(1));
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
        // Page Up / Page Down go through paged_selection, not here.
        for vk in [0x25, 0x27, 0x0D, 0x1B, 0x41, 0x24, 0x23, 0x21, 0x22] {
            assert_eq!(moved_selection(vk, Some(1), 3), None, "{vk:#x}");
        }
    }

    #[test]
    fn page_keys_move_by_one_visible_page_and_clamp() {
        // VK_NEXT = 0x22 (Page Down), VK_PRIOR = 0x21 (Page Up); 20 rows,
        // 8 visible.
        assert_eq!(paged_selection(0x22, Some(0), 20, 8), Some(8));
        assert_eq!(paged_selection(0x22, Some(8), 20, 8), Some(16));
        assert_eq!(paged_selection(0x22, Some(16), 20, 8), Some(19), "clamped to the last row");
        assert_eq!(paged_selection(0x22, Some(19), 20, 8), Some(19));
        assert_eq!(paged_selection(0x21, Some(19), 20, 8), Some(11));
        assert_eq!(paged_selection(0x21, Some(5), 20, 8), Some(0), "clamped to the first row");
        assert_eq!(paged_selection(0x21, Some(0), 20, 8), Some(0));
    }

    #[test]
    fn page_keys_on_an_empty_or_unselected_list() {
        assert_eq!(paged_selection(0x22, None, 0, 8), None, "empty list: nothing moves");
        assert_eq!(paged_selection(0x21, None, 0, 8), None, "empty list: nothing moves");
        assert_eq!(paged_selection(0x22, None, 20, 8), Some(8), "no selection starts from the top");
        assert_eq!(paged_selection(0x22, Some(40), 20, 8), Some(19), "stale selection clamped");
        assert_eq!(paged_selection(0x22, Some(0), 3, 8), Some(2), "page longer than the list");
        // A zero page (never produced by page_rows) still moves one row.
        assert_eq!(paged_selection(0x22, Some(0), 3, 0), Some(1));
    }

    #[test]
    fn only_the_page_keys_page() {
        for vk in [0x26, 0x28, 0x25, 0x27, 0x0D, 0x1B, 0x41, 0x24, 0x23] {
            assert_eq!(paged_selection(vk, Some(1), 20, 8), None, "{vk:#x}");
        }
    }

    #[test]
    fn a_page_is_the_number_of_whole_visible_rows() {
        // Client height / LB_GETITEMHEIGHT; at least one row.
        assert_eq!(page_rows(335, 20), Ok(16));
        assert_eq!(page_rows(40, 20), Ok(2));
        assert_eq!(page_rows(10, 20), Ok(1), "shorter than a row: one row");
        assert_eq!(page_rows(0, 20), Ok(1));
        // LB_ERR (-1) or a zero height: no page, reported by the caller.
        assert!(page_rows(335, -1).is_err());
        assert!(page_rows(335, 0).is_err());
    }

    #[test]
    fn a_row_runs_the_entry_it_shows() {
        // Rows show entries 4, 0, 7 after ranking.
        let shown = [4, 0, 7];
        assert_eq!(picked_entry(false, Some(0), &shown), Some(4));
        assert_eq!(picked_entry(false, Some(2), &shown), Some(7));
        assert_eq!(picked_entry(false, None, &shown), None, "no selection");
        assert_eq!(picked_entry(false, Some(3), &shown), None, "past the last row");
        assert_eq!(picked_entry(false, Some(0), &[]), None, "empty list");
    }

    #[test]
    fn the_first_choice_wins() {
        // Input already queued after the first choice (Enter after a double
        // click, a repeated Enter) is still dispatched before the dialog
        // returns: it must not replace the entry already chosen.
        assert_eq!(picked_entry(true, Some(1), &[4, 0, 7]), None);
        assert_eq!(picked_entry(true, Some(0), &[4, 0, 7]), None);
    }

    #[test]
    fn a_double_click_off_the_rows_runs_nothing() {
        use crate::ui::prompts_panel::clicked_row;
        // LB_ITEMFROMPOINT: HIWORD 1 = outside the client area / no row.
        assert_eq!(clicked_row(0x0001_0000, 0), None, "empty list");
        assert_eq!(clicked_row(0x0001_0002, 3), None, "below the last row");
        assert_eq!(clicked_row(3, 3), None, "index past the list");
        assert_eq!(clicked_row(2, 3), Some(2));
        assert_eq!(picked_entry(false, clicked_row(0x0001_0000, 0), &[]), None);
    }

    /// The pure decisions are the ones the dialog runs: Page Up / Page Down
    /// in the filter field go through paged_selection with the visible rows
    /// of the list, and a double click runs the row under the pointer
    /// through the same `choose` as Enter.
    #[test]
    fn the_page_keys_and_the_double_click_are_wired() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("ui")
            .join("dialogs")
            .join("command_palette.rs");
        let src = crate::read_source(path);
        let code = &src[..src.find("#[cfg(test)]").expect("test module")];
        assert!(code.contains("paged_selection(vk,"), "page keys");
        assert!(code.contains("page_rows(client.bottom - client.top,"), "visible page");
        assert!(code.contains("WM_LBUTTONDBLCLK"), "double click handled");
        assert!(code.contains("LB_ITEMFROMPOINT"), "row under the pointer");
        assert!(code.contains("choose(crate::ui::prompts_panel::clicked_row("), "double click runs that row");
        assert!(code.contains("choose(list.selection())"), "Enter runs the selected row");
        assert!(code.contains("picked_entry(already_chosen,"), "the first choice wins");
        assert!(code.contains("for raw in [keys, enter, dblclick]"), "raw handlers unbound");
    }
}
