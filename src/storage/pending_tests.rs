// Tests of the pending staging file (issue 055): what is done with a `.tmp`
// found at startup or at exit, the companion file and the rule by content,
// the session, the conflict between a `.tmp` and the exe, the atomic staging
// write and the startup cleanup.
//
// Every test works in its own scratch directory (process id + counter):
// nothing is read from or written to the real %LOCALAPPDATA%\LockNote, and
// several `cargo test` processes can run at once.

use super::*;
use std::cell::RefCell;
use std::time::Duration;

/// A scratch directory with a staging directory, a temp directory and the
/// path of a fake exe.
struct Fx {
    root: PathBuf,
    staging: PathBuf,
    temp: PathBuf,
    exe: PathBuf,
    tmp: PathBuf,
}

impl Drop for Fx {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn fx(tag: &str) -> Fx {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "locknote_c3_{}_{}_{}",
        tag,
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&root);
    let staging = root.join("staging");
    let temp = root.join("temp");
    let app = root.join("app");
    fs::create_dir_all(&staging).unwrap();
    fs::create_dir_all(&temp).unwrap();
    fs::create_dir_all(&app).unwrap();
    let exe = app.join("LockNote-Pro.exe");
    let tmp = get_tmp_path_in(&staging, &exe);
    Fx { root, staging, temp, exe, tmp }
}

impl Fx {
    fn companion_path(&self) -> PathBuf {
        companion_path_in(&self.staging, &self.exe)
    }

    /// Write the companion of the .tmp: based on `base` (the notes of the exe
    /// the pending save started from).
    fn companion(&self, base: Option<&[u8]>, from_updater: bool) {
        write_companion_in(
            &self.staging,
            &Companion { exe: self.exe.clone(), base: base.map(sha), program_from_updater: from_updater },
        )
        .unwrap();
    }

    fn read_companion(&self) -> Option<Result<Companion, CompanionError>> {
        read_companion_in(&self.staging, &self.exe)
    }

    fn settle(&self) -> PendingDecision {
        settle_pending_at_startup_in(&self.staging, &self.exe).unwrap()
    }

    fn open(&self) -> (Session, Option<Vec<u8>>) {
        open_session_in(&self.staging, &self.exe).unwrap()
    }

    fn decision(&self) -> PendingDecision {
        pending_decision_in(&self.staging, &self.exe)
    }
}

/// `[program][marker][payload]`
fn image(program: &[u8], payload: &[u8]) -> Vec<u8> {
    let mut out = program.to_vec();
    out.extend_from_slice(&get_marker());
    out.extend_from_slice(payload);
    out
}

/// A payload of valid size, different for each seed.
fn payload(seed: u8) -> Vec<u8> {
    (0..120u8).map(|i| i.wrapping_mul(3).wrapping_add(seed)).collect()
}

fn seconds_ago(secs: u64) -> SystemTime {
    SystemTime::now() - Duration::from_secs(secs)
}

fn set_mtime(path: &Path, when: SystemTime) {
    let f = fs::OpenOptions::new().write(true).open(path).unwrap();
    f.set_modified(when).unwrap();
}

/// Write `bytes` at `path`, dated `secs` seconds ago.
fn put(path: &Path, bytes: &[u8], secs: u64) {
    fs::write(path, bytes).unwrap();
    set_mtime(path, seconds_ago(secs));
}

/// Write `bytes` at `path`, dated `secs` seconds in the future.
fn put_future(path: &Path, bytes: &[u8], secs: u64) {
    fs::write(path, bytes).unwrap();
    set_mtime(path, SystemTime::now() + Duration::from_secs(secs));
}

fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = match fs::read_dir(dir) {
        Ok(entries) => entries
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect(),
        Err(_) => Vec::new(),
    };
    names.sort();
    names
}

fn swap_log(f: &Fx) -> String {
    fs::read_to_string(f.staging.join("swap-error.log")).unwrap_or_default()
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// Does any file under `root` still hold `needle`?
fn anywhere(root: &Path, needle: &[u8]) -> bool {
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if contains(&fs::read(&p).unwrap_or_default(), needle) {
                return true;
            }
        }
    }
    false
}

/// Run the end-of-process step with a fake launcher that records its calls
/// and returns `result`.
fn run_exit(f: &Fx, result: Result<(), String>) -> Vec<(PathBuf, PathBuf)> {
    let calls: RefCell<Vec<(PathBuf, PathBuf)>> = RefCell::new(Vec::new());
    let launch = |tmp: &Path, exe: &Path| -> Result<(), String> {
        calls.borrow_mut().push((tmp.to_path_buf(), exe.to_path_buf()));
        result.clone()
    };
    promote_pending_on_exit_in(&f.staging, &f.exe, &launch);
    calls.into_inner()
}

/// What a successful helper does: the .tmp replaces the exe, the companion
/// goes with it.
fn helper_moves(tmp: &Path, exe: &Path) -> Result<(), String> {
    fs::copy(tmp, exe).map_err(|e| e.to_string())?;
    fs::remove_file(tmp).map_err(|e| e.to_string())?;
    let _ = fs::remove_file(tmp.with_extension("base"));
    Ok(())
}

/// The exit with a helper that succeeds.
fn exit_with_swap(f: &Fx) {
    promote_pending_on_exit_in(&f.staging, &f.exe, &helper_moves);
}

// ── (b) Cleanup ──

#[test]
fn cleanup_keeps_the_pending_tmp_of_another_exe() {
    let f = fx("cleanup_foreign");
    // The pending data of ANOTHER exe: valid, and older than a minute.
    let other_exe = f.root.join("elsewhere").join("LockNote-Pro.exe");
    let other_tmp = get_tmp_path_in(&f.staging, &other_exe);
    let content = image(b"OTHER PROGRAM", &payload(1));
    put(&other_tmp, &content, 3600);
    let other_companion = companion_path_in(&f.staging, &other_exe);
    put(&other_companion, b"companion of another exe", 3600);

    cleanup_in(&f.staging, &f.temp, &f.exe);

    assert!(other_tmp.exists(), "the pending .tmp of another exe must survive the cleanup");
    assert_eq!(fs::read(&other_tmp).unwrap(), content);
    assert!(other_companion.exists());
}

#[test]
fn cleanup_keeps_the_tmp_of_this_exe_and_foreign_files() {
    let f = fx("cleanup_own");
    put(&f.tmp, &image(b"PROGRAM", &payload(1)), 3600);
    put(&f.companion_path(), b"x", 3600);
    let license = f.staging.join("license.dat");
    put(&license, b"license", 3600);
    let log = f.staging.join("swap-error.log");
    put(&log, b"[1] x\n", 3600);

    cleanup_in(&f.staging, &f.temp, &f.exe);

    assert!(f.tmp.exists());
    assert!(f.companion_path().exists());
    assert!(license.exists());
    assert!(log.exists());
}

#[test]
fn cleanup_removes_stale_extracted_helpers_only() {
    let f = fx("cleanup_helpers");
    let stale = f.temp.join("locknote-swap-1234-00112233445566778899aabbccddeeff.exe");
    let fresh = f.temp.join("locknote-swap-1234-ffeeddccbbaa99887766554433221100.exe");
    let other = f.temp.join("something-else.exe");
    let dir_named_like_one = f.temp.join("locknote-swap-dir.exe");
    put(&stale, b"MZ", 3600);
    put(&fresh, b"MZ", 5);
    put(&other, b"MZ", 3600);
    fs::create_dir_all(&dir_named_like_one).unwrap();

    cleanup_in(&f.staging, &f.temp, &f.exe);

    assert!(!stale.exists(), "an extracted helper older than a minute is removed");
    assert!(fresh.exists(), "a helper younger than a minute may still be running");
    assert!(other.exists());
    assert!(dir_named_like_one.exists(), "only regular files are removed");
}

#[test]
fn cleanup_removes_stale_swap_siblings_of_this_exe_only() {
    let f = fx("cleanup_siblings");
    let app = f.exe.parent().unwrap();
    fs::write(&f.exe, image(b"PROGRAM", &payload(1))).unwrap();
    let stale = app.join("LockNote-Pro.exe.0123456789abcdef.lnswap");
    let fresh = app.join("LockNote-Pro.exe.fedcba9876543210.lnswap");
    let other_exe = app.join("Other.exe.0123456789abcdef.lnswap");
    let bad_hex = app.join("LockNote-Pro.exe.0123456789ABCDEG.lnswap");
    let short_hex = app.join("LockNote-Pro.exe.0123.lnswap");
    let migrate_new = app.join("LockNote-Pro.exe.new");
    let backup = app.join("LockNote-Pro.exe.bak-20261116-101500");
    let dir_like = app.join("LockNote-Pro.exe.aaaaaaaaaaaaaaaa.lnswap");
    let user_close_1 = app.join("LockNote-Pro.exe.0123456789abcdef.lnswap.txt");
    let user_close_2 = app.join("LockNote-Pro.exe.notes.lnswap");
    put(&stale, b"copy", 3600);
    put(&fresh, b"copy", 5);
    put(&other_exe, b"copy", 3600);
    put(&bad_hex, b"copy", 3600);
    put(&short_hex, b"copy", 3600);
    put(&migrate_new, b"copy", 3600);
    put(&backup, b"copy", 3600);
    put(&user_close_1, b"user", 3600);
    put(&user_close_2, b"user", 3600);
    fs::create_dir_all(&dir_like).unwrap();

    cleanup_in(&f.staging, &f.temp, &f.exe);

    assert!(!stale.exists(), "a sibling left by an interrupted swap is removed after a minute");
    assert!(fresh.exists(), "a young sibling may belong to a swap in progress");
    assert!(other_exe.exists(), "the sibling of another exe is not ours");
    assert!(bad_hex.exists());
    assert!(short_hex.exists());
    assert!(migrate_new.exists(), "<exe>.new belongs to migrate-exe.ps1");
    assert!(backup.exists());
    assert!(user_close_1.exists() && user_close_2.exists(), "files with close names are the user's");
    assert!(dir_like.exists(), "only regular files are removed");
    assert!(f.exe.exists());
}

#[test]
fn cleanup_removes_stale_part_files_of_this_exe_only() {
    let f = fx("cleanup_parts");
    let tmp_name = f.tmp.file_name().unwrap().to_string_lossy().to_string();
    let base_name = f.companion_path().file_name().unwrap().to_string_lossy().to_string();
    let stale = f.staging.join(format!("{}.0123456789abcdef.part", tmp_name));
    let stale_base = f.staging.join(format!("{}.0123456789abcdef.part", base_name));
    let fresh = f.staging.join(format!("{}.fedcba9876543210.part", tmp_name));
    let foreign = f.staging.join("ffffffffffffffff.tmp.0123456789abcdef.part");
    let foreign_base = f.staging.join("ffffffffffffffff.base.0123456789abcdef.part");
    put(&stale, b"half", 3600);
    put(&stale_base, b"half", 3600);
    put(&fresh, b"half", 5);
    put(&foreign, b"half", 3600);
    put(&foreign_base, b"half", 3600);

    cleanup_in(&f.staging, &f.temp, &f.exe);

    assert!(!stale.exists(), "a part file left by an interrupted save is removed after a minute");
    assert!(!stale_base.exists(), "the same for the companion");
    assert!(fresh.exists());
    assert!(foreign.exists(), "the part file of another exe is not ours");
    assert!(foreign_base.exists());
}

/// Refuter A9: the cleanup next to an exe on another volume (a network share
/// dates files with its own clock). Opt-in: LOCKNOTE_TEST_OTHER_VOLUME names
/// a new directory on another volume; only files created here are touched.
#[test]
#[ignore = "needs LOCKNOTE_TEST_OTHER_VOLUME=<new directory on another volume>"]
fn cleanup_next_to_an_exe_on_another_volume() {
    let dir = PathBuf::from(std::env::var_os("LOCKNOTE_TEST_OTHER_VOLUME").expect("LOCKNOTE_TEST_OTHER_VOLUME"));
    let f = fx("cleanup_other_volume");
    let sub = dir.join(format!("cleanup-{}", swap_common::random_id()));
    fs::create_dir_all(&sub).unwrap();
    let exe = sub.join("Clean Me é.exe");
    fs::write(&exe, b"x").unwrap();
    let orphan_old = sub.join("Clean Me é.exe.0123456789abcdef.lnswap");
    let orphan_new = sub.join("Clean Me é.exe.1123456789abcdef.lnswap");
    let user1 = sub.join("Clean Me é.exe.0123456789abcdef.lnswap.txt");
    let user2 = sub.join("Clean Me é.exe.notes.lnswap");
    for p in [&orphan_old, &orphan_new, &user1, &user2] {
        fs::write(p, b"data").unwrap();
    }
    for p in [&orphan_old, &user1, &user2] {
        set_mtime(p, seconds_ago(120));
    }

    cleanup_in(&f.staging, &f.temp, &exe);

    let (old, new, u1, u2) = (orphan_old.exists(), orphan_new.exists(), user1.exists(), user2.exists());
    let _ = fs::remove_dir_all(&sub);
    assert!(!old, "an old orphan copy is removed");
    assert!(new, "a fresh one may belong to a swap in progress");
    assert!(u1 && u2, "the user's files are kept");
}

