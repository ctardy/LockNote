// Small Windows helper that waits for the parent LockNote process to exit,
// then replaces the .exe with its updated .tmp twin via MoveFileExW.
//
// The .tmp lives in %LOCALAPPDATA%\LockNote. When the exe is on another
// volume (USB key, second disk, network share) a move cannot work
// (ERROR_NOT_SAME_DEVICE): the .tmp is then copied next to the exe, checked,
// and that copy is moved over the exe, which is a rename on one volume.
//
// The .tmp is never deleted before the exe holds its content. Every failure
// is written to %LOCALAPPDATA%\LockNote\swap-error.log and leaves the .tmp in
// place: the application takes it up again at its next start.
//
// The whole operation runs under the swap lock of the exe (a named mutex
// derived from the exe path, `swap_common::swap_lock_name`): the application
// holds it during its session and hands it over at exit, and waits for it at
// startup. Two swaps of the same exe, or a swap and a session, never overlap.
//
// Right before each rename over the exe, the decision of the application is
// taken again on the files as they are then (`still_promotable`): if the exe
// no longer holds the notes the pending save was based on, nothing is
// touched and the next start asks.

#![windows_subsystem = "windows"]

use sha2::{Digest, Sha256};
use std::ffi::OsString;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

// Shared with the application: log, name of the sibling file, swap lock.
#[allow(dead_code)]
#[path = "../storage/swap_common.rs"]
mod swap_common;

// Shared with the application: the image format, the companion and the
// decision on a pending .tmp, checked again right before the final rename.
#[allow(dead_code)]
#[path = "../storage/decision.rs"]
mod decision;

/// Win32 `ERROR_NOT_SAME_DEVICE`: the source and the target of a move are
/// not on the same volume.
const ERROR_NOT_SAME_DEVICE: u32 = 17;

/// Exit codes: 0 done, 1 swap failed or abandoned (.tmp kept), 2 bad
/// arguments, 3 the swap lock was not obtained (nothing touched), 4 the exe
/// was replaced but could not be started again.
const EXIT_SWAP_FAILED: i32 = 1;
const EXIT_USAGE: i32 = 2;
const EXIT_NO_LOCK: i32 = 3;
const EXIT_RELAUNCH_FAILED: i32 = 4;

fn main() {
    let args: Vec<OsString> = std::env::args_os().collect();
    let args = match parse_args(&args) {
        Ok(a) => a,
        Err(e) => {
            swap_common::append_log(&e);
            std::process::exit(EXIT_USAGE);
        }
    };

    let parent_exited = std::cell::Cell::new(false);
    let outcome = run(
        &args.tmp,
        &args.exe,
        args.ready.as_deref(),
        swap_common::SWAP_LOCK_WAIT,
        &|| parent_exited.set(wait_for_parent_exit(args.parent_pid)),
        &Io { mv: &atomic_move, read: &read_file, recheck: &|| still_promotable(&args.tmp, &args.exe) },
    );
    // The lines of the swap are written before the restarted session reads
    // the log at its start (`check_recent_swap_errors`).
    for line in &outcome.log {
        swap_common::append_log(line);
    }
    // `run` has released the swap lock: the restarted session can take it.
    let restart = relaunch_after(
        Outcome { code: outcome.code, log: Vec::new() },
        args.relaunch,
        parent_exited.get(),
        &args.exe,
        &start_exe,
    );
    for line in &restart.log {
        swap_common::append_log(line);
    }
    std::process::exit(restart.code);
}

/// The command line of the helper, as `storage::launch_swap_helper` writes it.
#[derive(Debug)]
struct Args {
    parent_pid: u32,
    tmp: PathBuf,
    exe: PathBuf,
    ready: Option<String>,
    /// Start the exe again once it is replaced (`swap_common::RELAUNCH_ARG`).
    relaunch: bool,
}

/// `<parent_pid> <tmp_path> <exe_path> [<ready event> [--relaunch]]`, after
/// the program name. `Err` holds the line to log.
fn parse_args(args: &[OsString]) -> Result<Args, String> {
    if args.len() < 4 || args.len() > 6 {
        return Err(format!(
            "Usage: locknote-swap <parent_pid> <tmp_path> <exe_path> [<ready event> [{}]]",
            swap_common::RELAUNCH_ARG
        ));
    }
    let parent_pid = args[1]
        .to_string_lossy()
        .parse()
        .map_err(|e| format!("Invalid parent PID: {}", e))?;
    let relaunch = match args.get(5) {
        None => false,
        Some(a) if a == swap_common::RELAUNCH_ARG => true,
        Some(a) => return Err(format!("Unknown last argument: {}", a.to_string_lossy())),
    };
    Ok(Args {
        parent_pid,
        tmp: PathBuf::from(&args[2]),
        exe: PathBuf::from(&args[3]),
        ready: args.get(4).map(|a| a.to_string_lossy().to_string()),
        relaunch,
    })
}

/// After `run` (the swap lock is released): when the user asked for it,
/// start `exe` again, only if the swap is done and the parent has exited. A
/// failed swap does not restart anything, nor does a parent still running
/// at the wait cap (it still holds the single-instance mutex: the new
/// session would quit at once); the log says why, and the next start of
/// LockNote takes the .tmp up again.
fn relaunch_after(
    outcome: Outcome,
    wanted: bool,
    parent_exited: bool,
    exe: &Path,
    launch: &dyn Fn(&Path) -> Result<(), String>,
) -> Outcome {
    let Outcome { mut code, mut log } = outcome;
    if !wanted {
        return Outcome { code, log };
    }
    if code != 0 {
        log.push(format!("LockNote not restarted after the update: the swap did not complete: exe={}", exe.display()));
        return Outcome { code, log };
    }
    if !parent_exited {
        log.push(format!(
            "LockNote not restarted after the update: the previous LockNote was still running after the wait: exe={}",
            exe.display()
        ));
        return Outcome { code, log };
    }
    if let Err(e) = launch(exe) {
        log.push(format!("swap done, LockNote could not be restarted: exe={} err={}", exe.display(), e));
        code = EXIT_RELAUNCH_FAILED;
    }
    Outcome { code, log }
}

/// The production restart: the exe, in its own folder, without arguments
/// (a plain start). Its handle is closed at once; it runs on its own.
fn start_exe(exe: &Path) -> Result<(), String> {
    let mut cmd = std::process::Command::new(exe);
    if let Some(dir) = exe.parent() {
        cmd.current_dir(dir);
    }
    cmd.spawn().map(drop).map_err(|e| e.to_string())
}

/// What `run` did: the lines to log and the exit code.
#[derive(Debug, PartialEq)]
struct Outcome {
    code: i32,
    log: Vec<String>,
}

