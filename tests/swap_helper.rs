// Integration tests of the swap helper: the real `locknote-swap` binary is
// launched, as the application does when it exits.
//
// LOCALAPPDATA and TEMP of the child are redirected to a scratch directory, so
// nothing is read from or written to the real %LOCALAPPDATA%\LockNote.
//
// `swap_onto_another_volume` needs a second volume and is ignored by default:
//   set LOCKNOTE_TEST_OTHER_VOLUME=D:\some\new\empty\dir
//   cargo test --test swap_helper -- --ignored
#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

/// A fresh scratch directory under the OS temp dir.
fn scratch(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "locknote_swap_it_{}_{}_{}",
        tag,
        std::process::id(),
        nanos
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// PID of a process that has already exited: the helper waits for its
/// "parent" and must not wait for a live one here.
fn exited_pid() -> u32 {
    let mut child = Command::new("cmd").args(["/c", "exit"]).spawn().unwrap();
    let pid = child.id();
    child.wait().unwrap();
    pid
}

/// Run the helper to completion with its environment redirected to `local`.
fn run_helper(local: &Path, tmp: &Path, exe: &Path) -> std::process::ExitStatus {
    let temp = local.join("temp");
    std::fs::create_dir_all(&temp).unwrap();
    Command::new(env!("CARGO_BIN_EXE_locknote-swap"))
        .arg(exited_pid().to_string())
        .arg(tmp)
        .arg(exe)
        .env("LOCALAPPDATA", local)
        .env("TEMP", &temp)
        .env("TMP", &temp)
        .status()
        .expect("the helper binary must start")
}

fn swap_log(local: &Path) -> PathBuf {
    local.join("LockNote").join("swap-error.log")
}

/// `[program][marker][payload]`, as the application writes it.
fn image(program: &[u8], seed: u8) -> Vec<u8> {
    let mut out = program.to_vec();
    out.extend_from_slice(b"LN_DATA_\xDE\xAD\xBE\xEF\xCA\xFE\xF0\x0D");
    out.extend_from_slice(&payload(seed));
    out
}

fn payload(seed: u8) -> Vec<u8> {
    (0..120u8).map(|i| i.wrapping_mul(3).wrapping_add(seed)).collect()
}

/// The companion the application writes next to `tmp` for a save based on
/// the notes `payload(base_seed)` of `exe`: the helper checks it right before
/// renaming. With the `volume=` line every companion carries since issue 057
/// (the helper must accept it, or every swap would be abandoned).
fn write_companion(tmp: &Path, exe: &Path, base_seed: u8) -> PathBuf {
    use sha2::{Digest, Sha256};
    let hex: String = Sha256::digest(payload(base_seed)).iter().map(|b| format!("{:02x}", b)).collect();
    let path = tmp.with_extension("base");
    let text = format!(
        "LockNote staging companion 1\nexe={}\nbase={}\nprogram=exe\nvolume=0123abcd\n",
        exe.display(),
        hex
    );
    std::fs::write(&path, text).unwrap();
    path
}

/// Files left next to the exe by an interrupted cross-volume swap.
fn leftover_siblings(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".lnswap"))
        .collect()
}