// ── Atomic staging write ──

#[test]
fn failed_staging_write_keeps_the_previous_tmp() {
    let f = fx("atomic_fail");
    let previous = image(b"PROGRAM", &payload(1));
    fs::write(&f.tmp, &previous).unwrap();

    let res = write_staging_file(&f.tmp, |file| {
        use std::io::Write;
        file.write_all(b"HALF OF THE NEW CONT")?;
        Err(io::Error::new(io::ErrorKind::Other, "simulated: disk full"))
    });

    assert!(res.is_err(), "the failure must be reported");
    assert_eq!(
        fs::read(&f.tmp).unwrap(),
        previous,
        "a failed save must leave the previous .tmp untouched"
    );
    assert_eq!(
        names_in(&f.staging),
        vec![f.tmp.file_name().unwrap().to_string_lossy().to_string()],
        "no partial file is left behind"
    );
}

#[test]
fn failed_first_staging_write_leaves_no_tmp() {
    let f = fx("atomic_fail_first");

    let res = write_staging_file(&f.tmp, |file| {
        use std::io::Write;
        file.write_all(b"HALF")?;
        Err(io::Error::new(io::ErrorKind::Other, "simulated"))
    });

    assert!(res.is_err());
    assert!(!f.tmp.exists(), "a failed save must not leave a partial .tmp");
    assert!(names_in(&f.staging).is_empty());
}

#[test]
fn failed_final_rename_keeps_the_previous_tmp_and_removes_the_part_file() {
    // Refuter A7 / Reviewer 5: the .tmp is held open by another process
    // without sharing (an antivirus, a backup tool): the rename at the end
    // of the write fails.
    use std::os::windows::fs::OpenOptionsExt;
    let f = fx("atomic_rename_fail");
    let previous = image(b"PROGRAM", &payload(1));
    fs::write(&f.tmp, &previous).unwrap();

    let res = {
        let _held = fs::OpenOptions::new().read(true).share_mode(0).open(&f.tmp).unwrap();
        write_staging_file(&f.tmp, |file| {
            use std::io::Write;
            file.write_all(&image(b"PROGRAM", &payload(2)))
        })
    };

    let err = res.expect_err("the failed rename must be reported");
    assert!(err.raw_os_error().is_some(), "an OS error: {:?}", err);
    assert_eq!(fs::read(&f.tmp).unwrap(), previous, "the previous .tmp is untouched");
    assert_eq!(
        names_in(&f.staging),
        vec![f.tmp.file_name().unwrap().to_string_lossy().to_string()],
        "the part file is removed"
    );
}

#[test]
fn staging_write_replaces_the_tmp_and_leaves_only_tmp_and_companion() {
    let f = fx("atomic_ok");
    fs::write(&f.exe, b"PROGRAM").unwrap();
    let (mut s, _) = f.open();
    write_data_in(&f.staging, &mut s, &payload(1)).unwrap();
    write_data_in(&f.staging, &mut s, &payload(2)).unwrap();

    assert_eq!(fs::read(&f.tmp).unwrap(), image(b"PROGRAM", &payload(2)));
    let mut expected = vec![
        f.tmp.file_name().unwrap().to_string_lossy().to_string(),
        f.companion_path().file_name().unwrap().to_string_lossy().to_string(),
    ];
    expected.sort();
    assert_eq!(names_in(&f.staging), expected);
}

// ── The companion ──

#[test]
fn companion_text_round_trips_and_is_read_strictly() {
    let c = Companion {
        exe: PathBuf::from(r"X:\notes\LockNote-Pro.exe"),
        base: Some(sha(b"notes")),
        program_from_updater: true,
    };
    let text = c.to_text();
    assert!(text.starts_with("LockNote staging companion 1\n"), "{}", text);
    assert!(text.contains("\nexe=X:\\notes\\LockNote-Pro.exe\n"), "{}", text);
    assert!(text.contains("\nprogram=updater\n"), "{}", text);
    assert_eq!(Companion::parse(&text), Ok(c.clone()));
    let none = Companion { base: None, program_from_updater: false, ..c.clone() };
    assert!(none.to_text().contains("\nbase=none\nprogram=exe\n"));
    assert_eq!(Companion::parse(&none.to_text()), Ok(none));

    let good = c.to_text();
    for bad in [
        String::new(),
        good.replace("LockNote staging companion 1", "LockNote staging companion 2"),
        good.replace("program=updater", "program=other"),
        good.replace("program=updater\n", ""),
        good.replace("base=", "base=zz"),
        good.replace("exe=", "path="),
        format!("{}exe=C:\\other.exe\n", good),
        good.replace("exe=", "exe"),
    ] {
        assert!(Companion::parse(&bad).is_err(), "must be refused: {:?}", bad);
    }
}

#[test]
fn companion_volume_line_is_optional_and_read_strictly() {
    // Issue 057: the volume serial number of the exe, for the orphan check.
    let c = Companion { exe: PathBuf::from(r"E:\LockNote.exe"), base: None, program_from_updater: false };
    let with = c.to_text_with_volume(Some(0x00ab_12cd));
    assert!(with.ends_with("\nvolume=00ab12cd\n"), "{}", with);
    assert_eq!(Companion::parse_with_volume(&with), Ok((c.clone(), Some(0x00ab_12cd))));
    assert_eq!(Companion::parse(&with), Ok(c.clone()), "the decision ignores it");
    assert_eq!(Companion::parse_with_volume(&c.to_text()), Ok((c.clone(), None)));
    for bad in [
        format!("{}volume=00ab12cd\n", with),
        with.replace("00ab12cd", "ab12cd"),
        with.replace("00ab12cd", "00ab12cg"),
        with.replace("00ab12cd", "+0ab12cd"),
        with.replace("00ab12cd", ""),
    ] {
        assert!(Companion::parse(&bad).is_err(), "must be refused: {:?}", bad);
    }
}

#[test]
fn companion_of_another_exe_is_not_used() {
    let f = fx("companion_other_exe");
    write_companion_in(
        &f.staging,
        &Companion { exe: f.root.join("other.exe"), base: None, program_from_updater: false },
    )
    .unwrap();
    // Written under the name of the other exe: copy it under ours.
    fs::copy(companion_path_in(&f.staging, &f.root.join("other.exe")), f.companion_path()).unwrap();
    match f.read_companion() {
        Some(Err(CompanionError::Invalid(e))) => assert!(e.contains("another program file"), "{}", e),
        other => panic!("expected an unusable companion, got {:?}", other),
    }
    // Same path, other case: the same exe.
    let upper = PathBuf::from(f.exe.to_string_lossy().to_uppercase());
    write_companion_in(&f.staging, &Companion { exe: upper, base: None, program_from_updater: false }).unwrap();
    assert!(matches!(f.read_companion(), Some(Ok(_))));
}

// ── (c) What is done with an existing .tmp ──

#[test]
fn no_tmp_means_nothing_to_do() {
    let f = fx("none");
    fs::write(&f.exe, image(b"PROGRAM", &payload(1))).unwrap();

    assert_eq!(f.decision(), PendingDecision::None);
    assert!(run_exit(&f, Ok(())).is_empty());
    assert_eq!(read_data_in(&f.staging, &f.exe), Ok(Some(payload(1))));
    assert_eq!(f.open().1, Some(payload(1)));
    assert_eq!(swap_log(&f), "");
}

#[test]
fn companion_without_tmp_is_removed_at_startup() {
    let f = fx("lonely_companion");
    fs::write(&f.exe, image(b"PROGRAM", &payload(1))).unwrap();
    f.companion(Some(&payload(1)), false);

    assert_eq!(f.settle(), PendingDecision::None);
    assert!(!f.companion_path().exists());
    assert!(swap_log(&f).contains("companion without .tmp removed"), "{}", swap_log(&f));
}

#[test]
fn pending_save_is_promoted_while_the_exe_holds_its_base_whatever_the_dates() {
    for (tmp_age, exe_age) in [(30u64, 60u64), (60, 30), (30, 30)] {
        let f = fx("promote_by_content");
        put(&f.exe, &image(b"PROGRAM", &payload(1)), exe_age);
        put(&f.tmp, &image(b"PROGRAM", &payload(2)), tmp_age);
        f.companion(Some(&payload(1)), false);

        assert_eq!(f.settle(), PendingDecision::Promote, "tmp age {} exe age {}", tmp_age, exe_age);
        assert_eq!(f.open().1, Some(payload(2)));
        assert_eq!(read_data_in(&f.staging, &f.exe), Ok(Some(payload(2))));
        assert_eq!(run_exit(&f, Ok(())), vec![(f.tmp.clone(), f.exe.clone())]);
    }
    // An exe dated in the future of this computer (file server ahead).
    let f = fx("promote_future_exe");
    put_future(&f.exe, &image(b"PROGRAM", &payload(1)), 3600);
    put(&f.tmp, &image(b"PROGRAM", &payload(2)), 30);
    f.companion(Some(&payload(1)), false);
    assert_eq!(f.settle(), PendingDecision::Promote);
}

#[test]
fn exe_written_elsewhere_is_a_conflict_whatever_the_dates() {
    for (tmp_age, exe_age) in [(30u64, 60u64), (60, 30)] {
        let f = fx("conflict_by_content");
        let tmp_bytes = image(b"PROGRAM", &payload(2));
        put(&f.tmp, &tmp_bytes, tmp_age);
        put(&f.exe, &image(b"PROGRAM", &payload(3)), exe_age);
        f.companion(Some(&payload(1)), false); // based on notes the exe no longer holds

        match f.settle() {
            PendingDecision::Conflict { tmp_modified, exe_modified, exe_has_data, cause } => {
                assert_eq!(tmp_modified, fs::metadata(&f.tmp).unwrap().modified().unwrap());
                assert_eq!(exe_modified, fs::metadata(&f.exe).unwrap().modified().unwrap());
                assert!(exe_has_data);
                assert_eq!(cause, ConflictCause::ExeChanged);
            }
            other => panic!("expected a conflict, got {:?}", other),
        }
        // Unresolved: no session, nothing promoted, nothing deleted.
        assert!(open_session_in(&f.staging, &f.exe).is_err());
        assert_eq!(read_data_in(&f.staging, &f.exe), Ok(Some(payload(3))));
        assert!(run_exit(&f, Ok(())).is_empty(), "an unresolved conflict is never promoted");
        assert_eq!(fs::read(&f.tmp).unwrap(), tmp_bytes);
        assert!(swap_log(&f).contains("conflict"), "log: {:?}", swap_log(&f));
    }
}

#[test]
fn unusable_companion_is_a_conflict_not_a_guess() {
    let f = fx("bad_companion");
    put(&f.tmp, &image(b"PROGRAM", &payload(2)), 30);
    put(&f.exe, &image(b"PROGRAM", &payload(1)), 60);
    fs::write(f.companion_path(), b"garbage").unwrap();

    match f.settle() {
        PendingDecision::Conflict { cause: ConflictCause::CompanionInvalid(e), .. } => {
            assert!(e.contains("header"), "{}", e)
        }
        other => panic!("expected a conflict, got {:?}", other),
    }
    assert!(run_exit(&f, Ok(())).is_empty());
}

#[test]
fn truncated_tmp_is_ignored_neither_promoted_nor_deleted() {
    let f = fx("truncated");
    put(&f.exe, &image(b"PROGRAM", &payload(1)), 60);
    // Cut in the middle of the program: no marker.
    put(&f.tmp, b"PROGR", 30);

    let decision = f.settle();
    let launches = run_exit(&f, Ok(()));

    assert_eq!(decision, PendingDecision::Ignored(IgnoreReason::NoMarker));
    assert!(launches.is_empty(), "a .tmp without data must never replace the exe");
    assert_eq!(fs::read(&f.tmp).unwrap(), b"PROGR", "it is not deleted either");
    assert_eq!(f.open().1, Some(payload(1)), "the session opens on the exe");
    let log = swap_log(&f);
    assert!(log.contains("no marker"), "the reason is logged: {:?}", log);
}