/// The helper itself, with its waits and its file primitives given by the
/// caller: take the swap lock of `exe` (signalling `ready` when given), wait
/// for the parent, move the .tmp over the exe, remove its companion.
fn run(
    tmp: &Path,
    exe: &Path,
    ready: Option<&str>,
    lock_wait: std::time::Duration,
    wait_parent: &dyn Fn(),
    io: &Io,
) -> Outcome {
    let mut log = Vec::new();
    let name = swap_common::swap_lock_name(exe);
    let acquired = match ready {
        Some(event) => swap_common::SwapLock::acquire_signalling(&name, event, lock_wait),
        None => swap_common::SwapLock::acquire(&name, lock_wait),
    };
    let _lock = match acquired {
        Ok((lock, obtained)) => {
            if obtained == swap_common::Obtained::Abandoned {
                log.push(format!(
                    "swap lock {} was abandoned by its previous owner; swap goes on: tmp={} exe={}",
                    name,
                    tmp.display(),
                    exe.display()
                ));
            }
            lock
        }
        Err(e) => {
            // Another swap or a session of this exe is at work: touching
            // the files now could undo its work. The .tmp stays; the next
            // exit tries again.
            log.push(format!(
                "swap helper gave up, nothing touched (swap lock not obtained): tmp={} exe={} err={}",
                tmp.display(),
                exe.display(),
                e
            ));
            return Outcome { code: EXIT_NO_LOCK, log };
        }
    };

    wait_parent();

    let code = match promote(tmp, exe, io) {
        Ok(note) => {
            log.extend(note);
            // The companion goes with the .tmp it describes.
            if !tmp.exists() {
                let companion = tmp.with_extension(swap_common::COMPANION_EXTENSION);
                match std::fs::remove_file(&companion) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => log.push(format!(
                        "swap done, companion could not be removed (removed at the next start): file={} err={}",
                        companion.display(),
                        e
                    )),
                }
            }
            0
        }
        Err(msg) => {
            // The .tmp has not been deleted: it is the only copy of the
            // latest data until a later swap succeeds.
            log.push(msg);
            EXIT_SWAP_FAILED
        }
    };
    Outcome { code, log }
}

/// True when the parent has exited (or was already gone), false when it was
/// still running at the 30 s cap or the wait failed.
#[cfg(windows)]
fn wait_for_parent_exit(pid: u32) -> bool {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::Threading::{
        OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
    };
    unsafe {
        let h: HANDLE = match OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
            Ok(h) => h,
            Err(_) => return true, // Parent already gone — we're free to proceed
        };
        // 30s safety cap: if the parent hangs we still attempt the swap
        let waited = WaitForSingleObject(h, 30_000);
        let _ = CloseHandle(h);
        waited == windows::Win32::Foundation::WAIT_OBJECT_0
    }
}

#[cfg(not(windows))]
fn wait_for_parent_exit(_pid: u32) -> bool {
    true
}

/// Why a move failed: the Win32 error code when there is one, and the text
/// written to the log.
#[derive(Debug)]
struct MoveError {
    code: Option<u32>,
    detail: String,
}

/// The move primitive: `atomic_move` in production, a fake in the tests.
type MoveFn<'a> = dyn Fn(&Path, &Path) -> Result<(), MoveError> + 'a;

/// How the copy written next to the exe is read back for its check:
/// `std::fs::read` in production, a fake in the tests.
type ReadFn<'a> = dyn Fn(&Path) -> std::io::Result<Vec<u8>> + 'a;

/// The production read-back.
fn read_file(path: &Path) -> std::io::Result<Vec<u8>> {
    std::fs::read(path)
}

/// Whether the .tmp may still replace the exe, checked right before a rename
/// over the exe: `still_promotable` in production, a fake in the tests.
/// `Err` holds the line to log.
type RecheckFn<'a> = dyn Fn() -> Result<(), String> + 'a;

/// The file primitives of the swap.
struct Io<'a> {
    mv: &'a MoveFn<'a>,
    read: &'a ReadFn<'a>,
    recheck: &'a RecheckFn<'a>,
}

/// The decision of the application at exit, taken again on the files as they
/// are now (`decision::decide_pending`, the same function): the .tmp may
/// replace the exe only while it is still `Promote`. Anything else (another
/// writer put other notes into the exe since, the .tmp or its companion
/// changed, the exe cannot be read) stops the swap before anything is
/// touched; the next start of LockNote judges the files again (a question
/// when the exe holds other notes).
///
/// Limit: this narrows the window, it does not close it. Between this check
/// and the rename, a writer the swap lock does not cover (the lock is
/// `Local\`: another computer on a share, another Windows session) can still
/// write the exe.
fn still_promotable(tmp: &Path, exe: &Path) -> Result<(), String> {
    let companion = decision::read_companion(&tmp.with_extension(swap_common::COMPANION_EXTENSION), exe);
    let staged = decision::staged_facts(tmp);
    use decision::PendingDecision as D;
    let why = match decision::decide_pending(staged.as_ref(), companion.as_ref(), &|| decision::exe_facts(exe)) {
        D::Promote => return Ok(()),
        D::None => "the .tmp is gone".to_string(),
        D::Stale => "the program file already holds these notes".to_string(),
        D::Conflict { cause, .. } => format!("conflict: {}", cause),
        D::Ignored(reason) => format!("ignored: {}", reason),
        D::TmpUnreadable(e) | D::ExeUnreadable(e) => e,
    };
    Err(format!(
        "swap abandoned, nothing touched: the pending save is no longer to be promoted ({}); \
         it is judged again at the next start: tmp={} exe={}",
        why,
        tmp.display(),
        exe.display()
    ))
}

/// Replace `exe` with `tmp` through the primitives of `io`.
///
/// `Ok(None)`: done. `Ok(Some(note))`: the exe holds the content of the .tmp
/// but something is worth a log line (the .tmp was left in place). `Err`:
/// the exe was not replaced and the .tmp is intact; the text goes to the log.
///
/// Every rename over the exe is preceded by `io.recheck`.
fn promote(tmp: &Path, exe: &Path, io: &Io) -> Result<Option<String>, String> {
    (io.recheck)()?;
    match (io.mv)(tmp, exe) {
        Ok(()) => Ok(None),
        Err(e) if e.code == Some(ERROR_NOT_SAME_DEVICE) => copy_then_rename(tmp, exe, io),
        Err(e) => Err(format!(
            "MoveFileExW failed: tmp={} exe={} err={}",
            tmp.display(),
            exe.display(),
            e.detail
        )),
    }
}

