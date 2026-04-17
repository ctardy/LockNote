// FindReplaceDialog — modal Find / Find & Replace popup
//
// Layout (replace mode):
//   [Find:    ] [input       ] [◀ Prev] [Next ▶]
//   [Replace: ] [input       ] [Replace] [Replace All]
//   [☐ Match case] [☐ Whole word]
//   [status label: "3 / 12" or "Not found"]     [Close]
//
// Uses RichEdit messages (EM_FINDTEXTEXW, EM_EXSETSEL, EM_REPLACESEL) to
// operate directly on the editor HWND. Relies on the editor having the
// ES_NOHIDESEL style so selections stay visible while this dialog holds focus.

use native_windows_gui as nwg;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::SendMessageW;

#[cfg(feature = "i18n")]
fn t(key: &str, _fallback: &str) -> String {
    crate::pro::i18n::get(key).to_string()
}
#[cfg(not(feature = "i18n"))]
fn t(_key: &str, fallback: &str) -> String {
    fallback.to_string()
}

const EM_EXGETSEL: u32 = 0x0434;
const EM_EXSETSEL: u32 = 0x0437;
const EM_FINDTEXTEXW: u32 = 0x047C;
const EM_REPLACESEL: u32 = 0x00C2;
const EM_SCROLLCARET: u32 = 0x00B7;
const FR_DOWN: u32 = 0x0001;
const FR_WHOLEWORD: u32 = 0x0002;
const FR_MATCHCASE: u32 = 0x0004;

#[repr(C)]
struct CharRange {
    cp_min: i32,
    cp_max: i32,
}

#[repr(C)]
struct FindTextExW {
    chrg: CharRange,
    lpstr_text: *const u16,
    chrg_text: CharRange,
}

fn get_selection(hwnd: HWND) -> (i32, i32) {
    let mut cr = CharRange { cp_min: 0, cp_max: 0 };
    unsafe {
        SendMessageW(
            hwnd,
            EM_EXGETSEL,
            Some(WPARAM(0)),
            Some(LPARAM(&mut cr as *mut _ as isize)),
        );
    }
    (cr.cp_min, cr.cp_max)
}

fn set_selection(hwnd: HWND, start: i32, end: i32) {
    let cr = CharRange { cp_min: start, cp_max: end };
    unsafe {
        SendMessageW(
            hwnd,
            EM_EXSETSEL,
            Some(WPARAM(0)),
            Some(LPARAM(&cr as *const _ as isize)),
        );
        SendMessageW(hwnd, EM_SCROLLCARET, Some(WPARAM(0)), Some(LPARAM(0)));
    }
}

/// Low-level `EM_FINDTEXTEXW`. `chrg` defines the direction and range:
/// cp_min < cp_max with FR_DOWN is a forward search; cp_min > cp_max without
/// FR_DOWN is an upward search.
fn em_find(hwnd: HWND, term_wide: &[u16], flags: u32, cp_min: i32, cp_max: i32) -> Option<(i32, i32)> {
    let mut ft = FindTextExW {
        chrg: CharRange { cp_min, cp_max },
        lpstr_text: term_wide.as_ptr(),
        chrg_text: CharRange { cp_min: 0, cp_max: 0 },
    };
    let result = unsafe {
        SendMessageW(
            hwnd,
            EM_FINDTEXTEXW,
            Some(WPARAM(flags as usize)),
            Some(LPARAM(&mut ft as *mut _ as isize)),
        )
        .0
    };
    if result >= 0 {
        Some((ft.chrg_text.cp_min, ft.chrg_text.cp_max))
    } else {
        None
    }
}