#[test]
fn tmp_with_a_payload_too_short_is_ignored() {
    let f = fx("short");
    put(&f.exe, &image(b"PROGRAM", &payload(1)), 60);
    put(&f.tmp, &image(b"PROGRAM", &[0xAB; MIN_PAYLOAD_SIZE - 1]), 30);
    f.companion(Some(&payload(1)), true);

    assert_eq!(
        f.settle(),
        PendingDecision::Ignored(IgnoreReason::PayloadTooShort(MIN_PAYLOAD_SIZE - 1))
    );
    assert!(run_exit(&f, Ok(())).is_empty());
    assert!(f.tmp.exists());
    assert_eq!(f.open().1, Some(payload(1)));
}

#[test]
fn stale_tmp_is_removed_at_startup_and_logged() {
    // Same notes on both sides: the .tmp holds nothing the exe does not have.
    let f = fx("stale");
    put(&f.tmp, &image(b"OLD PROGRAM", &payload(1)), 30);
    let exe_bytes = image(b"NEW PROGRAM", &payload(1));
    put(&f.exe, &exe_bytes, 60);
    f.companion(Some(&payload(0)), false);

    assert_eq!(f.decision(), PendingDecision::Stale);
    assert!(run_exit(&f, Ok(())).is_empty(), "a stale .tmp would downgrade the program");
    assert_eq!(f.settle(), PendingDecision::Stale);

    assert!(!f.tmp.exists(), "the stale .tmp is removed");
    assert!(!f.companion_path().exists(), "with its companion");
    assert_eq!(fs::read(&f.exe).unwrap(), exe_bytes);
    assert_eq!(f.open().1, Some(payload(1)));
    assert!(swap_log(&f).contains("stale .tmp removed"), "the removal is logged: {}", swap_log(&f));
}

#[test]
fn missing_exe_blocks_the_promotion_and_the_session() {
    // The exe cannot be read (drive gone): nothing can be decided.
    let f = fx("missing_exe");
    put(&f.tmp, &image(b"PROGRAM", &payload(2)), 30);
    f.companion(Some(&payload(1)), false);

    match f.settle() {
        PendingDecision::ExeUnreadable(_) => {}
        other => panic!("expected an unreadable exe, got {:?}", other),
    }
    assert!(open_session_in(&f.staging, &f.exe).is_err(), "no session on the .tmp");
    assert!(run_exit(&f, Ok(())).is_empty());
    assert!(f.tmp.exists());
    assert!(swap_log(&f).contains("cannot be read"), "{}", swap_log(&f));
}

/// Reviewer 2: the exe cannot be read at startup (held, or a share that
/// does not answer): no session opens on the .tmp; once the exe is readable
/// again, its notes are never overwritten without a question.
#[test]
fn unreadable_exe_never_opens_the_session_on_the_tmp_nor_promotes_over_newer_notes() {
    use std::os::windows::fs::OpenOptionsExt;
    let f = fx("r2");
    let (p_base, p_tmp, p_exe) = (payload(0), payload(1), payload(2));
    put(&f.tmp, &image(b"PROGRAM", &p_tmp), 600);
    f.companion(Some(&p_base), false);
    put(&f.exe, &image(b"PROGRAM", &p_exe), 60); // written elsewhere since

    {
        let _held = fs::OpenOptions::new().read(true).share_mode(0).open(&f.exe).unwrap();
        assert!(matches!(f.settle(), PendingDecision::ExeUnreadable(_)));
        assert!(open_session_in(&f.staging, &f.exe).is_err(), "no session while the exe cannot be read");
        assert!(run_exit(&f, Ok(())).is_empty());
    }
    // Readable again: the content rule sees the exe was written elsewhere.
    assert!(matches!(f.settle(), PendingDecision::Conflict { cause: ConflictCause::ExeChanged, .. }));
    exit_with_swap(&f);
    assert!(anywhere(&f.root, &p_exe) && anywhere(&f.root, &p_tmp), "both versions survive");
}

#[test]
fn inherited_tmp_is_promoted_at_exit_without_a_save_in_this_session() {
    // A .tmp left by a previous session (the swap failed, or was never
    // launched) and no save in this one: the exit must still launch the swap.
    let f = fx("inherited");
    put(&f.exe, &image(b"PROGRAM", &payload(1)), 60);
    put(&f.tmp, &image(b"PROGRAM", &payload(2)), 30);
    f.companion(Some(&payload(1)), false);

    assert_eq!(f.settle(), PendingDecision::Promote);
    let (s, opened) = f.open();
    assert_eq!(opened, Some(payload(2)));
    assert_eq!(s.current(), Some(&payload(2)[..]));
    assert_eq!(run_exit(&f, Ok(())), vec![(f.tmp.clone(), f.exe.clone())]);
}

#[test]
fn launch_failure_is_logged_not_swallowed() {
    let f = fx("launch_fail");
    put(&f.exe, &image(b"PROGRAM", &payload(1)), 60);
    put(&f.tmp, &image(b"PROGRAM", &payload(2)), 30);
    f.companion(Some(&payload(1)), false);

    let launches = run_exit(&f, Err("spawn helper: access denied (simulated)".to_string()));

    assert_eq!(launches.len(), 1);
    let log = swap_log(&f);
    assert!(log.contains("spawn helper: access denied (simulated)"), "log: {:?}", log);
    assert!(log.starts_with('['), "same dated lines as the helper: {:?}", log);
    assert!(log.ends_with('\n'));
    assert!(f.tmp.exists(), "the pending data stays in place");
}

/// "Restart now" after an update: the application must know when no helper
/// will replace the exe (and so restart it), to tell the user.
#[test]
fn the_exit_step_says_whether_a_helper_is_queued() {
    let promotable = |name: &str| {
        let f = fx(name);
        put(&f.exe, &image(b"PROGRAM", &payload(1)), 60);
        put(&f.tmp, &image(b"PROGRAM", &payload(2)), 30);
        f.companion(Some(&payload(1)), false);
        f
    };
    let ok = |_: &Path, _: &Path| -> Result<(), String> { Ok(()) };
    let fails = |_: &Path, _: &Path| -> Result<(), String> { Err("spawn helper: simulated".into()) };

    let f = promotable("queued_ok");
    assert!(promote_pending_on_exit_in(&f.staging, &f.exe, &ok));
    let f = promotable("queued_fail");
    assert!(!promote_pending_on_exit_in(&f.staging, &f.exe, &fails), "a launch failure queues nothing");

    let f = fx("queued_conflict");
    put(&f.tmp, &image(b"PROGRAM", &payload(2)), 30);
    put(&f.exe, &image(b"PROGRAM", &payload(3)), 60);
    f.companion(Some(&payload(1)), false);
    assert!(!promote_pending_on_exit_in(&f.staging, &f.exe, &ok), "a conflict launches nothing");

    let f = fx("queued_nothing");
    put(&f.exe, &image(b"PROGRAM", &payload(1)), 60);
    assert!(!promote_pending_on_exit_in(&f.staging, &f.exe, &ok), "no .tmp, no helper");
}

// ── A .tmp left by an earlier version (no companion) ──

/// The user's real case (issue 055): a .tmp written at 14:23 by 1.2.23, the
/// exe on a network share dated 09:58, other notes. Promoted without a
/// question, and from then on judged by content.
#[test]
fn legacy_tmp_of_the_real_case_is_promoted_without_question() {
    let f = fx("legacy_real");
    let day = SystemTime::UNIX_EPOCH + Duration::from_secs(1_794_787_200); // 2026-11-16 00:00 UTC
    let (p_exe, p_tmp) = (payload(1), payload(2));
    fs::write(&f.tmp, image(b"PROGRAM 1.2.23", &p_tmp)).unwrap();
    set_mtime(&f.tmp, day + Duration::from_secs(14 * 3600 + 23 * 60));
    fs::write(&f.exe, image(b"PROGRAM 1.2.23", &p_exe)).unwrap();
    set_mtime(&f.exe, day + Duration::from_secs(9 * 3600 + 58 * 60));
    assert!(f.read_companion().is_none(), "left by 1.2.23: no companion");

    assert_eq!(f.settle(), PendingDecision::Promote);
    assert_eq!(
        f.read_companion(),
        Some(Ok(Companion { exe: f.exe.clone(), base: Some(sha(&p_exe)), program_from_updater: false })),
        "the companion is written at once, based on the notes the exe holds now"
    );
    assert!(swap_log(&f).contains("left by an earlier version taken up"), "{}", swap_log(&f));
    let (_, opened) = f.open();
    assert_eq!(opened, Some(p_tmp.clone()));

    // From now on dates no longer matter: the exe dated after the .tmp.
    set_mtime(&f.exe, SystemTime::now() + Duration::from_secs(60));
    assert_eq!(f.decision(), PendingDecision::Promote);
    exit_with_swap(&f);
    assert_eq!(read_payload_from_file(&f.exe), Some(p_tmp));
    assert!(!f.tmp.exists() && !f.companion_path().exists());
}

#[test]
fn legacy_tmp_older_than_the_exe_with_other_notes_is_a_conflict() {
    let f = fx("legacy_conflict");
    put(&f.tmp, &image(b"PROGRAM", &payload(2)), 60);
    put(&f.exe, &image(b"PROGRAM", &payload(1)), 30);

    match f.settle() {
        PendingDecision::Conflict { cause: ConflictCause::LegacyExeNewer, exe_has_data: true, .. } => {}
        other => panic!("expected a conflict, got {:?}", other),
    }
    assert!(f.read_companion().is_none(), "nothing is written for a conflict");
    // Same notes: stale, whatever the dates.
    let g = fx("legacy_stale");
    put(&g.tmp, &image(b"PROGRAM", &payload(1)), 60);
    put(&g.exe, &image(b"PROGRAM", &payload(1)), 30);
    assert_eq!(g.settle(), PendingDecision::Stale);
    assert!(!g.tmp.exists());
}

#[test]
fn legacy_tmp_is_not_promoted_at_an_exit_that_did_not_go_through_startup() {
    // The startup step failed before settling the .tmp (up to 1.2.27, also a
    // cancelled licence dialog): the session never started, the .tmp has no
    // companion yet. Its fate is decided at the next start, not by dates
    // at exit.
    let f = fx("legacy_exit");
    put(&f.exe, &image(b"PROGRAM", &payload(1)), 60);
    put(&f.tmp, &image(b"PROGRAM", &payload(2)), 30);

    assert!(run_exit(&f, Ok(())).is_empty());
    assert!(swap_log(&f).contains("no companion file yet"), "{}", swap_log(&f));
    assert!(f.tmp.exists());
}

// ── The session ──

/// Refuter A1: the exe becomes "more recent" than the .tmp while the session
/// runs (PC clock stepped back, file server ahead, a tool touching the exe).
/// The session gives back its last save, and the next start promotes it.
#[test]
fn session_gives_back_its_last_save_whatever_the_dates() {
    let f = fx("a1");
    let (p0, p1, p2) = (payload(10), payload(11), payload(12));
    put(&f.exe, &image(b"PROGRAM", &p0), 3600);
    assert_eq!(f.settle(), PendingDecision::None);
    let (mut s, opened) = f.open();
    assert_eq!(opened, Some(p0.clone()));

    write_data_in(&f.staging, &mut s, &p1).unwrap();
    set_mtime(&f.exe, SystemTime::now() + Duration::from_secs(5));
    assert_eq!(s.current(), Some(&p1[..]), "unlock after a lock shows the last save");
    assert_eq!(f.decision(), PendingDecision::Promote, "dates play no part");

    write_data_in(&f.staging, &mut s, &p2).unwrap();
    assert_eq!(s.current(), Some(&p2[..]));
    exit_with_swap(&f);
    assert_eq!(read_payload_from_file(&f.exe), Some(p2));
}

#[test]
fn session_ignores_what_is_written_by_something_else_after_it_opened() {
    let f = fx("session_own");
    put(&f.exe, &image(b"PROGRAM", &payload(1)), 60);
    let (mut s, _) = f.open();
    write_data_in(&f.staging, &mut s, &payload(2)).unwrap();
    // Something else rewrites the .tmp and the exe.
    fs::write(&f.tmp, image(b"PROGRAM", &payload(7))).unwrap();
    fs::write(&f.exe, image(b"PROGRAM", &payload(8))).unwrap();
    assert_eq!(s.current(), Some(&payload(2)[..]));
    // At exit the exe no longer holds the base: conflict, nothing promoted,
    // the question comes at the next start.
    assert!(run_exit(&f, Ok(())).is_empty());
    assert!(matches!(f.decision(), PendingDecision::Conflict { cause: ConflictCause::ExeChanged, .. }));
}

