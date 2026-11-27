// Tests only: one test thread at a time drives a live RichEdit.
//
// The `live_*` tests create real RICHEDIT50W controls (directly, or through
// nwg::RichTextBox), and libtest runs tests on parallel threads. msftedit is
// not safe for that use: with three threads creating and editing their own
// controls at the same time, the test binary died with
// STATUS_ACCESS_VIOLATION in gdi32full (a null read under RichEditWndProc ->
// CreateTextServices -> GetCharWidthW, while measuring text) in 6 to 10 runs
// of `case_ops::tests::live` out of 150, and other runs read wrong font
// metrics or converted only part of the text. No control had been destroyed
// and no test thread had ended when it crashed: the fault is the concurrent
// use itself. Serialized, 0 crashes in 300 runs.
//
// The application drives its editor from its one UI thread (worker threads
// never touch a window), so this only has to hold in the tests: every test
// that keeps a live RichEdit holds `guard()` until the control is destroyed.
// The guard is reentrant on one thread (a test may keep two controls) and
// survives a failed test (a poisoned lock is taken over, as `lang_guard`).

use std::cell::RefCell;
use std::marker::PhantomData;
use std::sync::{Mutex, MutexGuard};

static LIVE_RICH_EDIT: Mutex<()> = Mutex::new(());

thread_local! {
    /// Guards alive on this thread, and the lock they share.
    static HELD: RefCell<(usize, Option<MutexGuard<'static, ()>>)> = const { RefCell::new((0, None)) };
}

/// While alive, no other thread drives a live RichEdit. Declare it before the
/// controls (or as the last field of the struct that owns them) so that it is
/// dropped after them.
pub(in crate::ui::editor) struct LiveRichEdit {
    // Released on the thread that took it: the lock guard lives there.
    _not_send: PhantomData<*const ()>,
}

pub(in crate::ui::editor) fn guard() -> LiveRichEdit {
    let first = HELD.with(|h| h.borrow().0 == 0);
    // Waited for outside the RefCell borrow.
    let lock = first.then(|| LIVE_RICH_EDIT.lock().unwrap_or_else(|e| e.into_inner()));
    HELD.with(|h| {
        let mut h = h.borrow_mut();
        if let Some(lock) = lock {
            h.1 = Some(lock);
        }
        h.0 += 1;
    });
    LiveRichEdit { _not_send: PhantomData }
}

impl Drop for LiveRichEdit {
    fn drop(&mut self) {
        // The last guard of the thread releases the lock (a MutexGuard
        // dropped while panicking poisons it; `guard` takes it over).
        let last = HELD.with(|h| {
            let mut h = h.borrow_mut();
            h.0 -= 1;
            if h.0 == 0 { h.1.take() } else { None }
        });
        drop(last);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{mpsc, Arc};
    use std::time::Duration;

    /// The thread's lock is held while any of its guards is alive.
    fn held_here() -> bool {
        HELD.with(|h| h.borrow().1.is_some())
    }

    #[test]
    fn nested_guards_on_one_thread_share_the_lock() {
        let outer = guard();
        let inner = guard(); // would deadlock if not reentrant
        drop(outer);
        assert!(held_here(), "the inner guard still holds the lock");
        drop(inner);
        assert!(!held_here());
    }

    /// Sequence: another thread waits while the guard is held, and gets it
    /// once the last guard of this thread is dropped.
    #[test]
    fn another_thread_waits_until_the_last_guard_is_dropped() {
        let outer = guard();
        let inner = guard();
        let got = Arc::new(AtomicBool::new(false));
        let (started_tx, started_rx) = mpsc::channel();
        let other = {
            let got = got.clone();
            std::thread::spawn(move || {
                started_tx.send(()).expect("started");
                let _g = guard();
                got.store(true, Ordering::SeqCst);
            })
        };
        started_rx.recv().expect("started");
        std::thread::sleep(Duration::from_millis(100));
        assert!(!got.load(Ordering::SeqCst), "entered while both guards were alive");
        drop(outer);
        std::thread::sleep(Duration::from_millis(100));
        assert!(!got.load(Ordering::SeqCst), "entered while the inner guard was alive");
        drop(inner);
        other.join().expect("other thread");
        assert!(got.load(Ordering::SeqCst));
    }

    /// A test that fails while holding the guard does not block the others.
    #[test]
    fn a_failed_test_does_not_block_the_next_one() {
        let failed = std::thread::spawn(|| {
            let _g = guard();
            panic!("assertion failed in a live test");
        })
        .join();
        assert!(failed.is_err());
        let _g = guard();
        assert!(held_here());
    }

    /// Every test that creates a RichEdit takes the guard: in each source
    /// file, the test part holds at least as many `guard()` calls as
    /// RichEdit creations. (The race itself shows only around the first
    /// concurrent use in a process, about once in 25 runs: no single run can
    /// prove it gone, so the rule is checked instead.)
    #[test]
    fn every_live_rich_edit_test_takes_the_guard() {
        // Split so that this file does not count itself.
        let creations = [["w!(\"RICHEDIT", "50W\")"].concat(), ["RichTextBox::", "builder()"].concat()];
        let guard_call = ["live_rich_edit::", "guard()"].concat();
        let mut files = Vec::new();
        let mut dirs = vec![std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
        while let Some(dir) = dirs.pop() {
            for e in std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display())) {
                let path = e.expect("directory entry").path();
                if path.is_dir() {
                    dirs.push(path);
                } else if path.extension().map_or(false, |e| e == "rs") {
                    files.push(path);
                }
            }
        }
        assert!(files.len() > 50, "src not found: {} files", files.len());
        let mut checked = 0;
        for path in files {
            let text = crate::read_source(&path);
            let Some(at) = text.find("\n#[cfg(test)]") else { continue };
            let tests = &text[at..];
            let created: usize = creations.iter().map(|c| tests.matches(c.as_str()).count()).sum();
            if created == 0 {
                continue;
            }
            checked += 1;
            let guarded = tests.matches(guard_call.as_str()).count();
            assert!(guarded >= created, "{}: {created} RichEdit created by tests, {guarded} guard() calls", path.display());
        }
        // line_ops, surface (two harnesses) and tab_ops.
        assert!(checked >= 3, "only {checked} files create a RichEdit in tests");
    }
}