fn wide(term: &str) -> Vec<u16> {
    term.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Find next match going forward from `from`, with wrap-around to start.
fn find_next_forward(hwnd: HWND, term: &str, base_flags: u32, from: i32) -> Option<(i32, i32)> {
    if term.is_empty() { return None; }
    let w = wide(term);
    em_find(hwnd, &w, base_flags | FR_DOWN, from, -1)
        .or_else(|| {
            if from > 0 {
                em_find(hwnd, &w, base_flags | FR_DOWN, 0, from)
            } else {
                None
            }
        })
}

/// Find previous match going upward from `from`, with wrap-around to end.
fn find_next_backward(hwnd: HWND, term: &str, base_flags: u32, from: i32) -> Option<(i32, i32)> {
    if term.is_empty() { return None; }
    let w = wide(term);
    // Upward: cp_min > cp_max. Without FR_DOWN.
    em_find(hwnd, &w, base_flags, from, 0)
        .or_else(|| {
            // Wrap: search from end back to `from`.
            em_find(hwnd, &w, base_flags, i32::MAX, from)
        })
}

fn replace_selection(hwnd: HWND, text: &str) {
    let w = wide(text);
    unsafe {
        SendMessageW(
            hwnd,
            EM_REPLACESEL,
            Some(WPARAM(1)),
            Some(LPARAM(w.as_ptr() as isize)),
        );
    }
}

/// Count total matches of `term` in the control.
fn count_matches(hwnd: HWND, term: &str, base_flags: u32) -> usize {
    if term.is_empty() { return 0; }
    let w = wide(term);
    let mut count = 0usize;
    let mut from: i32 = 0;
    loop {
        match em_find(hwnd, &w, base_flags | FR_DOWN, from, -1) {
            Some((_, end)) => {
                count += 1;
                if end <= from {
                    break; // safety: degenerate match (empty/zero-width)
                }
                from = end;
            }
            None => break,
        }
    }
    count
}

pub struct FindReplaceDialog;

impl FindReplaceDialog {
    pub fn show(editor_hwnd: isize, replace_mode: bool, initial_find: &str) {
        let title_key = if replace_mode { "search.title" } else { "search.find.title" };
        let title = t(title_key, if replace_mode { "Find & Replace" } else { "Find" });

        let win_w = 540;
        let win_h = if replace_mode { 200 } else { 155 };

        let mut window = Default::default();
        nwg::Window::builder()
            .size((win_w, win_h))
            .position((400, 300))
            .title(&title)
            .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
            .build(&mut window)
            .expect("Failed to build FindReplaceDialog window");

        super::apply_dialog_theme(&window);

        let mut font = Default::default();
        nwg::Font::builder()
            .family("Segoe UI")
            .size(15)
            .build(&mut font)
            .expect("Failed to build font");

        // ── Row 1: Find ──
        let mut lbl_find = Default::default();
        nwg::Label::builder()
            .text(&t("search.find", "Find:"))
            .parent(&window)
            .position((15, 18))
            .size((90, 20))
            .font(Some(&font))
            .build(&mut lbl_find)
            .expect("lbl_find");

        let mut txt_find = Default::default();
        nwg::TextInput::builder()
            .text(initial_find)
            .parent(&window)
            .position((110, 15))
            .size((230, 26))
            .font(Some(&font))
            .focus(true)
            .build(&mut txt_find)
            .expect("txt_find");

        let mut btn_prev = Default::default();
        nwg::Button::builder()
            .text(&t("search.prev", "Previous"))
            .parent(&window)
            .position((348, 14))
            .size((85, 28))
            .font(Some(&font))
            .build(&mut btn_prev)
            .expect("btn_prev");

        let mut btn_next = Default::default();
        nwg::Button::builder()
            .text(&t("search.next", "Next"))
            .parent(&window)
            .position((438, 14))
            .size((85, 28))
            .font(Some(&font))
            .build(&mut btn_next)
            .expect("btn_next");

        // ── Row 2: Replace (optional) ──
        let (txt_replace_opt, btn_replace_opt, btn_replace_all_opt) = if replace_mode {
            let mut lbl_replace = Default::default();
            nwg::Label::builder()
                .text(&t("search.replace", "Replace:"))
                .parent(&window)
                .position((15, 51))
                .size((90, 20))
                .font(Some(&font))
                .build(&mut lbl_replace)
                .expect("lbl_replace");
            std::mem::forget(lbl_replace);

            let mut txt_replace = Default::default();
            nwg::TextInput::builder()
                .text("")
                .parent(&window)
                .position((110, 48))
                .size((230, 26))
                .font(Some(&font))
                .build(&mut txt_replace)
                .expect("txt_replace");

            let mut btn_replace = Default::default();
            nwg::Button::builder()
                .text(&t("search.btn.replace", "Replace"))
                .parent(&window)
                .position((348, 47))
                .size((85, 28))
                .font(Some(&font))
                .build(&mut btn_replace)
                .expect("btn_replace");

            let mut btn_replace_all = Default::default();
            nwg::Button::builder()
                .text(&t("search.btn.replaceall", "Replace All"))
                .parent(&window)
                .position((438, 47))
                .size((85, 28))
                .font(Some(&font))
                .build(&mut btn_replace_all)
                .expect("btn_replace_all");

            (Some(txt_replace), Some(btn_replace), Some(btn_replace_all))
        } else {
            (None, None, None)
        };

        // ── Row 3: Checkboxes ──
        let checkbox_y = if replace_mode { 84 } else { 51 };
        let mut chk_case = Default::default();
        nwg::CheckBox::builder()
            .text(&t("search.case", "Match case"))
            .parent(&window)
            .position((15, checkbox_y))
            .size((170, 22))
            .font(Some(&font))
            .build(&mut chk_case)
            .expect("chk_case");
        super::disable_control_theme(chk_case.handle);

        let mut chk_word = Default::default();
        nwg::CheckBox::builder()
            .text(&t("search.wholeword", "Whole word"))
            .parent(&window)
            .position((195, checkbox_y))
            .size((150, 22))
            .font(Some(&font))
            .build(&mut chk_word)
            .expect("chk_word");
        super::disable_control_theme(chk_word.handle);

        // ── Row 4: Status + Close ──
        let status_y = checkbox_y + 34;
        let mut lbl_status = Default::default();
        nwg::Label::builder()
            .text("")
            .parent(&window)
            .position((15, status_y + 4))
            .size((330, 22))
            .font(Some(&font))
            .build(&mut lbl_status)
            .expect("lbl_status");

        let mut btn_close = Default::default();
        nwg::Button::builder()
            .text(&t("search.close", "Close"))
            .parent(&window)
            .position((438, status_y))
            .size((85, 28))
            .font(Some(&font))
            .build(&mut btn_close)
            .expect("btn_close");

        super::bind_enter_to_button(window.handle, btn_next.handle);

        // ── State ──
        let hwnd = HWND(editor_hwnd as *mut _);
        let lbl_status = Rc::new(lbl_status);
        let total = Rc::new(Cell::new(0usize));
        let current = Rc::new(Cell::new(0usize)); // 1-based; 0 = no current match
        let last_term = Rc::new(RefCell::new(String::new()));

        // Capture handles before moving widgets into Rc.
        let window_handle = window.handle;
        let next_h = btn_next.handle;
        let prev_h = btn_prev.handle;
        let close_h = btn_close.handle;
        let replace_h = btn_replace_opt.as_ref().map(|b| b.handle);
        let replace_all_h = btn_replace_all_opt.as_ref().map(|b| b.handle);
        let find_input_h = txt_find.handle;
        let chk_case_h = chk_case.handle;
        let chk_word_h = chk_word.handle;

        // Wrap widgets we need to read from multiple closures.
        let txt_find = Rc::new(txt_find);
        let txt_replace_opt = txt_replace_opt.map(Rc::new);
        let chk_case_rc = Rc::new(chk_case);
        let chk_word_rc = Rc::new(chk_word);

        // Helper closure to read current flags.
        let chk_case_for_flags = chk_case_rc.clone();
        let chk_word_for_flags = chk_word_rc.clone();
        let read_flags = move || -> u32 {
            let mut f = 0u32;
            if chk_case_for_flags.check_state() == nwg::CheckBoxState::Checked { f |= FR_MATCHCASE; }
            if chk_word_for_flags.check_state() == nwg::CheckBoxState::Checked { f |= FR_WHOLEWORD; }
            f
        };
        let read_flags = Rc::new(read_flags);

        // Helper to refresh the status label from current total/current.
        let lbl_status_upd = lbl_status.clone();
        let total_upd = total.clone();
        let current_upd = current.clone();
        let update_status = move || {
            let tot = total_upd.get();
            let cur = current_upd.get();
            if tot == 0 {
                lbl_status_upd.set_text("");
            } else if cur == 0 {
                let s = t("search.count.total", "{total} matches")
                    .replace("{total}", &tot.to_string());
                lbl_status_upd.set_text(&s);
            } else {
                let s = t("search.count", "{current} / {total}")
                    .replace("{current}", &cur.to_string())
                    .replace("{total}", &tot.to_string());
                lbl_status_upd.set_text(&s);
            }
        };
        let update_status = Rc::new(update_status);

        // Recount total matches and reset counter. Called on term/flags change.
        let total_rc = total.clone();
        let current_rc = current.clone();
        let read_flags_rc = read_flags.clone();
        let update_status_rc = update_status.clone();
        let lbl_status_rc = lbl_status.clone();
        let txt_find_recount = txt_find.clone();
        let recount = move || {
            let term = txt_find_recount.text();
            if term.is_empty() {
                total_rc.set(0);
                current_rc.set(0);
                lbl_status_rc.set_text("");
                return;
            }
            let flags = read_flags_rc();
            let tot = count_matches(hwnd, &term, flags);
            total_rc.set(tot);
            current_rc.set(0);
            if tot == 0 {
                lbl_status_rc.set_text(&t("search.count.none", "No matches"));
            } else {
                update_status_rc();
            }
        };
        let recount = Rc::new(recount);

        // Initial count (may be non-zero if initial_find was provided).
        recount();

        let last_term_cb = last_term.clone();
        let total_cb = total.clone();
        let current_cb = current.clone();
        let read_flags_cb = read_flags.clone();
        let update_status_cb = update_status.clone();
        let recount_cb = recount.clone();
        let lbl_status_cb = lbl_status.clone();
        let txt_find_cb = txt_find.clone();
        let txt_replace_cb = txt_replace_opt.clone();

        let handler = nwg::full_bind_event_handler(&window_handle, move |evt, _data, handle| {
            match evt {
                // Re-count on term change.
                nwg::Event::OnTextInput => {
                    if handle == find_input_h {
                        *last_term_cb.borrow_mut() = txt_find_cb.text();
                        recount_cb();
                    }
                }
                nwg::Event::OnButtonClick => {
                    // Checkbox clicks fire OnButtonClick too.
                    if handle == chk_case_h || handle == chk_word_h {
                        recount_cb();
                        return;
                    }

                    if handle == next_h {
                        let term = txt_find_cb.text();
                        if term.is_empty() { return; }
                        let flags = read_flags_cb();
                        let (_, caret) = get_selection(hwnd);
                        match find_next_forward(hwnd, &term, flags, caret) {
                            Some((a, b)) => {
                                set_selection(hwnd, a, b);
                                // Advance the counter, wrapping at total.
                                let tot = total_cb.get();
                                if tot > 0 {
                                    let mut c = current_cb.get() + 1;
                                    if c > tot { c = 1; }
                                    current_cb.set(c);
                                }
                                update_status_cb();
                            }
                            None => {
                                lbl_status_cb.set_text(&t("search.count.none", "No matches"));
                            }
                        }
                    } else if handle == prev_h {
                        let term = txt_find_cb.text();
                        if term.is_empty() { return; }
                        let flags = read_flags_cb();
                        let (caret, _) = get_selection(hwnd);
                        match find_next_backward(hwnd, &term, flags, caret) {
                            Some((a, b)) => {
                                set_selection(hwnd, a, b);
                                let tot = total_cb.get();
                                if tot > 0 {
                                    let c = current_cb.get();
                                    let new_c = if c <= 1 { tot } else { c - 1 };
                                    current_cb.set(new_c);
                                }
                                update_status_cb();
                            }
                            None => {
                                lbl_status_cb.set_text(&t("search.count.none", "No matches"));
                            }
                        }
                    } else if Some(handle) == replace_h {
                        if let Some(ref txt_replace) = txt_replace_cb {
                            let term = txt_find_cb.text();
                            let replacement = txt_replace.text();
                            if term.is_empty() { return; }
                            let flags = read_flags_cb();
                            let (sel_a, sel_b) = get_selection(hwnd);
                            if sel_b > sel_a {
                                replace_selection(hwnd, &replacement);
                            }
                            let (_, caret) = get_selection(hwnd);
                            // Find the next match and refresh the counter from scratch.
                            recount_cb();
                            if let Some((a, b)) = find_next_forward(hwnd, &term, flags, caret) {
                                set_selection(hwnd, a, b);
                                if total_cb.get() > 0 { current_cb.set(1); }
                                update_status_cb();
                            }
                        }
                    } else if Some(handle) == replace_all_h {
                        if let Some(ref txt_replace) = txt_replace_cb {
                            let term = txt_find_cb.text();
                            let replacement = txt_replace.text();
                            if term.is_empty() { return; }
                            let flags = read_flags_cb();
                            let mut count = 0usize;
                            let mut from: i32 = 0;
                            loop {
                                match find_next_forward(hwnd, &term, flags, from) {
                                    Some((a, b)) => {
                                        // Avoid looping through wrapped matches we already replaced.
                                        if count > 0 && a < from { break; }
                                        set_selection(hwnd, a, b);
                                        replace_selection(hwnd, &replacement);
                                        let (_, new_caret) = get_selection(hwnd);
                                        if new_caret <= from && count > 0 { break; }
                                        from = new_caret;
                                        count += 1;
                                    }
                                    None => break,
                                }
                            }
                            let msg = t("search.replaceall.msg", "{count} occurrences replaced.")
                                .replace("{count}", &count.to_string());
                            lbl_status_cb.set_text(&msg);
                            total_cb.set(0);
                            current_cb.set(0);
                        }
                    } else if handle == close_h {
                        nwg::stop_thread_dispatch();
                    }
                }
                nwg::Event::OnWindowClose => {
                    nwg::stop_thread_dispatch();
                }
                _ => {}
            }
        });

        nwg::dispatch_thread_events();
        nwg::unbind_event_handler(&handler);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn module_compiles() {
        let _ = std::mem::size_of::<super::FindReplaceDialog>();
    }
}