#[test]
fn process_session_must_be_open_for_its_exe() {
    let f = fx("no_session");
    // The process session is not opened by the tests: every access must fail
    // explicitly rather than guess.
    assert!(read_session_data(&f.exe).is_err());
    assert!(write_data(&f.exe, &payload(1)).is_err());
    assert!(write_staged_update(&f.exe, b"PROGRAM").is_err());
    assert!(!f.tmp.exists());
}

/// The first save of a session opened on the exe while an ignored .tmp is
/// there moves that .tmp aside rather than overwriting it.
#[test]
fn first_save_sets_an_ignored_tmp_aside() {
    let f = fx("aside");
    put(&f.exe, &image(b"PROGRAM", &payload(1)), 60);
    put(&f.tmp, b"PROGR", 30);
    assert_eq!(f.settle(), PendingDecision::Ignored(IgnoreReason::NoMarker));
    let (mut s, _) = f.open();
    write_data_in(&f.staging, &mut s, &payload(2)).unwrap();
    let aside = names_in(&conflicts_dir_in(&f.staging));
    assert_eq!(aside.len(), 1);
    assert_eq!(fs::read(conflicts_dir_in(&f.staging).join(&aside[0])).unwrap(), b"PROGR");
    assert!(swap_log(&f).contains("set aside before its first save"), "{}", swap_log(&f));
    // A second save does not set its own .tmp aside.
    write_data_in(&f.staging, &mut s, &payload(3)).unwrap();
    assert_eq!(names_in(&conflicts_dir_in(&f.staging)).len(), 1);
}

#[test]
fn save_fails_explicitly_when_the_companion_of_its_tmp_disappears() {
    let f = fx("companion_gone");
    put(&f.exe, &image(b"PROGRAM", &payload(1)), 60);
    let (mut s, _) = f.open();
    write_data_in(&f.staging, &mut s, &payload(2)).unwrap();
    fs::remove_file(f.companion_path()).unwrap();
    let before = fs::read(&f.tmp).unwrap();
    assert!(write_data_in(&f.staging, &mut s, &payload(3)).is_err());
    assert_eq!(fs::read(&f.tmp).unwrap(), before);
    assert_eq!(s.current(), Some(&payload(2)[..]));
}

// ── The program part (A5, A8) ──

/// Refuter A5: a newer program copied over the exe with an older date (the
/// Explorer keeps the date of the copied file) while an older-program .tmp
/// is pending.
#[test]
fn pending_save_never_brings_an_older_program_back() {
    // Same notes: the .tmp is stale and removed.
    let f = fx("a5_same_notes");
    put(&f.tmp, &image(b"OLD PROGRAM 1.2.23", &payload(1)), 600);
    f.companion(Some(&payload(0)), false);
    put(&f.exe, &image(b"NEW PROGRAM 1.2.24", &payload(1)), 7200);
    assert_eq!(f.settle(), PendingDecision::Stale);
    assert!(!f.tmp.exists());

    // Newer notes in the .tmp: they are kept, behind the NEW program.
    let f = fx("a5_new_notes");
    put(&f.tmp, &image(b"OLD PROGRAM 1.2.23", &payload(2)), 600);
    f.companion(Some(&payload(1)), false);
    put(&f.exe, &image(b"NEW PROGRAM 1.2.24", &payload(1)), 7200);
    assert_eq!(f.settle(), PendingDecision::Promote);
    let (_s, opened) = f.open();
    assert_eq!(opened, Some(payload(2)));
    exit_with_swap(&f);
    let exe = fs::read(&f.exe).unwrap();
    assert_eq!(exe, image(b"NEW PROGRAM 1.2.24", &payload(2)), "new program, newest notes");
    assert!(swap_log(&f).contains("staged again behind the program of the exe"), "{}", swap_log(&f));

    // And a save in the session takes the program of the exe too.
    let f = fx("a5_save");
    put(&f.tmp, &image(b"OLD PROGRAM 1.2.23", &payload(2)), 600);
    f.companion(Some(&payload(1)), false);
    put(&f.exe, &image(b"NEW PROGRAM 1.2.24", &payload(1)), 7200);
    let (mut s, _) = f.open();
    write_data_in(&f.staging, &mut s, &payload(3)).unwrap();
    assert!(fs::read(&f.tmp).unwrap().starts_with(b"NEW PROGRAM 1.2.24"));
    exit_with_swap(&f);
    assert_eq!(fs::read(&f.exe).unwrap(), image(b"NEW PROGRAM 1.2.24", &payload(3)));
}

#[test]
fn staged_update_with_notes_is_promoted_and_kept_by_later_saves() {
    let f = fx("update_notes");
    put(&f.exe, &image(b"OLD PROGRAM", &payload(1)), 60);
    let (mut s, _) = f.open();
    write_staged_update_in(&f.staging, &mut s, b"NEW PROGRAM").unwrap();

    assert_eq!(fs::read(&f.tmp).unwrap(), image(b"NEW PROGRAM", &payload(1)));
    assert_eq!(f.decision(), PendingDecision::Promote, "same notes, but a new program");
    write_data_in(&f.staging, &mut s, &payload(2)).unwrap();
    assert_eq!(fs::read(&f.tmp).unwrap(), image(b"NEW PROGRAM", &payload(2)));
    exit_with_swap(&f);
    assert_eq!(fs::read(&f.exe).unwrap(), image(b"NEW PROGRAM", &payload(2)));
}

/// Refuter A8: online update of an exe that holds no notes yet.
#[test]
fn staged_update_without_notes_is_promoted_and_kept_by_the_first_save() {
    let f = fx("a8");
    put(&f.exe, b"OLD PROGRAM", 600);
    let (mut s, opened) = f.open();
    assert_eq!(opened, None);
    write_staged_update_in(&f.staging, &mut s, b"NEW PROGRAM").unwrap();
    assert_eq!(fs::read(&f.tmp).unwrap(), b"NEW PROGRAM", "no marker: no notes yet");
    assert_eq!(f.decision(), PendingDecision::Promote);
    assert_eq!(read_data_in(&f.staging, &f.exe), Ok(None));

    write_data_in(&f.staging, &mut s, &payload(1)).unwrap();
    assert_eq!(fs::read(&f.tmp).unwrap(), image(b"NEW PROGRAM", &payload(1)), "the first save keeps it");

    // Exit without that save: promoted as it is.
    let g = fx("a8_no_save");
    put(&g.exe, b"OLD PROGRAM", 600);
    let (mut s, _) = g.open();
    write_staged_update_in(&g.staging, &mut s, b"NEW PROGRAM").unwrap();
    exit_with_swap(&g);
    assert_eq!(fs::read(&g.exe).unwrap(), b"NEW PROGRAM");
}

#[test]
fn staged_update_refuses_a_program_that_already_holds_notes() {
    let f = fx("update_with_marker");
    put(&f.exe, &image(b"OLD PROGRAM", &payload(1)), 60);
    let (mut s, _) = f.open();
    assert!(write_staged_update_in(&f.staging, &mut s, &image(b"NEW", &payload(9))).is_err());
    assert!(!f.tmp.exists());
}

// ── Conflict resolution ──

#[test]
fn newer_exe_without_notes_is_a_conflict_too() {
    // A clean program dropped over the exe while a save was pending.
    let f = fx("conflict_empty_exe");
    put(&f.tmp, &image(b"OLD PROGRAM", &payload(2)), 60);
    f.companion(Some(&payload(1)), false);
    put(&f.exe, b"NEW PROGRAM WITHOUT NOTES", 30);

    match f.decision() {
        PendingDecision::Conflict { exe_has_data, .. } => assert!(!exe_has_data),
        other => panic!("expected a conflict, got {:?}", other),
    }
    assert_eq!(read_data_in(&f.staging, &f.exe), Ok(None));

    // Keeping the pending save puts it behind the NEW program.
    resolve_conflict_keep_tmp_in(&f.staging, &f.exe).unwrap();
    assert_eq!(fs::read(&f.tmp).unwrap(), image(b"NEW PROGRAM WITHOUT NOTES", &payload(2)));
    assert_eq!(f.open().1, Some(payload(2)));
}

#[test]
fn conflict_resolved_by_keeping_the_pending_save() {
    let f = fx("keep_tmp");
    put(&f.tmp, &image(b"OLD PROGRAM", &payload(2)), 60);
    f.companion(Some(&payload(0)), false);
    let exe_bytes = image(b"NEW PROGRAM", &payload(1));
    put(&f.exe, &exe_bytes, 30);

    let kept_aside = resolve_conflict_keep_tmp_in(&f.staging, &f.exe).unwrap();

    // The discarded version (the whole exe) is kept in conflicts\.
    let conflicts = conflicts_dir_in(&f.staging);
    assert_eq!(kept_aside.parent().unwrap(), conflicts);
    assert_eq!(fs::read(&kept_aside).unwrap(), exe_bytes);
    assert_eq!(names_in(&conflicts).len(), 1);
    let name = kept_aside.file_name().unwrap().to_string_lossy().to_string();
    let stem = f.tmp.file_stem().unwrap().to_string_lossy().to_string();
    assert!(name.starts_with(&format!("{}-", stem)), "name: {}", name);
    assert!(name.ends_with("-exe.exe"), "name: {}", name);
    // It reads as a LockNote file (the "import from another exe" path).
    assert_eq!(read_data_in(&f.staging, &kept_aside), Ok(Some(payload(1))));

    // The .tmp is staged again: program of the exe, notes of the .tmp, based
    // on what the exe holds now.
    assert_eq!(fs::read(&f.tmp).unwrap(), image(b"NEW PROGRAM", &payload(2)));
    assert_eq!(f.read_companion().unwrap().unwrap().base, Some(sha(&payload(1))));
    assert_eq!(fs::read(&f.exe).unwrap(), exe_bytes, "the exe is not touched");
    assert_eq!(f.decision(), PendingDecision::Promote);
    assert_eq!(f.open().1, Some(payload(2)));
    assert_eq!(run_exit(&f, Ok(())), vec![(f.tmp.clone(), f.exe.clone())]);
    let log = swap_log(&f);
    assert!(log.contains("conflict resolved, pending save kept") && log.contains(&kept_aside.display().to_string()), "{}", log);
}

#[test]
fn keeping_the_pending_save_works_with_an_exe_dated_in_the_future() {
    // Pass 1 refused this (the re-staged .tmp would still be "older"): the
    // rule by content does not look at dates.
    let f = fx("future_exe");
    put(&f.tmp, &image(b"PROGRAM", &payload(2)), 60);
    f.companion(Some(&payload(0)), false);
    put_future(&f.exe, &image(b"PROGRAM", &payload(1)), 3600);

    resolve_conflict_keep_tmp_in(&f.staging, &f.exe).unwrap();
    assert_eq!(f.decision(), PendingDecision::Promote);
}

#[test]
fn conflict_resolved_by_keeping_the_exe() {
    let f = fx("keep_exe");
    let tmp_bytes = image(b"OLD PROGRAM", &payload(2));
    put(&f.tmp, &tmp_bytes, 60);
    f.companion(Some(&payload(0)), false);
    let exe_bytes = image(b"NEW PROGRAM", &payload(1));
    put(&f.exe, &exe_bytes, 30);

    let kept_aside = resolve_conflict_keep_exe_in(&f.staging, &f.exe).unwrap();

    let conflicts = conflicts_dir_in(&f.staging);
    assert_eq!(kept_aside.parent().unwrap(), conflicts);
    assert_eq!(fs::read(&kept_aside).unwrap(), tmp_bytes, "the .tmp is moved, not deleted");
    assert_eq!(names_in(&conflicts).len(), 1);
    let name = kept_aside.file_name().unwrap().to_string_lossy().to_string();
    assert!(name.ends_with("-tmp.exe"), "name: {}", name);
    assert_eq!(read_data_in(&f.staging, &kept_aside), Ok(Some(payload(2))));

    assert!(!f.tmp.exists());
    assert!(!f.companion_path().exists());
    assert_eq!(fs::read(&f.exe).unwrap(), exe_bytes);
    assert_eq!(f.decision(), PendingDecision::None);
    assert_eq!(f.open().1, Some(payload(1)));
    assert!(run_exit(&f, Ok(())).is_empty());
    let log = swap_log(&f);
    assert!(log.contains("conflict resolved, program file kept") && log.contains(&kept_aside.display().to_string()), "{}", log);
}

