// Tests only: one test thread at a time drives a live RichEdit.
//
// The `live_*` tests create real RICHEDIT50W controls (directly, or through
// nwg::RichTextBox), and libtest runs tests on parallel threads. Observed,
// not documented by Microsoft (no symbols either): with three threads
// creating and editing their own controls at the same time, the test binary
// died with STATUS_ACCESS_VIOLATION in gdi32full (a null read under
// RichEditWndProc -> CreateTextServices -> GetCharWidthW, while measuring
// text) in 6 to 10 runs of `case_ops::tests::live` out of 150, and other
// runs read wrong font metrics or converted only part of the text. No
// control had been destroyed and no test thread had ended when it crashed,
// so the concurrent use itself is the trigger. Serialized, 0 crashes in 300
// runs.
//
// To re-run that check, run the test binary many times with a live filter,
// each run with LOCALAPPDATA, TEMP and TMP set to a fresh scratch directory
// (as scripts\test.cmd does), and count exit code 0xC0000005 (3221225477):
//   cargo test --no-run, then N times: target\debug\deps\locknote-<hash>.exe case_ops::tests::live
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
    use std::collections::HashSet;
    use std::path::PathBuf;
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

    /// The ways a test gets a RichEdit control: the class by name or by its
    /// constant, both nwg controls built on it (nwg's RichLabel is a
    /// RICHEDIT50W too) and the whole editor window. Split so that this file
    /// does not count itself.
    fn creations() -> Vec<String> {
        vec![
            ["w!(\"RICHEDIT", "50W\")"].concat(),
            ["MSFTEDIT", "_CLASS"].concat(),
            ["RichTextBox::", "builder()"].concat(),
            ["RichLabel::", "builder()"].concat(),
            ["EditorForm::", "build("].concat(),
        ]
    }

    fn guard_call() -> String {
        ["live_rich_edit::", "guard()"].concat()
    }

    /// The part of a file that only tests compile: all of it for a test-only
    /// module, else from its first `#[cfg(test)]` at the start of a line
    /// (none: the file has no test part).
    fn test_part(text: &str, test_only: bool) -> &str {
        if test_only {
            return text;
        }
        text.find("\n#[cfg(test)]").map_or("", |at| &text[at..])
    }

    /// RichEdit creations and `guard()` calls in a test part.
    fn count(tests: &str) -> (usize, usize) {
        let created = creations().iter().map(|c| tests.matches(c.as_str()).count()).sum();
        (created, tests.matches(guard_call().as_str()).count())
    }

    /// The files of the modules their parent declares `#[cfg(test)] mod name;`:
    /// test-only files, which need no `#[cfg(test)]` of their own. A
    /// declaration that resolves to no file fails the test, so no test-only
    /// file goes unclassified.
    fn test_only_files(files: &[(PathBuf, String)]) -> HashSet<PathBuf> {
        let mut found = HashSet::new();
        for (path, text) in files {
            let lines: Vec<&str> = text.lines().map(str::trim).collect();
            for (i, line) in lines.iter().enumerate() {
                if *line != "#[cfg(test)]" {
                    continue;
                }
                // The item under the attribute, past comments and other attributes.
                let Some(item) = lines[i + 1..].iter().find(|l| !l.starts_with("//") && !l.starts_with("#[")) else {
                    continue;
                };
                let decl = item.strip_prefix("pub(crate) ").or_else(|| item.strip_prefix("pub(super) ")).unwrap_or(item);
                let Some(name) = decl.strip_prefix("mod ").and_then(|r| r.strip_suffix(';')) else { continue };
                let stem = path.file_stem().and_then(|s| s.to_str()).expect("file stem");
                let dir = path.parent().expect("parent directory");
                let base = if matches!(stem, "mod" | "main" | "lib") { dir.to_path_buf() } else { dir.join(stem) };
                let file = [base.join(format!("{name}.rs")), base.join(name).join("mod.rs")]
                    .into_iter()
                    .find(|f| f.is_file())
                    .unwrap_or_else(|| panic!("{}: `mod {name};` resolves to no file under {}", path.display(), base.display()));
                found.insert(file);
            }
        }
        found
    }

    /// A test-only file without `#[cfg(test)]` (like editor/tests.rs) is
    /// read whole, so an unguarded RichEdit there is counted.
    #[test]
    fn a_test_only_file_is_checked_whole() {
        let rich = &creations()[0];
        let text = format!("// Unit tests.\nuse super::*;\n#[test]\nfn live_x() {{ let h = CreateWindowExW({rich}); }}\n");
        assert_eq!(count(test_part(&text, true)), (1, 0));
        // A production file keeps its own creations out of the count.
        let prod = format!("fn build() {{ {} }}\n#[cfg(test)]\nmod tests {{}}\n", creations()[1]);
        assert_eq!(count(test_part(&prod, false)), (0, 0));
    }

    /// Every way of getting a RichEdit counts.
    #[test]
    fn every_way_of_getting_a_rich_edit_counts() {
        assert_eq!(creations().len(), 5);
        for c in creations() {
            let text = format!("\n#[cfg(test)]\nmod tests {{ fn t() {{ let _g = {}; {c}; }} }}\n", guard_call());
            assert_eq!(count(test_part(&text, false)), (1, 1), "{c}");
        }
    }

    /// `#[cfg(test)] mod name;` marks name.rs (or name/mod.rs) as test-only:
    /// next to a mod.rs or main.rs parent, under the folder of a name.rs one.
    /// An inline `#[cfg(test)] mod name { .. }` is part of its parent file.
    #[test]
    fn test_only_modules_are_found_from_their_parent() {
        let root = std::env::temp_dir().join(format!("locknote-live-rich-edit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("a/sub/t4")).expect("create scratch tree");
        for f in ["main.rs", "t1.rs", "a/mod.rs", "a/t2.rs", "a/plain.rs", "a/sub.rs", "a/sub/t3.rs", "a/sub/t4/mod.rs"] {
            std::fs::write(root.join(f), "").expect("write scratch file");
        }
        let files = vec![
            (root.join("main.rs"), "#[cfg(test)]\nmod t1;\nmod a;\n".to_string()),
            (root.join("a/mod.rs"), "mod plain;\n#[cfg(test)]\n// Tests.\npub(crate) mod t2;\nmod sub;\n#[cfg(test)]\nmod inline {}\n".to_string()),
            (root.join("a/sub.rs"), "#[cfg(test)]\n#[allow(dead_code)]\nmod t3;\n#[cfg(test)]\nmod t4;\n".to_string()),
        ];
        let found = test_only_files(&files);
        std::fs::remove_dir_all(&root).expect("remove scratch tree");
        let want: HashSet<PathBuf> = ["t1.rs", "a/t2.rs", "a/sub/t3.rs", "a/sub/t4/mod.rs"].iter().map(|f| root.join(f)).collect();
        assert_eq!(found, want);
    }

    /// Every test that creates a RichEdit takes the guard: in each source
    /// file, the test part (the whole of a test-only file) holds at least as
    /// many `guard()` calls as RichEdit creations. (The race itself shows only
    /// around the first concurrent use in a process, about once in 25 runs:
    /// no single run can prove it gone, so the rule is checked instead.)
    /// Limits: the count is per file, so an extra `guard()` in a file could
    /// hide one unguarded creation there, and a RichEdit made by a helper is
    /// counted in the helper's file only.
    #[test]
    fn every_live_rich_edit_test_takes_the_guard() {
        let mut files = Vec::new();
        let mut dirs = vec![std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
        while let Some(dir) = dirs.pop() {
            for e in std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display())) {
                let path = e.expect("directory entry").path();
                if path.is_dir() {
                    dirs.push(path);
                } else if path.extension().map_or(false, |e| e == "rs") {
                    let text = crate::read_source(&path);
                    files.push((path, text));
                }
            }
        }
        assert!(files.len() > 50, "src not found: {} files", files.len());
        let test_only = test_only_files(&files);
        // editor/tests.rs, editor/live_rich_edit.rs, storage/pending_tests.rs,
        // dialogs/snapshot.rs, integration_tests.rs.
        assert!(test_only.len() >= 5, "only {} test-only files found", test_only.len());
        let mut checked = 0;
        for (path, text) in &files {
            let (created, guarded) = count(test_part(text, test_only.contains(path)));
            if created == 0 {
                continue;
            }
            checked += 1;
            assert!(guarded >= created, "{}: {created} RichEdit created by tests, {guarded} guard() calls", path.display());
        }
        // line_ops, surface (two harnesses), tab_ops and tom.
        assert!(checked >= 4, "only {checked} files create a RichEdit in tests");
    }
}