#[test]
fn swap_on_the_same_volume_replaces_the_exe_and_consumes_the_tmp() {
    let dir = scratch("same");
    let local = dir.join("local");
    std::fs::create_dir_all(&local).unwrap();
    let tmp = dir.join("0123456789abcdef.tmp");
    let exe = dir.join("LockNote-Pro.exe");
    std::fs::write(&tmp, image(b"PROGRAM", 1)).unwrap();
    std::fs::write(&exe, image(b"PROGRAM", 0)).unwrap();
    let companion = write_companion(&tmp, &exe, 0);

    let status = run_helper(&local, &tmp, &exe);

    assert!(status.success(), "helper exit status: {:?}", status);
    assert_eq!(std::fs::read(&exe).unwrap(), image(b"PROGRAM", 1));
    assert!(!tmp.exists(), "the .tmp is consumed by a successful swap");
    assert!(!companion.exists(), "its companion goes with it");
    assert!(leftover_siblings(&dir).is_empty());
    assert!(!swap_log(&local).exists(), "nothing is logged on success");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn failed_swap_keeps_the_tmp_and_logs_the_error() {
    let dir = scratch("fail");
    let local = dir.join("local");
    std::fs::create_dir_all(&local).unwrap();
    let tmp = dir.join("0123456789abcdef.tmp");
    let exe = dir.join("LockNote-Pro.exe");
    std::fs::write(&tmp, image(b"PROGRAM", 1)).unwrap();
    std::fs::write(&exe, image(b"PROGRAM", 0)).unwrap();
    write_companion(&tmp, &exe, 0);

    // The exe is held open (readable, not replaceable): the move fails.
    let status = {
        use std::os::windows::fs::OpenOptionsExt;
        let _held = std::fs::OpenOptions::new().read(true).share_mode(1).open(&exe).unwrap();
        run_helper(&local, &tmp, &exe)
    };

    assert_eq!(status.code(), Some(1), "a failed swap must not report success");
    assert_eq!(std::fs::read(&tmp).unwrap(), image(b"PROGRAM", 1));
    assert_eq!(std::fs::read(&exe).unwrap(), image(b"PROGRAM", 0));
    let log = std::fs::read_to_string(swap_log(&local)).expect("the failure must be logged");
    assert!(log.contains("MoveFileExW failed"), "log: {}", log);
    assert!(log.contains("LockNote-Pro.exe"), "log: {}", log);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The exe no longer holds the notes the pending save was based on (another
/// writer): the helper re-checks before renaming, touches nothing, logs it
/// and exits with code 1. The folder of the exe missing: the same.
#[test]
fn helper_touches_nothing_when_the_pending_save_is_no_longer_current() {
    let dir = scratch("recheck");
    let local = dir.join("local");
    std::fs::create_dir_all(&local).unwrap();
    let tmp = dir.join("0123456789abcdef.tmp");
    let exe = dir.join("LockNote-Pro.exe");
    std::fs::write(&tmp, image(b"PROGRAM", 1)).unwrap();
    std::fs::write(&exe, image(b"PROGRAM", 2)).unwrap();
    let companion = write_companion(&tmp, &exe, 0);

    let status = run_helper(&local, &tmp, &exe);

    assert_eq!(status.code(), Some(1));
    assert_eq!(std::fs::read(&exe).unwrap(), image(b"PROGRAM", 2), "the other notes are not overwritten");
    assert!(tmp.exists() && companion.exists());
    let log = std::fs::read_to_string(swap_log(&local)).unwrap();
    assert!(log.contains("swap abandoned, nothing touched"), "log: {}", log);

    let missing = dir.join("missing-folder").join("LockNote-Pro.exe");
    let status = run_helper(&local, &tmp, &missing);
    assert_eq!(status.code(), Some(1));
    assert!(tmp.exists() && !missing.exists());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn read_only_exe_is_not_replaced_and_the_tmp_is_kept() {
    let dir = scratch("readonly");
    let local = dir.join("local");
    std::fs::create_dir_all(&local).unwrap();
    let tmp = dir.join("0123456789abcdef.tmp");
    let exe = dir.join("LockNote-Pro.exe");
    std::fs::write(&tmp, image(b"PROGRAM", 1)).unwrap();
    std::fs::write(&exe, image(b"PROGRAM", 0)).unwrap();
    write_companion(&tmp, &exe, 0);
    let mut perms = std::fs::metadata(&exe).unwrap().permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&exe, perms).unwrap();

    let status = run_helper(&local, &tmp, &exe);

    assert!(!status.success());
    assert_eq!(std::fs::read(&exe).unwrap(), image(b"PROGRAM", 0));
    assert_eq!(std::fs::read(&tmp).unwrap(), image(b"PROGRAM", 1));
    let log = std::fs::read_to_string(swap_log(&local)).unwrap_or_default();
    assert!(log.contains("MoveFileExW failed"), "the move itself failed: {}", log);
    assert!(leftover_siblings(&dir).is_empty());
    assert!(swap_log(&local).exists(), "the failure must be logged");

    let mut perms = std::fs::metadata(&exe).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    perms.set_readonly(false);
    std::fs::set_permissions(&exe, perms).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// A ready event that does not exist (the application that started the
/// helper is gone before creating it, or a stray launch): the helper gives up
/// at once, with exit code 3, and touches nothing.
#[test]
fn helper_without_its_ready_event_gives_up_and_touches_nothing() {
    let dir = scratch("noevent");
    let local = dir.join("local");
    std::fs::create_dir_all(&local).unwrap();
    let tmp = dir.join("0123456789abcdef.tmp");
    let exe = dir.join("LockNote-Pro.exe");
    std::fs::write(&tmp, b"NEW").unwrap();
    std::fs::write(&exe, b"OLD").unwrap();
    let temp = local.join("temp");
    std::fs::create_dir_all(&temp).unwrap();

    let status = Command::new(env!("CARGO_BIN_EXE_locknote-swap"))
        .arg(exited_pid().to_string())
        .arg(&tmp)
        .arg(&exe)
        .arg("Local\\LockNote_SwapReady_does_not_exist_0123456789abcdef")
        .env("LOCALAPPDATA", &local)
        .env("TEMP", &temp)
        .env("TMP", &temp)
        .status()
        .unwrap();

    assert_eq!(status.code(), Some(3));
    assert_eq!(std::fs::read(&tmp).unwrap(), b"NEW");
    assert_eq!(std::fs::read(&exe).unwrap(), b"OLD");
    let log = std::fs::read_to_string(swap_log(&local)).unwrap();
    assert!(log.contains("gave up, nothing touched"), "log: {}", log);

    let _ = std::fs::remove_dir_all(&dir);
}

/// The defect of issue 055: the exe is not on the volume of the staging file.
/// `LOCKNOTE_TEST_OTHER_VOLUME` names a directory on another volume than the
/// OS temp dir; the test creates it if needed and only touches files it
/// created itself.
#[test]
#[ignore = "needs LOCKNOTE_TEST_OTHER_VOLUME=<directory on another volume>"]
fn swap_onto_another_volume() {
    let other = PathBuf::from(
        std::env::var_os("LOCKNOTE_TEST_OTHER_VOLUME")
            .expect("set LOCKNOTE_TEST_OTHER_VOLUME to a directory on another volume"),
    );
    std::fs::create_dir_all(&other).unwrap();
    let dir = scratch("othervol");
    let local = dir.join("local");
    std::fs::create_dir_all(&local).unwrap();

    // Prove the two directories are on different volumes: a plain rename
    // between them must fail with ERROR_NOT_SAME_DEVICE (17).
    let probe_src = dir.join("probe.bin");
    let probe_dst = other.join("locknote-c3-probe.bin");
    std::fs::write(&probe_src, b"probe").unwrap();
    match std::fs::rename(&probe_src, &probe_dst) {
        Err(e) => assert_eq!(
            e.raw_os_error(),
            Some(17),
            "expected ERROR_NOT_SAME_DEVICE, got {:?}",
            e
        ),
        Ok(()) => {
            let _ = std::fs::remove_file(&probe_dst);
            panic!(
                "{} is on the same volume as {}",
                other.display(),
                dir.display()
            );
        }
    }

    let tmp = dir.join("0123456789abcdef.tmp");
    let exe = other.join("LockNote-c3-test.exe");
    let program: Vec<u8> = (0..(3 * 1024 * 1024)).map(|i| (i % 251) as u8).collect();
    let new_image = image(&program, 1);
    std::fs::write(&tmp, &new_image).unwrap();
    std::fs::write(&exe, image(b"OLD PROGRAM", 0)).unwrap();
    write_companion(&tmp, &exe, 0);
    let tmp_modified = std::fs::metadata(&tmp).unwrap().modified().unwrap();

    let status = run_helper(&local, &tmp, &exe);

    let log = std::fs::read_to_string(swap_log(&local)).unwrap_or_default();
    assert!(status.success(), "helper failed, log: {}", log);
    assert_eq!(std::fs::read(&exe).unwrap(), new_image, "exe = content of the .tmp");
    assert!(!tmp.exists(), "the .tmp is removed once the exe holds its content");
    assert!(leftover_siblings(&other).is_empty(), "no sibling left next to the exe");
    assert!(log.is_empty(), "nothing is logged on success: {}", log);

    // The exe carries the date of the save, not the date of the copy (the
    // target file system may round it: exFAT keeps 10 ms, FAT 2 s).
    let exe_modified = std::fs::metadata(&exe).unwrap().modified().unwrap();
    let drift = match exe_modified.duration_since(tmp_modified) {
        Ok(d) => d,
        Err(e) => e.duration(),
    };
    assert!(drift <= Duration::from_secs(2), "mtime drift: {:?}", drift);

    let _ = std::fs::remove_file(&exe);
    let _ = std::fs::remove_dir_all(&dir);
}