#[test]
fn conflict_files_never_reuse_a_name() {
    let f = fx("conflict_names");
    let mut kept = Vec::new();
    for round in 0..3u8 {
        put(&f.tmp, &image(b"PROGRAM", &payload(10 + round)), 60);
        f.companion(Some(&payload(0)), false);
        put(&f.exe, &image(b"PROGRAM", &payload(1)), 30);
        kept.push(resolve_conflict_keep_exe_in(&f.staging, &f.exe).unwrap());
    }
    for round in 0..2u8 {
        put(&f.tmp, &image(b"PROGRAM", &payload(20 + round)), 60);
        f.companion(Some(&payload(0)), false);
        put(&f.exe, &image(b"PROGRAM", &payload(1)), 30);
        kept.push(resolve_conflict_keep_tmp_in(&f.staging, &f.exe).unwrap());
    }

    let mut unique = kept.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), 5, "five conflicts, five files: {:?}", kept);
    assert_eq!(names_in(&conflicts_dir_in(&f.staging)).len(), 5);
    for (round, path) in kept.iter().take(3).enumerate() {
        assert_eq!(
            read_data_in(&f.staging, path),
            Ok(Some(payload(10 + round as u8))),
            "an earlier conflict file must not be overwritten"
        );
    }
}

#[test]
fn resolving_is_refused_when_there_is_no_conflict() {
    let f = fx("no_conflict");
    put(&f.exe, &image(b"PROGRAM", &payload(1)), 60);
    let tmp_bytes = image(b"PROGRAM", &payload(2));
    put(&f.tmp, &tmp_bytes, 30);
    f.companion(Some(&payload(1)), false);

    assert!(resolve_conflict_keep_tmp_in(&f.staging, &f.exe).is_err());
    assert!(resolve_conflict_keep_exe_in(&f.staging, &f.exe).is_err());

    assert_eq!(fs::read(&f.tmp).unwrap(), tmp_bytes);
    assert!(names_in(&conflicts_dir_in(&f.staging)).is_empty());
}

/// Refuter A6: `conflicts` cannot be created (a file has that name).
#[test]
fn unusable_conflicts_folder_fails_explicitly_and_changes_nothing() {
    let f = fx("a6");
    let tmp_bytes = image(b"P", &payload(2));
    let exe_bytes = image(b"P", &payload(1));
    put(&f.tmp, &tmp_bytes, 60);
    f.companion(Some(&payload(0)), false);
    put(&f.exe, &exe_bytes, 30);
    fs::write(f.staging.join("conflicts"), b"i am a file").unwrap();

    assert!(resolve_conflict_keep_tmp_in(&f.staging, &f.exe).is_err());
    assert!(resolve_conflict_keep_exe_in(&f.staging, &f.exe).is_err());
    assert_eq!(fs::read(&f.tmp).unwrap(), tmp_bytes);
    assert_eq!(fs::read(&f.exe).unwrap(), exe_bytes);

    fs::remove_file(f.staging.join("conflicts")).unwrap();
    resolve_conflict_keep_tmp_in(&f.staging, &f.exe).unwrap();
    assert!(resolve_conflict_keep_tmp_in(&f.staging, &f.exe).is_err(), "resolved once only");
    assert!(resolve_conflict_keep_exe_in(&f.staging, &f.exe).is_err());
    assert_eq!(read_data_in(&f.staging, &f.exe), Ok(Some(payload(2))));
}

/// Refuter A7: a save while the .tmp is held without sharing fails, and the
/// .tmp keeps the previous save.
#[test]
fn save_while_the_tmp_is_held_fails_and_keeps_the_previous_save() {
    use std::os::windows::fs::OpenOptionsExt;
    let f = fx("a7");
    put(&f.exe, &image(b"PROGRAM", &payload(1)), 60);
    let (mut s, _) = f.open();
    write_data_in(&f.staging, &mut s, &payload(2)).unwrap();
    let before = fs::read(&f.tmp).unwrap();
    {
        let _held = fs::OpenOptions::new().read(true).share_mode(0).open(&f.tmp).unwrap();
        assert!(write_data_in(&f.staging, &mut s, &payload(3)).is_err());
    }
    assert_eq!(fs::read(&f.tmp).unwrap(), before);
    assert_eq!(s.current(), Some(&payload(2)[..]), "the session still holds its last successful save");
}

// ── The manual update procedure for the real case ──

/// The exe on a network share is a program <= 1.2.23 whose helper cannot
/// reach it; its .tmp waits. The runbook replaces the exe by the clean new
/// program, dated before the .tmp, then starts and closes LockNote.
#[test]
fn runbook_procedure_with_the_date_set_before_the_tmp() {
    let f = fx("runbook_dated");
    let (p_exe, p_tmp) = (payload(1), payload(2));
    put(&f.tmp, &image(b"PROGRAM 1.2.23", &p_tmp), 7200);
    let tmp_date = fs::metadata(&f.tmp).unwrap().modified().unwrap();
    put(&f.exe, &image(b"PROGRAM 1.2.23", &p_exe), 30000);
    let backup = fs::read(&f.exe).unwrap();

    // Steps 3-4: the clean new program replaces the exe, dated one minute
    // before the .tmp.
    fs::write(&f.exe, b"CLEAN PROGRAM 1.2.24").unwrap();
    set_mtime(&f.exe, tmp_date - Duration::from_secs(60));

    // Step 5: start (no question), the notes are those of the .tmp; close.
    assert_eq!(f.settle(), PendingDecision::Promote);
    let (mut s, opened) = f.open();
    assert_eq!(opened, Some(p_tmp.clone()));
    // With or without a save in the session:
    write_data_in(&f.staging, &mut s, &payload(3)).unwrap();
    exit_with_swap(&f);

    assert_eq!(fs::read(&f.exe).unwrap(), image(b"CLEAN PROGRAM 1.2.24", &payload(3)));
    assert!(!f.tmp.exists() && !f.companion_path().exists());
    // Step 6: the next start finds nothing pending.
    assert_eq!(f.settle(), PendingDecision::None);
    assert!(contains(&backup, &p_exe));

    // Without a save in the session: the .tmp is staged behind the new program.
    let g = fx("runbook_dated_nosave");
    put(&g.tmp, &image(b"PROGRAM 1.2.23", &p_tmp), 7200);
    let tmp_date = fs::metadata(&g.tmp).unwrap().modified().unwrap();
    fs::write(&g.exe, b"CLEAN PROGRAM 1.2.24").unwrap();
    set_mtime(&g.exe, tmp_date - Duration::from_secs(60));
    assert_eq!(g.settle(), PendingDecision::Promote);
    let _ = g.open();
    exit_with_swap(&g);
    assert_eq!(fs::read(&g.exe).unwrap(), image(b"CLEAN PROGRAM 1.2.24", &p_tmp));
}

/// The same procedure when the date is not set (the copy dates the exe
/// now): the question is asked, "Yes" keeps the pending save.
#[test]
fn runbook_procedure_without_the_date_asks_and_yes_keeps_the_pending_save() {
    for save in [false, true] {
        let f = fx("runbook_question");
        let p_tmp = payload(2);
        put(&f.tmp, &image(b"PROGRAM 1.2.23", &p_tmp), 7200);
        fs::write(&f.exe, b"CLEAN PROGRAM 1.2.24").unwrap();

        match f.settle() {
            PendingDecision::Conflict { cause: ConflictCause::LegacyExeNewer, exe_has_data: false, .. } => {}
            other => panic!("expected the question, got {:?}", other),
        }
        let aside = resolve_conflict_keep_tmp_in(&f.staging, &f.exe).unwrap();
        assert_eq!(fs::read(&aside).unwrap(), b"CLEAN PROGRAM 1.2.24");
        assert_eq!(f.settle(), PendingDecision::Promote);
        let (mut s, opened) = f.open();
        assert_eq!(opened, Some(p_tmp.clone()));
        let last = if save {
            write_data_in(&f.staging, &mut s, &payload(3)).unwrap();
            payload(3)
        } else {
            p_tmp.clone()
        };
        exit_with_swap(&f);
        assert_eq!(fs::read(&f.exe).unwrap(), image(b"CLEAN PROGRAM 1.2.24", &last));
        // The lines the runbook (case 2, step 5) says to expect after "Yes".
        let log = swap_log(&f);
        assert!(log.contains("pending .tmp in conflict with the program file"), "{}", log);
        assert!(log.contains("conflict resolved, pending save kept"), "{}", log);
        assert!(!log.contains("taken up") && !log.contains("staged again"), "save={}: {}", save, log);
    }
}

// ── The decision itself (pure) ──

#[test]
fn decision_table() {
    let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    let t1 = t0 + Duration::from_secs(1);
    let img = |program: &[u8], notes: Option<u8>| Image { program: sha(program), payload: notes.map(payload) };
    let staged = |program: &[u8], notes: Option<u8>, when: SystemTime| -> Result<StagedFacts, IgnoreReason> {
        Ok(StagedFacts { image: img(program, notes), modified: when })
    };
    let exe = |program: &'static [u8], notes: Option<u8>, when: SystemTime| {
        move || -> Result<ExeFacts, String> { Ok(ExeFacts { image: img(program, notes), modified: when }) }
    };
    let never = || -> Result<ExeFacts, String> { panic!("the exe must not be read in this case") };
    let comp = |base: Option<u8>, updater: bool| -> Result<Companion, CompanionError> {
        Ok(Companion { exe: PathBuf::from("x.exe"), base: base.map(|s| sha(&payload(s))), program_from_updater: updater })
    };

    // No .tmp; an unusable one, whatever the rest.
    assert_eq!(decide_pending(None, None, &never), PendingDecision::None);
    let broken: Result<StagedFacts, IgnoreReason> = Err(IgnoreReason::NoMarker);
    assert_eq!(decide_pending(Some(&broken), Some(&comp(Some(1), false)), &never), PendingDecision::Ignored(IgnoreReason::NoMarker));
    // One that cannot be read may be the latest save: nothing is decided.
    let unreadable: Result<StagedFacts, IgnoreReason> = Err(IgnoreReason::Unreadable("held".to_string()));
    assert_eq!(decide_pending(Some(&unreadable), Some(&comp(Some(1), false)), &never), PendingDecision::TmpUnreadable("held".to_string()));
    // Nor one whose companion cannot be read (refuter pass 3, A1): not a
    // conflict, and the exe is not even read.
    let held: Result<Companion, CompanionError> = Err(CompanionError::Unreadable("companion held".to_string()));
    for notes in [Some(2), None] {
        assert_eq!(decide_pending(Some(&staged(b"P", notes, t1)), Some(&held), &never), PendingDecision::TmpUnreadable("companion held".to_string()));
    }
    // No notes: only the updater's program is an image.
    assert_eq!(decide_pending(Some(&staged(b"P", None, t1)), None, &never), PendingDecision::Ignored(IgnoreReason::NoMarker));
    assert_eq!(decide_pending(Some(&staged(b"P", None, t1)), Some(&comp(Some(1), false)), &never), PendingDecision::Ignored(IgnoreReason::NoMarker));

    // The exe cannot be read: nothing is decided.
    assert_eq!(
        decide_pending(Some(&staged(b"P", Some(2), t0)), Some(&comp(Some(1), false)), &|| Err("access denied".to_string())),
        PendingDecision::ExeUnreadable("access denied".to_string())
    );

    // Same notes: stale (not when the updater's new program waits).
    assert_eq!(decide_pending(Some(&staged(b"OLD", Some(1), t1)), Some(&comp(Some(0), false)), &exe(b"NEW", Some(1), t0)), PendingDecision::Stale);
    assert_eq!(decide_pending(Some(&staged(b"NEW", Some(1), t1)), Some(&comp(Some(1), true)), &exe(b"OLD", Some(1), t0)), PendingDecision::Promote);
    assert_eq!(decide_pending(Some(&staged(b"SAME", Some(1), t1)), Some(&comp(Some(1), true)), &exe(b"SAME", Some(1), t0)), PendingDecision::Stale);
    assert_eq!(decide_pending(Some(&staged(b"NEW", None, t1)), Some(&comp(None, true)), &exe(b"OLD", None, t0)), PendingDecision::Promote);

    // With a companion: content only, both date orders.
    for (tt, te) in [(t0, t1), (t1, t0), (t0, t0)] {
        assert_eq!(decide_pending(Some(&staged(b"P", Some(2), tt)), Some(&comp(Some(1), false)), &exe(b"P", Some(1), te)), PendingDecision::Promote);
        assert_eq!(
            decide_pending(Some(&staged(b"P", Some(2), tt)), Some(&comp(Some(1), false)), &exe(b"P", Some(3), te)),
            PendingDecision::Conflict { tmp_modified: tt, exe_modified: te, exe_has_data: true, cause: ConflictCause::ExeChanged }
        );
        assert_eq!(
            decide_pending(Some(&staged(b"P", Some(2), tt)), Some(&comp(Some(1), false)), &exe(b"P", None, te)),
            PendingDecision::Conflict { tmp_modified: tt, exe_modified: te, exe_has_data: false, cause: ConflictCause::ExeChanged }
        );
        assert_eq!(decide_pending(Some(&staged(b"P", Some(2), tt)), Some(&comp(None, false)), &exe(b"P", None, te)), PendingDecision::Promote);
        let bad: Result<Companion, CompanionError> = Err(CompanionError::Invalid("garbage".to_string()));
        assert_eq!(
            decide_pending(Some(&staged(b"P", Some(2), tt)), Some(&bad), &exe(b"P", Some(1), te)),
            PendingDecision::Conflict { tmp_modified: tt, exe_modified: te, exe_has_data: true, cause: ConflictCause::CompanionInvalid("garbage".to_string()) }
        );
    }

    // Without a companion (earlier version): dates, strictly.
    assert_eq!(decide_pending(Some(&staged(b"P", Some(2), t1)), None, &exe(b"P", Some(1), t0)), PendingDecision::Promote);
    assert_eq!(decide_pending(Some(&staged(b"P", Some(2), t0)), None, &exe(b"P", Some(1), t0)), PendingDecision::Promote);
    assert_eq!(
        decide_pending(Some(&staged(b"P", Some(2), t0)), None, &exe(b"P", Some(1), t1)),
        PendingDecision::Conflict { tmp_modified: t0, exe_modified: t1, exe_has_data: true, cause: ConflictCause::LegacyExeNewer }
    );
}