/// The .tmp is on another volume than the exe: copy it next to the exe under
/// a name of its own, check the copy, then move the copy over the exe (a
/// rename within one volume, as atomic as the direct move).
///
/// MOVEFILE_COPY_ALLOWED is not used: it copies straight onto the target, so
/// an interruption would leave a half-written exe.
fn copy_then_rename(tmp: &Path, exe: &Path, io: &Io) -> Result<Option<String>, String> {
    let fail = |step: &str, err: &dyn std::fmt::Display| {
        format!(
            "cross-volume swap failed: tmp={} exe={} step={} err={}",
            tmp.display(),
            exe.display(),
            step,
            err
        )
    };

    let dir = match exe.parent() {
        Some(d) if !d.as_os_str().is_empty() => d,
        _ => return Err(fail("locate", &"the exe path has no parent directory")),
    };
    let exe_name = match exe.file_name() {
        Some(n) => n.to_string_lossy().to_string(),
        None => return Err(fail("locate", &"the exe path has no file name")),
    };

    // The source, as it is now.
    let src_modified = std::fs::metadata(tmp)
        .and_then(|m| m.modified())
        .map_err(|e| fail("stat tmp", &e))?;
    let data = std::fs::read(tmp).map_err(|e| fail("read tmp", &e))?;
    let src_hash = Sha256::digest(&data);

    // A name of our own, created exclusively: nothing existing is overwritten.
    let sibling = dir.join(swap_common::sibling_name(&exe_name, &swap_common::random_id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&sibling)
        .map_err(|e| fail("create sibling", &e))?;

    // From here on the sibling exists: every failure removes it.
    let discard = |msg: String| -> String {
        match std::fs::remove_file(&sibling) {
            Ok(()) => msg,
            Err(e) => format!(
                "{} (and the sibling {} could not be removed: {})",
                msg,
                sibling.display(),
                e
            ),
        }
    };

    let written = {
        use std::io::Write;
        file.write_all(&data).and_then(|_| file.sync_all())
    };
    drop(file);
    if let Err(e) = written {
        return Err(discard(fail("write sibling", &e)));
    }

    // Check what the target volume gives back against the source.
    let copy = match (io.read)(&sibling) {
        Ok(c) => c,
        Err(e) => return Err(discard(fail("read back sibling", &e))),
    };
    if copy.len() != data.len() || Sha256::digest(&copy) != src_hash {
        return Err(discard(fail(
            "verify sibling",
            &"size or SHA-256 differs from the .tmp",
        )));
    }
    drop(copy);

    // The exe carries the date of the save, not the date of this copy (the
    // application judges by content; dates only matter for a .tmp left by
    // an earlier version, and for the user looking at the file).
    let dated = std::fs::OpenOptions::new()
        .write(true)
        .open(&sibling)
        .and_then(|f| f.set_modified(src_modified));
    if let Err(e) = dated {
        return Err(discard(fail("date sibling", &e)));
    }

    // The copy took time (a slow share): the exe may have been written since
    // the check made before the direct move.
    if let Err(e) = (io.recheck)() {
        return Err(discard(e));
    }
    if let Err(e) = (io.mv)(&sibling, exe) {
        return Err(discard(fail("rename sibling", &e.detail)));
    }

    // The exe now holds the content of the .tmp. Remove the .tmp only if it
    // is still the file that was copied.
    let unchanged = match std::fs::metadata(tmp).and_then(|m| m.modified()) {
        Ok(modified) if modified == src_modified => match std::fs::read(tmp) {
            Ok(now) => now.len() == data.len() && Sha256::digest(&now) == src_hash,
            Err(_) => false,
        },
        _ => false,
    };
    if !unchanged {
        return Ok(Some(format!(
            "cross-volume swap done, .tmp kept (it changed during the swap): tmp={} exe={}",
            tmp.display(),
            exe.display()
        )));
    }
    match std::fs::remove_file(tmp) {
        Ok(()) => Ok(None),
        Err(e) => Ok(Some(format!(
            "cross-volume swap done, .tmp could not be removed: tmp={} exe={} err={}",
            tmp.display(),
            exe.display(),
            e
        ))),
    }
}

#[cfg(windows)]
fn atomic_move(src: &Path, dst: &Path) -> Result<(), MoveError> {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    let src_w: Vec<u16> = src
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let dst_w: Vec<u16> = dst
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        MoveFileExW(
            PCWSTR(src_w.as_ptr()),
            PCWSTR(dst_w.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
        .map_err(|e| {
            // A Win32 error travels as HRESULT 0x8007xxxx: the low word is
            // the Win32 code (17 = ERROR_NOT_SAME_DEVICE, 5 = access denied).
            let hr = e.code().0 as u32;
            let code = if hr & 0xFFFF_0000 == 0x8007_0000 {
                Some(hr & 0xFFFF)
            } else {
                None
            };
            MoveError { code, detail: format!("{:?}", e) }
        })
    }
}

#[cfg(not(windows))]
fn atomic_move(_src: &Path, _dst: &Path) -> Result<(), MoveError> {
    Err(MoveError { code: None, detail: "unsupported platform".into() })
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::time::SystemTime;

    // Returns a unique file path in the OS temp dir without creating it.
    fn unique_tmp_path(suffix: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let ts = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("locknote_swap_test_{}_{}_{}_{}.bin", std::process::id(), ts, n, suffix))
    }

    #[test]
    fn atomic_move_replaces_destination_and_preserves_bytes() {
        let src = unique_tmp_path("src");
        let dst = unique_tmp_path("dst");
        let src_payload = b"NEW BINARY CONTENT v2";
        let dst_payload = b"OLD BINARY CONTENT v1";
        std::fs::write(&src, src_payload).unwrap();
        std::fs::write(&dst, dst_payload).unwrap();

        atomic_move(&src, &dst).expect("MoveFileExW must succeed on valid paths");

        // After swap: dst holds new bytes, src is consumed.
        let after = std::fs::read(&dst).unwrap();
        assert_eq!(after, src_payload, "dst must contain the new payload byte-for-byte");
        assert!(!src.exists(), "src (.tmp) must be gone after successful swap");

        let _ = std::fs::remove_file(&dst);
    }

    #[test]
    fn atomic_move_creates_dst_when_missing() {
        // First-install / post-crash scenario: the .exe target does not exist
        // yet. MoveFileExW(REPLACE_EXISTING) must still promote .tmp in place.
        let src = unique_tmp_path("fresh_src");
        let dst = unique_tmp_path("fresh_dst");
        std::fs::write(&src, b"fresh install").unwrap();
        assert!(!dst.exists(), "precondition: dst must not exist");

        atomic_move(&src, &dst).expect("MoveFileExW must promote .tmp when dst is absent");

        assert_eq!(std::fs::read(&dst).unwrap(), b"fresh install");
        assert!(!src.exists());
        let _ = std::fs::remove_file(&dst);
    }

    // Core invariant for issue #034: when MoveFileExW cannot complete, .tmp
    // must remain untouched so the user never loses the latest binary/data.
    #[test]
    fn atomic_move_failure_preserves_tmp() {
        let src = unique_tmp_path("preserved_src");
        let src_payload = b"DATA THAT MUST SURVIVE A FAILED SWAP";
        std::fs::write(&src, src_payload).unwrap();

        // The destination is in a directory that does not exist (a unique
        // name under the OS temp dir, never created): MoveFileExW fails with
        // ERROR_PATH_NOT_FOUND (3) without touching any real file. No drive
        // letter is assumed to be free: any letter may be a mounted drive.
        let bad_dst = unique_tmp_path("no_such_dir").join("subdir").join("target.exe");

        let res = atomic_move(&src, &bad_dst);
        let err = res.expect_err("move into a missing directory must fail");
        assert_eq!(err.code, Some(3), "ERROR_PATH_NOT_FOUND expected: {:?}", err);

        // CRITICAL: src (.tmp) survives with identical bytes.
        assert!(src.exists(), ".tmp must be preserved when the swap fails");
        assert_eq!(
            std::fs::read(&src).unwrap(),
            src_payload,
            ".tmp payload must be byte-identical after a failed swap"
        );

        let _ = std::fs::remove_file(&src);
    }

    // Missing source: MoveFileExW returns Err, nothing is created at dst.
    // This path is not expected in production (caller checks .tmp existence
    // first) but we lock it down so a regression cannot silently create an
    // empty .exe.
    #[test]
    fn atomic_move_missing_src_fails_without_creating_dst() {
        let src = unique_tmp_path("absent");
        let dst = unique_tmp_path("should_not_appear");
        assert!(!src.exists());
        assert!(!dst.exists());

        let res = atomic_move(&src, &dst);
        assert!(res.is_err(), "missing src must fail the move");
        assert!(!dst.exists(), "failed move must not create dst");
    }

    // Round-trip large payload to catch any silent truncation by
    // MOVEFILE_WRITE_THROUGH on large buffers.
    #[test]
    fn atomic_move_preserves_large_payload() {
        let src = unique_tmp_path("large_src");
        let dst = unique_tmp_path("large_dst");
        let payload: Vec<u8> = (0..(2 * 1024 * 1024)).map(|i| (i % 251) as u8).collect();
        std::fs::write(&src, &payload).unwrap();

        atomic_move(&src, &dst).expect("MoveFileExW must handle multi-MB payloads");

        let read_back = std::fs::read(&dst).unwrap();
        assert_eq!(read_back.len(), payload.len());
        assert_eq!(read_back, payload);
        let _ = std::fs::remove_file(&dst);
    }

    // ── Cross-volume fallback (issue 055) ──
    //
    // A second volume is not needed: the move primitive is injected, and a
    // fake answers ERROR_NOT_SAME_DEVICE for the direct move, as Windows does
    // between two volumes. `tests/swap_helper.rs` has the test on a real
    // second volume (opt-in).

    /// A scratch directory holding `staging\<hash>.tmp` and `app\LockNote.exe`.
    struct Scene {
        root: PathBuf,
        tmp: PathBuf,
        exe: PathBuf,
    }

    impl Drop for Scene {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn scene(tag: &str, tmp_bytes: &[u8], exe_bytes: &[u8]) -> Scene {
        let root = unique_tmp_path(tag).with_extension("dir");
        let staging = root.join("staging");
        let app = root.join("app");
        std::fs::create_dir_all(&staging).unwrap();
        std::fs::create_dir_all(&app).unwrap();
        let tmp = staging.join("0123456789abcdef.tmp");
        let exe = app.join("LockNote.exe");
        std::fs::write(&tmp, tmp_bytes).unwrap();
        std::fs::write(&exe, exe_bytes).unwrap();
        Scene { root, tmp, exe }
    }

    fn files_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        names.sort();
        names
    }

    fn not_same_device() -> MoveError {
        MoveError { code: Some(ERROR_NOT_SAME_DEVICE), detail: "simulated 0x80070011".into() }
    }

    #[test]
    fn cross_volume_error_falls_back_to_copy_then_rename() {
        let new_image: Vec<u8> = (0..300_000).map(|i| (i % 253) as u8).collect();
        let s = scene("xvol_ok", &new_image, b"OLD PROGRAM AND OLD NOTES");
        // The date of the save, well in the past.
        let saved_at = SystemTime::now() - std::time::Duration::from_secs(7200);
        std::fs::OpenOptions::new()
            .write(true)
            .open(&s.tmp)
            .unwrap()
            .set_modified(saved_at)
            .unwrap();
        let saved_at = std::fs::metadata(&s.tmp).unwrap().modified().unwrap();

        let calls: RefCell<Vec<(PathBuf, PathBuf)>> = RefCell::new(Vec::new());
        let mv = |src: &Path, dst: &Path| -> Result<(), MoveError> {
            calls.borrow_mut().push((src.to_path_buf(), dst.to_path_buf()));
            if src == s.tmp {
                Err(not_same_device())
            } else {
                atomic_move(src, dst)
            }
        };

        let res = promote(&s.tmp, &s.exe, &with_move(&mv));

        assert_eq!(res, Ok(None));
        assert_eq!(std::fs::read(&s.exe).unwrap(), new_image, "exe = content of the .tmp");
        assert!(!s.tmp.exists(), "the .tmp is removed once the exe holds its content");
        assert_eq!(
            files_in(s.exe.parent().unwrap()),
            vec!["LockNote.exe".to_string()],
            "no sibling left next to the exe"
        );
        assert_eq!(
            std::fs::metadata(&s.exe).unwrap().modified().unwrap(),
            saved_at,
            "the exe carries the date of the save"
        );

        let calls = calls.into_inner();
        assert_eq!(calls.len(), 2, "direct move, then rename of the sibling");
        assert_eq!(calls[0], (s.tmp.clone(), s.exe.clone()));
        let (sibling, target) = &calls[1];
        assert_eq!(target, &s.exe);
        assert_eq!(sibling.parent(), s.exe.parent(), "the copy is in the folder of the exe");
        let name = sibling.file_name().unwrap().to_string_lossy().to_string();
        assert!(swap_common::is_sibling_name(&name, "LockNote.exe"), "sibling name: {}", name);
        assert!(!name.ends_with(".exe") && !name.ends_with(".new"));
    }

    #[test]
    fn cross_volume_final_rename_failure_removes_the_sibling_and_keeps_the_tmp() {
        let s = scene("xvol_rename_fail", b"NEW PROGRAM AND NEW NOTES", b"OLD PROGRAM AND OLD NOTES");
        let calls: RefCell<Vec<(PathBuf, PathBuf)>> = RefCell::new(Vec::new());
        let sibling_was_whole: RefCell<bool> = RefCell::new(false);
        let mv = |src: &Path, dst: &Path| -> Result<(), MoveError> {
            calls.borrow_mut().push((src.to_path_buf(), dst.to_path_buf()));
            if src == s.tmp {
                Err(not_same_device())
            } else {
                // The exe is read-only, still running, or held by an antivirus.
                *sibling_was_whole.borrow_mut() = std::fs::read(src)
                    .map(|b| b == b"NEW PROGRAM AND NEW NOTES")
                    .unwrap_or(false);
                Err(MoveError { code: Some(5), detail: "simulated 0x80070005 access denied".into() })
            }
        };

        let res = promote(&s.tmp, &s.exe, &with_move(&mv));

        let msg = res.expect_err("the swap must report the failure");
        assert_eq!(calls.borrow().len(), 2, "the rename of the sibling was attempted");
        assert!(*sibling_was_whole.borrow(), "the sibling was a full copy when the rename was tried");
        assert!(msg.contains("0x80070005"), "the code is logged: {}", msg);
        assert!(msg.contains("step=rename sibling"), "{}", msg);
        assert_eq!(std::fs::read(&s.tmp).unwrap(), b"NEW PROGRAM AND NEW NOTES", ".tmp intact");
        assert_eq!(std::fs::read(&s.exe).unwrap(), b"OLD PROGRAM AND OLD NOTES", "exe intact");
        assert_eq!(
            files_in(s.exe.parent().unwrap()),
            vec!["LockNote.exe".to_string()],
            "the sibling is removed"
        );
    }

    #[test]
    fn other_errors_do_not_start_a_copy() {
        let s = scene("direct_fail", b"NEW", b"OLD");
        let calls: RefCell<u32> = RefCell::new(0);
        let mv = |_: &Path, _: &Path| -> Result<(), MoveError> {
            *calls.borrow_mut() += 1;
            Err(MoveError { code: Some(5), detail: "simulated 0x80070005".into() })
        };

        let res = promote(&s.tmp, &s.exe, &with_move(&mv));

        let msg = res.expect_err("must fail");
        assert!(msg.starts_with("MoveFileExW failed: tmp="), "same log line as before: {}", msg);
        assert_eq!(*calls.borrow(), 1, "only ERROR_NOT_SAME_DEVICE starts the copy");
        assert_eq!(std::fs::read(&s.tmp).unwrap(), b"NEW");
        assert_eq!(std::fs::read(&s.exe).unwrap(), b"OLD");
        assert_eq!(files_in(s.exe.parent().unwrap()), vec!["LockNote.exe".to_string()]);
    }

    #[test]
    fn tmp_changed_during_a_cross_volume_swap_is_kept() {
        let s = scene("xvol_changed", b"SAVE NUMBER ONE", b"OLD");
        let mv = |src: &Path, dst: &Path| -> Result<(), MoveError> {
            if src == s.tmp {
                Err(not_same_device())
            } else {
                atomic_move(src, dst)?;
                // Something writes the .tmp again right after the copy.
                std::fs::write(&s.tmp, b"SAVE NUMBER TWO").unwrap();
                Ok(())
            }
        };

        let res = promote(&s.tmp, &s.exe, &with_move(&mv));

        let note = res.expect("the exe was replaced").expect("the kept .tmp is worth a log line");
        assert!(note.contains(".tmp kept"), "{}", note);
        assert_eq!(std::fs::read(&s.exe).unwrap(), b"SAVE NUMBER ONE");
        assert_eq!(std::fs::read(&s.tmp).unwrap(), b"SAVE NUMBER TWO", "the newer .tmp is not deleted");
    }

    #[test]
    fn cross_volume_swap_with_a_missing_exe_folder_keeps_the_tmp() {
        // The drive of the exe is gone: the sibling cannot even be created.
        let s = scene("xvol_no_dir", b"NEW", b"OLD");
        std::fs::remove_dir_all(s.exe.parent().unwrap()).unwrap();
        let mv = |_: &Path, _: &Path| -> Result<(), MoveError> { Err(not_same_device()) };

        let res = promote(&s.tmp, &s.exe, &with_move(&mv));

        let msg = res.expect_err("must fail");
        assert!(msg.contains("step=create sibling"), "{}", msg);
        assert_eq!(std::fs::read(&s.tmp).unwrap(), b"NEW");
    }
    fn with_move<'a>(mv: &'a MoveFn<'a>) -> Io<'a> {
        Io { mv, read: &read_file, recheck: &still_current }
    }

    /// The tests of the move itself: the decision is not in question.
    fn still_current() -> Result<(), String> {
        Ok(())
    }

    fn no_wait() {}

    /// A ready event as the application creates it.
    fn ready_event() -> (String, windows::Win32::Foundation::HANDLE) {
        use windows::core::HSTRING;
        use windows::Win32::System::Threading::CreateEventW;
        let name = format!("Local\\LockNote_SwapReady_test_{}", swap_common::random_id());
        let h = unsafe { CreateEventW(None, true, false, &HSTRING::from(name.as_str())) }.unwrap();
        (name, h)
    }

    fn wait_event(h: windows::Win32::Foundation::HANDLE, ms: u32) -> bool {
        use windows::Win32::System::Threading::WaitForSingleObject;
        unsafe { WaitForSingleObject(h, ms) == windows::Win32::Foundation::WAIT_OBJECT_0 }
    }

    // ── Reviewer 5: the check of the copy written next to the exe ──

    #[test]
    fn cross_volume_copy_that_reads_back_different_is_removed_and_the_tmp_kept() {
        let original: Vec<u8> = (0..4096).map(|i| (i % 251) as u8).collect();
        let shorter = original[..4000].to_vec();
        let mut flipped = original.clone();
        flipped[2000] ^= 0xFF;
        for (tag, given_back) in [("short", shorter), ("flipped", flipped)] {
            let s = scene(&format!("xvol_verify_{}", tag), &original, b"OLD");
            let mv = |src: &Path, dst: &Path| -> Result<(), MoveError> {
                if src == s.tmp { Err(not_same_device()) } else { atomic_move(src, dst) }
            };
            let renames = RefCell::new(0);
            let counted = |src: &Path, dst: &Path| -> Result<(), MoveError> {
                if src != s.tmp {
                    *renames.borrow_mut() += 1;
                }
                mv(src, dst)
            };
            let read = |_: &Path| -> std::io::Result<Vec<u8>> { Ok(given_back.clone()) };

            let res = promote(&s.tmp, &s.exe, &Io { mv: &counted, read: &read, recheck: &still_current });

            let msg = res.expect_err("a copy that differs must stop the swap");
            assert!(msg.contains("step=verify sibling"), "{}: {}", tag, msg);
            assert_eq!(*renames.borrow(), 0, "{}: the copy is never renamed over the exe", tag);
            assert_eq!(std::fs::read(&s.tmp).unwrap(), original, "{}: .tmp intact", tag);
            assert_eq!(std::fs::read(&s.exe).unwrap(), b"OLD", "{}: exe intact", tag);
            assert_eq!(files_in(s.exe.parent().unwrap()), vec!["LockNote.exe".to_string()], "{}", tag);
        }
    }

    // ── Restart after an update ──

    fn os(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    #[test]
    fn the_relaunch_argument_comes_last_after_the_ready_event() {
        let base = ["swap.exe", "42", "a.tmp", "LockNote.exe", "Local\\ready"];
        assert!(!parse_args(&os(&base)).unwrap().relaunch);
        let mut with = base.to_vec();
        with.push(swap_common::RELAUNCH_ARG);
        let args = parse_args(&os(&with)).unwrap();
        assert!(args.relaunch);
        assert_eq!(args.ready.as_deref(), Some("Local\\ready"));
        assert_eq!(args.parent_pid, 42);
        let mut wrong = base.to_vec();
        wrong.push("--other");
        assert!(parse_args(&os(&wrong)).is_err(), "an unknown last argument is refused");
        assert!(parse_args(&os(&base[..3])).is_err());
        assert!(parse_args(&os(&["swap.exe", "x", "a.tmp", "b.exe"])).is_err());
    }

    #[test]
    fn a_done_swap_starts_the_new_exe_once_the_swap_lock_is_free() {
        let s = scene("relaunch_ok", b"NEW", b"OLD");
        let name = swap_common::swap_lock_name(&s.exe);
        let out = run(&s.tmp, &s.exe, None, std::time::Duration::from_secs(5), &no_wait, &with_move(&atomic_move));
        let started = RefCell::new(Vec::new());
        let launch = |exe: &Path| -> Result<(), String> {
            // The new session must get the lock at once (another thread: a
            // mutex is re-entrant for its owner).
            let name = name.clone();
            let free = std::thread::spawn(move || swap_common::SwapLock::try_acquire(&name).map(|l| l.is_some()))
                .join()
                .unwrap();
            assert_eq!(free, Ok(true), "the lock is released before the restart");
            assert_eq!(std::fs::read(exe).unwrap(), b"NEW", "the exe is already the new one");
            started.borrow_mut().push(exe.to_path_buf());
            Ok(())
        };

        let out = relaunch_after(out, true, true, &s.exe, &launch);

        assert_eq!(out, Outcome { code: 0, log: vec![] });
        assert_eq!(*started.borrow(), vec![s.exe.clone()]);
    }

    #[test]
    fn no_restart_when_not_asked_or_when_the_swap_failed() {
        let never = |_: &Path| -> Result<(), String> { panic!("must not start the exe") };
        let done = Outcome { code: 0, log: vec![] };
        assert_eq!(relaunch_after(done, false, true, Path::new("x.exe"), &never), Outcome { code: 0, log: vec![] });

        // The previous LockNote still running at the wait cap: restarting
        // would only bring its window forward (single instance).
        let hung = relaunch_after(Outcome { code: 0, log: vec![] }, true, false, Path::new("x.exe"), &never);
        assert_eq!(hung.code, 0, "the swap itself is done");
        assert!(hung.log[0].contains("still running"), "{:?}", hung.log);

        let failed = Outcome { code: EXIT_SWAP_FAILED, log: vec!["swap failed".into()] };
        let out = relaunch_after(failed, true, true, Path::new("x.exe"), &never);
        assert_eq!(out.code, EXIT_SWAP_FAILED);
        assert!(out.log[1].contains("not restarted"), "{:?}", out.log);
    }

    #[test]
    fn a_restart_that_fails_is_logged_with_its_own_code() {
        let fails = |_: &Path| -> Result<(), String> { Err("simulated 0x80070002".into()) };
        let out = relaunch_after(Outcome { code: 0, log: vec![] }, true, true, Path::new("x.exe"), &fails);
        assert_eq!(out.code, EXIT_RELAUNCH_FAILED);
        assert!(out.log[0].contains("0x80070002") && out.log[0].contains("x.exe"), "{:?}", out.log);
    }

    // ── Exit code and companion ──

    #[test]
    fn run_reports_a_failed_swap_with_a_non_zero_code_and_keeps_the_companion() {
        let s = scene("run_fail", b"NEW", b"OLD");
        let companion = s.tmp.with_extension("base");
        std::fs::write(&companion, b"companion").unwrap();
        let mv = |_: &Path, _: &Path| -> Result<(), MoveError> {
            Err(MoveError { code: Some(5), detail: "simulated 0x80070005".into() })
        };

        let out = run(&s.tmp, &s.exe, None, std::time::Duration::from_secs(5), &no_wait, &with_move(&mv));

        assert_eq!(out.code, EXIT_SWAP_FAILED);
        assert!(out.log.iter().any(|l| l.contains("0x80070005")), "{:?}", out.log);
        assert!(s.tmp.exists() && companion.exists());
    }

    #[test]
    fn run_removes_the_companion_with_the_tmp_it_moved() {
        let s = scene("run_ok", b"NEW", b"OLD");
        let companion = s.tmp.with_extension("base");
        std::fs::write(&companion, b"companion").unwrap();

        let out = run(&s.tmp, &s.exe, None, std::time::Duration::from_secs(5), &no_wait, &with_move(&atomic_move));

        assert_eq!(out, Outcome { code: 0, log: vec![] });
        assert_eq!(std::fs::read(&s.exe).unwrap(), b"NEW");
        assert!(!s.tmp.exists() && !companion.exists());
    }

    // ── H-A: the swap lock ──

    /// Refuter H-A: helper 1 stalls between its checked copy and its final
    /// rename (slow share). While it holds the swap lock, LockNote cannot
    /// start on that exe (so no second save exists), and a second helper
    /// gives up without touching anything.
    #[test]
    fn two_swaps_or_a_swap_and_a_session_never_overlap() {
        let s = scene("two_helpers", b"SAVE ONE (session 1)", b"OLD");
        let name = swap_common::swap_lock_name(&s.exe);
        let (tmp, exe) = (s.tmp.clone(), s.exe.clone());
        let mv1 = |src: &Path, dst: &Path| -> Result<(), MoveError> {
            if src == s.tmp {
                return Err(not_same_device());
            }
            // Stalled here. Another thread (a mutex is re-entrant for its
            // owner thread) tries what a new session and a second helper do.
            let (name, tmp, exe) = (name.clone(), tmp.clone(), exe.clone());
            let (app, helper2) = std::thread::spawn(move || {
                let app = swap_common::SwapLock::acquire(&name, std::time::Duration::from_millis(300))
                    .map(|_| ());
                let xvol = |src: &Path, dst: &Path| -> Result<(), MoveError> {
                    if src == tmp { Err(not_same_device()) } else { atomic_move(src, dst) }
                };
                let helper2 = run(&tmp, &exe, None, std::time::Duration::from_millis(300), &no_wait, &with_move(&xvol));
                (app, helper2)
            })
            .join()
            .unwrap();
            assert!(app.is_err(), "a new session cannot start while the swap runs");
            assert_eq!(helper2.code, EXIT_NO_LOCK);
            assert!(helper2.log[0].contains("gave up, nothing touched"), "{:?}", helper2.log);
            assert_eq!(std::fs::read(&s.tmp).unwrap(), b"SAVE ONE (session 1)", "helper 2 touched nothing");
            assert_eq!(std::fs::read(&s.exe).unwrap(), b"OLD");
            atomic_move(src, dst)
        };

        let out = run(&s.tmp, &s.exe, None, std::time::Duration::from_secs(5), &no_wait, &with_move(&mv1));

        assert_eq!(out.code, 0, "{:?}", out.log);
        assert_eq!(std::fs::read(&s.exe).unwrap(), b"SAVE ONE (session 1)");
        assert!(!s.tmp.exists());
        assert_eq!(files_in(s.exe.parent().unwrap()), vec!["LockNote.exe".to_string()]);
        // Released: the next session can start.
        let name = swap_common::swap_lock_name(&s.exe);
        let next = std::thread::spawn(move || {
            swap_common::SwapLock::acquire(&name, std::time::Duration::from_millis(300)).map(|(_, o)| o)
        })
        .join()
        .unwrap();
        assert_eq!(next, Ok(swap_common::Obtained::Free));
    }

    /// The hand-over at exit: the application holds the lock during its
    /// session; the helper signals the ready event as it queues on the lock;
    /// the application releases the lock only then, and the helper owns it.
    #[test]
    fn helper_takes_the_lock_handed_over_by_the_application() {
        let s = scene("handover", b"NEW", b"OLD");
        let name = swap_common::swap_lock_name(&s.exe);
        let (ready_name, ready) = ready_event();
        let ready_raw = ready.0 as isize;

        // The application: holds the lock, waits for "ready", releases.
        let (to_app, app_rx) = std::sync::mpsc::channel::<()>();
        let app_name = name.clone();
        let app = std::thread::spawn(move || {
            let (lock, _) = swap_common::SwapLock::acquire(&app_name, std::time::Duration::from_secs(5)).unwrap();
            to_app.send(()).unwrap();
            let h = windows::Win32::Foundation::HANDLE(ready_raw as *mut core::ffi::c_void);
            let signalled = wait_event(h, 5000);
            drop(lock);
            signalled
        });
        app_rx.recv().unwrap(); // the session holds the lock

        let out = run(&s.tmp, &s.exe, Some(&ready_name), std::time::Duration::from_secs(5), &no_wait, &with_move(&atomic_move));

        assert!(app.join().unwrap(), "the helper signalled it was queued");
        assert_eq!(out.code, 0, "{:?}", out.log);
        assert_eq!(std::fs::read(&s.exe).unwrap(), b"NEW");
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(ready);
        }
    }

    #[test]
    fn helper_without_the_lock_in_time_touches_nothing() {
        let s = scene("no_handover", b"NEW", b"OLD");
        let name = swap_common::swap_lock_name(&s.exe);
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
        let holder = std::thread::spawn(move || {
            let (_lock, _) = swap_common::SwapLock::acquire(&name, std::time::Duration::from_secs(5)).unwrap();
            tx.send(()).unwrap();
            done_rx.recv().unwrap();
        });
        rx.recv().unwrap();

        let out = run(&s.tmp, &s.exe, None, std::time::Duration::from_millis(300), &no_wait, &with_move(&atomic_move));
        done_tx.send(()).unwrap();
        holder.join().unwrap();

        assert_eq!(out.code, EXIT_NO_LOCK);
        assert_eq!(std::fs::read(&s.tmp).unwrap(), b"NEW");
        assert_eq!(std::fs::read(&s.exe).unwrap(), b"OLD");
    }

    #[test]
    fn abandoned_lock_is_logged_and_the_swap_goes_on() {
        let s = scene("abandoned", b"NEW", b"OLD");
        let name = swap_common::swap_lock_name(&s.exe);
        // A thread that ends while owning the lock (as a crashed owner).
        std::thread::spawn(move || {
            let (lock, _) = swap_common::SwapLock::acquire(&name, std::time::Duration::from_secs(5)).unwrap();
            std::mem::forget(lock);
        })
        .join()
        .unwrap();

        let out = run(&s.tmp, &s.exe, None, std::time::Duration::from_secs(5), &no_wait, &with_move(&atomic_move));

        assert_eq!(out.code, 0);
        assert!(out.log.iter().any(|l| l.contains("abandoned")), "{:?}", out.log);
        assert_eq!(std::fs::read(&s.exe).unwrap(), b"NEW");
    }

    // ── N3: the decision is checked again right before the final rename ──

    /// `[program][marker][payload]`, as the application writes it.
    fn image(program: &[u8], payload: &[u8]) -> Vec<u8> {
        let mut out = program.to_vec();
        out.extend_from_slice(b"LN_DATA_\xDE\xAD\xBE\xEF\xCA\xFE\xF0\x0D");
        out.extend_from_slice(payload);
        out
    }

    fn payload(seed: u8) -> Vec<u8> {
        (0..120u8).map(|i| i.wrapping_mul(3).wrapping_add(seed)).collect()
    }

    /// The companion the application writes for a save based on `base`,
    /// with the `volume=` line it carries since issue 057.
    fn write_companion(s: &Scene, base: &[u8], updater: bool) -> PathBuf {
        let hex: String = Sha256::digest(base).iter().map(|b| format!("{:02x}", b)).collect();
        let path = s.tmp.with_extension(swap_common::COMPANION_EXTENSION);
        let text = format!(
            "LockNote staging companion 1\nexe={}\nbase={}\nprogram={}\nvolume=0123abcd\n",
            s.exe.display(),
            hex,
            if updater { "updater" } else { "exe" }
        );
        std::fs::write(&path, text).unwrap();
        path
    }

    /// Another writer (another computer, another Windows session: the
    /// `Local\` lock does not cover them) puts P2 into the exe after the
    /// application decided at exit and before the helper renames.
    #[test]
    fn exe_written_after_the_exit_decision_is_not_overwritten() {
        let p2_image = image(b"PROGRAM", &payload(2));
        let s = scene("n3_direct", &image(b"PROGRAM", &payload(1)), &image(b"PROGRAM", &payload(0)));
        let companion = write_companion(&s, &payload(0), false);
        let other_writer = || std::fs::write(&s.exe, &p2_image).unwrap();

        let recheck = || still_promotable(&s.tmp, &s.exe);
        let out = run(&s.tmp, &s.exe, None, std::time::Duration::from_secs(5), &other_writer, &Io { mv: &atomic_move, read: &read_file, recheck: &recheck });

        assert_ne!(out.code, 0, "{:?}", out.log);
        assert_eq!(std::fs::read(&s.exe).unwrap(), p2_image, "P2 is not overwritten");
        assert_eq!(std::fs::read(&s.tmp).unwrap(), image(b"PROGRAM", &payload(1)), ".tmp kept");
        assert!(companion.exists(), "companion kept");
        assert!(out.log.iter().any(|l| l.contains("nothing touched")), "{:?}", out.log);
    }

    #[test]
    fn exe_written_during_a_cross_volume_copy_is_not_overwritten() {
        let p2_image = image(b"PROGRAM", &payload(2));
        let s = scene("n3_xvol", &image(b"PROGRAM", &payload(1)), &image(b"PROGRAM", &payload(0)));
        let companion = write_companion(&s, &payload(0), false);
        let mv = |src: &Path, dst: &Path| -> Result<(), MoveError> {
            if src == s.tmp { Err(not_same_device()) } else { atomic_move(src, dst) }
        };
        // The other writer acts while the copy is read back.
        let read = |p: &Path| -> std::io::Result<Vec<u8>> {
            std::fs::write(&s.exe, &p2_image).unwrap();
            std::fs::read(p)
        };

        let recheck = || still_promotable(&s.tmp, &s.exe);
        let out = run(&s.tmp, &s.exe, None, std::time::Duration::from_secs(5), &no_wait, &Io { mv: &mv, read: &read, recheck: &recheck });

        assert_ne!(out.code, 0, "{:?}", out.log);
        assert_eq!(std::fs::read(&s.exe).unwrap(), p2_image, "P2 is not overwritten");
        assert!(s.tmp.exists() && companion.exists());
        assert_eq!(files_in(s.exe.parent().unwrap()), vec!["LockNote.exe".to_string()], "no sibling left");
    }

    /// The check takes the decision of the application: while the exe holds
    /// the base, the swap goes on, directly or through a copy; a program
    /// staged by the updater (no notes yet) too; a .tmp without companion
    /// follows the rule for a .tmp left by an earlier version (dates).
    #[test]
    fn swap_goes_on_while_the_decision_is_still_to_promote() {
        let xvol = |tmp: PathBuf| move |src: &Path, dst: &Path| -> Result<(), MoveError> {
            if src == tmp { Err(not_same_device()) } else { atomic_move(src, dst) }
        };
        for cross_volume in [false, true] {
            // A save based on the notes the exe holds.
            let s = scene("recheck_ok", &image(b"PROGRAM", &payload(1)), &image(b"PROGRAM", &payload(0)));
            let companion = write_companion(&s, &payload(0), false);
            let recheck = || still_promotable(&s.tmp, &s.exe);
            let mv = xvol(s.tmp.clone());
            let io = if cross_volume {
                Io { mv: &mv, read: &read_file, recheck: &recheck }
            } else {
                Io { mv: &atomic_move, read: &read_file, recheck: &recheck }
            };
            let out = run(&s.tmp, &s.exe, None, std::time::Duration::from_secs(5), &no_wait, &io);
            assert_eq!(out.code, 0, "cross volume {}: {:?}", cross_volume, out.log);
            assert_eq!(std::fs::read(&s.exe).unwrap(), image(b"PROGRAM", &payload(1)));
            assert!(!s.tmp.exists() && !companion.exists());
        }

        // The updater's program, the exe without notes (companion base=none).
        let s = scene("recheck_updater", b"NEW PROGRAM", b"OLD PROGRAM");
        std::fs::write(
            s.tmp.with_extension(swap_common::COMPANION_EXTENSION),
            format!("LockNote staging companion 1\nexe={}\nbase=none\nprogram=updater\n", s.exe.display()),
        )
        .unwrap();
        let recheck = || still_promotable(&s.tmp, &s.exe);
        let out = run(&s.tmp, &s.exe, None, std::time::Duration::from_secs(5), &no_wait, &Io { mv: &atomic_move, read: &read_file, recheck: &recheck });
        assert_eq!(out.code, 0, "{:?}", out.log);
        assert_eq!(std::fs::read(&s.exe).unwrap(), b"NEW PROGRAM");

        // No companion: the .tmp is at least as recent as the exe -> goes on;
        // the exe more recent -> stops (a question at the next start).
        for (exe_newer, expected) in [(false, 0), (true, EXIT_SWAP_FAILED)] {
            let s = scene("recheck_legacy", &image(b"PROGRAM", &payload(1)), &image(b"PROGRAM", &payload(0)));
            let old = SystemTime::now() - std::time::Duration::from_secs(3600);
            let older = if exe_newer { &s.tmp } else { &s.exe };
            std::fs::OpenOptions::new().write(true).open(older).unwrap().set_modified(old).unwrap();
            let recheck = || still_promotable(&s.tmp, &s.exe);
            let out = run(&s.tmp, &s.exe, None, std::time::Duration::from_secs(5), &no_wait, &Io { mv: &atomic_move, read: &read_file, recheck: &recheck });
            assert_eq!(out.code, expected, "exe newer {}: {:?}", exe_newer, out.log);
        }
    }

    /// Refuter pass 3, A1: the companion cannot be read when the helper
    /// re-checks (held without sharing: before the direct move, or only from
    /// the read-back of the cross-volume copy on). The swap is abandoned,
    /// nothing touched, and the log names the companion.
    #[test]
    fn companion_unreadable_at_the_recheck_abandons_the_swap() {
        use std::os::windows::fs::OpenOptionsExt;
        let hold = |p: &Path| std::fs::OpenOptions::new().read(true).share_mode(0).open(p).unwrap();
        for cross_volume in [false, true] {
            let exe_bytes = image(b"PROGRAM", &payload(0));
            let tmp_bytes = image(b"PROGRAM", &payload(1));
            let s = scene("companion_held", &tmp_bytes, &exe_bytes);
            let companion = write_companion(&s, &payload(0), false);
            let held: std::cell::RefCell<Option<std::fs::File>> = std::cell::RefCell::new(None);
            if !cross_volume {
                *held.borrow_mut() = Some(hold(&companion));
            }
            let mv = |src: &Path, dst: &Path| -> Result<(), MoveError> {
                if cross_volume && src == s.tmp { Err(not_same_device()) } else { atomic_move(src, dst) }
            };
            // Cross volume: the companion is held once the copy is read back,
            // so only the re-check before the rename of the copy meets it.
            let read = |p: &Path| -> std::io::Result<Vec<u8>> {
                if p != s.tmp && held.borrow().is_none() {
                    *held.borrow_mut() = Some(hold(&companion));
                }
                std::fs::read(p)
            };
            let recheck = || still_promotable(&s.tmp, &s.exe);
            let out = run(&s.tmp, &s.exe, None, std::time::Duration::from_secs(5), &no_wait, &Io { mv: &mv, read: &read, recheck: &recheck });
            assert!(held.borrow().is_some(), "cross volume {}: the companion was held", cross_volume);
            drop(held.borrow_mut().take());
            assert_eq!(out.code, EXIT_SWAP_FAILED, "cross volume {}: {:?}", cross_volume, out.log);
            assert_eq!(std::fs::read(&s.exe).unwrap(), exe_bytes, "cross volume {}: exe untouched", cross_volume);
            assert_eq!(std::fs::read(&s.tmp).unwrap(), tmp_bytes, "cross volume {}: .tmp kept", cross_volume);
            assert!(companion.exists(), "cross volume {}: companion kept", cross_volume);
            assert_eq!(files_in(s.exe.parent().unwrap()), vec!["LockNote.exe".to_string()], "no sibling left");
            let companion_named = companion.display().to_string();
            assert!(
                out.log.iter().any(|l| l.contains("nothing touched") && l.contains(&companion_named)),
                "cross volume {}: {:?}",
                cross_volume,
                out.log
            );
        }
    }
}