/// Refuter A3, extended with the companion: (.tmp state) x (exe state) x
/// (date order) x (companion). After startup, opening and exit (helper
/// simulated), every valid payload that existed must still exist somewhere.
/// The one accepted overwrite is the exe's notes replaced by the pending
/// save that was based on them (the normal end of a session), or, without
/// companion, by the legacy rule (.tmp at least as recent), as before issue
/// 055. A pending save based on other notes never replaces the exe.
///
/// Pass 3: every combination is also played with a failure injected at each
/// step that changes the pair during that first life (`with_fault`), then a
/// later life without failure ("Yes" to a question); the same rule holds.
#[test]
fn brute_force_no_payload_is_lost() {
    let pa = payload(1);
    let pb = payload(2);
    let short: Vec<u8> = vec![7u8; 79];
    let tmp_states: Vec<(&str, Option<Vec<u8>>)> = vec![
        ("absent", None),
        ("nomarker", Some(b"JUNK NO MARKER".to_vec())),
        ("short", Some(image(b"TP", &short))),
        ("validA", Some(image(b"TP", &pa))),
        ("empty", Some(Vec::new())),
    ];
    let exe_states: Vec<(&str, Option<Vec<u8>>)> = vec![
        ("absent", None),
        ("clean", Some(b"EP CLEAN".to_vec())),
        ("short", Some(image(b"EP", &short))),
        ("validA", Some(image(b"EP", &pa))),
        ("validB", Some(image(b"EP", &pb))),
    ];
    // Companion: none, based on what the exe holds, based on something else.
    let companions = ["none", "base=exe", "base=other"];
    let base = SystemTime::now() - Duration::from_secs(1000);
    let mut losses = Vec::new();
    let mut runs = 0;
    for (tn, tb) in &tmp_states {
        for (en, eb) in &exe_states {
            for rel in ["tmp<exe", "tmp=exe", "tmp>exe"] {
                for comp in companions {
                    // k = 0: no failure; k > 0: the k-th step that changes the
                    // pair during the first life fails (issue 055, pass 3), and
                    // a later life without failure follows.
                    let mut k = 0u32;
                    loop {
                        let f = fx("a3");
                        if let Some(b) = tb {
                            fs::write(&f.tmp, b).unwrap();
                            set_mtime(&f.tmp, base);
                        }
                        if let Some(b) = eb {
                            fs::write(&f.exe, b).unwrap();
                            set_mtime(&f.exe, match rel {
                                "tmp<exe" => base + Duration::from_secs(10),
                                "tmp=exe" => base,
                                _ => base - Duration::from_secs(10),
                            });
                        }
                        let exe_notes = eb.as_deref().map(exe_image).and_then(|i| i.payload);
                        match comp {
                            "base=exe" => f.companion(exe_notes.as_deref(), false),
                            "base=other" => f.companion(Some(&payload(9)), false),
                            _ => {}
                        }
                        let had_a = tb.as_ref().map_or(false, |b| contains(b, &pa))
                            || eb.as_ref().map_or(false, |b| contains(b, &pa));
                        let had_b = eb.as_ref().map_or(false, |b| contains(b, &pb));
                        let (seen, d) = with_fault((k > 0).then_some(k), || {
                            let d = settle_pending_at_startup_in(&f.staging, &f.exe);
                            if let Ok((mut s, _)) = open_session_in(&f.staging, &f.exe) {
                                // The session saves what it opened (re-encrypted in
                                // reality; the same bytes here).
                                if let Some(p) = s.current().map(|p| p.to_vec()) {
                                    let _ = write_data_in(&f.staging, &mut s, &p);
                                }
                            }
                            exit_with_swap(&f);
                            d
                        });
                        if k > 0 && seen < k {
                            break;
                        }
                        if k > 0 {
                            if let Ok(PendingDecision::Conflict { .. }) = settle_pending_at_startup_in(&f.staging, &f.exe) {
                                let _ = resolve_conflict_keep_tmp_in(&f.staging, &f.exe);
                            }
                            let _ = open_session_in(&f.staging, &f.exe);
                            exit_with_swap(&f);
                        }
                        runs += 1;
                        let lost_a = had_a && !anywhere(&f.root, &pa);
                        let lost_b = had_b && !anywhere(&f.root, &pb);
                        // The exe's notes B replaced by the pending save A is the
                        // normal end of a session that started from B; without
                        // companion, it is the legacy rule.
                        let replaced_by_its_save = *tn == "validA"
                            && *en == "validB"
                            && (comp == "base=exe" || (comp == "none" && rel != "tmp<exe"));
                        if (lost_a || lost_b) && !replaced_by_its_save {
                            losses.push(format!("tmp={} exe={} {} companion={} failed step={} decision={:?}", tn, en, rel, comp, k, d));
                        }
                        k += 1;
                    }
                }
            }
        }
    }
    println!("brute force: {} runs", runs);
    assert!(losses.is_empty(), "payloads lost: {:#?}", losses);
}

// ── Pass 3 ──

/// The program part of a LockNote image (everything before the marker).
fn program_of(data: &[u8]) -> Vec<u8> {
    match find_marker(data) {
        Some(p) => data[..p].to_vec(),
        None => data.to_vec(),
    }
}

/// Refuter pass 2, N1: the updater's write of the .tmp fails after its
/// companion was switched to program=updater. The pending .tmp holds an
/// older program than the exe (a .tmp of 1.2.23 taken up behind a newer
/// exe). The exit must never bring that older program back.
#[test]
fn n1_failed_update_write_leaves_program_updater_on_an_old_program() {
    use std::os::windows::fs::OpenOptionsExt;
    let f = fx("n1");
    put(&f.exe, &image(b"NEW PROGRAM 1.2.24", &payload(1)), 7200);
    put(&f.tmp, &image(b"OLD PROGRAM 1.2.23", &payload(2)), 600);
    assert_eq!(f.settle(), PendingDecision::Promote);
    let (mut s, _) = f.open();
    let staged = {
        // Held without delete sharing (an antivirus): the rename fails.
        let _held = fs::OpenOptions::new().read(true).share_mode(1).open(&f.tmp).unwrap();
        write_staged_update_in(&f.staging, &mut s, b"UPDATER PROGRAM 1.2.25")
    };
    assert!(staged.is_err(), "the failed staging is reported");
    let c = f.read_companion().unwrap().unwrap();
    assert!(!c.program_from_updater, "the companion must not claim the updater's program over the old .tmp");
    write_data_in(&f.staging, &mut s, &payload(3)).unwrap();
    exit_with_swap(&f);
    assert_eq!(fs::read(&f.exe).unwrap(), image(b"NEW PROGRAM 1.2.24", &payload(3)));
}

/// Refuter pass 2, N2: a stale .tmp that cannot be removed at startup must
/// keep its companion, or it becomes a .tmp "left by an earlier version",
/// judged by dates, and can later replace newer notes without a question.
#[test]
fn n2_stale_tmp_that_cannot_be_removed_loses_its_companion() {
    use std::os::windows::fs::OpenOptionsExt;
    let f = fx("n2");
    let p1 = payload(1);
    let t = seconds_ago(300);
    fs::write(&f.exe, image(b"PROGRAM", &p1)).unwrap();
    fs::write(&f.tmp, image(b"PROGRAM", &p1)).unwrap();
    set_mtime(&f.tmp, t);
    set_mtime(&f.exe, t);
    f.companion(Some(&payload(0)), false);
    {
        let _held = fs::OpenOptions::new().read(true).share_mode(1).open(&f.tmp).unwrap();
        assert_eq!(f.settle(), PendingDecision::Stale);
    }
    assert!(f.tmp.exists(), "held: not removed");
    assert!(f.companion_path().exists(), "the companion stays while its .tmp is there");
    // Another writer puts P2 into the exe, dated before the .tmp.
    let p2 = payload(2);
    fs::write(&f.exe, image(b"PROGRAM", &p2)).unwrap();
    set_mtime(&f.exe, t - Duration::from_secs(10));
    let d = f.settle();
    assert!(matches!(d, PendingDecision::Conflict { cause: ConflictCause::ExeChanged, .. }), "{:?}", d);
    exit_with_swap(&f);
    assert!(anywhere(&f.root, &p2), "P2 must survive");
}

/// Reviewer pass 2, 3: "Yes" whose re-staging of the .tmp fails is reported
/// as not applied: the state must still be the conflict, never a promotion
/// at exit.
#[test]
fn r3_failed_restaging_leaves_the_conflict_unresolved() {
    use std::os::windows::fs::OpenOptionsExt;
    let f = fx("r3");
    put(&f.tmp, &image(b"OLD PROGRAM", &payload(2)), 60);
    f.companion(Some(&payload(0)), false);
    let exe_bytes = image(b"NEW PROGRAM", &payload(1));
    put(&f.exe, &exe_bytes, 30);
    let resolved = {
        let _held = fs::OpenOptions::new().read(true).share_mode(1).open(&f.tmp).unwrap();
        resolve_conflict_keep_tmp_in(&f.staging, &f.exe)
    };
    assert!(resolved.is_err());
    assert!(matches!(f.decision(), PendingDecision::Conflict { .. }), "{:?}", f.decision());
    assert!(run_exit(&f, Ok(())).is_empty(), "a choice reported as not applied is not applied at exit");
    assert_eq!(fs::read(&f.exe).unwrap(), exe_bytes);
}

/// Reviewer pass 2, 1: a .tmp that cannot be read, or is not a regular
/// file, stops the startup with a message naming it; it is never set aside
/// by a save, nor promoted.
#[test]
fn unreadable_tmp_stops_the_startup_like_an_unreadable_exe() {
    use std::os::windows::fs::OpenOptionsExt;
    // A directory where the .tmp should be.
    let f = fx("tmp_dir3");
    put(&f.exe, &image(b"PROGRAM", &payload(1)), 60);
    fs::create_dir_all(&f.tmp).unwrap();
    match f.settle() {
        PendingDecision::TmpUnreadable(detail) => {
            assert!(detail.contains(&f.tmp.display().to_string()) && detail.contains("not a regular file"), "{}", detail)
        }
        other => panic!("expected an unreadable .tmp, got {:?}", other),
    }
    assert!(swap_log(&f).contains("it cannot be read"), "{}", swap_log(&f));
    let err = open_session_in(&f.staging, &f.exe).expect_err("no session");
    assert!(err.contains(&f.tmp.display().to_string()), "{}", err);
    assert!(run_exit(&f, Ok(())).is_empty());
    assert!(f.tmp.is_dir(), "left as it is");

    // A .tmp held open without sharing.
    let f = fx("tmp_locked3");
    put(&f.exe, &image(b"PROGRAM", &payload(1)), 60);
    put(&f.tmp, &image(b"PROGRAM", &payload(2)), 30);
    f.companion(Some(&payload(1)), false);
    {
        let _held = fs::OpenOptions::new().read(true).share_mode(0).open(&f.tmp).unwrap();
        assert!(matches!(f.settle(), PendingDecision::TmpUnreadable(_)));
        assert!(open_session_in(&f.staging, &f.exe).is_err());
        assert!(run_exit(&f, Ok(())).is_empty());
    }
    assert_eq!(fs::read(&f.tmp).unwrap(), image(b"PROGRAM", &payload(2)), "nothing was changed");
    assert!(names_in(&conflicts_dir_in(&f.staging)).is_empty(), "nothing set aside");
    assert_eq!(f.decision(), PendingDecision::Promote, "readable again: taken up as usual");
}

/// Reviewer pass 2, 5: the message of an unreadable exe names it.
#[test]
fn unreadable_exe_is_named_in_the_message() {
    let f = fx("r5");
    put(&f.tmp, &image(b"PROGRAM", &payload(2)), 30);
    f.companion(Some(&payload(1)), false);
    match f.settle() {
        PendingDecision::ExeUnreadable(detail) => {
            assert!(detail.contains(&f.exe.display().to_string()), "{}", detail)
        }
        other => panic!("expected an unreadable exe, got {:?}", other),
    }
}

// ── The order of the pair (.tmp, companion) ──

/// Run `action` with the n-th step that changes the pair failing; returns
/// the number of steps seen and the result.
fn with_fault<T>(n: Option<u32>, action: impl FnOnce() -> T) -> (u32, T) {
    PAIR_FAULT.with(|c| c.set((0, n)));
    let out = action();
    let seen = PAIR_FAULT.with(|c| c.get()).0;
    PAIR_FAULT.with(|c| c.set((0, None)));
    (seen, out)
}

/// One case of the proof: a starting state, an action on the pair, and what
/// must survive.
struct PairCase {
    name: &'static str,
    /// Builds the state; returns the session when the action needs one.
    setup: fn(&Fx) -> Option<Session>,
    /// The action; Ok(true) when it reports success.
    action: fn(&Fx, Option<&mut Session>) -> bool,
    /// Payloads that must be somewhere at the end, whatever happened.
    keep: Vec<Vec<u8>>,
    /// Must survive only when the action reported success / a failure.
    keep_if_ok: Vec<Vec<u8>>,
    keep_if_err: Vec<Vec<u8>>,
}

const OLD: &[u8] = b"OLD PROGRAM";
const EXE: &[u8] = b"EXE PROGRAM";
const UPD: &[u8] = b"UPDATER PROGRAM";

fn pair_cases() -> Vec<PairCase> {
    fn open(f: &Fx) -> Option<Session> {
        assert!(!matches!(f.settle(), PendingDecision::Conflict { .. }));
        Some(f.open().0)
    }
    fn conflict_tmp(f: &Fx, program: &[u8], comp: &str) {
        put(&f.tmp, &image(program, &payload(2)), 7200);
        match comp {
            "base" => f.companion(Some(&payload(0)), false),
            "updater" => f.companion(Some(&payload(0)), true),
            "invalid" => fs::write(f.companion_path(), b"garbage").unwrap(),
            _ => {}
        }
        put(&f.exe, &image(EXE, &payload(1)), 60);
        assert!(matches!(f.decision(), PendingDecision::Conflict { .. }), "setup: {:?}", f.decision());
    }
    vec![
        PairCase {
            name: "first save",
            setup: |f| { put(&f.exe, &image(EXE, &payload(0)), 60); open(f) },
            action: |f, s| write_data_in(&f.staging, s.unwrap(), &payload(1)).is_ok(),
            keep: vec![], keep_if_ok: vec![payload(1)], keep_if_err: vec![payload(0)],
        },
        PairCase {
            name: "second save",
            setup: |f| {
                put(&f.exe, &image(EXE, &payload(0)), 60);
                let mut s = open(f).unwrap();
                write_data_in(&f.staging, &mut s, &payload(1)).unwrap();
                Some(s)
            },
            action: |f, s| write_data_in(&f.staging, s.unwrap(), &payload(2)).is_ok(),
            keep: vec![], keep_if_ok: vec![payload(2)], keep_if_err: vec![payload(1)],
        },
        PairCase {
            name: "first save over an ignored .tmp",
            setup: |f| { put(&f.exe, &image(EXE, &payload(0)), 60); put(&f.tmp, b"JUNK", 30); open(f) },
            action: |f, s| write_data_in(&f.staging, s.unwrap(), &payload(1)).is_ok(),
            keep: vec![], keep_if_ok: vec![payload(1)], keep_if_err: vec![payload(0)],
        },
        PairCase {
            name: "update over a .tmp of an older program (N1)",
            setup: |f| { put(&f.exe, &image(EXE, &payload(0)), 7200); put(&f.tmp, &image(OLD, &payload(2)), 600); open(f) },
            action: |f, s| write_staged_update_in(&f.staging, s.unwrap(), UPD).is_ok(),
            keep: vec![payload(2)], keep_if_ok: vec![], keep_if_err: vec![],
        },
        PairCase {
            name: "update without a .tmp",
            setup: |f| { put(&f.exe, &image(EXE, &payload(0)), 60); open(f) },
            action: |f, s| write_staged_update_in(&f.staging, s.unwrap(), UPD).is_ok(),
            keep: vec![payload(0)], keep_if_ok: vec![], keep_if_err: vec![],
        },
        PairCase {
            name: "update after a save",
            setup: |f| {
                put(&f.exe, &image(EXE, &payload(0)), 60);
                let mut s = open(f).unwrap();
                write_data_in(&f.staging, &mut s, &payload(1)).unwrap();
                Some(s)
            },
            action: |f, s| write_staged_update_in(&f.staging, s.unwrap(), UPD).is_ok(),
            keep: vec![payload(1)], keep_if_ok: vec![], keep_if_err: vec![],
        },
        PairCase {
            name: "keep the pending save",
            setup: |f| { conflict_tmp(f, OLD, "base"); None },
            action: |f, _| resolve_conflict_keep_tmp_in(&f.staging, &f.exe).is_ok(),
            keep: vec![payload(1), payload(2)], keep_if_ok: vec![], keep_if_err: vec![],
        },
        PairCase {
            name: "keep the pending save, no companion",
            setup: |f| { conflict_tmp(f, OLD, "none"); None },
            action: |f, _| resolve_conflict_keep_tmp_in(&f.staging, &f.exe).is_ok(),
            keep: vec![payload(1), payload(2)], keep_if_ok: vec![], keep_if_err: vec![],
        },
        PairCase {
            name: "keep the pending save, invalid companion",
            setup: |f| { conflict_tmp(f, OLD, "invalid"); None },
            action: |f, _| resolve_conflict_keep_tmp_in(&f.staging, &f.exe).is_ok(),
            keep: vec![payload(1), payload(2)], keep_if_ok: vec![], keep_if_err: vec![],
        },
        PairCase {
            name: "keep the pending update",
            setup: |f| { conflict_tmp(f, UPD, "updater"); None },
            action: |f, _| resolve_conflict_keep_tmp_in(&f.staging, &f.exe).is_ok(),
            keep: vec![payload(1), payload(2)], keep_if_ok: vec![], keep_if_err: vec![],
        },
        PairCase {
            name: "keep the program file",
            setup: |f| { conflict_tmp(f, OLD, "base"); None },
            action: |f, _| resolve_conflict_keep_exe_in(&f.staging, &f.exe).is_ok(),
            keep: vec![payload(1), payload(2)], keep_if_ok: vec![], keep_if_err: vec![],
        },
        PairCase {
            name: "stale .tmp at startup",
            setup: |f| {
                put(&f.tmp, &image(OLD, &payload(1)), 300);
                f.companion(Some(&payload(0)), false);
                put(&f.exe, &image(EXE, &payload(1)), 300);
                None
            },
            action: |f, _| settle_pending_at_startup_in(&f.staging, &f.exe).is_ok(),
            keep: vec![payload(1)], keep_if_ok: vec![], keep_if_err: vec![],
        },
        PairCase {
            name: "earlier version's .tmp taken up",
            setup: |f| { put(&f.exe, &image(EXE, &payload(1)), 7200); put(&f.tmp, &image(EXE, &payload(2)), 600); None },
            action: |f, _| settle_pending_at_startup_in(&f.staging, &f.exe).is_ok(),
            keep: vec![payload(2)], keep_if_ok: vec![], keep_if_err: vec![],
        },
        PairCase {
            name: "companion without .tmp",
            setup: |f| { put(&f.exe, &image(EXE, &payload(1)), 60); f.companion(Some(&payload(1)), false); None },
            action: |f, _| settle_pending_at_startup_in(&f.staging, &f.exe).is_ok(),
            keep: vec![payload(1)], keep_if_ok: vec![], keep_if_err: vec![],
        },
        PairCase {
            name: "exit, staged again behind the program of the exe",
            setup: |f| {
                put(&f.tmp, &image(OLD, &payload(2)), 60);
                f.companion(Some(&payload(1)), false);
                put(&f.exe, &image(EXE, &payload(1)), 60);
                None
            },
            action: |f, _| { run_exit(f, Ok(())); true },
            keep: vec![payload(2)], keep_if_ok: vec![], keep_if_err: vec![],
        },
    ]
}

/// The invariant of the order of the pair, proved by injecting a failure at
/// every step of every function that writes, moves or removes the .tmp or
/// its companion (an error return there leaves the files as a crash at that
/// step would, but for the removal of an unfinished part or conflict file).
/// After each failure:
/// - the companion never says program=updater over a .tmp whose program is
///   not the updater's;
/// - a companion is never removed while its .tmp is still there;
/// - a conflict whose resolution reports a failure is still a conflict;
/// - then a next start ("Yes" to a question), a session and two exits with a
///   successful helper: every payload that must survive is somewhere, and
///   the exe never ends with the older program.
#[test]
fn every_failure_of_a_pair_step_leaves_a_state_judged_without_loss_or_downgrade() {
    let mut failures = Vec::new();
    let mut proved = 0;
    for case in pair_cases() {
        let mut k = 1;
        loop {
            let f = fx("pair");
            let mut s = (case.setup)(&f);
            let had_companion = f.companion_path().exists();
            let (seen, ok) = with_fault(Some(k), || (case.action)(&f, s.as_mut()));
            if seen < k {
                break; // every step of this action has been made to fail
            }
            proved += 1;
            let tag = format!("{} / step {} failed (ok={})", case.name, k, ok);
            // Direct checks of the pair.
            if let Some(Ok(c)) = f.read_companion() {
                if c.program_from_updater && f.tmp.is_file() && program_of(&fs::read(&f.tmp).unwrap()) != UPD {
                    failures.push(format!("{}: program=updater over another program", tag));
                }
            }
            if had_companion && f.tmp.exists() && !f.companion_path().exists() {
                failures.push(format!("{}: companion removed while its .tmp is there", tag));
            }
            if case.name.starts_with("keep the") && !ok && !matches!(f.decision(), PendingDecision::Conflict { .. }) {
                failures.push(format!("{}: reported as not applied, but state {:?}", tag, f.decision()));
            }
            // The rest of the life of these files.
            drop(s.take());
            for _ in 0..2 {
                if let Ok(PendingDecision::Conflict { .. }) = settle_pending_at_startup_in(&f.staging, &f.exe) {
                    let _ = resolve_conflict_keep_tmp_in(&f.staging, &f.exe);
                }
                let _ = open_session_in(&f.staging, &f.exe);
                exit_with_swap(&f);
            }
            let must: Vec<&Vec<u8>> = case.keep.iter()
                .chain(if ok { case.keep_if_ok.iter() } else { case.keep_if_err.iter() })
                .collect();
            for p in must {
                if !anywhere(&f.root, p) {
                    failures.push(format!("{}: payload {} lost", tag, p[0]));
                }
            }
            match fs::read(&f.exe) {
                Ok(exe) if program_of(&exe) == OLD => failures.push(format!("{}: older program in the exe", tag)),
                Ok(_) => {}
                Err(e) => failures.push(format!("{}: exe unreadable {}", tag, e)),
            }
            k += 1;
        }
    }
    println!("pair steps made to fail: {}", proved);
    assert!(proved >= 20, "steps proved: {}", proved);
    assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
}

/// Reviewer pass 2, 6: at exit, a helper that ends before it is queued on the
/// swap lock is not waited for the whole delay.
///
/// Issue #36: the life of the helper process is not timed. Under the load of
/// a full run, `cmd /c exit 3` was seen living 6 s (15 ms of CPU) and more
/// than 10 s from its creation to its exit, while its end was seen at once:
/// the helpers here end when the test says so, never on their own pace.
#[test]
fn a_helper_that_ends_before_it_is_ready_is_not_waited_for() {
    use std::os::windows::io::AsRawHandle;
    use std::os::windows::process::CommandExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::Threading::{CreateEventW, SetEvent, TerminateProcess, CREATE_SUSPENDED};
    const ENDED: &str = "helper ended (exit code 3) before it was queued on the swap lock";
    let ready = unsafe { CreateEventW(None, true, false, PCWSTR::null()) }.unwrap();

    // Ended before the wait starts: the wait returns at once.
    let mut child = std::process::Command::new("cmd").args(["/c", "exit 3"]).spawn().unwrap();
    child.wait().unwrap(); // the handle stays open until `child` is dropped
    let helper = HANDLE(child.as_raw_handle());
    let started = std::time::Instant::now();
    let waited = wait_helper_ready(ready, helper, Duration::from_secs(10));
    let elapsed = started.elapsed();
    assert_eq!(waited, Err(ENDED.to_string()));
    assert!(elapsed < Duration::from_secs(5), "waited {:?}", elapsed);

    // Ends during the wait: a helper held suspended, ended by the test with
    // code 3. The timeout message would name the delay, not the exit code.
    let mut child = std::process::Command::new("cmd")
        .args(["/c", "exit 0"])
        .creation_flags(CREATE_SUSPENDED.0)
        .spawn()
        .unwrap();
    let raw = child.as_raw_handle() as usize;
    let ender = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        unsafe { TerminateProcess(HANDLE(raw as *mut _), 3) }
    });
    let waited = wait_helper_ready(ready, HANDLE(child.as_raw_handle()), Duration::from_secs(60));
    if let Err(e) = ender.join().unwrap() {
        // A suspended helper is not killed by dropping the Child.
        let _ = child.kill();
        panic!("TerminateProcess failed: {e}");
    }
    child.wait().unwrap();
    assert_eq!(waited, Err(ENDED.to_string()));

    // Ready while the helper runs: no error.
    let mut child = std::process::Command::new("cmd").args(["/c", "ping -n 3 127.0.0.1 >nul"]).spawn().unwrap();
    let helper = HANDLE(child.as_raw_handle());
    unsafe { SetEvent(ready) }.unwrap();
    assert_eq!(wait_helper_ready(ready, helper, Duration::from_secs(10)), Ok(()));
    let _ = child.wait();
    unsafe {
        let _ = CloseHandle(ready);
    }
}

/// Issue #39: a helper not queued on the swap lock in time is ended before
/// the application releases that lock, so no helper of a finished session
/// can reach the lock later, behind the session that follows.
///
/// The helper is a process held suspended: it never signals "ready", as a
/// helper stalled at its start under load.
#[test]
fn a_helper_late_for_the_hand_over_is_ended_before_the_lock_is_released() {
    use std::os::windows::io::AsRawHandle;
    use std::os::windows::process::CommandExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::Threading::{CreateEventW, CREATE_SUSPENDED};
    let ready = unsafe { CreateEventW(None, true, false, PCWSTR::null()) }.unwrap();
    let mut child = std::process::Command::new("cmd")
        .args(["/c", "exit 0"])
        .creation_flags(CREATE_SUSPENDED.0)
        .spawn()
        .unwrap();

    let waited = wait_helper_ready(ready, HANDLE(child.as_raw_handle()), Duration::from_millis(300));

    let still_running = matches!(child.try_wait(), Ok(None));
    if still_running {
        // A suspended process is not ended by dropping the Child.
        let _ = child.kill();
    }
    let _ = child.wait();
    unsafe {
        let _ = CloseHandle(ready);
    }
    assert!(!still_running, "the late helper still runs once the wait is over: {:?}", waited);
    assert_eq!(
        waited,
        Err("helper started but not queued on the swap lock within 0 s; \
             it was ended, nothing touched (it held no lock)"
            .to_string())
    );
}
/// Issue #39, challenger: a helper that ends by itself right after the
/// timeout is not reported as "could not be ended (it may still run)"
/// (TerminateProcess refuses an ended process): the log says it had ended.
#[test]
fn a_late_helper_that_ended_by_itself_is_reported_as_ended() {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    let mut child = std::process::Command::new("cmd").args(["/c", "exit 7"]).spawn().unwrap();
    child.wait().unwrap(); // the handle stays open until `child` is dropped
    assert_eq!(
        end_late_helper(HANDLE(child.as_raw_handle())),
        "it had already ended (exit code 7), nothing touched (it held no lock)"
    );
}

/// Issue #39, challenger: `launch_swap_helper` ends a late helper (inside
/// `wait_helper_ready`) before it releases the swap lock, and releases it
/// only there. Released first, a late helper could reach the lock behind
/// the next session. Not reachable by a behaviour test: the function starts
/// the embedded helper and holds the process-wide lock.
#[test]
fn the_swap_lock_is_released_only_after_the_wait_for_the_helper() {
    let src = crate::read_source(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("storage").join("mod.rs"));
    let start = src.find("pub fn launch_swap_helper(tmp_path").expect("the Windows launch_swap_helper");
    let end = start + src[start..].find("fn wait_helper_ready(").expect("wait_helper_ready after it");
    let body = &src[start..end];
    let wait = body.find("wait_helper_ready(ready, helper,").expect("launch_swap_helper must call wait_helper_ready");
    let release = body.find("release_swap_lock()").expect("launch_swap_helper must release the swap lock");
    assert_eq!(body.matches("release_swap_lock()").count(), 1, "the swap lock is released at one place only");
    assert_eq!(body.matches("SWAP_LOCK").count(), 0, "the lock is released through release_swap_lock only");
    assert!(wait < release, "the late helper must be ended before the swap lock is released");
}
// ── Final touch (refuter pass 3: A1, A1b, A2) ──

/// Open `path` without any sharing (an antivirus or a backup tool holding
/// it): reading it fails with error 32 while the handle lives.
fn hold(path: &Path) -> fs::File {
    use std::os::windows::fs::OpenOptionsExt;
    fs::OpenOptions::new().read(true).share_mode(0).open(path).unwrap()
}

/// A session based on P0 saved P1; its swap did not happen.
fn pending_save_of_a_session(tag: &str) -> Fx {
    let f = fx(tag);
    fs::write(&f.exe, image(EXE, &payload(0))).unwrap();
    let (mut s, _) = f.open();
    write_data_in(&f.staging, &mut s, &payload(1)).unwrap();
    f
}

/// Refuter pass 3, A1: a companion that cannot be read (held, or not a
/// regular file) says nothing about the .tmp: it is not an invalid
/// companion, and must not become a conflict whose "No" sets the latest save
/// aside. It stops the startup like an unreadable .tmp, naming the companion.
#[test]
fn a1_companion_held_at_startup_is_not_a_conflict() {
    let f = pending_save_of_a_session("a1");
    let comp = f.companion_path().display().to_string();
    {
        let _held = hold(&f.companion_path());
        match f.settle() {
            PendingDecision::TmpUnreadable(detail) => assert!(detail.contains(&comp), "{}", detail),
            other => panic!("expected the startup to stop, got {:?}", other),
        }
        assert!(swap_log(&f).contains(&comp), "{}", swap_log(&f));
        let err = open_session_in(&f.staging, &f.exe).expect_err("no session");
        assert!(err.contains(&comp), "{}", err);
        assert!(run_exit(&f, Ok(())).is_empty(), "nothing promoted at exit");
    }
    assert_eq!(fs::read(&f.tmp).unwrap(), image(EXE, &payload(1)), "nothing was changed");
    assert!(names_in(&conflicts_dir_in(&f.staging)).is_empty(), "nothing set aside");
    assert_eq!(f.settle(), PendingDecision::Promote, "readable again: taken up as usual");
    assert_eq!(f.open().1, Some(payload(1)));

    // A directory where the companion should be.
    let f = fx("a1_dir");
    put(&f.exe, &image(EXE, &payload(0)), 60);
    put(&f.tmp, &image(EXE, &payload(1)), 30);
    fs::create_dir_all(f.companion_path()).unwrap();
    match f.settle() {
        PendingDecision::TmpUnreadable(detail) => {
            assert!(detail.contains(&f.companion_path().display().to_string()) && detail.contains("not a regular file"), "{}", detail)
        }
        other => panic!("expected the startup to stop, got {:?}", other),
    }
    assert!(run_exit(&f, Ok(())).is_empty());
    assert!(f.companion_path().is_dir() && f.tmp.is_file(), "left as they are");
}

/// Refuter pass 3, A1b: the companion becomes unreadable between the
/// question and the answer (or is unreadable all along). A choice is never
/// applied on a decision taken with a companion that cannot be read: nothing
/// set aside, nothing written.
#[test]
fn a1b_companion_held_through_the_answer() {
    for keep_tmp in [false, true] {
        // Readable at the question (the exe was written elsewhere), held at the answer.
        let f = fx("a1b");
        let tmp_bytes = image(EXE, &payload(2));
        put(&f.tmp, &tmp_bytes, 30);
        f.companion(Some(&payload(0)), false);
        let exe_bytes = image(EXE, &payload(1));
        put(&f.exe, &exe_bytes, 60);
        let companion_text = fs::read(f.companion_path()).unwrap();
        assert!(matches!(f.settle(), PendingDecision::Conflict { cause: ConflictCause::ExeChanged, .. }));
        let resolved = {
            let _held = hold(&f.companion_path());
            if keep_tmp {
                resolve_conflict_keep_tmp_in(&f.staging, &f.exe)
            } else {
                resolve_conflict_keep_exe_in(&f.staging, &f.exe)
            }
        };
        assert!(resolved.is_err(), "keep_tmp={}: {:?}", keep_tmp, resolved);
        assert!(names_in(&conflicts_dir_in(&f.staging)).is_empty(), "keep_tmp={}: nothing set aside", keep_tmp);
        assert_eq!(fs::read(&f.tmp).unwrap(), tmp_bytes, "keep_tmp={}", keep_tmp);
        assert_eq!(fs::read(f.companion_path()).unwrap(), companion_text, "keep_tmp={}", keep_tmp);
        assert_eq!(fs::read(&f.exe).unwrap(), exe_bytes, "keep_tmp={}", keep_tmp);
        assert!(matches!(f.decision(), PendingDecision::Conflict { .. }), "readable again: the question again");

        // Held all along: no question, and an answer cannot be applied either.
        let f = pending_save_of_a_session("a1b_all");
        let held = hold(&f.companion_path());
        assert!(!matches!(f.settle(), PendingDecision::Conflict { .. }));
        let resolved = if keep_tmp {
            resolve_conflict_keep_tmp_in(&f.staging, &f.exe)
        } else {
            resolve_conflict_keep_exe_in(&f.staging, &f.exe)
        };
        drop(held);
        assert!(resolved.is_err(), "keep_tmp={}: {:?}", keep_tmp, resolved);
        assert!(names_in(&conflicts_dir_in(&f.staging)).is_empty());
        assert_eq!(fs::read(&f.tmp).unwrap(), image(EXE, &payload(1)));
        assert_eq!(f.settle(), PendingDecision::Promote);
    }
}

/// Final-touch review: the companion becomes unreadable between the
/// inspection and the write (save, "keep the pending save", restaging at
/// exit). `stage_pair_in` writes nothing over a .tmp whose companion cannot
/// be read, and the same write succeeds once the companion is released.
#[test]
fn staging_refuses_to_write_over_a_tmp_whose_companion_cannot_be_read() {
    let f = pending_save_of_a_session("stage_held");
    let tmp_before = fs::read(&f.tmp).unwrap();
    let companion_before = fs::read(f.companion_path()).unwrap();
    let target = Companion { exe: f.exe.clone(), base: Some(sha(&payload(0))), program_from_updater: false };
    let new_image = image(EXE, &payload(3));

    let refused = {
        let _held = hold(&f.companion_path());
        stage_pair_in(&f.staging, &target, &new_image, None)
    };
    assert!(refused.is_err(), "{:?}", refused);
    assert_eq!(fs::read(&f.tmp).unwrap(), tmp_before, "the .tmp is untouched");
    assert_eq!(fs::read(f.companion_path()).unwrap(), companion_before, "the companion is untouched");

    stage_pair_in(&f.staging, &target, &new_image, None).unwrap();
    assert_eq!(fs::read(&f.tmp).unwrap(), new_image);
    assert_eq!(f.read_companion(), Some(Ok(target)));
}

/// Refuter pass 3, A2: importing from another LockNote file whose .tmp, or
/// its companion, cannot be read must not give the older notes of its exe
/// without a word: an explicit error naming the file, nothing imported.
#[test]
fn a2_import_with_an_unreadable_tmp_does_not_return_the_older_notes() {
    for held_file in ["tmp", "companion"] {
        let f = pending_save_of_a_session("a2");
        let path = if held_file == "tmp" { f.tmp.clone() } else { f.companion_path() };
        let got = {
            let _held = hold(&path);
            read_data_in(&f.staging, &f.exe)
        };
        match got {
            Err(e) => assert!(e.contains(&path.display().to_string()), "{} held: {}", held_file, e),
            Ok(notes) => panic!("{} held: notes given without a word (older ones: {})", held_file, notes == Some(payload(0))),
        }
        assert_eq!(read_data_in(&f.staging, &f.exe), Ok(Some(payload(1))), "readable again");
    }
}
